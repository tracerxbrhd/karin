// SPDX-License-Identifier: GPL-3.0-or-later
//! Music transport controls. Provider resolution and native media stay off GTK.

use std::{
    thread,
    time::{Duration, Instant},
};

use super::*;
use crate::{
    playback::{
        PlaybackController, PlaybackError, PlaybackOutput, PlaybackSnapshot, PlaybackSource,
        PlaybackState,
    },
    queue::PlaybackContext,
    stream::{DeliveryCancellation, DeliveryReport, StreamResolver},
};

struct Intent {
    serial: u64,
    session: u64,
    track: String,
    autoplay: bool,
    position: Option<Duration>,
}

#[derive(Clone, Copy)]
enum Failure {
    Delivery(DeliveryReport),
    Engine(PlaybackError),
}

pub(super) struct MusicPlayback {
    controller: Option<PlaybackController>,
    retired: Vec<PlaybackController>,
    serial: u64,
    track: Option<String>,
    resolving: bool,
    requested: bool,
    cancellation: Option<DeliveryCancellation>,
    pending: Option<Intent>,
    preparing: Option<(bool, Option<Duration>)>,
    failure: Option<Failure>,
    owned_error: Option<String>,
    reported_error: Option<(u64, PlaybackError)>,
    ended_handled: bool,
    stopped: bool,
    volume: f64,
    output: PlaybackOutput,
    #[cfg(test)]
    fixture_resolver: Option<StreamResolver>,
}

impl Default for MusicPlayback {
    fn default() -> Self {
        Self {
            controller: None,
            retired: Vec::new(),
            serial: 0,
            track: None,
            resolving: false,
            requested: false,
            cancellation: None,
            pending: None,
            preparing: None,
            failure: None,
            owned_error: None,
            reported_error: None,
            ended_handled: false,
            stopped: false,
            volume: 0.7,
            output: PlaybackOutput::System,
            #[cfg(test)]
            fixture_resolver: None,
        }
    }
}

impl Controller {
    #[cfg(test)]
    pub(super) fn set_fixture_resolver_for_test(&self, resolver: StreamResolver) {
        let mut player = self.player.borrow_mut();
        player.fixture_resolver = Some(resolver);
        player.output = PlaybackOutput::Fakesink;
    }

