// SPDX-License-Identifier: GPL-3.0-or-later
//! Flatpak's own application monitor and restart transport.
use std::{
    cell::RefCell,
    collections::HashMap,
    io::{Read, Write},
    os::{fd::FromRawFd, unix::net::UnixStream},
    rc::Rc,
    time::Duration,
};

use gio::prelude::*;
use glib::variant::{Handle, ObjectPath};
use gtk::{gio, glib};
use ring::rand::{SecureRandom, SystemRandom};

use super::{Deployment, KEY_PIN};
use crate::config;

const PORTAL: &str = "org.freedesktop.portal.Flatpak";
const ROOT: &str = "/org/freedesktop/portal/Flatpak";
const MONITOR: &str = "org.freedesktop.portal.Flatpak.UpdateMonitor";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const DBUS: &str = "org.freedesktop.DBus";
const BUS_OBJECT: &str = "/org/freedesktop/DBus";
const CALL_TIMEOUT_MS: i32 = 10_000;
const LATEST_VERSION: u32 = 2;
const RESTART_FLAG: &str = "--internal-flatpak-restart";
const RESTART_FD: u32 = 9;
const READY: u8 = 1;
const COMMIT: u8 = 2;
const START_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_TIMEOUT: Duration = Duration::from_secs(90);

type Dict = HashMap<String, glib::Variant>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalError {
    Unavailable,
    InvalidIdentity,
    Denied,
    ManagerRequired,
    Failed,
    RestartFailed,
    Cancelled,
    Stale,
}

