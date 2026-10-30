// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-independent playback on a dedicated worker, separate from service access.
//!
//! Public source constructors accept local files or an opaque resolver-issued
//! remote source. No raw URL/token constructor is exposed to callers. No source
//! paths, resource URLs or native decoder messages are exposed
//! in snapshots/errors. Commands and shutdown never wait for GStreamer on GTK.

use std::{
    fmt,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use gst::prelude::*;
use gstreamer as gst;
use reqwest::Url;
use thiserror::Error;
use zeroize::Zeroizing;

mod remote;
#[cfg(test)]
mod remote_tests;

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const TRANSITION_TIMEOUT: Duration = Duration::from_secs(5);
const QUEUE_CAPACITY: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackOutput {
    /// Real decoding and clocked playback, without a physical sound device.
    Fakesink,
    /// Physical output must only follow an explicit developer/user action.
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PlaybackState {
    #[default]
    Idle,
    Loading,
    Ready,
    Playing,
    Paused,
    Stopped,
    Ended,
    Error,
}

/// Deliberately excludes source names, URLs, native debug strings and metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackSnapshot {
    pub state: PlaybackState,
    pub position: Option<Duration>,
    pub duration: Option<Duration>,
    pub seekable: bool,
    pub volume: f64,
    pub last_error: Option<PlaybackError>,
    pub worker_running: bool,
    /// Changes when a source is replaced; old pipeline messages cannot apply.
    pub generation: u64,
}

impl Default for PlaybackSnapshot {
    fn default() -> Self {
        Self {
            state: PlaybackState::Idle,
            position: None,
            duration: None,
            seekable: false,
            volume: 0.5,
            last_error: None,
            worker_running: true,
            generation: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum PlaybackError {
    #[error("The playback worker could not start.")]
    WorkerUnavailable,
    #[error("GStreamer could not initialize.")]
    Initialization,
    #[error("The media source is unavailable or is not supported.")]
    SourceUnavailable,
    #[error("The playback source admission has expired; resolve it again.")]
    SourceExpired,
    #[error("The remote playback source violates its transport policy.")]
    SourcePolicy,
    #[error("The media server rejected playback (HTTP {0}).")]
    RemoteHttp(u16),
    #[error("The required playback or audio-output component is unavailable.")]
    Pipeline,
    #[error("The media could not be decoded or the audio output failed.")]
    Playback,
    #[error("The playback state transition failed.")]
    StateChange,
    #[error("The playback state transition timed out.")]
    Timeout,
    #[error("This action is unavailable in the current playback state.")]
    InvalidState,
    #[error("Volume must be a finite number between zero and one.")]
    InvalidVolume,
    #[error("The requested seek position is invalid or outside this media.")]
    InvalidSeek,
    #[error("The current media does not support seeking.")]
    NotSeekable,
    #[error("The media could not seek to the requested position.")]
    SeekFailed,
    #[error("The playback command queue is full; try again shortly.")]
    QueueFull,
    #[error("The playback controller is shutting down.")]
    ShuttingDown,
    #[error("The generated developer media could not be created.")]
    DiagnosticMedia,
}

#[derive(Clone)]
pub struct PlaybackSource {
    path: PathBuf,
    remote: Option<RemotePlaybackSource>,
}

impl fmt::Debug for PlaybackSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(if self.remote.is_some() {
            "PlaybackSource::RemoteHttps([redacted])"
        } else {
            "PlaybackSource::LocalFile([redacted])"
        })
    }
}

impl PlaybackSource {
    /// This filesystem validation may block; perform it on a worker for GTK.
    pub fn local_file(path: impl AsRef<Path>) -> Result<Self, PlaybackError> {
        let path = fs::canonicalize(path).map_err(|_| PlaybackError::SourceUnavailable)?;
        if !path.is_file() {
            return Err(PlaybackError::SourceUnavailable);
        }
        Ok(Self { path, remote: None })
    }

    /// Only an opaque, short-lived source issued by the service resolver can
    /// reach this path. In particular this is not a URL/token entry point.
    pub fn remote(source: RemotePlaybackSource) -> Self {
        Self {
            path: PathBuf::new(),
            remote: Some(source),
        }
    }
}

/// In-memory admission for a server-described legacy media resource. The
/// monotonic deadline limits new connections; it is not a parsed server expiry
/// and does not interrupt an already playing track when admission ages out.
#[derive(Clone)]
pub struct RemotePlaybackSource {
    uri: Zeroizing<String>,
    expires_at: Instant,
    #[cfg(test)]
    ca_file: Option<PathBuf>,
}

impl fmt::Debug for RemotePlaybackSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RemotePlaybackSource([redacted])")
    }
}

/// A deliberately narrow generated host family corroborated by the published
/// legacy descriptor example `s268myt.storage.yandex.net`. It is not a promise
/// that every current service CDN name is supported.
pub(crate) fn permitted_media_host(host: &str) -> bool {
    if !host.is_ascii() {
        return false;
    }
    let Some(label) = host.strip_suffix(".storage.yandex.net") else {
        return false;
    };
    let Some(label) = label.strip_prefix('s') else {
        return false;
    };
    if !(4..=8).contains(&label.len()) {
        return false;
    }
    let (digits, suffix) = label.split_at(label.len() - 3);
    !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && suffix.bytes().all(|byte| byte.is_ascii_lowercase())
}

