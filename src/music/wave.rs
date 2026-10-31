// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in modern radio-session protocol, not a visible or accepted My Wave UI.
//!
//! Endpoint/JSON/event knowledge was compared with Cassette v0.2.4 at
//! 9d34251d01bcd414ffbec2bef7931090d33c9124 (api/yam-client.vala,
//! api/objects/rotor, player/mods/flow.vala and client/utils.vala). This original
//! adapter imports no upstream implementation, OAuth identity or signing code.
//! Published protocol, successful HTTP acknowledgement and real service/playing
//! acceptance are separate facts. Normal UI stays hidden until end-to-end
//! session/tracks/feedback acceptance is established with Karin on Fedora.

use super::{
    Id, MusicError, MusicService, PAGE_SIZE, SearchBinding, Track, decode_track_batch,
    forbidden_error, network_error, read_body, validate_id,
};
use crate::{playback::PlaybackState, session::AccessToken};
use reqwest::{
    blocking::Body,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::{
    fmt,
    io::Cursor,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const MAX_QUEUE: usize = 500;
const MAX_HEARD: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const FROM: &str = "unofficial-client-own";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaveStage {
    Session,
    Tracks,
    Normalization,
    Feedback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WaveError {
    #[error(transparent)]
    Music(#[from] MusicError),
    #[error("Wave {stage:?} request failed: {error}")]
    Request { stage: WaveStage, error: MusicError },
    #[error("The Wave operation was cancelled locally")]
    Cancelled,
    #[error("Another Wave operation is still running")]
    Busy,
    #[error("The Wave response does not establish the supported session contract")]
    UnsupportedResponse,
    #[error("The Wave session is no longer recognized")]
    UnknownSession,
    #[error("The Wave response has no normalized tracks")]
    NoTracks,
    #[error("Invalid Wave lifecycle observation")]
    InvalidObservation,
    #[error("The Wave write outcome is unknown; do not replay it automatically")]
    WriteOutcomeUnknown,
}
impl WaveError {
    pub fn stage(self) -> Option<WaveStage> {
        match self {
            Self::Request { stage, .. } => Some(stage),
            _ => None,
        }
    }
    pub fn music_error(self) -> Option<MusicError> {
        match self {
            Self::Music(error) | Self::Request { error, .. } => Some(error),
            _ => None,
        }
    }
    fn at(self, stage: WaveStage) -> Self {
        match self {
            Self::Music(error) => Self::Request { stage, error },
            error => error,
        }
    }
}

/// One-shot local cancellation; it is not remote session deletion/revocation.
#[derive(Clone, Default)]
pub struct WaveCancellation(Arc<AtomicBool>);
impl WaveCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    fn check(&self) -> Result<(), WaveError> {
        if self.is_cancelled() {
            Err(WaveError::Cancelled)
        } else {
            Ok(())
        }
    }
}
impl fmt::Debug for WaveCancellation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WaveCancellation([redacted])")
    }
}

struct Identity {
    binding: SearchBinding,
    radio_id: Zeroizing<String>,
    cancellation: WaveCancellation,
    operation: Mutex<()>,
}

/// Session identity is opaque, memory-only and account/epoch bound.
#[derive(Clone)]
pub struct WaveSession(Arc<Identity>);
impl WaveSession {
    pub fn cancel(&self) {
        self.0.cancellation.cancel();
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.cancellation.is_cancelled()
    }
}
impl fmt::Debug for WaveSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WaveSession([redacted])")
    }
}

/// Original-batch provenance, separate from track metadata and never cached.
#[derive(Clone)]
pub struct WaveBatchMarker {
    identity: Arc<Identity>,
    batch_id: Zeroizing<String>,
    tracks: Arc<Vec<String>>,
}
impl fmt::Debug for WaveBatchMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WaveBatchMarker([redacted])")
    }
}

pub struct WaveBatch {
    pub tracks: Vec<Track>,
    pub omitted: usize,
    pub http_status: u16,
    pub lookup_http_status: u16,
    marker: WaveBatchMarker,
}
impl WaveBatch {
    pub fn marker(&self) -> WaveBatchMarker {
        self.marker.clone()
    }
}
impl fmt::Debug for WaveBatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaveBatch")
            .field("count", &self.tracks.len())
            .field("omitted", &self.omitted)
            .field("http_status", &self.http_status)
            .field("lookup_http_status", &self.lookup_http_status)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaveEventKind {
    RadioStarted,
    TrackStarted,
    TrackFinished,
    Skip,
    RadioFinished,
}
impl WaveEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RadioStarted => "radioStarted",
            Self::TrackStarted => "trackStarted",
            Self::TrackFinished => "trackFinished",
            Self::Skip => "skip",
            Self::RadioFinished => "radioFinished",
        }
    }
}