impl std::fmt::Display for PortalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "Портал обновлений Flatpak недоступен.",
            Self::InvalidIdentity => "Не удалось подтвердить соответствие сборки опубликованному обновлению.",
            Self::Denied => "Система отклонила установку обновления.",
            Self::ManagerRequired => "Установите обновление через системный менеджер приложений.",
            Self::Failed => "Не удалось установить обновление.",
            Self::RestartFailed => "Не удалось запустить подтверждённую версию. Закройте Karin и откройте его снова через системное меню.",
            Self::Cancelled => "Обновление отменено.",
            Self::Stale => "Предложение обновления изменилось. Проверьте обновления снова.",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Availability {
    Waiting,
    Install,
    Restart,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Progress {
    Running(Option<f64>),
    Empty,
    Done,
    Failed(PortalError),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PortalEvent {
    OfferChanged,
    Progress(Progress),
    Disconnected,
}

#[derive(Debug, PartialEq, Eq)]
struct Instance {
    app_ref: String,
    commit: String,
}

impl Instance {
    fn current() -> Result<Self, PortalError> {
        let mut data = String::new();
        std::fs::File::open("/.flatpak-info")
            .map_err(|_| PortalError::Unavailable)?
            .take(65_537)
            .read_to_string(&mut data)
            .map_err(|_| PortalError::InvalidIdentity)?;
        if data.len() > 65_536 {
            return Err(PortalError::InvalidIdentity);
        }
        Self::parse(&data).ok_or(PortalError::InvalidIdentity)
    }

    fn parse(data: &str) -> Option<Self> {
        let file = glib::KeyFile::new();
        file.load_from_data(data, glib::KeyFileFlags::NONE).ok()?;
        let name = file.string("Application", "name").ok()?;
        if name != config::APP_ID {
            return None;
        }
        let arch = file.string("Instance", "arch").ok()?;
        let branch = file.string("Instance", "branch").ok()?;
        let commit = file.string("Instance", "app-commit").ok()?;
        if !valid_checksum(&commit, 64)
            || !valid_ref_component(&arch)
            || !valid_ref_component(&branch)
        {
            return None;
        }
        Some(Self {
            app_ref: format!("app/{name}/{arch}/{branch}"),
            commit: commit.to_string(),
        })
    }
}

fn valid_ref_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn valid_checksum(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn confirmed_pin(pin: &str) -> bool {
    pin.len() == 40
        && pin
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte))
        && KEY_PIN.trim() == pin
}

#[derive(Debug, PartialEq, Eq)]
struct Offer {
    running: String,
    local: String,
    remote: String,
}

impl Offer {
    fn parse(data: &Dict) -> Option<Self> {
        let read = |name: &str| {
            let value = data.get(name)?.get::<String>()?;
            valid_checksum(&value, 64).then_some(value)
        };
        Some(Self {
            running: read("running-commit")?,
            local: read("local-commit")?,
            remote: read("remote-commit")?,
        })
    }
}

#[derive(Default)]
struct MonitorState {
    offer: Option<Offer>,
    installing: Option<String>,
    installed: Option<String>,
    closed: bool,
}

impl MonitorState {
    fn availability(&self, running: &str, target: &str) -> Availability {
        if self.closed || self.installing.is_some() || target == running {
            return Availability::Waiting;
        }
        if self.installed.as_deref() == Some(target) {
            return Availability::Restart;
        }
        match &self.offer {
            Some(offer) if offer.running == running && offer.local == target => {
                Availability::Restart
            }
            Some(offer)
                if offer.running == running && offer.local == running && offer.remote == target =>
            {
                Availability::Install
            }
            _ => Availability::Waiting,
        }
    }

    fn transition(&mut self, signal: &str, data: &Dict) -> Option<PortalEvent> {
        if self.closed {
            return None;
        }
        match signal {
            "UpdateAvailable" => {
                self.offer = Offer::parse(data);
                Some(PortalEvent::OfferChanged)
            }
            "Progress" if self.installing.is_some() => {
                let progress = parse_progress(data);
                if progress == Progress::Done {
                    self.installed = self.installing.take();
                } else if !matches!(progress, Progress::Running(_)) {
                    self.installing = None;
                }
                Some(PortalEvent::Progress(progress))
            }
            _ => None,
        }
    }
}

fn parse_progress(data: &Dict) -> Progress {
    let number = |field: &str| data.get(field)?.get::<u32>();
    match number("status") {
        Some(0) => {
            let percent = (|| {
                let count = number("n_ops")?;
                let operation = number("op")?;
                let percent = number("progress")?;
                if count == 0 || operation >= count || percent > 100 {
                    return None;
                }
                Some((f64::from(operation) + f64::from(percent) / 100.0) / f64::from(count))
            })();
            Progress::Running(percent)
        }
        Some(1) => Progress::Empty,
        Some(2) => Progress::Done,
        Some(3) => Progress::Failed(
            data.get("error")
                .and_then(|error| error.get::<String>())
                .as_deref()
                .map_or(PortalError::Failed, error_category),
        ),
        _ => Progress::Failed(PortalError::Failed),
    }
}

fn error_category(name: &str) -> PortalError {
    match name {
        "org.freedesktop.DBus.Error.AccessDenied" | "org.freedesktop.portal.Error.NotAllowed" => {
            PortalError::Denied
        }
        "org.freedesktop.DBus.Error.NotSupported" => PortalError::ManagerRequired,
        "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner"
        | "org.freedesktop.DBus.Error.UnknownInterface"
        | "org.freedesktop.DBus.Error.UnknownMethod"
        | "org.freedesktop.DBus.Error.Disconnected" => PortalError::Unavailable,
        "org.freedesktop.portal.Error.Cancelled"
        | "org.gtk.GDBus.UnmappedGError.Quark._g_2dio_2derror_2dquark.Code19" => {
            PortalError::Cancelled
        }
        _ => PortalError::Failed,
    }
}

fn classify(error: glib::Error) -> PortalError {
    if error.matches(gio::IOErrorEnum::Cancelled) {
        return PortalError::Cancelled;
    }
    gio::DBusError::remote_error(&error)
        .as_deref()
        .map_or(PortalError::Failed, error_category)
}

fn object_path(connection: &gio::DBusConnection, token: u64) -> Option<(String, String)> {
    let token = format!("karin{token:016x}");
    let sender = connection.unique_name()?;
    let sender = sender.strip_prefix(':')?.replace('.', "_");
    Some((format!("{ROOT}/update_monitor/{sender}/{token}"), token))
}

pub(crate) struct FlatpakMonitor {
    bus: gio::DBusConnection,
    path: String,
    instance: Instance,
    state: Rc<RefCell<MonitorState>>,
    _signals: [gio::SignalSubscription; 2],
}

impl FlatpakMonitor {
    pub(crate) async fn new(callback: impl Fn(PortalEvent) + 'static) -> Result<Self, PortalError> {
        let instance = Instance::current()?;
        let bus = gio::bus_get_future(gio::BusType::Session)
            .await
            .map_err(|_| PortalError::Unavailable)?;
        let version = bus
            .call_future(
                Some(PORTAL),
                ROOT,
                PROPERTIES,
                "Get",
                Some(&(PORTAL, "version").to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                CALL_TIMEOUT_MS,
            )
            .await
            .map_err(classify)?;
        if version
            .get::<(glib::Variant,)>()
            .and_then(|(version,)| version.get::<u32>())
            .is_none_or(|version| version < 2)
        {
            return Err(PortalError::Unavailable);
        }
        let mut nonce = [0; 8];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| PortalError::Unavailable)?;
        let (path, token) =
            object_path(&bus, u64::from_ne_bytes(nonce)).ok_or(PortalError::Unavailable)?;
        let state = Rc::new(RefCell::new(MonitorState::default()));
        let callback = Rc::new(callback);
        let monitor_signals = {
            let state = state.clone();
            let callback = callback.clone();
            bus.subscribe_to_signal(
                Some(PORTAL),
                Some(MONITOR),
                None,
                Some(&path),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    let Some((data,)) = signal.parameters.get::<(Dict,)>() else {
                        return;
                    };
                    let event = state.borrow_mut().transition(signal.signal_name, &data);
                    if let Some(event) = event {
                        callback(event);
                    }
                },
            )
        };
        let portal_owner = {
            let state = state.clone();
            bus.subscribe_to_signal(
                Some(DBUS),
                Some(DBUS),
                Some("NameOwnerChanged"),
                Some(BUS_OBJECT),
                Some(PORTAL),
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    let Some((name, old, new)) =
                        signal.parameters.get::<(String, String, String)>()
                    else {
                        return;
                    };
                    if name == PORTAL
                        && !old.is_empty()
                        && old != new
                        && !state
                            .replace(MonitorState {
                                closed: true,
                                ..Default::default()
                            })
                            .closed
                    {
                        callback(PortalEvent::Disconnected);
                    }
                },
            )
        };
        let monitor = Self {
            bus,
            path,
            instance,
            state,
            _signals: [monitor_signals, portal_owner],
        };
        let opts = Dict::from([("handle_token".into(), token.to_variant())]);
        let reply = monitor
            .bus
            .call_future(
                Some(PORTAL),
                ROOT,
                PORTAL,
                "CreateUpdateMonitor",
                Some(&(opts,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                CALL_TIMEOUT_MS,
            )
            .await
            .map_err(classify)?;
        if reply
            .get::<(ObjectPath,)>()
            .is_none_or(|(handle,)| handle.as_str() != monitor.path)
            || monitor.state.borrow().closed
        {
            return Err(PortalError::Unavailable);
        }
        Ok(monitor)
    }

    pub(crate) fn is_open(&self) -> bool {
        !self.state.borrow().closed
    }

    pub(crate) fn availability(&self, deployment: &Deployment) -> Availability {
        if self.instance.app_ref != deployment.app_ref
            || !confirmed_pin(&deployment.key_fingerprint)
        {
            return Availability::Waiting;
        }
        self.state
            .borrow()
            .availability(&self.instance.commit, &deployment.ostree_commit)
    }

    pub(crate) async fn install(
        &self,
        deployment: &Deployment,
        parent_window: &str,
    ) -> Result<(), PortalError> {
        if self.availability(deployment) != Availability::Install {
            return Err(PortalError::Stale);
        }
        self.state.borrow_mut().installing = Some(deployment.ostree_commit.clone());
        let args = (parent_window, Dict::new()).to_variant();
        let result = self
            .bus
            .call_future(
                Some(PORTAL),
                &self.path,
                MONITOR,
                "Update",
                Some(&args),
                None,
                gio::DBusCallFlags::NONE,
                CALL_TIMEOUT_MS,
            )
            .await;
        match result {
            Ok(_) if !self.state.borrow().closed => Ok(()),
            Ok(_) => Err(PortalError::Cancelled),
            Err(error) => {
                // The reply may be lost after the portal accepted the request.
                // Close this monitor before allowing another transaction.
                self.close();
                Err(classify(error))
            }
        }
    }

    pub(crate) async fn restart(
        &self,
        deployment: &Deployment,
    ) -> Result<RestartHandoff, PortalError> {
        if self.availability(deployment) != Availability::Restart {
            return Err(PortalError::Stale);
        }
        spawn_latest(&self.bus, deployment).await
    }

    pub(crate) fn close(&self) {
        if self
            .state
            .replace(MonitorState {
                closed: true,
                ..Default::default()
            })
            .closed
        {
            return;
        }
        self.bus.call(
            Some(PORTAL),
            &self.path,
            MONITOR,
            "Close",
            None,
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            CALL_TIMEOUT_MS,
            gio::Cancellable::NONE,
            |_| {},
        );
    }
}

impl Drop for FlatpakMonitor {
    fn drop(&mut self) {
        self.close();
    }
}

thread_local! {
    static RESTART: RefCell<Option<RestartHandoff>> = const { RefCell::new(None) };
}

/// Keeps the verified new process waiting until the old application's normal
/// shutdown has completed, including its durable settings/continuation holds.
pub(crate) struct RestartHandoff(UnixStream);

impl RestartHandoff {
    pub(crate) fn close_application(
        self,
        app: &impl IsA<gio::Application>,
    ) -> Result<(), PortalError> {
        let app: &gio::Application = app.as_ref();
        if !app.has_action("quit") || RESTART.with_borrow(Option::is_some) {
            return Err(PortalError::RestartFailed);
        }
        RESTART.with_borrow_mut(|pending| *pending = Some(self));
        app.activate_action("quit", None);
        Ok(())
    }
}

async fn spawn_latest(
    bus: &gio::DBusConnection,
    deployment: &Deployment,
) -> Result<RestartHandoff, PortalError> {
    let (parent, child) = UnixStream::pair().map_err(|_| PortalError::RestartFailed)?;
    let list = gio::UnixFDList::new();
    let handle = list
        .append(&child)
        .map_err(|_| PortalError::RestartFailed)?;
    drop(child);
    let fds = HashMap::from([(RESTART_FD, Handle(handle))]);
    let version = deployment.version.to_string();
    let arguments = [
        "/app/bin/karin",
        RESTART_FLAG,
        &version,
        &deployment.source_commit,
        &deployment.ostree_commit,
        &deployment.app_ref,
        &deployment.key_fingerprint,
    ];
    let argv: Vec<Vec<u8>> = arguments
        .into_iter()
        .map(|arg| {
            let mut bytes = arg.as_bytes().to_vec();
            bytes.push(0);
            bytes
        })
        .collect();
    let args = (
        vec![0u8],
        argv,
        fds,
        HashMap::<String, String>::new(),
        LATEST_VERSION,
        Dict::new(),
    )
        .to_variant();
    bus.call_with_unix_fd_list_future(
        Some(PORTAL),
        ROOT,
        PORTAL,
        "Spawn",
        Some(&args),
        None,
        gio::DBusCallFlags::NONE,
        CALL_TIMEOUT_MS,
        Some(&list),
    )
    .await
    .map_err(|_| PortalError::RestartFailed)?;
    // The parent's only retained endpoint must never contain the child's FD.
    drop(list);
    gio::spawn_blocking(move || wait_ready(parent))
        .await
        .map_err(|_| PortalError::RestartFailed)?
}

fn wait_ready(mut stream: UnixStream) -> Result<RestartHandoff, PortalError> {
    stream
        .set_read_timeout(Some(START_TIMEOUT))
        .and_then(|_| stream.set_write_timeout(Some(START_TIMEOUT)))
        .map_err(|_| PortalError::RestartFailed)?;
    let mut byte = [0];
    stream
        .read_exact(&mut byte)
        .map_err(|_| PortalError::RestartFailed)?;
    if byte != [READY] {
        return Err(PortalError::InvalidIdentity);
    }
    Ok(RestartHandoff(stream))
}

/// Invoked before GTK registration or creation of any playback controller.
pub fn accept_restart(args: &mut Vec<String>) -> Result<(), PortalError> {
    if args.first().is_none_or(|flag| flag != RESTART_FLAG) {
        return Ok(());
    }
    let [_, version, revision, commit, reference, fingerprint] = args.as_slice() else {
        return Err(PortalError::InvalidIdentity);
    };
    let instance = Instance::current()?;
    if config::VERSION != version
        || config::SOURCE_STATUS != "clean"
        || config::SOURCE_REVISION != revision
        || !valid_checksum(revision, 40)
        || instance.app_ref != *reference
        || instance.commit != *commit
        || !confirmed_pin(fingerprint)
    {
        return Err(PortalError::InvalidIdentity);
    }
    let fd = i32::try_from(RESTART_FD).expect("fixed descriptor fits i32");
    let link = std::fs::read_link(format!("/proc/self/fd/{fd}"))
        .map_err(|_| PortalError::RestartFailed)?;
    if !link.as_os_str().as_encoded_bytes().starts_with(b"socket:[") {
        return Err(PortalError::RestartFailed);
    }
    // The fixed descriptor was inherited from Flatpak Spawn. Readlink proved
    // it is an open socket. This runs before Karin starts any other threads,
    // so no concurrent Karin code can close it or transfer ownership.
    let mut stream = unsafe { UnixStream::from_raw_fd(fd) };
    stream
        .set_read_timeout(Some(STOP_TIMEOUT))
        .and_then(|_| stream.set_write_timeout(Some(START_TIMEOUT)))
        .and_then(|_| stream.write_all(&[READY]))
        .map_err(|_| PortalError::RestartFailed)?;
    let mut byte = [0];
    stream
        .read_exact(&mut byte)
        .map_err(|_| PortalError::RestartFailed)?;
    if byte != [COMMIT] || !matches!(stream.read(&mut byte), Ok(0)) {
        return Err(PortalError::RestartFailed);
    }
    args.clear();
    Ok(())
}

/// Called after the old GApplication loop and its shutdown holds have ended.
pub fn finish_restart(clean_exit: bool) {
    if let Some(RestartHandoff(mut stream)) = RESTART.with_borrow_mut(Option::take)
        && clean_exit
    {
        let _ = stream.write_all(&[COMMIT]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatpak_identity_is_bound_to_the_running_ref() {
        let info = format!(
            "[Application]\nname={}\n[Instance]\narch=x86_64\nbranch=master\napp-commit={}\n",
            config::APP_ID,
            "a".repeat(64)
        );
        let instance = Instance::parse(&info).unwrap();
        assert_eq!(
            instance.app_ref,
            format!("app/{}/x86_64/master", config::APP_ID)
        );
        assert!(Instance::parse(&info.replace("master", "../wrong")).is_none());
        assert!(Instance::parse(&info.replace(config::APP_ID, "org.example.Other")).is_none());
        assert!(Instance::parse(&info.replace(&"a".repeat(64), "1234")).is_none());
    }

    #[test]
    fn requires_an_exact_remote_or_local_offer() {
        let running = "a".repeat(64);
        let target = "b".repeat(64);
        let unrelated = "c".repeat(64);
        let mut state = MonitorState {
            offer: Some(Offer {
                running: running.clone(),
                local: running.clone(),
                remote: target.clone(),
            }),
            ..Default::default()
        };
        assert_eq!(
            state.availability(&running, &running),
            Availability::Waiting
        );
        assert_eq!(
            state.availability(&running, &unrelated),
            Availability::Waiting
        );
        assert_eq!(state.availability(&running, &target), Availability::Install);
        state.offer.as_mut().unwrap().local = unrelated;
        assert_eq!(state.availability(&running, &target), Availability::Waiting);
        state.offer.as_mut().unwrap().local = target.clone();
        assert_eq!(state.availability(&running, &target), Availability::Restart);
    }

    #[test]
    fn progress_only_for_pending_install_and_never_treats_failure_as_success() {
        let mut state = MonitorState::default();
        let done = Dict::from([("status".into(), 2u32.to_variant())]);
        assert!(state.transition("Progress", &done).is_none());
        let running = "a".repeat(64);
        let target = "b".repeat(64);
        state.installing = Some(target.clone());
        let denied = Dict::from([
            ("status".into(), 3u32.to_variant()),
            (
                "error".into(),
                "org.freedesktop.DBus.Error.AccessDenied".to_variant(),
            ),
        ]);
        assert!(matches!(
            state.transition("Progress", &denied),
            Some(PortalEvent::Progress(Progress::Failed(PortalError::Denied)))
        ));
        assert_eq!(state.availability(&running, &target), Availability::Waiting);
        state.installing = Some(target.clone());
        assert!(matches!(
            state.transition("Progress", &done),
            Some(PortalEvent::Progress(Progress::Done))
        ));
        assert_eq!(state.availability(&running, &target), Availability::Restart);
    }

    #[test]
    fn restart_rejects_early_exit_or_unexpected_acknowledgement() {
        let (parent, child) = UnixStream::pair().unwrap();
        drop(child);
        assert!(matches!(
            wait_ready(parent),
            Err(PortalError::RestartFailed)
        ));
        let (parent, mut child) = UnixStream::pair().unwrap();
        child.write_all(&[COMMIT]).unwrap();
        assert!(matches!(
            wait_ready(parent),
            Err(PortalError::InvalidIdentity)
        ));
        let (parent, mut child) = UnixStream::pair().unwrap();
        child.write_all(&[READY]).unwrap();
        assert!(wait_ready(parent).is_ok());
    }
}