impl RemotePlaybackSource {
    pub(crate) fn from_resolver(uri: &str, expires_at: Instant) -> Result<Self, PlaybackError> {
        let url = checked_remote_url(uri)?;
        if url.scheme() != "https"
            || url.port_or_known_default() != Some(443)
            || !url.host_str().is_some_and(permitted_media_host)
        {
            return Err(PlaybackError::SourcePolicy);
        }
        Self::new_admission(url.as_str(), expires_at)
    }

    fn new_admission(uri: &str, expires_at: Instant) -> Result<Self, PlaybackError> {
        let remaining = expires_at
            .checked_duration_since(Instant::now())
            .ok_or(PlaybackError::SourceExpired)?;
        if remaining.is_zero() || remaining > Duration::from_secs(120) {
            return Err(PlaybackError::SourceExpired);
        }
        Ok(Self {
            uri: Zeroizing::new(uri.to_owned()),
            expires_at,
            #[cfg(test)]
            ca_file: None,
        })
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    #[cfg(test)]
    pub(crate) fn from_test_resolver(
        uri: &str,
        expires_at: Instant,
    ) -> Result<Self, PlaybackError> {
        let url = checked_remote_url(uri)?;
        if url.scheme() != "http"
            || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))
            || url.port().is_none_or(|port| port == 0)
        {
            return Err(PlaybackError::SourcePolicy);
        }
        Self::new_admission(url.as_str(), expires_at)
    }
}

fn checked_remote_url(uri: &str) -> Result<Url, PlaybackError> {
    if uri.len() > 4096
        || !uri.is_ascii()
        || uri
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control() || byte == b'\\')
    {
        return Err(PlaybackError::SourcePolicy);
    }
    let authority = uri
        .split_once("://")
        .map(|(_, tail)| tail)
        .and_then(|tail| tail.split(['/', '?', '#']).next())
        .ok_or(PlaybackError::SourcePolicy)?;
    // Url deliberately normalizes an empty userinfo/port away. Those raw forms
    // still violate the no-userinfo, exact-origin admission contract.
    if authority.is_empty() || authority.contains('@') || authority.ends_with(':') {
        return Err(PlaybackError::SourcePolicy);
    }
    let url = Url::parse(uri).map_err(|_| PlaybackError::SourcePolicy)?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url
            .path()
            .strip_prefix("/get-mp3/")
            .is_some_and(|path| !path.is_empty())
    {
        return Err(PlaybackError::SourcePolicy);
    }
    Ok(url)
}

enum Command {
    Prepare(PlaybackSource),
    Play,
    Pause,
    Resume,
    Stop,
    Volume(f64),
    Seek(Duration),
}

struct Shared {
    snapshot: Mutex<PlaybackSnapshot>,
    shutdown: AtomicBool,
    #[cfg(test)]
    pipelines: Mutex<Vec<gst::glib::WeakRef<gst::Element>>>,
}

impl Shared {
    fn snapshot(&self) -> MutexGuard<'_, PlaybackSnapshot> {
        self.snapshot
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

struct Handle {
    sender: SyncSender<Command>,
    shared: Arc<Shared>,
}

impl Drop for Handle {
    fn drop(&mut self) {
        // Do not join a native-media thread from the GTK main thread. The worker
        // observes this before processing further commands and owns all cleanup.
        self.shared.shutdown.store(true, Ordering::Release);
    }
}

#[derive(Clone)]
pub struct PlaybackController {
    handle: Arc<Handle>,
}

impl PlaybackController {
    /// Spawns a worker; GStreamer initialization and media work happen there.
    pub fn new(output: PlaybackOutput) -> Result<Self, PlaybackError> {
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(PlaybackSnapshot::default()),
            shutdown: AtomicBool::new(false),
            #[cfg(test)]
            pipelines: Mutex::new(Vec::new()),
        });
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let worker_shared = shared.clone();
        thread::Builder::new()
            .name("karin-playback".into())
            .spawn(move || Worker::run(output, receiver, worker_shared))
            .map_err(|_| PlaybackError::WorkerUnavailable)?;
        Ok(Self {
            handle: Arc::new(Handle { sender, shared }),
        })
    }

    pub fn snapshot(&self) -> PlaybackSnapshot {
        self.handle.shared.snapshot().clone()
    }

    fn send(&self, command: Command) -> Result<(), PlaybackError> {
        if self.handle.shared.shutdown.load(Ordering::Acquire) {
            return Err(PlaybackError::ShuttingDown);
        }
        self.handle
            .sender
            .try_send(command)
            .map_err(|error| match error {
                TrySendError::Full(_) => PlaybackError::QueueFull,
                TrySendError::Disconnected(_) => PlaybackError::WorkerUnavailable,
            })
    }

    pub fn prepare(&self, source: PlaybackSource) -> Result<(), PlaybackError> {
        self.send(Command::Prepare(source))
    }

    pub fn play(&self) -> Result<(), PlaybackError> {
        self.send(Command::Play)
    }

    pub fn pause(&self) -> Result<(), PlaybackError> {
        self.send(Command::Pause)
    }

    pub fn resume(&self) -> Result<(), PlaybackError> {
        self.send(Command::Resume)
    }

    pub fn stop(&self) -> Result<(), PlaybackError> {
        self.send(Command::Stop)
    }

    pub fn set_volume(&self, volume: f64) -> Result<(), PlaybackError> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err(PlaybackError::InvalidVolume);
        }
        self.send(Command::Volume(volume))
    }

    pub fn seek(&self, position: Duration) -> Result<(), PlaybackError> {
        // GStreamer ClockTime reserves u64::MAX and seeks use signed offsets.
        if position.as_nanos() > i64::MAX as u128 {
            return Err(PlaybackError::InvalidSeek);
        }
        self.send(Command::Seek(position))
    }

    /// Cancels queued work immediately; poll `worker_running` for cleanup.
    pub fn shutdown(&self) {
        self.handle.shared.shutdown.store(true, Ordering::Release);
    }
}