/// Reports are emitted by observations of the existing worker, not selection or
/// preloading. No serializable/public session ID or token field is provided.
#[derive(Clone)]
pub struct WaveEvent {
    marker: WaveBatchMarker,
    kind: WaveEventKind,
    track_id: Option<String>,
    heard: Duration,
}
impl WaveEvent {
    pub fn kind(&self) -> WaveEventKind {
        self.kind
    }
    pub fn heard(&self) -> Duration {
        self.heard
    }
}
impl fmt::Debug for WaveEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaveEvent")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// HTTP acceptance only; not proof of personalized algorithm effect or sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveFeedbackAck {
    pub http_status: u16,
}

impl MusicService {
    /// Explicit local-owner experiment only. Does not start GStreamer or report
    /// invented listening; no normal UI or automatic account-start probe calls it.
    pub fn begin_wave(
        &self,
        token: &AccessToken,
        cancellation: &WaveCancellation,
    ) -> Result<(WaveSession, WaveBatch), WaveError> {
        cancellation.check()?;
        let binding = self.search_binding(token)?;
        let body = serde_json::json!({"seeds":["user:onyourwave"],"includeTracksInResponse":true,"includeWaveModel":true});
        let (bytes, http) = self
            .wave_post(token, &["rotor", "session", "new"], &body)
            .map_err(|error| error.at(WaveStage::Session))
            .inspect_err(|_| cancellation.cancel())?;
        cancellation.check()?;
        self.check_search_binding(token, &binding)?;
        let wire = decode_session(&bytes).inspect_err(|_| cancellation.cancel())?;
        let radio_id = wire
            .radio_session_id
            .clone()
            .ok_or(WaveError::UnsupportedResponse)
            .inspect_err(|_| cancellation.cancel())?;
        let session = WaveSession(Arc::new(Identity {
            binding,
            radio_id: Zeroizing::new(radio_id),
            cancellation: cancellation.clone(),
            operation: Mutex::new(()),
        }));
        let batch = self
            .wave_normalize(token, &session, wire, http)
            .map_err(|error| error.at(WaveStage::Normalization))
            .inspect_err(|_| session.cancel())?;
        Ok((session, batch))
    }

    /// Fetch one bounded next batch using the actual already-loaded queue. POST
    /// cursor operations are not automatically replayed after uncertain outcomes.
    pub fn wave_batch(
        &self,
        token: &AccessToken,
        session: &WaveSession,
        queue: &[String],
    ) -> Result<WaveBatch, WaveError> {
        self.wave_check(token, session)?;
        if queue.len() > MAX_QUEUE {
            return Err(MusicError::InvalidInput.into());
        }
        for id in queue {
            validate_id(id)?;
        }
        let _operation = session
            .0
            .operation
            .try_lock()
            .map_err(|_| WaveError::Busy)?;
        self.wave_check(token, session)?;
        let (bytes, http) = self
            .wave_post(
                token,
                &["rotor", "session", &session.0.radio_id, "tracks"],
                &serde_json::json!({"queue":queue}),
            )
            .map_err(|error| error.at(WaveStage::Tracks))
            .inspect_err(|_| session.cancel())?;
        self.wave_check(token, session)?;
        let wire = decode_session(&bytes).inspect_err(|_| session.cancel())?;
        if wire
            .radio_session_id
            .as_ref()
            .is_some_and(|id| id != session.0.radio_id.as_str())
        {
            session.cancel();
            return Err(WaveError::UnsupportedResponse);
        }
        self.wave_normalize(token, session, wire, http)
            .map_err(|error| error.at(WaveStage::Normalization))
            .inspect_err(|_| session.cancel())
    }