    pub(super) fn connect_music_player(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.widgets.play.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.toggle_music();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.stop.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.stop_music();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets
            .progress
            .connect_change_value(move |_, _, value| {
                if let Some(this) = weak.upgrade()
                    && value.is_finite()
                    && value >= 0.0
                {
                    this.seek_music(Duration::from_secs_f64(value));
                }
                glib::Propagation::Proceed
            });
        let weak = Rc::downgrade(self);
        self.widgets
            .volume
            .connect_change_value(move |_, _, value| {
                if let Some(this) = weak.upgrade()
                    && value.is_finite()
                    && (0.0..=1.0).contains(&value)
                {
                    this.set_music_volume(value);
                }
                glib::Propagation::Proceed
            });
        // No strong widget/controller capture: closing the host ends polling.
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(80), move || {
            let Some(this) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if this.model.borrow().closed {
                return glib::ControlFlow::Break;
            }
            this.tick_music();
            glib::ControlFlow::Continue
        });
    }

    pub(super) fn track_play_button(self: &Rc<Self>, track: &Track, compact: bool) -> gtk::Button {
        let button = Self::music_play_button(track, compact);
        let weak = Rc::downgrade(self);
        let selected = track.clone();
        button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.play_music_track(selected.clone());
            }
        });
        button
    }

    pub(super) fn context_play_button(
        self: &Rc<Self>,
        track: &Track,
        compact: bool,
        context: PlaybackContext,
    ) -> gtk::Button {
        let button = Self::music_play_button(track, compact);
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.play_music_context(context.clone());
            }
        });
        button
    }

    fn music_play_button(track: &Track, compact: bool) -> gtk::Button {
        let button = if compact {
            icon_button("media-playback-start-symbolic", "Воспроизвести трек")
        } else {
            let button = gtk::Button::with_label("Воспроизвести");
            button.add_css_class("suggested-action");
            button.set_halign(gtk::Align::Start);
            button
        };
        button.set_widget_name("music-track-play");
        button.set_sensitive(track.available);
        if !track.available {
            button.set_tooltip_text(Some("Трек недоступен по данным сервиса"));
        }
        button
    }

    pub(super) fn play_music_context(self: &Rc<Self>, context: PlaybackContext) {
        if !context.items()[context.current_index()].available {
            self.widgets
                .status
                .set_title("Сервис пометил трек как недоступный.");
            return;
        }
        {
            let mut model = self.model.borrow_mut();
            if model.closed || !model.account_valid {
                return;
            }
            if model.token.as_ref().is_none_or(AccessToken::is_expired) {
                drop(model);
                self.show_error(MusicError::Unauthenticated);
                return;
            }
            // A context contains one already loaded, bounded list. Replace it
            // atomically and keep the exact clicked occurrence, including IDs
            // repeated within a playlist or a search page. No catalog fetch is
            // implied, and manually appended extras belong to the old context.
            model.queue.replace_context(context);
        }
        self.info_selection.borrow_mut().take();
        self.render_queue();
        self.request_music(true, None);
    }

    pub(super) fn play_music_queue(self: &Rc<Self>, index: usize) {
        {
            let mut model = self.model.borrow_mut();
            if model.closed || !model.account_valid {
                return;
            }
            if model
                .queue
                .items()
                .get(index)
                .is_none_or(|track| !track.available)
            {
                return;
            }
            model.queue.select(index);
        }
        self.info_selection.borrow_mut().take();
        self.render_queue();
        self.request_music(true, None);
    }

    pub(super) fn play_music_track(self: &Rc<Self>, track: Track) {
        if !track.available {
            self.widgets
                .status
                .set_title("Сервис пометил трек как недоступный.");
            return;
        }
        {
            let mut model = self.model.borrow_mut();
            if model.closed || !model.account_valid {
                return;
            }
            // Prefer the already queued occurrence; never clear the user's queue.
            let index = model
                .queue
                .items()
                .iter()
                .position(|item| item.id == track.id);
            let index = match index {
                Some(index) => index,
                None => {
                    let index = model.queue.items().len();
                    if model.queue.append(track).is_err() {
                        drop(model);
                        self.widgets
                            .status
                            .set_title("Очередь заполнена. Удалите ненужные треки.");
                        return;
                    }
                    index
                }
            };
            model.queue.select(index);
        }
        self.render_queue();
        self.request_music(true, None);
    }

    pub(super) fn toggle_music(self: &Rc<Self>) {
        if self.music_requested() {
            return;
        }
        let snapshot = self.music_snapshot();
        match snapshot.as_ref().map(|snapshot| snapshot.state) {
            Some(PlaybackState::Playing) => self.music_command(PlaybackController::pause),
            Some(PlaybackState::Paused) => self.music_command(PlaybackController::resume),
            Some(PlaybackState::Ready) => {
                if let Some(snapshot) = &snapshot {
                    self.play_prepared_music(snapshot);
                }
            }
            Some(PlaybackState::Loading) => {}
            _ => self.request_music(true, snapshot.and_then(|snapshot| snapshot.position)),
        }
    }

    pub(super) fn play_music(self: &Rc<Self>) {
        if self.music_requested() {
            return;
        }
        let snapshot = self.music_snapshot();
        match snapshot.as_ref().map(|snapshot| snapshot.state) {
            Some(PlaybackState::Playing | PlaybackState::Loading) => {}
            Some(PlaybackState::Paused) => self.music_command(PlaybackController::resume),
            Some(PlaybackState::Ready) => {
                if let Some(snapshot) = &snapshot {
                    self.play_prepared_music(snapshot);
                }
            }
            _ => self.request_music(true, snapshot.and_then(|snapshot| snapshot.position)),
        }
    }

    pub(super) fn pause_music(&self) {
        if self
            .music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Playing)
        {
            self.music_command(PlaybackController::pause);
        }
    }

    fn play_prepared_music(&self, snapshot: &PlaybackSnapshot) {
        // The worker can reach Ready between GUI polls. Explicit Play must
        // consume that preparation before the worker leaves Ready; otherwise
        // its already-completed preparation would keep the UI buffering. Apply
        // the same pending resume position that the ordinary poll would use.
        let preparation = self.player.borrow_mut().preparing.take();
        if let Some((_, position)) = preparation {
            self.apply_music_resume_position(position, snapshot);
        }
        self.music_command(PlaybackController::play);
    }

    fn apply_music_resume_position(&self, position: Option<Duration>, snapshot: &PlaybackSnapshot) {
        if let Some(position) = position.filter(|value| !value.is_zero())
            && snapshot.seekable
            && snapshot.duration.is_none_or(|duration| position < duration)
        {
            self.seek_music(position);
        }
    }

    pub(super) fn seek_music(&self, position: Duration) {
        let before = self
            .music_snapshot()
            .map_or(0, |snapshot| snapshot.seek_serial);
        let result = self
            .player
            .borrow()
            .controller
            .as_ref()
            .map(|controller| controller.seek(position));
        if self.music_command_result(result) {
            // Acceptance only arms the bridge. It emits Seeked after a later
            // actual worker snapshot acknowledges this position/track binding.
            self.note_music_seek_from(position, before);
        }
    }

    pub(super) fn set_music_volume(&self, volume: f64) {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return;
        }
        self.player.borrow_mut().volume = volume;
        self.music_command(|controller| controller.set_volume(volume));
        self.render_music_player();
    }

    fn request_music(self: &Rc<Self>, autoplay: bool, position: Option<Duration>) {
        let (session, track) = {
            let model = self.model.borrow();
            if model.closed || !model.account_valid {
                return;
            }
            if model.token.as_ref().is_none_or(AccessToken::is_expired) {
                drop(model);
                self.show_error(MusicError::Unauthenticated);
                return;
            }
            let Some(track) = model.queue.current() else {
                return;
            };
            if !track.available {
                drop(model);
                self.widgets
                    .status
                    .set_title("Сервис пометил трек как недоступный.");
                return;
            }
            (model.generation, track.id.clone())
        };
        self.stop_music();
        self.widgets.status.set_title("Получаем источник аудио…");
        self.widgets.status.set_subtitle("");
        {
            let mut player = self.player.borrow_mut();
            player.serial = player.serial.wrapping_add(1);
            player.track = Some(track.clone());
            player.pending = Some(Intent {
                serial: player.serial,
                session,
                track,
                autoplay,
                position,
            });
            player.requested = true;
            player.stopped = false;
        }
        self.render_music_player();
        self.begin_music_resolution();
    }

    fn begin_music_resolution(self: &Rc<Self>) {
        let (intent, cancellation, retired) = {
            let mut player = self.player.borrow_mut();
            if player.resolving {
                return;
            }
            let Some(intent) = player.pending.take() else {
                return;
            };
            let cancellation = DeliveryCancellation::new();
            player.cancellation = Some(cancellation.clone());
            player.resolving = true;
            (intent, cancellation, player.retired.clone())
        };
        let (Some(app), Some(service), Some(token)) = (
            self.app.upgrade(),
            self.service.clone(),
            self.model.borrow().token.clone(),
        ) else {
            let mut player = self.player.borrow_mut();
            player.resolving = false;
            player.requested = false;
            player.failure = Some(Failure::Engine(PlaybackError::WorkerUnavailable));
            drop(player);
            self.render_music_player();
            return;
        };
        #[cfg(test)]
        let fixture_resolver = self.player.borrow().fixture_resolver.clone();
        #[cfg(not(test))]
        let fixture_resolver: Option<StreamResolver> = None;
        let hold = app.hold();
        let weak = Rc::downgrade(self);
        let track = intent.track.clone();
        glib::spawn_future_local(async move {
            let _hold_until_work_finishes = hold;
            let result = gio::spawn_blocking(move || {
                // A replacement cannot overlap an old native pipeline. Neither
                // this wait nor bounded HTTP resolution occupies GTK.
                if !wait_for_music_cleanup(&retired) {
                    return Err(Failure::Engine(PlaybackError::Timeout));
                }
                let resolver = match fixture_resolver {
                    Some(resolver) => resolver,
                    None => StreamResolver::new().map_err(Failure::Delivery)?,
                };
                resolver
                    .resolve(&service, &token, &track, &cancellation)
                    .map(PlaybackSource::remote)
                    .map_err(Failure::Delivery)
            })
            .await
            .unwrap_or(Err(Failure::Engine(PlaybackError::WorkerUnavailable)));
            let Some(this) = weak.upgrade() else {
                return;
            };
            let current = {
                let model = this.model.borrow();
                !model.closed
                    && model.account_valid
                    && model.generation == intent.session
                    && this.player.borrow().serial == intent.serial
                    && model
                        .queue
                        .current()
                        .is_some_and(|track| track.id == intent.track)
                    && model
                        .token
                        .as_ref()
                        .is_some_and(|token| !token.is_expired())
            };
            {
                let mut player = this.player.borrow_mut();
                player.resolving = false;
                player.cancellation = None;
                player
                    .retired
                    .retain(|controller| controller.snapshot().worker_running);
                if current {
                    player.requested = false;
                }
            }
            if current {
                match result {
                    Ok(source) => {
                        let output = this.player.borrow().output;
                        let volume = this.player.borrow().volume;
                        let prepared = PlaybackController::new(output).and_then(|controller| {
                            controller.set_volume(volume)?;
                            controller.prepare(source)?;
                            Ok(controller)
                        });
                        match prepared {
                            Ok(controller) => {
                                let mut player = this.player.borrow_mut();
                                player.controller = Some(controller);
                                player.preparing = Some((intent.autoplay, intent.position));
                                player.failure = None;
                                player.ended_handled = false;
                                drop(player);
                                if this.widgets.status.title() == "Получаем источник аудио…"
                                {
                                    this.widgets
                                        .status
                                        .set_title("Источник получен. Готовим воспроизведение…");
                                }
                            }
                            Err(error) => this.music_failure(Failure::Engine(error)),
                        }
                    }
                    Err(failure) => this.music_failure(failure),
                }
            }
            this.render_music_player();
            // One running resolution and only the latest pending intent.
            this.begin_music_resolution();
        });
    }

    pub(super) fn stop_music(&self) {
        let (old, owned_error) = {
            let mut player = self.player.borrow_mut();
            player.serial = player.serial.wrapping_add(1);
            if let Some(cancellation) = player.cancellation.take() {
                cancellation.cancel();
            }
            player.pending = None;
            player.track = None;
            player.requested = false;
            player.preparing = None;
            player.failure = None;
            let owned_error = player.owned_error.take();
            player.reported_error = None;
            player.ended_handled = false;
            player.stopped = true;
            let old = player.controller.take();
            if let Some(controller) = &old {
                controller.shutdown();
                player.retired.push(controller.clone());
            }
            (old, owned_error)
        };
        if owned_error
            .as_deref()
            .is_some_and(|message| self.widgets.status.subtitle().as_deref() == Some(message))
        {
            self.widgets
                .status
                .set_title("Воспроизведение остановлено.");
            self.widgets.status.set_subtitle("");
        }
        self.widgets.player_status.set_tooltip_text(None);
        if let Some(old) = old
            && let Some(app) = self.app.upgrade()
        {
            let hold = app.hold();
            glib::spawn_future_local(async move {
                let _hold_until_cleanup = hold;
                let cleaned = gio::spawn_blocking(move || wait_for_music_cleanup(&[old]))
                    .await
                    .unwrap_or(false);
                if !cleaned {
                    eprintln!("Karin playback: event=cleanup category=deadline");
                }
            });
        }
        self.render_controls();
        self.render_music_player();
    }

    pub(super) fn music_snapshot(&self) -> Option<PlaybackSnapshot> {
        self.player
            .borrow()
            .controller
            .as_ref()
            .map(PlaybackController::snapshot)
    }

    pub(super) fn music_requested(&self) -> bool {
        self.player.borrow().requested
    }

    pub(super) fn music_volume(&self) -> f64 {
        self.player.borrow().volume
    }

    fn music_command(
        &self,
        command: impl FnOnce(&PlaybackController) -> Result<(), PlaybackError>,
    ) {
        let result = self.player.borrow().controller.as_ref().map(command);
        self.music_command_result(result);
    }

    fn music_command_result(&self, result: Option<Result<(), PlaybackError>>) -> bool {
        match result {
            Some(Err(error)) => {
                self.music_failure(Failure::Engine(error));
                false
            }
            Some(Ok(())) => {
                let owned_error = {
                    let mut player = self.player.borrow_mut();
                    player.failure = None;
                    player.owned_error.take()
                };
                if owned_error.as_deref().is_some_and(|message| {
                    self.widgets.status.subtitle().as_deref() == Some(message)
                }) {
                    self.widgets
                        .status
                        .set_title("Управление воспроизведением.");
                    self.widgets.status.set_subtitle("");
                }
                self.widgets.player_status.set_tooltip_text(None);
                self.render_controls();
                true
            }
            None => false,
        }
    }

    fn music_failure(&self, failure: Failure) {
        self.player.borrow_mut().failure = Some(failure);
        let (title, message) = match failure {
            Failure::Delivery(report) => {
                eprintln!("Karin playback: event=resolution-result {report}");
                ("Не удалось получить источник аудио", report.to_string())
            }
            Failure::Engine(PlaybackError::RemoteHttp(status)) => {
                eprintln!("Karin playback: stage=media category=http-rejected http={status}");
                (
                    "Сервер аудио отклонил запрос",
                    format!(
                        "HTTP {status}. Нажмите воспроизведение, чтобы заново проверить доступ и получить источник. Запрос не повторяется автоматически."
                    ),
                )
            }
            Failure::Engine(error) => {
                eprintln!(
                    "Karin playback: stage=media category={} http=none",
                    music_playback_category(error)
                );
                (
                    if music_command_failure(error) {
                        "Не удалось выполнить действие"
                    } else {
                        "Не удалось воспроизвести трек"
                    },
                    music_playback_error(error).to_owned(),
                )
            }
        };
        self.player.borrow_mut().owned_error = Some(message.clone());
        self.widgets.player_status.set_tooltip_text(Some(&message));
        self.widgets.status.set_title(title);
        self.widgets.status.set_subtitle(&message);
        self.render_controls();
    }

    fn tick_music(self: &Rc<Self>) {
        self.guard_music_binding();
        let has_activity = {
            let player = self.player.borrow();
            player.requested || player.controller.is_some()
        };
        if has_activity
            && self
                .model
                .borrow()
                .token
                .as_ref()
                .is_some_and(AccessToken::is_expired)
        {
            self.show_error(MusicError::Unauthenticated);
            return;
        }
        let snapshot = self.music_snapshot();
        let mut ready = None;
        let mut ended = false;
        let mut failure = None;
        {
            let mut player = self.player.borrow_mut();
            player
                .retired
                .retain(|controller| controller.snapshot().worker_running);
            if let Some(snapshot) = &snapshot {
                if snapshot.state == PlaybackState::Ready {
                    ready = player.preparing.take();
                }
                if snapshot.state == PlaybackState::Error {
                    player.preparing = None;
                }
                if let Some(error) = snapshot.last_error
                    && player.reported_error != Some((player.serial, error))
                {
                    player.reported_error = Some((player.serial, error));
                    failure = Some(Failure::Engine(error));
                }
                if snapshot.state == PlaybackState::Ended && !player.ended_handled {
                    player.ended_handled = true;
                    ended = true;
                }
            }
        }
        if let Some((autoplay, position)) = ready {
            if let Some(snapshot) = &snapshot {
                self.apply_music_resume_position(position, snapshot);
            }
            if autoplay {
                self.music_command(PlaybackController::play);
            }
        }
        if let Some(failure) = failure {
            self.music_failure(failure);
        }
        if ended {
            self.skip_music(true, true);
        }
        self.render_music_player();
    }

    fn continuation(&self) -> (bool, bool) {
        let player = self.player.borrow();
        let state = player
            .controller
            .as_ref()
            .map(|controller| controller.snapshot().state);
        let active = player.requested || player.controller.is_some();
        let autoplay = player.requested
            || matches!(
                state,
                Some(PlaybackState::Playing | PlaybackState::Loading | PlaybackState::Ended)
            );
        (active, autoplay)
    }

    pub(super) fn guard_music_binding(&self) {
        let mismatched = {
            let model = self.model.borrow();
            let player = self.player.borrow();
            player
                .track
                .as_ref()
                .is_some_and(|id| model.queue.current().is_none_or(|track| &track.id != id))
        };
        if mismatched {
            self.stop_music();
        }
    }

    pub(super) fn skip_music(self: &Rc<Self>, forward: bool, eos: bool) {
        let (active, autoplay) = self.continuation();
        let advanced = {
            let mut model = self.model.borrow_mut();
            let current = model.queue.current_index();
            let next = current.and_then(|current| {
                if forward {
                    model.queue.items()[current + 1..]
                        .iter()
                        .position(|track| track.available)
                        .map(|offset| current + 1 + offset)
                } else {
                    model.queue.items()[..current]
                        .iter()
                        .rposition(|track| track.available)
                }
            });
            next.is_some_and(|index| model.queue.select(index).is_some())
        };
        if !advanced {
            return;
        }
        self.info_selection.borrow_mut().take();
        self.render_queue();
        if active {
            self.request_music(autoplay || eos, None);
        }
    }

    #[cfg(test)]
    pub(super) fn select_music_queue(self: &Rc<Self>, index: usize) {
        let (active, autoplay) = self.continuation();
        let changed = {
            let mut model = self.model.borrow_mut();
            let changed = model.queue.current_index() != Some(index);
            changed && model.queue.select(index).is_some()
        };
        if !changed {
            return;
        }
        self.render_queue();
        if active {
            self.request_music(autoplay, None);
        }
    }

    pub(super) fn remove_music_queue(self: &Rc<Self>, index: usize) {
        let (active, autoplay) = self.continuation();
        let removed_current = {
            let mut model = self.model.borrow_mut();
            let current = model.queue.current_index() == Some(index);
            model.queue.remove(index);
            current
        };
        self.render_queue();
        if active && removed_current {
            self.stop_music();
            if self.model.borrow().queue.current().is_some() {
                self.request_music(autoplay, None);
            }
        }
    }

    pub(super) fn render_music_player(&self) {
        let model = self.model.borrow();
        let player = self.player.borrow();
        let snapshot = player.controller.as_ref().map(PlaybackController::snapshot);
        let state = snapshot
            .as_ref()
            .map_or(PlaybackState::Idle, |snapshot| snapshot.state);
        let selected = model.queue.current();
        let ready = model.account_valid
            && !model.closed
            && model
                .token
                .as_ref()
                .is_some_and(|token| !token.is_expired());
        let loading =
            player.requested || player.preparing.is_some() || state == PlaybackState::Loading;
        let caption = if let Some(failure) = player.failure {
            match failure {
                Failure::Delivery(_) => "Источник аудио недоступен",
                Failure::Engine(PlaybackError::SourceExpired) => "Источник аудио истёк",
                Failure::Engine(error) if music_command_failure(error) => "Действие недоступно",
                Failure::Engine(_) => "Ошибка воспроизведения",
            }
        } else if player.requested {
            "Получаем аудио…"
        } else if player.preparing.is_some() {
            "Буферизация…"
        } else if player.controller.is_none()
            && player
                .retired
                .iter()
                .any(|controller| controller.snapshot().worker_running)
        {
            "Останавливаем…"
        } else {
            match state {
                PlaybackState::Loading => "Буферизация…",
                PlaybackState::Ready => "Готово к воспроизведению",
                PlaybackState::Playing => "Воспроизведение",
                PlaybackState::Paused => "Пауза",
                PlaybackState::Ended => "Трек завершён",
                PlaybackState::Stopped => "Остановлено",
                PlaybackState::Error => "Ошибка воспроизведения",
                PlaybackState::Idle if player.stopped && selected.is_some() => "Остановлено",
                _ => "",
            }
        };
        self.widgets.player_status.set_text(caption);
        self.widgets.player_status.set_visible(!caption.is_empty());
        self.widgets
            .play
            .set_icon_name(if state == PlaybackState::Playing {
                "media-playback-pause-symbolic"
            } else {
                "media-playback-start-symbolic"
            });
        self.widgets.play.set_tooltip_text(Some(match state {
            PlaybackState::Playing => "Пауза",
            PlaybackState::Paused => "Продолжить",
            PlaybackState::Error => "Получить источник заново и повторить",
            _ => "Воспроизвести выбранный трек",
        }));
        self.widgets
            .play
            .set_sensitive(ready && selected.is_some_and(|track| track.available) && !loading);
        self.widgets
            .stop
            .set_sensitive(player.requested || player.controller.is_some());
        self.widgets
            .volume
            .set_sensitive(ready && player.controller.is_some() && state != PlaybackState::Error);
        self.widgets.volume.set_value(player.volume);
        let seekable = ready
            && snapshot.as_ref().is_some_and(|snapshot| snapshot.seekable)
            && matches!(
                state,
                PlaybackState::Ready | PlaybackState::Playing | PlaybackState::Paused
            );
        self.widgets.progress.set_sensitive(seekable);
        if let Some(snapshot) = snapshot {
            let duration = snapshot.duration;
            self.widgets.progress.set_range(
                0.0,
                duration.map_or(1.0, |duration| duration.as_secs_f64().max(0.1)),
            );
            self.widgets.progress.set_value(
                snapshot
                    .position
                    .map_or(0.0, |position| position.as_secs_f64()),
            );
            self.widgets.position.set_text(&format!(
                "{} / {}",
                snapshot.position.map_or_else(|| "—".into(), music_time),
                duration.map_or_else(|| "—".into(), music_time)
            ));
        } else {
            self.widgets.progress.set_range(0.0, 1.0);
            self.widgets.progress.set_value(0.0);
            self.widgets.position.set_text(
                &selected.and_then(|track| track.duration_ms).map_or_else(
                    || "— / —".into(),
                    |duration| format!("— / {}", format_duration(duration)),
                ),
            );
        }
        drop(player);
        drop(model);
        self.publish_mpris();
        self.sync_track_pane();
    }
}