struct ActivePipeline {
    element: gst::Element,
    bus: gst::Bus,
    transport_refused: Option<Arc<AtomicBool>>,
}

impl Drop for ActivePipeline {
    fn drop(&mut self) {
        // Native calls stay on the playback worker, including stop/replacement,
        // decoder error, EOS and application shutdown.
        let _ = self.element.set_state(gst::State::Null);
    }
}

struct Pending {
    target: gst::State,
    result: PlaybackState,
    started: Instant,
}

struct Worker {
    output: PlaybackOutput,
    shared: Arc<Shared>,
    source: Option<PlaybackSource>,
    pipeline: Option<ActivePipeline>,
    pending: Option<Pending>,
}

struct WorkerLifecycle(Arc<Shared>);

impl Drop for WorkerLifecycle {
    fn drop(&mut self) {
        let mut snapshot = self.0.snapshot();
        if snapshot.worker_running {
            // This also covers an unexpected worker unwind. Declared before
            // Worker, this guard runs after its pipeline has been dropped.
            snapshot.state = PlaybackState::Error;
            snapshot.last_error = Some(PlaybackError::WorkerUnavailable);
            snapshot.seekable = false;
            snapshot.worker_running = false;
        }
    }
}

impl Worker {
    fn run(output: PlaybackOutput, receiver: Receiver<Command>, shared: Arc<Shared>) {
        let _lifecycle = WorkerLifecycle(shared.clone());
        if gst::init().is_err() {
            let mut snapshot = shared.snapshot();
            snapshot.state = PlaybackState::Error;
            snapshot.last_error = Some(PlaybackError::Initialization);
            snapshot.worker_running = false;
            return;
        }
        let mut worker = Self {
            output,
            shared,
            source: None,
            pipeline: None,
            pending: None,
        };
        while !worker.shared.shutdown.load(Ordering::Acquire) {
            // Bound each batch so rapid actions cannot starve bus/timeout work.
            for _ in 0..8 {
                if worker.shared.shutdown.load(Ordering::Acquire) {
                    break;
                }
                match receiver.try_recv() {
                    Ok(command) => worker.command(command),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        worker.shared.shutdown.store(true, Ordering::Release);
                        break;
                    }
                }
            }
            worker.poll();
            thread::sleep(POLL_INTERVAL);
        }
        worker.pending = None;
        worker.pipeline = None;
        worker.source = None;
        let mut snapshot = worker.shared.snapshot();
        snapshot.state = PlaybackState::Stopped;
        snapshot.seekable = false;
        snapshot.worker_running = false;
    }

    fn command(&mut self, command: Command) {
        let result = match command {
            Command::Prepare(source) => {
                self.pipeline = None;
                self.pending = None;
                self.source = Some(source);
                {
                    let mut snapshot = self.shared.snapshot();
                    snapshot.generation += 1;
                    snapshot.state = PlaybackState::Loading;
                    snapshot.position = None;
                    snapshot.duration = None;
                    snapshot.seekable = false;
                    snapshot.last_error = None;
                }
                self.start_pipeline(PlaybackState::Ready)
            }
            Command::Play | Command::Resume => {
                if self
                    .source
                    .as_ref()
                    .and_then(|source| source.remote.as_ref())
                    .is_some_and(RemotePlaybackSource::is_expired)
                {
                    self.fail(PlaybackError::SourceExpired);
                    return;
                }
                let state = self.shared.snapshot().state;
                if matches!(state, PlaybackState::Stopped | PlaybackState::Ended) {
                    self.start_pipeline(PlaybackState::Playing)
                } else if matches!(
                    state,
                    PlaybackState::Ready
                        | PlaybackState::Paused
                        | PlaybackState::Loading
                        | PlaybackState::Playing
                ) {
                    self.transition(gst::State::Playing, PlaybackState::Playing)
                } else {
                    Err(PlaybackError::InvalidState)
                }
            }
            Command::Pause => {
                let state = self.shared.snapshot().state;
                if matches!(
                    state,
                    PlaybackState::Playing | PlaybackState::Loading | PlaybackState::Paused
                ) {
                    self.transition(gst::State::Paused, PlaybackState::Paused)
                } else {
                    Err(PlaybackError::InvalidState)
                }
            }
            Command::Stop => {
                self.pending = None;
                self.pipeline = None;
                if self
                    .source
                    .as_ref()
                    .is_some_and(|source| source.remote.is_some())
                {
                    self.source = None;
                }
                let mut snapshot = self.shared.snapshot();
                snapshot.state = PlaybackState::Stopped;
                snapshot.position = Some(Duration::ZERO);
                snapshot.seekable = false;
                snapshot.last_error = None;
                Ok(())
            }
            Command::Volume(volume) => {
                if let Some(pipeline) = &self.pipeline {
                    pipeline.element.set_property("volume", volume);
                }
                let mut snapshot = self.shared.snapshot();
                snapshot.volume = volume;
                if snapshot.state != PlaybackState::Error {
                    snapshot.last_error = None;
                }
                Ok(())
            }
            Command::Seek(position) => self.seek(position),
        };
        if let Err(error) = result {
            if matches!(
                error,
                PlaybackError::InvalidState
                    | PlaybackError::InvalidSeek
                    | PlaybackError::NotSeekable
                    | PlaybackError::SeekFailed
            ) {
                let mut snapshot = self.shared.snapshot();
                if snapshot.state != PlaybackState::Error {
                    snapshot.last_error = Some(error);
                }
            } else {
                self.fail(error);
            }
        }
    }

    fn start_pipeline(&mut self, result: PlaybackState) -> Result<(), PlaybackError> {
        self.pipeline = None;
        self.pending = None;
        let source = self.source.as_ref().ok_or(PlaybackError::InvalidState)?;
        let uri = if let Some(remote) = &source.remote {
            if remote.is_expired() {
                return Err(PlaybackError::SourceExpired);
            }
            // Native GStreamer debug messages can contain complete resource
            // properties and response locations. Disable them before the URI
            // enters GStreamer, including when GST_DEBUG was set by a caller.
            // Public snapshots/bus handling below retain fixed diagnostics.
            // Merely toggling debug active is insufficient: a later-loaded
            // plugin category can restore its GST_DEBUG threshold. Reset the
            // category rules and remove native output before source creation.
            // The category/log-handler APIs serialize their own updates.
            gst::log::set_threshold_from_string("0", true);
            gst::log::remove_default_log_function();
            remote.uri.clone()
        } else {
            Zeroizing::new(
                gst::glib::filename_to_uri(&source.path, None)
                    .map_err(|_| PlaybackError::SourceUnavailable)?
                    .to_string(),
            )
        };
        let element = gst::ElementFactory::make("playbin")
            .build()
            .map_err(|_| PlaybackError::Pipeline)?;
        let bus = element.bus().ok_or(PlaybackError::Pipeline)?;
        // Audio-only: local media must never create an unrequested video window.
        element.set_property_from_str("flags", "audio+soft-volume");
        let transport_refused = source
            .remote
            .as_ref()
            .map(|source| remote::guard_pipeline(&element, source.clone()))
            .transpose()?;
        element.set_property("uri", uri.as_str());
        let volume = self.shared.snapshot().volume;
        element.set_property("volume", volume);
        if self.output == PlaybackOutput::Fakesink {
            let sink = gst::ElementFactory::make("fakesink")
                .property("sync", true)
                .build()
                .map_err(|_| PlaybackError::Pipeline)?;
            element.set_property("audio-sink", &sink);
        }
        #[cfg(test)]
        self.shared
            .pipelines
            .lock()
            .unwrap()
            .push(element.downgrade());
        self.pipeline = Some(ActivePipeline {
            element,
            bus,
            transport_refused,
        });
        let target = if result == PlaybackState::Playing {
            gst::State::Playing
        } else {
            gst::State::Paused
        };
        self.transition(target, result)
    }

    fn transition(
        &mut self,
        target: gst::State,
        result: PlaybackState,
    ) -> Result<(), PlaybackError> {
        let pipeline = self.pipeline.as_ref().ok_or(PlaybackError::InvalidState)?;
        if pipeline.element.set_state(target).is_err() {
            if let Some(refused) = &pipeline.transport_refused {
                if refused.load(Ordering::Acquire) {
                    return Err(PlaybackError::SourcePolicy);
                }
                // A synchronous native refusal can post its structured error
                // before set_state returns. Retain only a documented numeric
                // HTTP status before dropping that failed pipeline.
                for _ in 0..64 {
                    let Some(message) = pipeline.bus.pop() else {
                        break;
                    };
                    if let gst::MessageView::Error(error) = message.view()
                        && let Some(status) = remote::error_http_status(error)
                    {
                        return Err(PlaybackError::RemoteHttp(status));
                    }
                }
            }
            return Err(PlaybackError::StateChange);
        }
        self.pending = Some(Pending {
            target,
            result,
            started: Instant::now(),
        });
        let mut snapshot = self.shared.snapshot();
        snapshot.state = PlaybackState::Loading;
        snapshot.last_error = None;
        Ok(())
    }

    fn seek(&mut self, position: Duration) -> Result<(), PlaybackError> {
        if self
            .source
            .as_ref()
            .and_then(|source| source.remote.as_ref())
            .is_some_and(RemotePlaybackSource::is_expired)
        {
            return Err(PlaybackError::SourceExpired);
        }
        let snapshot = self.shared.snapshot().clone();
        if !matches!(
            snapshot.state,
            PlaybackState::Ready | PlaybackState::Paused | PlaybackState::Playing
        ) {
            return Err(PlaybackError::InvalidState);
        }
        if !snapshot.seekable {
            return Err(PlaybackError::NotSeekable);
        }
        if snapshot
            .duration
            .is_some_and(|duration| position >= duration)
        {
            return Err(PlaybackError::InvalidSeek);
        }
        let pipeline = self.pipeline.as_ref().ok_or(PlaybackError::InvalidState)?;
        pipeline
            .element
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::ClockTime::from_nseconds(position.as_nanos() as u64),
            )
            .map_err(|_| PlaybackError::SeekFailed)?;
        self.shared.snapshot().last_error = None;
        Ok(())
    }

    fn poll(&mut self) {
        let mut eos = false;
        let mut failure = None;
        if let Some(pipeline) = &self.pipeline {
            if pipeline
                .transport_refused
                .as_ref()
                .is_some_and(|refused| refused.load(Ordering::Acquire))
            {
                self.fail(PlaybackError::SourcePolicy);
                return;
            }
            for _ in 0..64 {
                let Some(message) = pipeline.bus.pop() else {
                    break;
                };
                match message.view() {
                    gst::MessageView::Error(error) => {
                        failure = Some(
                            if pipeline
                                .transport_refused
                                .as_ref()
                                .is_some_and(|refused| refused.load(Ordering::Acquire))
                            {
                                PlaybackError::SourcePolicy
                            } else if pipeline.transport_refused.is_some() {
                                remote::error_http_status(error)
                                    .map(PlaybackError::RemoteHttp)
                                    .unwrap_or(PlaybackError::Playback)
                            } else {
                                PlaybackError::Playback
                            },
                        );
                        break;
                    }
                    gst::MessageView::Eos(_) => {
                        eos = true;
                        break;
                    }
                    gst::MessageView::Redirect(_) if pipeline.transport_refused.is_some() => {
                        failure = Some(PlaybackError::SourcePolicy);
                        break;
                    }
                    gst::MessageView::Element(message)
                        if pipeline.transport_refused.is_some()
                            && message.structure().is_some_and(|structure| {
                                structure.name() == "redirect"
                                    || structure.has_field("new-location")
                            }) =>
                    {
                        failure = Some(PlaybackError::SourcePolicy);
                        break;
                    }
                    _ => {}
                }
            }
        }
        if let Some(error) = failure {
            self.fail(error);
            return;
        }
        if eos {
            self.pending = None;
            self.pipeline = None;
            if self
                .source
                .as_ref()
                .is_some_and(|source| source.remote.is_some())
            {
                self.source = None;
            }
            let mut snapshot = self.shared.snapshot();
            snapshot.state = PlaybackState::Ended;
            snapshot.position = snapshot.duration;
            snapshot.seekable = false;
            return;
        }
        let Some(pipeline) = &self.pipeline else {
            return;
        };
        let mut completed_state = None;
        if let Some(pending) = &self.pending {
            let (result, current, _) = pipeline.element.state(gst::ClockTime::ZERO);
            if result.is_err() {
                self.fail(PlaybackError::StateChange);
                return;
            }
            if current == pending.target {
                completed_state = Some(pending.result);
                self.pending = None;
            } else if pending.started.elapsed() > TRANSITION_TIMEOUT {
                self.fail(PlaybackError::Timeout);
                return;
            }
        }
        let mut seeking = gst::query::Seeking::new(gst::Format::Time);
        let seekable = pipeline.element.query(&mut seeking) && seeking.result().0;
        let position = pipeline
            .element
            .query_position::<gst::ClockTime>()
            .map(|value| Duration::from_nanos(value.nseconds()));
        let duration = pipeline
            .element
            .query_duration::<gst::ClockTime>()
            .map(|value| Duration::from_nanos(value.nseconds()));
        let mut snapshot = self.shared.snapshot();
        if let Some(state) = completed_state {
            snapshot.state = state;
        }
        snapshot.seekable = seekable;
        snapshot.position = position;
        snapshot.duration = duration;
    }

    fn fail(&mut self, error: PlaybackError) {
        self.pending = None;
        self.pipeline = None;
        if self
            .source
            .as_ref()
            .is_some_and(|source| source.remote.is_some())
        {
            self.source = None;
        }
        let mut snapshot = self.shared.snapshot();
        snapshot.state = PlaybackState::Error;
        snapshot.last_error = Some(error);
        snapshot.seekable = false;
    }
}