    /// Send one original-batch event from the pure coordinator. No retries,
    /// account switching, unbounded event queue or raw provider messages.
    pub fn wave_feedback(
        &self,
        token: &AccessToken,
        session: &WaveSession,
        event: &WaveEvent,
    ) -> Result<WaveFeedbackAck, WaveError> {
        self.wave_check(token, session)?;
        if !Arc::ptr_eq(&session.0, &event.marker.identity)
            || event.heard > MAX_HEARD
            || event
                .track_id
                .as_ref()
                .is_some_and(|id| !event.marker.tracks.contains(id))
        {
            return Err(WaveError::InvalidObservation);
        }
        let _operation = session
            .0
            .operation
            .try_lock()
            .map_err(|_| WaveError::Busy)?;
        self.wave_check(token, session)?;
        let timestamp = utc_timestamp(SystemTime::now())?;
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Event<'a> {
            timestamp: &'a str,
            #[serde(rename = "type")]
            kind: &'static str,
            from: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            track_id: Option<&'a str>,
            total_played_seconds: f64,
        }
        let body = serde_json::json!({"batchId":event.marker.batch_id.as_str(),"event":Event {timestamp:&timestamp,kind:event.kind.as_str(),from:FROM,track_id:event.track_id.as_deref(),total_played_seconds:event.heard.as_secs_f64()}});
        let (body, http) = self
            .wave_post(
                token,
                &["rotor", "session", &session.0.radio_id, "feedback"],
                &body,
            )
            .map_err(|error| error.at(WaveStage::Feedback))
            .inspect_err(|_| session.cancel())?;
        self.wave_check(token, session)?;
        // Cassette's feedback contract acknowledges HTTP200 without a typed
        // body. Still refuse an explicit error/malformed nonempty JSON instead
        // of calling it success. No inferred recommendation effect is reported.
        if !body.is_empty() {
            let value: serde_json::Value = serde_json::from_slice(&body)
                .map_err(|_| WaveError::WriteOutcomeUnknown)
                .inspect_err(|_| session.cancel())?;
            if !value.is_object() || value.get("error").is_some_and(|e| !e.is_null()) {
                session.cancel();
                return Err(WaveError::WriteOutcomeUnknown);
            }
        }
        Ok(WaveFeedbackAck { http_status: http })
    }

    fn wave_check(&self, token: &AccessToken, session: &WaveSession) -> Result<(), WaveError> {
        session.0.cancellation.check()?;
        self.check_search_binding(token, &session.0.binding)?;
        Ok(())
    }

    fn wave_normalize(
        &self,
        token: &AccessToken,
        session: &WaveSession,
        wire: SessionWire,
        http: u16,
    ) -> Result<WaveBatch, WaveError> {
        self.wave_check(token, session)?;
        let ids = wire
            .sequence
            .into_iter()
            .map(|e| e.track.id.0)
            .collect::<Vec<_>>();
        let response = self
            .track_batch_response(token, &ids)
            .map_err(|error| WaveError::Music(error).at(WaveStage::Normalization))?;
        let lookup_http_status = response.status().as_u16();
        let bytes = read_body(response)
            .map_err(|error| WaveError::Music(error).at(WaveStage::Normalization))?;
        let distinct = ids.iter().collect::<std::collections::BTreeSet<_>>().len();
        let normalized = decode_track_batch(&bytes, distinct)
            .map_err(|error| WaveError::Music(error).at(WaveStage::Normalization))?;
        self.wave_check(token, session)?;
        let mut tracks = Vec::with_capacity(ids.len());
        for id in &ids {
            let bare = id.split(':').next().ok_or(MusicError::InvalidInput)?;
            if let Some(track) = normalized.iter().find(|t| t.id == *id || t.id == bare) {
                tracks.push(track.clone());
            }
        }
        let omitted = ids.len() - tracks.len();
        if tracks.is_empty() {
            return Err(WaveError::NoTracks);
        }
        let marker = WaveBatchMarker {
            identity: session.0.clone(),
            batch_id: Zeroizing::new(wire.batch_id),
            tracks: Arc::new(tracks.iter().map(|t| t.id.clone()).collect()),
        };
        Ok(WaveBatch {
            tracks,
            omitted,
            http_status: http,
            lookup_http_status,
            marker,
        })
    }

    fn wave_post<T: Serialize>(
        &self,
        token: &AccessToken,
        path: &[&str],
        value: &T,
    ) -> Result<(Zeroizing<Vec<u8>>, u16), WaveError> {
        if token.is_expired() {
            return Err(MusicError::Unauthenticated.into());
        }
        let mut url = self.origin.clone();
        {
            let mut parts = url
                .path_segments_mut()
                .map_err(|_| MusicError::Configuration)?;
            parts.pop_if_empty();
            for part in path {
                if part.is_empty() || matches!(*part, "." | "..") {
                    return Err(MusicError::InvalidInput.into());
                }
                parts.push(part);
            }
        }
        let payload =
            Zeroizing::new(serde_json::to_vec(value).map_err(|_| MusicError::InvalidInput)?);
        if payload.len() > 128 * 1024 {
            return Err(MusicError::InvalidInput.into());
        }
        let length = payload.len() as u64;
        let authorization = Zeroizing::new(format!("OAuth {}", token.expose()));
        let mut header =
            HeaderValue::from_str(&authorization).map_err(|_| MusicError::Unauthenticated)?;
        header.set_sensitive(true);
        let response = self
            .client
            .post(url)
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, header)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::sized(Cursor::new(payload), length))
            .send()
            .map_err(network_error)
            .map_err(uncertain)?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(match status {
                401 => MusicError::AccessDenied.into(),
                429 => MusicError::RateLimited.into(),
                451 => MusicError::Restricted.into(),
                403 => read_body(response)
                    .map(|b| forbidden_error(&b))
                    .unwrap_or(MusicError::Forbidden)
                    .into(),
                200..=399 | 500..=599 => WaveError::WriteOutcomeUnknown,
                _ => MusicError::Http(status).into(),
            });
        }
        Ok((read_body(response).map_err(uncertain)?, status))
    }
}