fn wait_for_music_cleanup(controllers: &[PlaybackController]) -> bool {
    let deadline = Instant::now() + Duration::from_secs(6);
    while controllers
        .iter()
        .any(|controller| controller.snapshot().worker_running)
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(10));
    }
    controllers
        .iter()
        .all(|controller| !controller.snapshot().worker_running)
}

fn music_time(value: Duration) -> String {
    format!("{}:{:02}", value.as_secs() / 60, value.as_secs() % 60)
}

fn music_playback_error(error: PlaybackError) -> &'static str {
    match error {
        PlaybackError::RemoteHttp(_) => "Сервер аудио отклонил запрос. Получите источник заново.",
        PlaybackError::SourceExpired => {
            "Адрес аудио истёк. Нажмите воспроизведение, чтобы получить источник заново."
        }
        PlaybackError::SourcePolicy => "Источник аудио не прошёл безопасную проверку.",
        PlaybackError::NotSeekable | PlaybackError::SeekFailed | PlaybackError::InvalidSeek => {
            "Перемотка сейчас недоступна."
        }
        PlaybackError::InvalidVolume => "Укажите громкость от 0 до 100%.",
        PlaybackError::QueueFull => "Слишком много команд. Повторите действие.",
        PlaybackError::Timeout | PlaybackError::StateChange => {
            "Аудиодвижок не завершил переход. Нажмите воспроизведение для повторной попытки."
        }
        PlaybackError::Pipeline | PlaybackError::Initialization => {
            "Проверьте установку GStreamer и системный аудиовыход."
        }
        PlaybackError::Playback
        | PlaybackError::SourceUnavailable
        | PlaybackError::DiagnosticMedia => {
            "Не удалось прочитать аудио или открыть аудиовыход. Нажмите воспроизведение для повторной попытки."
        }
        PlaybackError::InvalidState => "Дождитесь готовности аудио и повторите действие.",
        PlaybackError::WorkerUnavailable | PlaybackError::ShuttingDown => {
            "Проигрыватель завершает работу. Нажмите воспроизведение для повторной попытки."
        }
    }
}