/// Original synthetic WAV media, generated only for explicit developer checks.
/// The owned temporary file is private and is removed when this guard is dropped.
pub struct DiagnosticMedia {
    source: PlaybackSource,
}

impl fmt::Debug for DiagnosticMedia {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DiagnosticMedia([generated local WAV])")
    }
}

impl DiagnosticMedia {
    /// Generate a quiet 440 Hz mono PCM WAV; no service content is involved.
    pub fn create(duration: Duration) -> Result<Self, PlaybackError> {
        if duration < Duration::from_millis(50) || duration > Duration::from_secs(30) {
            return Err(PlaybackError::DiagnosticMedia);
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| PlaybackError::DiagnosticMedia)?
            .as_nanos();
        let mut opened = None;
        for _ in 0..8 {
            let path = std::env::temp_dir().join(format!(
                "karin-generated-{}-{nonce}-{}.wav",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(file) => {
                    opened = Some((path, file));
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(PlaybackError::DiagnosticMedia),
            }
        }
        let (path, file) = opened.ok_or(PlaybackError::DiagnosticMedia)?;
        let mut file = std::io::BufWriter::new(file);
        let source = PlaybackSource { path, remote: None };
        let guard = Self { source };
        let sample_rate = 22_050_u32;
        let samples = (duration.as_nanos() * sample_rate as u128 / 1_000_000_000) as u32;
        let data_len = samples * 2;
        let write = (|| -> std::io::Result<()> {
            file.write_all(b"RIFF")?;
            file.write_all(&(36 + data_len).to_le_bytes())?;
            file.write_all(b"WAVEfmt ")?;
            file.write_all(&16_u32.to_le_bytes())?;
            file.write_all(&1_u16.to_le_bytes())?;
            file.write_all(&1_u16.to_le_bytes())?;
            file.write_all(&sample_rate.to_le_bytes())?;
            file.write_all(&(sample_rate * 2).to_le_bytes())?;
            file.write_all(&2_u16.to_le_bytes())?;
            file.write_all(&16_u16.to_le_bytes())?;
            file.write_all(b"data")?;
            file.write_all(&data_len.to_le_bytes())?;
            for sample in 0..samples {
                let wave =
                    (sample as f64 * 440.0 * std::f64::consts::TAU / sample_rate as f64).sin();
                let pcm = (wave * 0.03 * i16::MAX as f64) as i16;
                file.write_all(&pcm.to_le_bytes())?;
            }
            file.flush()
        })();
        write.map_err(|_| PlaybackError::DiagnosticMedia)?;
        Ok(guard)
    }

    pub fn source(&self) -> PlaybackSource {
        self.source.clone()
    }
}

impl Drop for DiagnosticMedia {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.source.path);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaybackReport;

impl fmt::Display for PlaybackReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Generated local WAV playback passed through playbin/fakesink: prepare, play, pause, resume, volume, seek, stop, EOS and worker cleanup. This is not Yandex Music playback.")
    }
}

