// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in radio intent, bounded provenance and serialized provider work. Audio,
//! queue selection and media-system metadata remain owned by the existing player.

use super::*;
use crate::{
    music::wave::{
        WaveBatch, WaveBatchMarker, WaveCancellation, WaveCoordinator, WaveError, WaveEvent,
        WaveSession, WaveStage,
    },
    playback::{PlaybackSnapshot, PlaybackState},
    queue::{OccurrenceId, RepeatMode},
};
use std::{collections::VecDeque, fmt, time::Instant};

const MAX_EVENTS: usize = 16;
const KEEP_HISTORY: usize = 25;
const MAX_BUFFERED: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RadioStage {
    Session,
    Tracks,
    Normalization,
    Delivery,
    Playback,
    Feedback,
    Local,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RadioFailure {
    Authentication,
    Forbidden,
    Restricted,
    RateLimited,
    Network,
    Timeout,
    Http,
    Unsupported,
    Empty,
    Unavailable,
    Busy,
    OutcomeUnknown,
    Capacity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RadioPhase {
    Ready,
    Starting,
    Active,
    LoadingNext,
    Finishing,
    Failed {
        stage: RadioStage,
        failure: RadioFailure,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RadioStatus {
    pub phase: RadioPhase,
    pub playback: Option<PlaybackState>,
    pub authenticated: bool,
}

struct Binding {
    occurrence: OccurrenceId,
    marker: WaveBatchMarker,
    // Position in this admitted normalized batch, independent of queue edits.
    _batch_position: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FeedbackOrigin {
    Playback,
    Selection,
    Retirement,
}
impl FeedbackOrigin {
    fn as_str(self) -> &'static str {
        match self {
            Self::Playback => "playback",
            Self::Selection => "selection",
            Self::Retirement => "retirement",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FinishReason {
    SourceChange,
    Stop,
}
impl FinishReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::SourceChange => "source-change",
            Self::Stop => "stop",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiagnosticPhase {
    Starting,
    Active,
    Retiring,
}
impl DiagnosticPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Active => "active",
            Self::Retiring => "retiring",
        }
    }
}
struct PendingFeedback {
    event: WaveEvent,
    origin: FeedbackOrigin,
}
fn pending_events(
    events: Vec<WaveEvent>,
    origin: FeedbackOrigin,
) -> impl Iterator<Item = PendingFeedback> {
    events
        .into_iter()
        .map(move |event| PendingFeedback { event, origin })
}
/// Closed local classifications only. No provider payload, identity, listening
/// duration or URL is accepted by this diagnostic boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OperationDiagnostic {
    stage: RadioStage,
    category: &'static str,
    http: Option<u16>,
    event: Option<crate::music::wave::WaveEventKind>,
    phase: DiagnosticPhase,
    origin: Option<FeedbackOrigin>,
    finish: Option<FinishReason>,
}
impl fmt::Display for OperationDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Karin wave: stage={} category={} http=",
            stage_name(self.stage),
            self.category
        )?;
        match self.http {
            Some(http) => write!(f, "{http}")?,
            None => f.write_str("none")?,
        }
        write!(
            f,
            " event={} phase={} origin={} finish={}",
            self.event.map_or("none", |event| event.as_str()),
            self.phase.as_str(),
            self.origin.map_or("none", FeedbackOrigin::as_str),
            self.finish.map_or("none", FinishReason::as_str)
        )
    }
}
struct Active {
    epoch: u64,
    context: String,
    session: WaveSession,
    coordinator: WaveCoordinator,
    bindings: Vec<Binding>,
    events: VecDeque<PendingFeedback>,
    cycle: Option<u64>,
    batch_pending: bool,
    batch_running: bool,
    batch_for: Option<OccurrenceId>,
    waiting_next: Option<bool>,
}
struct Retiring {
    reason: FinishReason,
    epoch: u64,
    session: WaveSession,
    events: VecDeque<PendingFeedback>,
}
#[derive(Default)]
pub(super) struct RadioLifecycle {
    epoch: u64,
    serial: u64,
    running: Option<u64>,
    begin: Option<WaveCancellation>,
    begin_pending: bool,
    active: Option<Active>,
    retiring: Option<Retiring>,
    failure: Option<(RadioStage, RadioFailure)>,
    saved_modes: Option<(bool, RepeatMode)>,
    #[cfg(test)]
    last_diagnostic: Option<OperationDiagnostic>,
}
impl RadioLifecycle {
    fn cancel(&mut self) {
        if let Some(begin) = self.begin.take() {
            begin.cancel();
        }
        if let Some(mut active) = self.active.take() {
            active.coordinator.cancel();
        }
        if let Some(retiring) = self.retiring.take() {
            retiring.session.cancel();
        }
        self.begin_pending = false;
        self.failure = None;
        #[cfg(test)]
        {
            self.last_diagnostic = None;
        }
        self.epoch = self.epoch.wrapping_add(1);
        // An admitted worker retains the only running slot until it actually
        // completes. Cancellation cannot create overlapping replacement jobs.
    }
}
enum Work {
    Begin(WaveCancellation),
    Batch(WaveSession, Vec<String>),
    Feedback(WaveSession, PendingFeedback),
}
enum ResultValue {
    Begin(Result<(WaveSession, WaveBatch), WaveError>),
    Batch(Result<WaveBatch, WaveError>),
    Feedback(Result<u16, WaveError>),
}
struct Task {
    serial: u64,
    epoch: u64,
    retiring: bool,
    stage: RadioStage,
    event: Option<crate::music::wave::WaveEventKind>,
    origin: Option<FeedbackOrigin>,
    finish: Option<FinishReason>,
    work: Work,
}

