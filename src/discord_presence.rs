// SPDX-License-Identifier: GPL-3.0-or-later
//! Local Discord RPC. GTK supplies actual player events and owns no socket.

use std::{
    env, io,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, OnceLock},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use crate::music::{Track, public_links};

mod flatpak;
mod peer;
mod wire;
pub use flatpak::run_discord_socket_helper;
use wire::Connection;

const APPLICATION_ID: &str = "1575553368280137790";
const DISCORD_FLATPAK: &str = "com.discordapp.Discord";
const RETRY_DELAY: Duration = Duration::from_secs(5);
const ACTIVITY_INTERVAL: Duration = Duration::from_secs(4);
// Set to Some("karin_logo") when the owner registers that Rich Presence art asset.
const FALLBACK_IMAGE: Option<&str> = None;

fn diagnostic(
    event: &'static str,
    error: Option<&io::Error>,
    code: Option<i64>,
    started: Option<Instant>,
) {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    if !*ENABLED.get_or_init(|| {
        env::var_os("KARIN_DISCORD_DIAGNOSTICS").as_deref() == Some(std::ffi::OsStr::new("1"))
    }) {
        return;
    }
    let kind = error.map(io::Error::kind);
    let errno = error.and_then(io::Error::raw_os_error);
    let elapsed_ms = started.map(|start| start.elapsed().as_millis());
    eprintln!(
        "Karin Discord: event={event} kind={kind:?} errno={errno:?} code={code:?} elapsed_ms={elapsed_ms:?}"
    );
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Status {
    #[default]
    Disabled,
    Waiting,
    Connected,
    Unavailable,
    Failed,
}

impl Status {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Disabled => "Отключено",
            Self::Waiting => "Ожидание Discord",
            Self::Connected => "Подключено",
            Self::Unavailable => "Discord недоступен",
            Self::Failed => "Ошибка подключения",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Progress {
    Playing {
        position: Option<Duration>,
        duration: Option<Duration>,
    },
    Paused,
}

#[derive(PartialEq, Eq, Serialize)]
pub(crate) struct Activity {
    #[serde(rename = "type")]
    kind: u8,
    details: String,
    details_url: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamps: Option<Timestamps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assets: Option<Assets>,
    #[serde(skip)]
    paused: bool,
}

#[derive(PartialEq, Eq, Serialize)]
struct Timestamps {
    start: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    end: Option<u64>,
}

#[derive(PartialEq, Eq, Serialize)]
struct Assets {
    large_image: String,
}

impl Activity {
    pub(crate) fn new(track: &Track, progress: Progress, now: SystemTime) -> Option<Self> {
        // A direct numeric track path alone says nothing about whether an
        // account-local upload belongs to the public catalog. Require its
        // canonical album relationship as the admission boundary.
        let track_link = track.album_id.as_deref().and_then(|album_id| {
            public_links::album(album_id)?;
            public_links::track(&track.id, Some(album_id))
        });
        let Some(track_link) = track_link else {
            diagnostic("activity_skipped_catalog_link", None, None, None);
            return None;
        };
        if !track.available {
            diagnostic("activity_skipped_unavailable", None, None, None);
            return None;
        }
        let details = field(&track.title);
        let artist = field(&track.artists.join(", "));
        let album = track
            .album_title
            .as_deref()
            .map(field)
            .filter(|album| !album.is_empty() && album != &details && album != &artist);
        let state = match album {
            Some(album) if artist.is_empty() => album,
            Some(album) => format!("{artist} · {album}"),
            None => artist,
        };
        let (paused, timestamps) = match progress {
            Progress::Playing { position, duration } => {
                let timestamps = position.and_then(|position| {
                    let start = now.checked_sub(position)?.duration_since(UNIX_EPOCH).ok()?;
                    Some(Timestamps {
                        start: start.as_secs(),
                        end: duration
                            .filter(|duration| *duration > position)
                            .and_then(|duration| start.checked_add(duration))
                            .map(|end| end.as_secs())
                            .filter(|end| *end > start.as_secs()),
                    })
                });
                (false, timestamps)
            }
            Progress::Paused => (true, None),
        };
        let state = match (paused, state.is_empty()) {
            (true, true) => "Пауза".into(),
            (true, false) => format!("Пауза · {state}"),
            (false, _) => state,
        };
        let assets = track
            .cover
            .as_ref()
            .and_then(crate::artwork::CoverArt::public_track_image)
            .or_else(|| FALLBACK_IMAGE.map(str::to_owned))
            .map(|large_image| Assets { large_image });
        Some(Self {
            kind: 2,
            details,
            details_url: track_link,
            state: field(&state),
            timestamps,
            assets,
            paused,
        })
    }
}

fn field(input: &str) -> String {
    input
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(128)
        .collect()
}

#[derive(Default)]
struct State {
    session: u64,
    enabled: bool,
    closed: bool,
    status: Status,
    activity: Option<Arc<Activity>>,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn status(&self, session: u64, status: Status) {
        let mut state = self.state.lock().unwrap();
        if state.enabled && state.session == session {
            state.status = status;
        }
    }

    fn wait(&self, session: u64, duration: Duration) {
        let state = self.state.lock().unwrap();
        if !state.closed && state.session == session {
            drop(self.changed.wait_timeout(state, duration).unwrap());
        }
    }
}

/// One bounded latest-value mailbox. Work and waits always run outside its lock.
#[derive(Default)]
pub(crate) struct Presence {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl Presence {
    pub(crate) fn status(&self) -> Status {
        self.shared.state.lock().unwrap().status
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.shared.state.lock().unwrap().enabled
    }

    pub(crate) fn enable(&mut self, enabled: bool) -> bool {
        {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed || state.enabled == enabled {
                return false;
            }
            state.session = state.session.wrapping_add(1);
            state.enabled = enabled;
            state.activity = None;
            state.status = if enabled {
                Status::Waiting
            } else {
                Status::Disabled
            };
        }
        if enabled && self.worker.is_none() {
            let shared = self.shared.clone();
            self.worker = thread::Builder::new()
                .name("karin-discord".into())
                .spawn(move || {
                    let flatpak = is_flatpak();
                    run(&shared, &platform_sockets(flatpak), flatpak);
                })
                .inspect_err(|error| diagnostic("worker_start", Some(error), None, None))
                .ok();
            if self.worker.is_none() {
                self.shared.state.lock().unwrap().status = Status::Failed;
            }
        }
        self.shared.changed.notify_one();
        true
    }

    pub(crate) fn update(&self, activity: Option<Activity>) {
        let mut state = self.shared.state.lock().unwrap();
        if state.enabled && state.activity.as_deref() != activity.as_ref() {
            state.activity = activity.map(Arc::new);
            self.shared.changed.notify_one();
        }
    }

    pub(crate) fn retire_activity(&self) {
        let mut state = self.shared.state.lock().unwrap();
        if state.enabled {
            // An account generation cannot inherit an old socket or payload.
            state.session = state.session.wrapping_add(1);
            state.activity = None;
            state.status = Status::Waiting;
            self.shared.changed.notify_one();
        }
    }

    /// The UI can retain an application hold and join this off GTK.
    pub(crate) fn shutdown(&mut self) -> Option<JoinHandle<()>> {
        let mut state = self.shared.state.lock().unwrap();
        state.closed = true;
        state.enabled = false;
        state.activity = None;
        state.status = Status::Disabled;
        self.shared.changed.notify_one();
        self.worker.take()
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn is_flatpak() -> bool {
    Path::new("/.flatpak-info").is_file()
}

fn runtime_directory() -> Option<PathBuf> {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn platform_sockets(sandboxed: bool) -> Vec<PathBuf> {
    let runtime = runtime_directory();
    let temporary = ["TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .find_map(|key| {
            env::var_os(key)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        })
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    socket_paths(runtime.as_deref(), &temporary, sandboxed)
}

fn socket_paths(runtime: Option<&Path>, temporary: &Path, sandboxed: bool) -> Vec<PathBuf> {
    let mut sockets = Vec::new();
    if let Some(runtime) = runtime {
        let count = if sandboxed { 1 } else { 10 };
        sockets.extend((0..count).map(|i| runtime.join(format!("discord-ipc-{i}"))));
        sockets.push(
            runtime
                .join("app")
                .join(DISCORD_FLATPAK)
                .join("discord-ipc-0"),
        );
    } else if !sandboxed {
        sockets.extend((0..10).map(|i| temporary.join(format!("discord-ipc-{i}"))));
    }
    sockets
}

fn socket_unavailable(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    )
}

fn connect(
    sockets: &[PathBuf],
    shared: &Shared,
    session: u64,
    refresh_native: bool,
) -> Result<Connection, Status> {
    let active = || {
        let state = shared.state.lock().unwrap();
        state.enabled && state.session == session && !state.closed
    };
    let mut status = Status::Unavailable;
    let mut refresh = false;
    for (index, path) in sockets.iter().enumerate() {
        if !active() {
            return Err(status);
        }
        match Connection::open(path) {
            Ok(connection) => return Ok(connection),
            // The first Flatpak candidate is the individually mounted native socket.
            Err(error) if socket_unavailable(&error) => refresh |= refresh_native && index == 0,
            Err(_) => status = Status::Failed,
        }
    }
    if refresh && active() {
        let started = Instant::now();
        let result = flatpak::refresh_native_socket().inspect_err(|error| {
            diagnostic("socket_refresh", Some(error), None, Some(started));
        });
        match result.and_then(Connection::handshake) {
            Ok(connection) => return Ok(connection),
            Err(error) if socket_unavailable(&error) => {}
            Err(_) => status = Status::Failed,
        }
    }
    Err(status)
}

fn run(shared: &Shared, sockets: &[PathBuf], refresh_native: bool) {
    if sockets.is_empty() {
        diagnostic("socket_no_candidates", None, None, None);
    }
    let mut session = 0;
    let mut connection: Option<Connection> = None;
    let mut sent: Option<Arc<Activity>> = None;
    let mut retry = Instant::now();
    let mut next_update = Instant::now();
    loop {
        let (closed, enabled, generation, activity) = {
            let state = shared.state.lock().unwrap();
            (
                state.closed,
                state.enabled,
                state.session,
                state.activity.clone(),
            )
        };
        if generation != session || closed {
            if let Some(mut socket) = connection.take()
                && sent.take().is_some()
            {
                let _ = socket.set_activity(None);
            }
            session = generation;
            retry = Instant::now();
            next_update = retry;
        }
        if closed {
            return;
        }
        if !enabled {
            let mut state = shared.state.lock().unwrap();
            while !state.enabled && !state.closed {
                state = shared.changed.wait(state).unwrap();
            }
            continue;
        }
        if connection.is_none() {
            let delay = retry.saturating_duration_since(Instant::now());
            if !delay.is_zero() {
                shared.wait(session, delay);
                continue;
            }
            match connect(sockets, shared, session, refresh_native) {
                Ok(socket) => {
                    connection = Some(socket);
                    shared.status(session, Status::Connected);
                }
                Err(status) => {
                    shared.status(session, status);
                    retry = Instant::now() + RETRY_DELAY;
                }
            }
            // The latest event might have changed during the handshake.
            continue;
        }
        let socket = connection.as_mut().unwrap();
        let must_clear = activity.is_none() && sent.is_some();
        let must_pause = activity.as_ref().is_some_and(|activity| activity.paused)
            && sent.as_ref().is_some_and(|activity| !activity.paused);
        let result =
            if activity != sent && (must_clear || must_pause || Instant::now() >= next_update) {
                let current = {
                    let state = shared.state.lock().unwrap();
                    state.enabled
                        && !state.closed
                        && state.session == session
                        && match (&state.activity, &activity) {
                            (Some(latest), Some(observed)) => Arc::ptr_eq(latest, observed),
                            (None, None) => true,
                            _ => false,
                        }
                };
                if !current {
                    continue;
                }
                let result = socket.set_activity(activity.as_deref());
                if result.is_ok() {
                    sent = activity;
                    if sent.is_some() {
                        next_update = Instant::now() + ACTIVITY_INTERVAL;
                    }
                }
                result
            } else {
                socket.poll()
            };
        if let Err(error) = result {
            connection = None;
            sent = None;
            retry = Instant::now() + RETRY_DELAY;
            next_update = retry;
            shared.status(
                session,
                if matches!(
                    error.kind(),
                    io::ErrorKind::UnexpectedEof
                        | io::ErrorKind::BrokenPipe
                        | io::ErrorKind::ConnectionReset
                ) {
                    Status::Unavailable
                } else {
                    Status::Failed
                },
            );
        }
    }
}

#[cfg(test)]
mod tests;