fn wait_for(
    controller: &PlaybackController,
    predicate: impl Fn(&PlaybackSnapshot) -> bool,
) -> Result<PlaybackSnapshot, PlaybackError> {
    let deadline = Instant::now() + TRANSITION_TIMEOUT + Duration::from_secs(1);
    loop {
        let snapshot = controller.snapshot();
        if predicate(&snapshot) {
            return Ok(snapshot);
        }
        if snapshot.state == PlaybackState::Error {
            return Err(snapshot.last_error.unwrap_or(PlaybackError::Playback));
        }
        if Instant::now() >= deadline {
            return Err(PlaybackError::Timeout);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn wait_for_shutdown(controller: &PlaybackController) -> Result<(), PlaybackError> {
    let deadline = Instant::now() + TRANSITION_TIMEOUT + Duration::from_secs(1);
    while controller.snapshot().worker_running {
        if Instant::now() >= deadline {
            return Err(PlaybackError::Timeout);
        }
        thread::sleep(POLL_INTERVAL);
    }
    Ok(())
}

/// Exercise the same real controller used by developer GTK controls, silently.
/// This bounded diagnostic blocks its caller; GTK must run it on a worker.
pub fn run_self_test() -> Result<PlaybackReport, PlaybackError> {
    let media = DiagnosticMedia::create(Duration::from_secs(2))?;
    let controller = PlaybackController::new(PlaybackOutput::Fakesink)?;
    let result = (|| {
        controller.prepare(media.source())?;
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Ready && snapshot.seekable
        })?;
        controller.set_volume(0.25)?;
        controller.play()?;
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Playing
                && snapshot
                    .position
                    .is_some_and(|position| position >= Duration::from_millis(80))
        })?;
        controller.pause()?;
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Paused
        })?;
        controller.seek(Duration::from_millis(500))?;
        wait_for(&controller, |snapshot| {
            snapshot
                .position
                .is_some_and(|position| position >= Duration::from_millis(450))
        })?;
        controller.resume()?;
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Playing
        })?;
        controller.stop()?;
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Stopped
        })?;
        controller.play()?;
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Playing && snapshot.seekable
        })?;
        controller.seek(Duration::from_millis(1800))?;
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Ended
        })?;
        Ok(PlaybackReport)
    })();
    controller.shutdown();
    let cleanup = wait_for_shutdown(&controller);
    result.and(cleanup.map(|()| PlaybackReport))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controller() -> PlaybackController {
        PlaybackController::new(PlaybackOutput::Fakesink).expect("worker starts")
    }

    fn shutdown(controller: &PlaybackController) {
        controller.shutdown();
        wait_for_shutdown(controller).expect("worker cleaned up in time");
        assert!(
            controller
                .handle
                .shared
                .pipelines
                .lock()
                .unwrap()
                .iter()
                .all(|pipeline| pipeline.upgrade().is_none()),
            "all playback objects released"
        );
    }

    fn prepare(controller: &PlaybackController, media: &DiagnosticMedia) -> PlaybackSnapshot {
        let generation = controller.snapshot().generation;
        controller.prepare(media.source()).expect("prepare queued");
        // An enqueue receipt is not a completion: an earlier Error snapshot may
        // remain visible until the worker accepts this replacement source.
        let deadline = Instant::now() + Duration::from_secs(6);
        while controller.snapshot().generation == generation {
            assert!(Instant::now() < deadline, "new preparation accepted");
            thread::sleep(POLL_INTERVAL);
        }
        wait_for(controller, |snapshot| {
            snapshot.state == PlaybackState::Ready && snapshot.seekable
        })
        .expect("real WAV prerolls and supports seek")
    }

    #[test]
    fn real_playback_path_completes_the_developer_self_test() {
        let report = run_self_test().expect("real player self-test passes");
        assert!(report.to_string().contains("playbin/fakesink"));
        assert!(report.to_string().contains("not Yandex Music playback"));
    }

    #[test]
    fn clocked_media_progresses_pauses_seeks_and_reaches_eos() {
        let media = DiagnosticMedia::create(Duration::from_millis(1100)).unwrap();
        let controller = controller();
        let ready = prepare(&controller, &media);
        assert!(ready.duration.unwrap() >= Duration::from_millis(1090));
        assert!(ready.duration.unwrap() <= Duration::from_millis(1110));
        controller.play().unwrap();
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Playing
                && snapshot
                    .position
                    .is_some_and(|position| position >= Duration::from_millis(100))
        })
        .unwrap();
        controller.pause().unwrap();
        let paused = wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Paused
        })
        .unwrap();
        thread::sleep(Duration::from_millis(130));
        let still_paused = controller.snapshot();
        assert_eq!(still_paused.state, PlaybackState::Paused);
        assert!(
            still_paused
                .position
                .unwrap()
                .abs_diff(paused.position.unwrap())
                < Duration::from_millis(25)
        );
        controller.seek(Duration::from_millis(750)).unwrap();
        wait_for(&controller, |snapshot| {
            snapshot
                .position
                .is_some_and(|position| position >= Duration::from_millis(700))
        })
        .unwrap();
        controller.resume().unwrap();
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Ended
        })
        .unwrap();
        assert_eq!(
            controller.snapshot().position,
            controller.snapshot().duration
        );
        assert!(
            controller
                .handle
                .shared
                .pipelines
                .lock()
                .unwrap()
                .iter()
                .all(|pipeline| pipeline.upgrade().is_none())
        );
        shutdown(&controller);
    }

    #[test]
    fn volume_and_invalid_commands_are_bounded_without_poisoning_playback() {
        let controller = controller();
        for volume in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
            assert_eq!(
                controller.set_volume(volume),
                Err(PlaybackError::InvalidVolume)
            );
        }
        assert_eq!(
            controller.seek(Duration::MAX),
            Err(PlaybackError::InvalidSeek)
        );
        controller.pause().unwrap();
        wait_for(&controller, |snapshot| {
            snapshot.last_error == Some(PlaybackError::InvalidState)
        })
        .unwrap();
        assert_eq!(controller.snapshot().state, PlaybackState::Idle);
        let media = DiagnosticMedia::create(Duration::from_secs(1)).unwrap();
        prepare(&controller, &media);
        controller.seek(Duration::from_secs(3)).unwrap();
        wait_for(&controller, |snapshot| {
            snapshot.last_error == Some(PlaybackError::InvalidSeek)
        })
        .unwrap();
        assert_eq!(controller.snapshot().state, PlaybackState::Ready);
        controller.set_volume(0.0).unwrap();
        wait_for(&controller, |snapshot| {
            snapshot.volume == 0.0 && snapshot.last_error.is_none()
        })
        .unwrap();
        controller.set_volume(1.0).unwrap();
        wait_for(&controller, |snapshot| snapshot.volume == 1.0).unwrap();
        let pipeline = controller.handle.shared.pipelines.lock().unwrap()[0]
            .upgrade()
            .unwrap();
        assert_eq!(pipeline.property::<f64>("volume"), 1.0);
        drop(pipeline);
        shutdown(&controller);
    }

    #[test]
    fn decode_failure_is_sanitized_releases_pipeline_and_allows_retry() {
        let media = DiagnosticMedia::create(Duration::from_millis(800)).unwrap();
        fs::write(&media.source.path, b"not a supported media format").unwrap();
        let controller = controller();
        controller.prepare(media.source()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(6);
        while controller.snapshot().state != PlaybackState::Error {
            assert!(Instant::now() < deadline, "decode error reported");
            thread::sleep(POLL_INTERVAL);
        }
        let snapshot = controller.snapshot();
        assert!(matches!(
            snapshot.last_error,
            Some(PlaybackError::Playback | PlaybackError::StateChange)
        ));
        let fatal_error = snapshot.last_error;
        // Adjusting volume or rejecting an inapplicable command must not hide
        // the decoder failure while the controller remains in Error.
        controller.pause().unwrap();
        controller.set_volume(0.3).unwrap();
        let volume_deadline = Instant::now() + Duration::from_secs(6);
        while controller.snapshot().volume != 0.3 {
            assert!(Instant::now() < volume_deadline, "volume command applied");
            thread::sleep(POLL_INTERVAL);
        }
        let after_volume = controller.snapshot();
        assert_eq!(after_volume.state, PlaybackState::Error);
        assert_eq!(after_volume.last_error, fatal_error);
        assert!(
            !format!("{snapshot:?}").contains(&media.source.path.to_string_lossy().to_string())
        );
        assert!(
            controller
                .handle
                .shared
                .pipelines
                .lock()
                .unwrap()
                .iter()
                .all(|pipeline| pipeline.upgrade().is_none())
        );
        let good_media = DiagnosticMedia::create(Duration::from_millis(700)).unwrap();
        prepare(&controller, &good_media);
        assert!(controller.snapshot().last_error.is_none());
        shutdown(&controller);
    }

    #[test]
    fn stop_restart_and_source_replacement_release_old_pipelines() {
        let first = DiagnosticMedia::create(Duration::from_secs(2)).unwrap();
        let second = DiagnosticMedia::create(Duration::from_secs(1)).unwrap();
        let controller = controller();
        prepare(&controller, &first);
        controller.play().unwrap();
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Playing
        })
        .unwrap();
        controller.stop().unwrap();
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Stopped
        })
        .unwrap();
        assert!(
            controller.handle.shared.pipelines.lock().unwrap()[0]
                .upgrade()
                .is_none()
        );
        controller.play().unwrap();
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Playing
        })
        .unwrap();
        controller.prepare(second.source()).unwrap();
        let replaced = wait_for(&controller, |snapshot| {
            snapshot.generation == 2 && snapshot.state == PlaybackState::Ready
        })
        .unwrap();
        assert!(replaced.duration.unwrap() < Duration::from_millis(1010));
        assert!(
            controller.handle.shared.pipelines.lock().unwrap()[1]
                .upgrade()
                .is_none()
        );
        shutdown(&controller);
    }

    #[test]
    fn expired_player_transition_reports_timeout_and_releases_playbin() {
        gst::init().expect("GStreamer initializes");
        let media = DiagnosticMedia::create(Duration::from_secs(1)).unwrap();
        let element = gst::ElementFactory::make("playbin")
            .build()
            .expect("real playbin exists");
        let uri = gst::glib::filename_to_uri(&media.source.path, None).unwrap();
        element.set_property("uri", uri.as_str());
        element.set_state(gst::State::Ready).unwrap();
        assert_eq!(element.state(gst::ClockTime::ZERO).1, gst::State::Ready);
        let weak = element.downgrade();
        let bus = element.bus().expect("playbin has a bus");
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(PlaybackSnapshot {
                state: PlaybackState::Loading,
                ..PlaybackSnapshot::default()
            }),
            shutdown: AtomicBool::new(false),
            pipelines: Mutex::new(vec![weak.clone()]),
        });
        let mut worker = Worker {
            output: PlaybackOutput::Fakesink,
            shared: shared.clone(),
            source: Some(media.source()),
            pipeline: Some(ActivePipeline {
                element,
                bus,
                transport_refused: None,
            }),
            // Paused was deliberately never requested: this real playbin stays
            // Ready, allowing the worker's own deadline path to be tested
            // without sleeping five seconds or depending on a stalled plugin.
            pending: Some(Pending {
                target: gst::State::Paused,
                result: PlaybackState::Ready,
                started: Instant::now() - TRANSITION_TIMEOUT - Duration::from_secs(1),
            }),
        };
        worker.poll();
        assert_eq!(shared.snapshot().state, PlaybackState::Error);
        assert_eq!(shared.snapshot().last_error, Some(PlaybackError::Timeout));
        assert!(worker.pending.is_none());
        assert!(worker.pipeline.is_none());
        assert!(weak.upgrade().is_none(), "timed-out playbin was released");
    }

    #[test]
    fn rapid_actions_apply_backpressure_and_shutdown_discards_pending_work() {
        let media = DiagnosticMedia::create(Duration::from_secs(2)).unwrap();
        let controller = controller();
        prepare(&controller, &media);
        let mut rejected = 0;
        for _ in 0..100 {
            for result in [
                controller.play(),
                controller.pause(),
                controller.stop(),
                controller.prepare(media.source()),
            ] {
                if result == Err(PlaybackError::QueueFull) {
                    rejected += 1;
                } else {
                    result.unwrap();
                }
            }
        }
        assert!(rejected > 0, "bounded queue applies actual backpressure");
        let started = Instant::now();
        controller.shutdown();
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "shutdown is a signal, never a join"
        );
        assert_eq!(controller.play(), Err(PlaybackError::ShuttingDown));
        shutdown(&controller);
    }

    #[test]
    fn dropping_the_final_handle_stops_an_active_pipeline() {
        let media = DiagnosticMedia::create(Duration::from_secs(3)).unwrap();
        let controller = controller();
        prepare(&controller, &media);
        controller.play().unwrap();
        wait_for(&controller, |snapshot| {
            snapshot.state == PlaybackState::Playing
        })
        .unwrap();
        let shared = controller.handle.shared.clone();
        let clone = controller.clone();
        drop(controller);
        assert!(!shared.shutdown.load(Ordering::Acquire));
        drop(clone);
        let deadline = Instant::now() + Duration::from_secs(6);
        while shared.snapshot().worker_running {
            assert!(Instant::now() < deadline, "last drop triggers cleanup");
            thread::sleep(POLL_INTERVAL);
        }
        assert!(
            shared
                .pipelines
                .lock()
                .unwrap()
                .iter()
                .all(|pipeline| pipeline.upgrade().is_none())
        );
    }

    #[test]
    fn diagnostic_files_are_private_owned_and_source_names_are_redacted() {
        use std::os::unix::fs::PermissionsExt;
        let media = DiagnosticMedia::create(Duration::from_millis(100)).unwrap();
        let path = media.source.path.clone();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!format!("{:?}", media.source()).contains(&path.to_string_lossy().to_string()));
        assert!(PlaybackSource::local_file(std::env::temp_dir()).is_err());
        assert!(PlaybackSource::local_file(path.with_extension("missing")).is_err());
        drop(media);
        assert!(!path.exists());
        assert!(DiagnosticMedia::create(Duration::MAX).is_err());
    }
}