fn uncertain(error: MusicError) -> WaveError {
    match error {
        MusicError::Network
        | MusicError::Timeout
        | MusicError::InvalidResponse
        | MusicError::ResponseTooLarge => WaveError::WriteOutcomeUnknown,
        error => error.into(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionWire {
    radio_session_id: Option<String>,
    batch_id: String,
    sequence: Vec<Sequence>,
    #[serde(default)]
    unknown_session: bool,
}
#[derive(Deserialize)]
struct Sequence {
    #[serde(rename = "type")]
    kind: String,
    track: TrackIdentity,
}
#[derive(Deserialize)]
struct TrackIdentity {
    id: Id,
}
#[derive(Deserialize)]
struct SessionEnvelope {
    result: Option<SessionWire>,
    error: Option<Box<RawValue>>,
}

fn opaque_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.'))
}
fn decode_session(body: &[u8]) -> Result<SessionWire, WaveError> {
    let envelope: SessionEnvelope =
        serde_json::from_slice(body).map_err(|_| WaveError::UnsupportedResponse)?;
    if envelope.error.is_some_and(|e| e.get() != "null") {
        return Err(WaveError::UnsupportedResponse);
    }
    let wire = envelope.result.ok_or(WaveError::UnsupportedResponse)?;
    if wire.unknown_session {
        return Err(WaveError::UnknownSession);
    }
    if wire
        .radio_session_id
        .as_ref()
        .is_some_and(|id| !opaque_id(id))
        || !opaque_id(&wire.batch_id)
        || wire.sequence.len() > PAGE_SIZE
        || wire.sequence.iter().any(|e| e.kind != "track")
    {
        return Err(WaveError::UnsupportedResponse);
    }
    if wire.sequence.is_empty() {
        return Err(WaveError::NoTracks);
    }
    Ok(wire)
}

fn utc_timestamp(now: SystemTime) -> Result<String, WaveError> {
    let since = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WaveError::InvalidObservation)?;
    let seconds = since.as_secs();
    if seconds > 253_402_300_799 {
        return Err(WaveError::InvalidObservation);
    }
    let mut days = seconds / 86_400;
    let mut year = 1970u32;
    let leap = |y: u32| y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    loop {
        let count = if leap(year) { 366 } else { 365 };
        if days < count {
            break;
        }
        days -= count;
        year += 1;
    }
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for count in months {
        if days < count {
            break;
        }
        days -= count;
        month += 1;
    }
    Ok(format!(
        "{year:04}-{month:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        days + 1,
        (seconds % 86_400) / 3600,
        (seconds % 3600) / 60,
        seconds % 60,
        since.subsec_millis()
    ))
}

struct Active {
    occurrence: u64,
    track_id: String,
    marker: WaveBatchMarker,
    started: bool,
    completed: bool,
    playing: bool,
    heard: Duration,
    last: Instant,
}

/// Feedback observation only, not an audio engine or a synthetic Wave queue.
/// Call with actual worker state and monotonic time. Position/seek distance is
/// deliberately not an input, so it cannot inflate heard time. Precision follows
/// the caller's real state-observation interval; no full duration is invented.
pub struct WaveCoordinator {
    session: WaveSession,
    active: Option<Active>,
    radio_started: bool,
    closed: bool,
}
impl fmt::Debug for WaveCoordinator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WaveCoordinator([redacted])")
    }
}
impl WaveCoordinator {
    pub fn new(session: &WaveSession) -> Self {
        Self {
            session: session.clone(),
            active: None,
            radio_started: false,
            closed: false,
        }
    }
    /// Supply an accepted source/intent occurrence, not only a service track ID.
    /// Selecting/loading emits no start; replacing a genuinely heard track emits
    /// one skip tied to its original marker, even after later batches arrived.
    pub fn select(
        &mut self,
        occurrence: u64,
        track: &Track,
        marker: &WaveBatchMarker,
        now: Instant,
    ) -> Result<Vec<WaveEvent>, WaveError> {
        self.check()?;
        if !Arc::ptr_eq(&self.session.0, &marker.identity)
            || !marker.tracks.contains(&track.id)
            || !track.available
        {
            return Err(WaveError::InvalidObservation);
        }
        if self.active.as_ref().is_some_and(|a| {
            a.occurrence == occurrence
                && a.track_id == track.id
                && a.marker.batch_id == marker.batch_id
        }) {
            return Ok(Vec::new());
        }
        let events = self.complete(WaveEventKind::Skip, now)?;
        self.active = Some(Active {
            occurrence,
            track_id: track.id.clone(),
            marker: marker.clone(),
            started: false,
            completed: false,
            playing: false,
            heard: Duration::ZERO,
            last: now,
        });
        Ok(events)
    }
    pub fn observe(
        &mut self,
        state: PlaybackState,
        now: Instant,
    ) -> Result<Vec<WaveEvent>, WaveError> {
        self.check()?;
        let Some(active) = self.active.as_mut() else {
            return Ok(Vec::new());
        };
        advance(active, now)?;
        if active.completed {
            return Ok(Vec::new());
        }
        let mut events = Vec::new();
        if state == PlaybackState::Playing {
            if !active.started {
                if !self.radio_started {
                    events.push(event(
                        active,
                        WaveEventKind::RadioStarted,
                        None,
                        Duration::ZERO,
                    ));
                    self.radio_started = true;
                }
                events.push(event(
                    active,
                    WaveEventKind::TrackStarted,
                    Some(active.track_id.clone()),
                    Duration::ZERO,
                ));
                active.started = true;
            }
            active.playing = true;
        } else {
            active.playing = false;
            let kind = match state {
                PlaybackState::Ended => Some(WaveEventKind::TrackFinished),
                PlaybackState::Stopped | PlaybackState::Error => Some(WaveEventKind::Skip),
                _ => None,
            };
            if let Some(kind) = kind {
                if active.started {
                    events.push(event(
                        active,
                        kind,
                        Some(active.track_id.clone()),
                        active.heard,
                    ));
                }
                active.completed = true;
            }
        }
        Ok(events)
    }
    /// Normal explicit session end while credentials are still valid. Send the
    /// returned reports once in order, then cancel locally; no server-delete
    /// endpoint or completed remote teardown is implied.
    pub fn finish(&mut self, now: Instant) -> Result<Vec<WaveEvent>, WaveError> {
        self.check()?;
        let mut reports = self.complete(WaveEventKind::Skip, now)?;
        if self.radio_started
            && let Some(active) = self.active.as_ref()
        {
            reports.push(event(
                active,
                WaveEventKind::RadioFinished,
                None,
                Duration::ZERO,
            ));
        }
        self.closed = true;
        Ok(reports)
    }
    /// Logout/close cancellation does not send late work or fabricated feedback.
    pub fn cancel(&mut self) {
        self.session.cancel();
        self.active = None;
        self.closed = true;
    }
    fn check(&self) -> Result<(), WaveError> {
        self.session.0.cancellation.check()?;
        if self.closed {
            Err(WaveError::InvalidObservation)
        } else {
            Ok(())
        }
    }
    fn complete(&mut self, kind: WaveEventKind, now: Instant) -> Result<Vec<WaveEvent>, WaveError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(Vec::new());
        };
        advance(active, now)?;
        if active.completed {
            return Ok(Vec::new());
        }
        active.completed = true;
        active.playing = false;
        if active.started {
            Ok(vec![event(
                active,
                kind,
                Some(active.track_id.clone()),
                active.heard,
            )])
        } else {
            Ok(Vec::new())
        }
    }
}
fn advance(active: &mut Active, now: Instant) -> Result<(), WaveError> {
    let elapsed = now
        .checked_duration_since(active.last)
        .ok_or(WaveError::InvalidObservation)?;
    if active.playing {
        active.heard = active.heard.saturating_add(elapsed);
    }
    active.last = now;
    Ok(())
}
fn event(
    active: &Active,
    kind: WaveEventKind,
    track_id: Option<String>,
    heard: Duration,
) -> WaveEvent {
    WaveEvent {
        marker: active.marker.clone(),
        kind,
        track_id,
        heard,
    }
}

#[cfg(test)]
mod tests;
