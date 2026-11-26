// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in, local Discord RPC. GTK supplies actual player events and owns no socket.

use std::{
    env, io,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use crate::music::{Track, public_links};

mod wire;
use wire::Connection;

const APPLICATION_ID: &str = "1575553368280137790";
const DISCORD_FLATPAK: &str = "com.discordapp.Discord";
const RETRY_DELAY: Duration = Duration::from_secs(5);
const ACTIVITY_INTERVAL: Duration = Duration::from_secs(4);
const DOWNLOAD_URL: &str = "https://github.com/tracerxbrhd/karin/releases/latest";
// Set to Some("karin_logo") when the owner registers that Rich Presence art asset.
const FALLBACK_IMAGE: Option<&str> = None;

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
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamps: Option<Timestamps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assets: Option<Assets>,
    buttons: Vec<Button>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    large_text: Option<String>,
}

#[derive(PartialEq, Eq, Serialize)]
struct Button {
    label: &'static str,
    url: String,
}

impl Activity {
    pub(crate) fn new(track: &Track, progress: Progress, now: SystemTime) -> Option<Self> {
        // A direct numeric track path alone says nothing about whether an
        // account-local upload belongs to the public catalog. Require its
        // canonical album relationship as the admission boundary.
        let album_id = track.album_id.as_deref()?;
        public_links::album(album_id)?;
        let track_link = public_links::track(&track.id, Some(album_id))?;
        if !track.available {
            return None;
        }
        let album = track.album_title.as_deref();
        let details = field(&track.title);
        let artist = track.artists.join(", ");
        let state = if let Some(album) = album.filter(|title| !title.is_empty()) {
            format!("{artist} · {album}")
        } else {
            artist
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
        let state = if paused {
            format!("Пауза · {state}")
        } else {
            state
        };
        let assets = track
            .cover
            .as_ref()
            .and_then(crate::artwork::CoverArt::public_track_image)
            .or_else(|| FALLBACK_IMAGE.map(str::to_owned))
            .map(|large_image| Assets {
                large_image,
                large_text: album.map(field),
            });
        let buttons = vec![
            Button {
                label: "Слушать в Яндекс Музыке",
                url: track_link,
            },
            Button {
                label: "Скачать Karin",
                url: DOWNLOAD_URL.into(),
            },
        ];
        Some(Self {
            kind: 2,
            details,
            state: field(&state),
            timestamps,
            assets,
            buttons,
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
                .spawn(move || run(&shared, &platform_sockets()))
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

fn platform_sockets() -> Vec<PathBuf> {
    let runtime = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    let temporary = ["TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .find_map(|key| {
            env::var_os(key)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        })
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let sandboxed = Path::new("/.flatpak-info").is_file();
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

fn connect(sockets: &[PathBuf], shared: &Shared, session: u64) -> Result<Connection, Status> {
    let mut status = Status::Unavailable;
    for path in sockets {
        let active = {
            let state = shared.state.lock().unwrap();
            state.enabled && state.session == session && !state.closed
        };
        if !active {
            break;
        }
        match Connection::open(path) {
            Ok(connection) => return Ok(connection),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) => {}
            Err(_) => status = Status::Failed,
        }
    }
    Err(status)
}

fn run(shared: &Shared, sockets: &[PathBuf]) {
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
            match connect(sockets, shared, session) {
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