impl Controller {
    pub(super) fn radio_status(&self) -> RadioStatus {
        let model = self.model.borrow();
        let authenticated = !model.closed
            && model.account_valid
            && model
                .token
                .as_ref()
                .is_some_and(|token| !token.is_expired());
        let radio = self.radio.borrow();
        let phase = if let Some((stage, failure)) = radio.failure.or_else(|| {
            radio
                .active
                .as_ref()
                .and_then(|_| self.wave_playback_failure())
        }) {
            RadioPhase::Failed { stage, failure }
        } else if radio.retiring.is_some() {
            RadioPhase::Finishing
        } else if radio.begin.is_some() {
            RadioPhase::Starting
        } else if let Some(active) = &radio.active {
            if active.waiting_next.is_some() {
                RadioPhase::LoadingNext
            } else {
                RadioPhase::Active
            }
        } else {
            RadioPhase::Ready
        };
        let playback = self
            .is_wave_queue()
            .then(|| self.music_snapshot().map(|s| s.state))
            .flatten();
        RadioStatus {
            phase,
            playback,
            authenticated,
        }
    }
    pub(super) fn start_wave(self: &Rc<Self>) {
        let status = self.radio_status();
        if !status.authenticated {
            return;
        }
        if self.radio.borrow().active.is_some() {
            if !matches!(status.phase, RadioPhase::Failed { .. }) {
                return;
            }
            self.leave_wave();
        }
        {
            let mut radio = self.radio.borrow_mut();
            if radio.begin.is_some() || radio.active.is_some() {
                return;
            }
            radio.epoch = radio.epoch.wrapping_add(1);
            radio.begin = Some(WaveCancellation::default());
            radio.begin_pending = true;
            radio.failure = None;
        }
        // Keep the user's ordinary source intact until playable data arrives.
        self.render_wave();
        self.drive_radio();
    }
    /// One Hero action delegates to the existing player/session. Navigating
    /// Home never calls this method and cannot create another radio identity.
    pub(super) fn activate_wave_hero(self: &Rc<Self>) {
        let status = self.radio_status();
        match status.phase {
            RadioPhase::Ready | RadioPhase::Failed { .. } => self.start_wave(),
            RadioPhase::Active | RadioPhase::LoadingNext => match status.playback {
                Some(PlaybackState::Playing) => self.toggle_music(),
                Some(PlaybackState::Paused | PlaybackState::Ready) => self.play_music(),
                _ => {}
            },
            RadioPhase::Starting | RadioPhase::Finishing => {}
        }
    }
    /// Cancel only a pending request. Current ordinary/already admitted audio
    /// and its queue stay intact; there is no active-radio Finish UI action.
    pub(super) fn cancel_wave_start(self: &Rc<Self>) {
        let mut radio = self.radio.borrow_mut();
        if let Some(begin) = radio.begin.take() {
            begin.cancel();
        }
        radio.begin_pending = false;
        drop(radio);
        self.render_wave();
    }
    pub(super) fn is_wave_queue(&self) -> bool {
        self.model
            .borrow()
            .queue
            .context()
            .is_some_and(|c| c.source_kind() == ContextKind::Wave)
    }
    /// A later deliberate ordinary-source action supersedes a pending start.
    /// Automatic EOS never calls this; final reports already retiring retain
    /// their separate bounded lane and validated-account binding.
    pub(super) fn cancel_pending_wave_for_ordinary_action(&self) {
        if self.is_wave_queue() {
            return;
        }
        let mut radio = self.radio.borrow_mut();
        if let Some(begin) = radio.begin.take() {
            begin.cancel();
        }
        radio.begin_pending = false;
    }
    #[cfg(test)]
    pub(super) fn radio_idle_for_test(&self) -> bool {
        let radio = self.radio.borrow();
        radio.running.is_none() && !radio.begin_pending && radio.retiring.is_none()
    }
    #[cfg(test)]
    pub(super) fn wave_session_for_test(&self) -> Option<WaveSession> {
        self.radio
            .borrow()
            .active
            .as_ref()
            .map(|active| active.session.clone())
    }
    #[cfg(test)]
    pub(super) fn poison_wave_session_for_test(&self) {
        self.radio
            .borrow()
            .active
            .as_ref()
            .expect("synthetic active Wave")
            .session
            .cancel();
    }
    pub(super) fn wave_manual_append_allowed(&self) -> bool {
        if self.is_wave_queue() {
            self.widgets
                .status
                .set_title("Волна использует очередь рекомендаций.");
            self.widgets.status.set_subtitle(
                "Выберите обычный трек или плейлист, затем добавьте нужные треки в очередь.",
            );
            self.render_controls();
            false
        } else {
            true
        }
    }
    pub(super) fn wave_current_allowed(&self) -> bool {
        if !self.is_wave_queue() {
            return true;
        }
        let model = self.model.borrow();
        let radio = self.radio.borrow();
        radio.active.as_ref().is_some_and(|active| {
            !active.session.is_cancelled()
                && model.queue.context().and_then(PlaybackContext::source_id)
                    == Some(active.context.as_str())
                && model.queue.current_occurrence_id().is_some_and(|id| {
                    active
                        .bindings
                        .iter()
                        .any(|binding| &binding.occurrence == id)
                })
        })
    }
    /// Hard account/window retirement: no new POST may be dispatched, including
    /// already queued final feedback. An already admitted HTTP call may finish.
    pub(super) fn cancel_radio(&self) {
        let saved = {
            let mut radio = self.radio.borrow_mut();
            radio.cancel();
            radio.saved_modes.take()
        };
        self.restore_radio_modes(saved);
    }
    fn restore_radio_modes(&self, saved: Option<(bool, RepeatMode)>) {
        if let Some((shuffle, repeat)) = saved {
            let mut model = self.model.borrow_mut();
            model.queue.set_shuffle(shuffle);
            model.queue.set_repeat_mode(repeat);
        }
    }
    /// Normal source replacement while the validated account still exists:
    /// finish once, drain in order, then locally retire the opaque identity.
    pub(super) fn leave_wave(&self) {
        self.leave_wave_with_reason(FinishReason::SourceChange);
    }
    pub(super) fn leave_wave_for_stop(&self) {
        self.leave_wave_with_reason(FinishReason::Stop);
    }
    fn leave_wave_with_reason(&self, reason: FinishReason) {
        self.observe_wave_snapshot(self.music_snapshot().as_ref());
        let (saved, failure) = {
            let mut radio = self.radio.borrow_mut();
            if let Some(begin) = radio.begin.take() {
                begin.cancel();
            }
            radio.begin_pending = false;
            let pending = radio.running.is_some();
            let mut failure = None;
            if let Some(mut active) = radio.active.take() {
                match active.coordinator.finish(Instant::now()) {
                    Ok(events) if event_capacity(active.events.len(), events.len()) => {
                        active
                            .events
                            .extend(pending_events(events, FeedbackOrigin::Retirement));
                        radio.retiring = Some(Retiring {
                            reason,
                            epoch: active.epoch,
                            session: active.session,
                            events: active.events,
                        });
                    }
                    Err(WaveError::Cancelled) if pending => {
                        // The provider has poisoned its identity but the typed
                        // result is still pending on GTK. Retain that exact job
                        // epoch, not any unsent reports, so its primary HTTP or
                        // unknown-write category survives a source switch.
                        radio.retiring = Some(Retiring {
                            reason,
                            epoch: active.epoch,
                            session: active.session,
                            events: VecDeque::new(),
                        });
                    }
                    Ok(_) => {
                        active.session.cancel();
                        failure = Some(RadioFailure::Capacity);
                    }
                    Err(error) => {
                        active.session.cancel();
                        failure = Some(classify(error));
                    }
                }
            }
            radio.failure = failure.map(|failure| (RadioStage::Local, failure));
            (radio.saved_modes.take(), failure)
        };
        self.restore_radio_modes(saved);
        if let Some(failure) = failure {
            eprintln!(
                "Karin wave: stage=local category={} http=none",
                failure_name(failure)
            );
        }
    }
    pub(super) fn cancel_wave(self: &Rc<Self>) {
        self.leave_wave_for_stop();
        if self.is_wave_queue() {
            self.stop_music_engine();
            self.model.borrow_mut().queue.clear();
            self.render_queue();
        }
        self.render_wave();
        self.drive_radio();
    }
    pub(super) fn select_wave_cycle(&self, serial: u64) {
        let (occurrence, track) = {
            let model = self.model.borrow();
            (
                model.queue.current_occurrence_id().cloned(),
                model.queue.current().cloned(),
            )
        };
        let mut radio = self.radio.borrow_mut();
        let pending = radio.running.is_some();
        let Some(active) = radio.active.as_mut() else {
            return;
        };
        let (Some(occurrence), Some(track)) = (occurrence, track) else {
            return;
        };
        let Some(marker) = active
            .bindings
            .iter()
            .find(|b| b.occurrence == occurrence)
            .map(|b| b.marker.clone())
        else {
            return;
        };
        match active
            .coordinator
            .select(serial, &track, &marker, Instant::now())
        {
            Ok(events) if event_capacity(active.events.len(), events.len()) => {
                active
                    .events
                    .extend(pending_events(events, FeedbackOrigin::Selection));
                active.cycle = Some(serial);
                // A later row/Previous selection supersedes a waiting Next.
                // Its in-flight batch remains useful, but cannot auto-select.
                active.waiting_next = None;
            }
            Err(WaveError::Cancelled) if pending => {}
            Ok(_) => {
                drop(radio);
                self.fail_radio(RadioStage::Local, RadioFailure::Capacity);
            }
            Err(error) => {
                drop(radio);
                self.fail_radio(RadioStage::Local, classify(error));
            }
        }
    }
    pub(super) fn observe_wave_snapshot(&self, snapshot: Option<&PlaybackSnapshot>) {
        let serial = self.music_serial();
        let Some(snapshot) = snapshot else {
            return;
        };
        let mut radio = self.radio.borrow_mut();
        let pending = radio.running.is_some();
        let Some(active) = radio.active.as_mut().filter(|a| a.cycle == Some(serial)) else {
            return;
        };
        match active.coordinator.observe_measured(
            snapshot.state,
            snapshot.playing_time,
            Instant::now(),
        ) {
            Ok(events) if event_capacity(active.events.len(), events.len()) => active
                .events
                .extend(pending_events(events, FeedbackOrigin::Playback)),
            Err(WaveError::Cancelled) if pending => {}
            Ok(_) => {
                drop(radio);
                self.fail_radio(RadioStage::Local, RadioFailure::Capacity);
            }
            Err(error) => {
                drop(radio);
                self.fail_radio(RadioStage::Local, classify(error));
            }
        }
    }
    pub(super) fn tick_radio(self: &Rc<Self>, snapshot: Option<&PlaybackSnapshot>) {
        self.observe_wave_snapshot(snapshot);
        let refresh_queue = snapshot.is_some_and(|snapshot| {
            matches!(
                snapshot.state,
                PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ended
            )
        }) && self.maybe_wave_batch(false, false);
        if refresh_queue {
            self.render_queue();
        }
        self.drive_radio();
        self.render_wave();
    }
    pub(super) fn wave_wait_for_next(self: &Rc<Self>, eos: bool) -> bool {
        if !self.is_wave_queue() {
            return false;
        }
        if !self.wave_current_allowed() {
            return true;
        }
        if self.maybe_wave_batch(true, eos) {
            self.render_queue();
        }
        self.drive_radio();
        self.render_wave();
        true
    }
    fn maybe_wave_batch(&self, waiting: bool, eos: bool) -> bool {
        if self.radio.borrow().active.is_none() {
            return false;
        }
        let (occurrence, upcoming, count, trimmed) = {
            let mut model = self.model.borrow_mut();
            let before = model.queue.items().len();
            model.queue.trim_wave_history(KEEP_HISTORY);
            (
                model.queue.current_occurrence_id().cloned(),
                model.queue.upcoming_indices().len(),
                model.queue.items().len(),
                before != model.queue.items().len(),
            )
        };
        let mut radio = self.radio.borrow_mut();
        let Some(active) = radio.active.as_mut() else {
            return trimmed;
        };
        if waiting {
            active.waiting_next = Some(active.waiting_next.unwrap_or(false) || eos);
        }
        if count < MAX_BUFFERED
            && (waiting || upcoming <= 3)
            && !active.batch_pending
            && !active.batch_running
            && (waiting || active.batch_for != occurrence)
        {
            active.batch_pending = true;
            active.batch_for = occurrence;
        }
        trimmed
    }
    fn record_radio_diagnostic(&self, diagnostic: OperationDiagnostic) {
        #[cfg(test)]
        {
            self.radio.borrow_mut().last_diagnostic = Some(diagnostic);
        }
        eprintln!("{diagnostic}");
    }
    #[cfg(test)]
    pub(super) fn wave_diagnostic_for_test(&self) -> Option<String> {
        self.radio
            .borrow()
            .last_diagnostic
            .map(|value| value.to_string())
    }
    fn fail_radio(&self, stage: RadioStage, failure: RadioFailure) {
        let (phase, finish) = self.diagnostic_phase(None);
        self.record_radio_diagnostic(OperationDiagnostic {
            stage,
            category: failure_name(failure),
            http: None,
            event: None,
            phase,
            origin: None,
            finish,
        });
        self.apply_radio_failure(stage, failure);
    }
    fn diagnostic_phase(&self, epoch: Option<u64>) -> (DiagnosticPhase, Option<FinishReason>) {
        let radio = self.radio.borrow();
        if let Some(retiring) = radio
            .retiring
            .as_ref()
            .filter(|retiring| epoch.is_none_or(|epoch| retiring.epoch == epoch))
        {
            (DiagnosticPhase::Retiring, Some(retiring.reason))
        } else if radio.begin.is_some() {
            (DiagnosticPhase::Starting, None)
        } else {
            (DiagnosticPhase::Active, None)
        }
    }
    fn apply_radio_failure(&self, stage: RadioStage, failure: RadioFailure) {
        let mut radio = self.radio.borrow_mut();
        if let Some(begin) = radio.begin.take() {
            begin.cancel();
        }
        radio.begin_pending = false;
        if let Some(mut active) = radio.active.take() {
            active.coordinator.cancel();
        }
        if let Some(retiring) = radio.retiring.take() {
            retiring.session.cancel();
        }
        radio.failure = Some((stage, failure));
        // Current audio is independent. It may finish, pause or resume; failed
        // radio cannot automatically select another unreported occurrence.
    }
    fn drive_radio(self: &Rc<Self>) {
        if !self.radio_status().authenticated {
            self.cancel_radio();
            return;
        }
        let task = {
            let mut radio = self.radio.borrow_mut();
            if radio.running.is_some() {
                return;
            }
            if radio.retiring.as_ref().is_some_and(|r| r.events.is_empty())
                && let Some(retiring) = radio.retiring.take()
            {
                retiring.session.cancel();
            }
            let picked = if let Some(retiring) = radio.retiring.as_mut() {
                retiring.events.pop_front().map(|event| {
                    (
                        retiring.epoch,
                        true,
                        RadioStage::Feedback,
                        Work::Feedback(retiring.session.clone(), event),
                    )
                })
            } else if radio.begin_pending {
                radio.begin_pending = false;
                radio.begin.as_ref().map(|cancel| {
                    (
                        radio.epoch,
                        false,
                        RadioStage::Session,
                        Work::Begin(cancel.clone()),
                    )
                })
            } else if let Some(active) = radio.active.as_mut() {
                if let Some(event) = active.events.pop_front() {
                    Some((
                        active.epoch,
                        false,
                        RadioStage::Feedback,
                        Work::Feedback(active.session.clone(), event),
                    ))
                } else if active.batch_pending {
                    active.batch_pending = false;
                    active.batch_running = true;
                    let queue = self
                        .model
                        .borrow()
                        .queue
                        .items()
                        .iter()
                        .map(|t| t.id.clone())
                        .collect();
                    Some((
                        active.epoch,
                        false,
                        RadioStage::Tracks,
                        Work::Batch(active.session.clone(), queue),
                    ))
                } else {
                    None
                }
            } else {
                None
            };
            let Some((epoch, retiring, stage, work)) = picked else {
                return;
            };
            radio.serial = radio.serial.wrapping_add(1);
            let serial = radio.serial;
            radio.running = Some(serial);
            let (event, origin) = match &work {
                Work::Feedback(_, feedback) => (Some(feedback.event.kind()), Some(feedback.origin)),
                _ => (None, None),
            };
            let finish = retiring.then(|| radio.retiring.as_ref().unwrap().reason);
            Task {
                serial,
                epoch,
                retiring,
                stage,
                event,
                origin,
                finish,
                work,
            }
        };
        let (Some(app), Some(service), Some(token)) = (
            self.app.upgrade(),
            self.service.clone(),
            self.model.borrow().token.clone(),
        ) else {
            self.radio.borrow_mut().running = None;
            self.fail_radio(RadioStage::Local, RadioFailure::Unavailable);
            return;
        };
        let generation = self.model.borrow().generation;
        let hold = app.hold();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let _hold = hold;
            let work = task.work;
            let result = gio::spawn_blocking(move || match work {
                Work::Begin(cancel) => ResultValue::Begin(service.begin_wave(&token, &cancel)),
                Work::Batch(session, queue) => {
                    ResultValue::Batch(service.wave_batch(&token, &session, &queue))
                }
                Work::Feedback(session, feedback) => ResultValue::Feedback(
                    service
                        .wave_feedback(&token, &session, &feedback.event)
                        .map(|ack| ack.http_status),
                ),
            })
            .await;
            let Some(this) = weak.upgrade() else {
                return;
            };
            let same_account = {
                let model = this.model.borrow();
                !model.closed && model.generation == generation && model.account_valid
            };
            let current = {
                let mut radio = this.radio.borrow_mut();
                if radio.running != Some(task.serial) {
                    return;
                }
                radio.running = None;
                same_account
                    && if task.retiring {
                        radio
                            .retiring
                            .as_ref()
                            .is_some_and(|r| r.epoch == task.epoch)
                    } else if task.stage == RadioStage::Session {
                        radio.epoch == task.epoch && radio.begin.is_some()
                    } else {
                        radio.active.as_ref().is_some_and(|a| a.epoch == task.epoch)
                            || radio
                                .retiring
                                .as_ref()
                                .is_some_and(|r| r.epoch == task.epoch)
                    }
            };
            if current {
                match result {
                    Ok(ResultValue::Begin(Ok((session, batch)))) => {
                        this.install_wave(task.epoch, session, batch)
                    }
                    Ok(ResultValue::Batch(Ok(batch))) => {
                        let active = this
                            .radio
                            .borrow()
                            .active
                            .as_ref()
                            .is_some_and(|a| a.epoch == task.epoch);
                        if active {
                            if let Some(active) = this.radio.borrow_mut().active.as_mut() {
                                active.batch_running = false;
                            }
                            this.append_wave(batch);
                        }
                    }
                    Ok(ResultValue::Feedback(Ok(http))) => {
                        let (phase, finish) = this.diagnostic_phase(Some(task.epoch));
                        this.record_radio_diagnostic(OperationDiagnostic {
                            stage: RadioStage::Feedback,
                            category: "accepted",
                            http: Some(http),
                            event: task.event,
                            phase,
                            origin: task.origin,
                            finish: finish.or(task.finish),
                        });
                    }
                    Ok(ResultValue::Begin(Err(error)))
                    | Ok(ResultValue::Batch(Err(error)))
                    | Ok(ResultValue::Feedback(Err(error))) => {
                        let stage = error.stage().map_or(task.stage, |stage| match stage {
                            WaveStage::Session => RadioStage::Session,
                            WaveStage::Tracks => RadioStage::Tracks,
                            WaveStage::Normalization => RadioStage::Normalization,
                            WaveStage::Feedback => RadioStage::Feedback,
                        });
                        let (phase, finish) = this.diagnostic_phase(Some(task.epoch));
                        this.record_radio_diagnostic(OperationDiagnostic {
                            stage,
                            category: error.category(),
                            http: error.http_status(),
                            event: task.event,
                            phase,
                            origin: task.origin,
                            finish: finish.or(task.finish),
                        });
                        this.apply_radio_failure(stage, classify(error));
                    }
                    Err(_) => {
                        let (phase, finish) = this.diagnostic_phase(Some(task.epoch));
                        this.record_radio_diagnostic(OperationDiagnostic {
                            stage: task.stage,
                            category: "worker-unavailable",
                            http: None,
                            event: task.event,
                            phase,
                            origin: task.origin,
                            finish: finish.or(task.finish),
                        });
                        this.apply_radio_failure(task.stage, RadioFailure::Unavailable);
                    }
                }
            } else if let Ok(ResultValue::Begin(Ok((session, _)))) = result {
                session.cancel();
            }
            // Apply a confirmed radio failure/cancellation to the existing
            // transport and MPRIS capabilities in this same reply, not only
            // after the next periodic player poll.
            this.render_music_player();
            this.render_wave();
            this.drive_radio();
        });
    }
    fn install_wave(self: &Rc<Self>, epoch: u64, session: WaveSession, batch: WaveBatch) {
        let Some(index) = batch.tracks.iter().position(|t| t.available) else {
            session.cancel();
            self.fail_radio(RadioStage::Normalization, RadioFailure::Empty);
            return;
        };
        eprintln!(
            "Karin wave: stage=session category=accepted http={} count={} omitted={}",
            batch.http_status,
            batch.tracks.len(),
            batch.omitted
        );
        eprintln!(
            "Karin wave: stage=normalization category=accepted http={} count={}",
            batch.lookup_http_status,
            batch.tracks.len()
        );
        let marker = batch.marker();
        let context_id = format!("wave-{epoch}");
        let Ok(context) = PlaybackContext::new(
            ContextKind::Wave,
            Some(context_id.clone()),
            batch.tracks,
            index,
            true,
        ) else {
            session.cancel();
            self.fail_radio(RadioStage::Local, RadioFailure::Capacity);
            return;
        };
        // Deliberate successful source replacement, through the same engine.
        self.stop_music_engine_for(crate::audio_cache::bridge::Retirement::WaveReplacement);
        let (saved_modes, bindings) = {
            let mut model = self.model.borrow_mut();
            let saved = (model.queue.shuffle(), model.queue.repeat_mode());
            model.queue.set_shuffle(false);
            model.queue.set_repeat_mode(RepeatMode::Off);
            model.queue.replace_context(context);
            let bindings = (0..model.queue.items().len())
                .map(|position| Binding {
                    occurrence: model.queue.occurrence_id(position).unwrap().clone(),
                    marker: marker.clone(),
                    _batch_position: position,
                })
                .collect();
            (saved, bindings)
        };
        {
            let mut radio = self.radio.borrow_mut();
            // A failed Wave may still own the queue's forced Off modes.
            // Explicit fresh-session start preserves the original user modes.
            radio.saved_modes.get_or_insert(saved_modes);
            radio.begin = None;
            radio.active = Some(Active {
                epoch,
                context: context_id,
                coordinator: WaveCoordinator::new(&session),
                session,
                bindings,
                events: VecDeque::new(),
                cycle: None,
                batch_pending: false,
                batch_running: false,
                batch_for: None,
                waiting_next: None,
            });
        }
        self.follow_current_occurrence();
        self.render_queue();
        self.request_music(true, None);
    }
    fn append_wave(self: &Rc<Self>, batch: WaveBatch) {
        eprintln!(
            "Karin wave: stage=tracks category=accepted http={} count={} omitted={}",
            batch.http_status,
            batch.tracks.len(),
            batch.omitted
        );
        if !batch.tracks.iter().any(|t| t.available) {
            self.fail_radio(RadioStage::Normalization, RadioFailure::Empty);
            return;
        }
        let marker = batch.marker();
        let count = batch.tracks.len();
        let bindings = {
            let mut model = self.model.borrow_mut();
            model.queue.trim_wave_history(KEEP_HISTORY);
            let first = model.queue.items().len();
            if first.saturating_add(count) > MAX_BUFFERED
                || model.queue.append_wave_batch(batch.tracks).is_err()
            {
                drop(model);
                self.fail_radio(RadioStage::Local, RadioFailure::Capacity);
                return;
            }
            (0..count)
                .map(|position| Binding {
                    occurrence: model.queue.occurrence_id(first + position).unwrap().clone(),
                    marker: marker.clone(),
                    _batch_position: position,
                })
                .collect::<Vec<_>>()
        };
        let waiting = {
            let model = self.model.borrow();
            let mut radio = self.radio.borrow_mut();
            let Some(active) = radio.active.as_mut() else {
                return;
            };
            active.bindings.retain(|binding| {
                (0..model.queue.items().len())
                    .any(|index| model.queue.occurrence_id(index) == Some(&binding.occurrence))
            });
            active.bindings.extend(bindings);
            active.waiting_next.take()
        };
        self.render_queue();
        if let Some(eos) = waiting {
            self.skip_music(true, eos);
        }
    }
}