fn music_playback_category(error: PlaybackError) -> &'static str {
    match error {
        PlaybackError::RemoteHttp(_) => "http-rejected",
        PlaybackError::SourceExpired => "source-expired",
        PlaybackError::SourcePolicy => "source-policy",
        PlaybackError::NotSeekable | PlaybackError::SeekFailed | PlaybackError::InvalidSeek => {
            "seek-unavailable"
        }
        PlaybackError::InvalidVolume => "invalid-volume",
        PlaybackError::QueueFull => "command-queue-full",
        PlaybackError::Timeout => "timeout",
        PlaybackError::StateChange => "state-change",
        PlaybackError::Pipeline | PlaybackError::Initialization => "audio-component-unavailable",
        PlaybackError::Playback
        | PlaybackError::SourceUnavailable
        | PlaybackError::DiagnosticMedia => "media-or-output-failed",
        PlaybackError::InvalidState => "invalid-state",
        PlaybackError::WorkerUnavailable | PlaybackError::ShuttingDown => "worker-unavailable",
    }
}

fn music_command_failure(error: PlaybackError) -> bool {
    matches!(
        error,
        PlaybackError::NotSeekable
            | PlaybackError::SeekFailed
            | PlaybackError::InvalidSeek
            | PlaybackError::InvalidVolume
            | PlaybackError::QueueFull
            | PlaybackError::InvalidState
    )
}

#[cfg(test)]
mod tests;