fn event_capacity(queued: usize, additional: usize) -> bool {
    queued
        .checked_add(additional)
        .is_some_and(|count| count <= MAX_EVENTS)
}

fn classify(error: WaveError) -> RadioFailure {
    if let Some(error) = error.music_error() {
        return match error {
            MusicError::Unauthenticated | MusicError::AccessDenied => RadioFailure::Authentication,
            MusicError::Forbidden => RadioFailure::Forbidden,
            MusicError::Restricted => RadioFailure::Restricted,
            MusicError::RateLimited => RadioFailure::RateLimited,
            MusicError::Network => RadioFailure::Network,
            MusicError::Timeout => RadioFailure::Timeout,
            MusicError::Http(_) => RadioFailure::Http,
            _ => RadioFailure::Unsupported,
        };
    }
    match error.category() {
        "write-outcome-unknown" => RadioFailure::OutcomeUnknown,
        "busy" => RadioFailure::Busy,
        "no-tracks" => RadioFailure::Empty,
        "cancelled" | "session-terminated" | "unknown-session" => RadioFailure::Unavailable,
        _ => RadioFailure::Unsupported,
    }
}

fn stage_name(stage: RadioStage) -> &'static str {
    match stage {
        RadioStage::Session => "session",
        RadioStage::Tracks => "tracks",
        RadioStage::Normalization => "normalization",
        RadioStage::Delivery => "delivery",
        RadioStage::Playback => "playback",
        RadioStage::Feedback => "feedback",
        RadioStage::Local => "local",
    }
}
fn failure_name(failure: RadioFailure) -> &'static str {
    match failure {
        RadioFailure::Authentication => "authentication",
        RadioFailure::Forbidden => "forbidden",
        RadioFailure::Restricted => "restricted",
        RadioFailure::RateLimited => "rate-limited",
        RadioFailure::Network => "network",
        RadioFailure::Timeout => "timeout",
        RadioFailure::Http => "http",
        RadioFailure::Unsupported => "unsupported",
        RadioFailure::Empty => "empty",
        RadioFailure::Unavailable => "unavailable",
        RadioFailure::Busy => "busy",
        RadioFailure::OutcomeUnknown => "write-outcome-unknown",
        RadioFailure::Capacity => "capacity",
    }
}

#[cfg(test)]
mod tests;
