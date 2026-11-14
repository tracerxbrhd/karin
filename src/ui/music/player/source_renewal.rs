// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit, bounded renewal of the same queue occurrence's source admission.
//! This is not a new playback cycle, player, radio session or saved resource.

use super::*;
use crate::playback::PlaybackSourceKind;

#[derive(Clone, Copy)]
enum Phase {
    Direct,
    Guarding,
    Waiting,
    Resolving,
    Preparing(u64),
}

pub(super) struct Renewal {
    ticket: u64,
    session: u64,
    serial: u64,
    occurrence: OccurrenceId,
    position: Option<Duration>,
    confirmed_position: Option<Duration>,
    autoplay: bool,
    user_seek: bool,
    attempted: bool,
    cache_retry: bool,
    force_online: bool,
    phase: Phase,
}

impl Renewal {
    fn active(&self) -> bool {
        matches!(
            self.phase,
            Phase::Waiting | Phase::Resolving | Phase::Preparing(_)
        )
    }
}

impl Controller {
    pub(in crate::ui::music) fn source_renewal_active(&self) -> bool {
        self.player
            .borrow()
            .renewal
            .as_ref()
            .is_some_and(Renewal::active)
    }

    pub(super) fn cancel_source_renewal(&self) {
        let mut player = self.player.borrow_mut();
        if let Some(renewal) = player.renewal.take() {
            if renewal.active() {
                player.requested = false;
                player.play_intent = false;
            }
            if matches!(renewal.phase, Phase::Resolving)
                && let Some(cancellation) = &player.cancellation
            {
                cancellation.cancel();
            }
        }
        player.renewal_serial = player.renewal_serial.wrapping_add(1);
    }

    /// An explicit repeated Play updates the existing intent, never submits a
    /// second HTTP job. A paused, cancelled native preparation can resume too.
    pub(super) fn play_pending_source_renewal(&self) -> bool {
        let preparing = {
            let mut player = self.player.borrow_mut();
            let Some(renewal) = player.renewal.as_mut().filter(|renewal| renewal.active()) else {
                return false;
            };
            renewal.autoplay = true;
            let preparing = matches!(renewal.phase, Phase::Preparing(_));
            player.play_intent = true;
            player.requested = true;
            preparing
        };
        if preparing {
            self.music_command(PlaybackController::resume);
        }
        true
    }

    pub(super) fn pause_source_renewal(&self) -> bool {
        let phase = {
            let mut player = self.player.borrow_mut();
            let Some(renewal) = player.renewal.as_mut() else {
                return false;
            };
            let phase = renewal.phase;
            renewal.autoplay = false;
            player.play_intent = false;
            player.requested = false;
            phase
        };
        match phase {
            Phase::Waiting | Phase::Resolving => {
                self.cancel_source_renewal();
            }
            Phase::Preparing(_) => self.music_command(PlaybackController::pause),
            Phase::Direct | Phase::Guarding => {
                self.cancel_source_renewal();
                self.music_command(PlaybackController::pause);
            }
        }
        self.note_playback_intent();
        self.render_music_player();
        true
    }

    /// Returns true only when an explicit Seek belongs to a pending renewal.
    /// The latest target is sent to the native worker, which acknowledges this
    /// user seek separately from the unacknowledged internal resume position.
    pub(super) fn seek_source_renewal(&self, position: Duration) -> bool {
        let preparing = {
            let mut player = self.player.borrow_mut();
            let Some(renewal) = player.renewal.as_mut().filter(|renewal| renewal.active()) else {
                return false;
            };
            renewal.position = Some(position);
            renewal.user_seek = true;
            matches!(renewal.phase, Phase::Preparing(_))
        };
        let before = self
            .player
            .borrow()
            .controller
            .as_ref()
            .map_or(0, |p| p.snapshot().seek_serial);
        self.note_music_seek_from(position, before);
        if preparing {
            self.music_command(|controller| controller.seek(position));
        }
        self.note_playback_intent();
        true
    }

    pub(super) fn resume_current_source(self: &Rc<Self>) {
        if self.play_pending_source_renewal() {
            return;
        }
        let Some(snapshot) = self
            .player
            .borrow()
            .controller
            .as_ref()
            .map(PlaybackController::snapshot)
        else {
            return;
        };
        if snapshot.source_kind == PlaybackSourceKind::LocalFile && !snapshot.source_expired {
            self.music_command(PlaybackController::resume);
            return;
        }
        let (session, occurrence) = {
            let model = self.model.borrow();
            if model.closed
                || !model.account_valid
                || model.token.as_ref().is_none_or(AccessToken::is_expired)
            {
                return;
            }
            let Some(occurrence) = model.queue.current_occurrence_id().cloned() else {
                return;
            };
            (model.generation, occurrence)
        };
        self.observe_wave_snapshot(Some(&snapshot));
        self.note_playback_intent();
        {
            let mut player = self.player.borrow_mut();
            player.renewal_serial = player.renewal_serial.wrapping_add(1);
            let position = player
                .preparing
                .and_then(|(_, position)| position)
                .or(snapshot.position);
            player.renewal = Some(Renewal {
                ticket: player.renewal_serial,
                session,
                serial: player.serial,
                occurrence,
                position,
                confirmed_position: snapshot.position,
                autoplay: true,
                user_seek: false,
                attempted: false,
                cache_retry: false,
                force_online: false,
                phase: Phase::Direct,
            });
            player.play_intent = true;
        }
        if snapshot.source_expired || snapshot.last_error == Some(PlaybackError::SourceExpired) {
            self.queue_source_renewal(false);
        } else {
            self.music_command(PlaybackController::resume);
        }
    }

    fn queue_source_renewal(self: &Rc<Self>, force_online: bool) {
        let position_unavailable = {
            let mut player = self.player.borrow_mut();
            let snapshot = player.controller.as_ref().map(PlaybackController::snapshot);
            let Some(renewal) = player.renewal.as_mut() else {
                return;
            };
            if !force_online && !renewal.user_seek {
                renewal.position = snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.position)
                    .or(renewal.position);
                renewal.confirmed_position = snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.position)
                    .or(renewal.confirmed_position);
            }
            renewal.position.is_none()
                && snapshot
                    .as_ref()
                    .is_some_and(|snapshot| !snapshot.playing_time.is_zero())
        };
        if position_unavailable {
            self.cancel_source_renewal();
            let message = "Сохранённая позиция недоступна. Воспроизведение остаётся на паузе. Чтобы начать сначала, выберите трек в списке.";
            let mut player = self.player.borrow_mut();
            player.failure = Some(Failure::Engine(PlaybackError::NotSeekable));
            player.owned_error = Some(message.to_owned());
            player.reported_error = player
                .controller
                .as_ref()
                .and_then(|controller| controller.snapshot().last_error)
                .map(|error| (player.serial, error));
            drop(player);
            self.widgets
                .status
                .set_title("Не удалось продолжить с сохранённого места");
            self.widgets.status.set_subtitle(message);
            self.render_controls();
            return;
        }
        {
            let mut player = self.player.borrow_mut();
            let Some(renewal) = player.renewal.as_mut() else {
                return;
            };
            renewal.phase = Phase::Waiting;
            renewal.attempted = true;
            renewal.force_online = force_online;
            player.requested = true;
            player.failure = None;
            player.owned_error = None;
            player.reported_error = None;
            if let Some(fill) = player.cache_fill_prepared.take() {
                fill.control()
                    .cancel(crate::audio_cache::diagnostic::Cause::Policy(
                        crate::audio_cache::http_policy::Reason::Cancelled,
                    ));
            }
        }
        self.widgets
            .status
            .set_title("Возобновляем воспроизведение…");
        self.widgets.status.set_subtitle("");
        self.widgets.player_status.set_tooltip_text(None);
        self.begin_source_renewal();
    }

    pub(super) fn begin_source_renewal(self: &Rc<Self>) {
        let (ticket, session, serial, occurrence, force_online, cancellation, track) = {
            let model = self.model.borrow();
            let mut player = self.player.borrow_mut();
            if player.resolving || model.closed {
                return;
            }
            let Some(renewal) = player
                .renewal
                .as_mut()
                .filter(|r| matches!(r.phase, Phase::Waiting))
            else {
                return;
            };
            if model.generation != renewal.session
                || model.queue.current_occurrence_id() != Some(&renewal.occurrence)
            {
                player.renewal = None;
                return;
            }
            let Some(track) = model.queue.current().map(|track| track.id.clone()) else {
                return;
            };
            renewal.phase = Phase::Resolving;
            let data = (
                renewal.ticket,
                renewal.session,
                renewal.serial,
                renewal.occurrence.clone(),
                renewal.force_online,
                DeliveryCancellation::new(),
                track,
            );
            player.resolving = true;
            player.cancellation = Some(data.5.clone());
            data
        };
        let (Some(service), Some(token), Some(app)) = (
            self.service.clone(),
            self.model.borrow().token.clone(),
            self.app.upgrade(),
        ) else {
            let mut player = self.player.borrow_mut();
            player.resolving = false;
            player.requested = false;
            player.renewal = None;
            return;
        };
        let binding = if force_online {
            None
        } else {
            self.audio_cache_binding()
        };
        #[cfg(test)]
        let fixture = self.player.borrow().fixture_resolver.clone();
        #[cfg(not(test))]
        let fixture: Option<StreamResolver> = None;
        let weak = Rc::downgrade(self);
        let hold = app.hold();
        glib::spawn_future_local(async move {
            let _hold_until_result = hold;
            let result = gio::spawn_blocking(move || {
                let resolver = match fixture {
                    Some(resolver) => resolver,
                    None => StreamResolver::new().map_err(Failure::Delivery)?,
                };
                crate::audio_cache::bridge::resolve(
                    resolver,
                    &service,
                    &token,
                    &track,
                    &cancellation,
                    binding,
                )
                .map_err(Failure::Delivery)
            })
            .await
            .unwrap_or(Err(Failure::Engine(PlaybackError::WorkerUnavailable)));
            let Some(this) = weak.upgrade() else { return };
            let current = {
                let model = this.model.borrow();
                let player = this.player.borrow();
                !model.closed
                    && model.account_valid
                    && model.generation == session
                    && model.queue.current_occurrence_id() == Some(&occurrence)
                    && model
                        .token
                        .as_ref()
                        .is_some_and(|token| !token.is_expired())
                    && player.serial == serial
                    && player.renewal.as_ref().is_some_and(|renewal| {
                        renewal.ticket == ticket && matches!(renewal.phase, Phase::Resolving)
                    })
            };
            {
                let mut player = this.player.borrow_mut();
                player.resolving = false;
                player.cancellation = None;
            }
            if current {
                match result {
                    Ok(selection) => {
                        let (position, autoplay, user_seek, controller) = {
                            let player = this.player.borrow();
                            let renewal = player.renewal.as_ref().unwrap();
                            (
                                renewal.position,
                                renewal.autoplay,
                                renewal.user_seek,
                                player.controller.clone(),
                            )
                        };
                        if let Some(controller) = controller {
                            let revision = controller.snapshot().source_revision.saturating_add(1);
                            let result = controller.renew_source(
                                selection.source.admit_cache_or_online(),
                                position,
                                autoplay,
                            );
                            if result.is_ok() {
                                if user_seek && let Some(position) = position {
                                    let _ = controller.seek(position);
                                }
                                this.player.borrow_mut().renewal.as_mut().unwrap().phase =
                                    Phase::Preparing(revision);
                                if let Some(fill) = selection.fill {
                                    this.queue_cache_fill(fill, autoplay);
                                }
                            } else {
                                this.player.borrow_mut().renewal = None;
                                this.player.borrow_mut().requested = false;
                                this.music_command_result(Some(result));
                            }
                        }
                    }
                    Err(error) => {
                        let mut player = this.player.borrow_mut();
                        player.renewal = None;
                        player.requested = false;
                        drop(player);
                        this.music_failure(error);
                    }
                }
            } else if this
                .player
                .borrow()
                .renewal
                .as_ref()
                .is_some_and(|renewal| renewal.ticket == ticket)
            {
                this.cancel_source_renewal();
            }
            this.render_music_player();
            this.begin_music_resolution();
            this.begin_source_renewal();
        });
    }

    /// This state is paused intent while I/O/preroll runs, never fabricated
    /// Playing. Actual source acknowledgement and the worker clock remain real.
    pub(super) fn resume_presentation(
        &self,
        mut snapshot: Option<PlaybackSnapshot>,
    ) -> Option<PlaybackSnapshot> {
        let player = self.player.borrow();
        if let Some(renewal) = player.renewal.as_ref().filter(|renewal| renewal.active())
            && let Some(snapshot) = &mut snapshot
        {
            snapshot.state = PlaybackState::Paused;
            snapshot.position = renewal.confirmed_position.or(snapshot.position);
            snapshot.last_error = None;
        }
        snapshot
    }

    /// Returns true while normal failure/EOS handling must wait for the bounded
    /// same-occurrence refresh. No queue selection or Wave cycle is generated.
    pub(super) fn tick_source_renewal(self: &Rc<Self>) -> bool {
        let Some(snapshot) = self
            .player
            .borrow()
            .controller
            .as_ref()
            .map(PlaybackController::snapshot)
        else {
            return false;
        };
        let action = {
            let model = self.model.borrow();
            let mut player = self.player.borrow_mut();
            let Some(renewal) = player.renewal.as_mut() else {
                return false;
            };
            if renewal.session != model.generation
                || model.queue.current_occurrence_id() != Some(&renewal.occurrence)
            {
                drop(player);
                drop(model);
                self.cancel_source_renewal();
                return false;
            }
            match renewal.phase {
                Phase::Direct | Phase::Guarding
                    if snapshot.last_error == Some(PlaybackError::SourceExpired)
                        && !renewal.attempted =>
                {
                    1
                }
                Phase::Preparing(revision) if snapshot.source_revision >= revision => {
                    if snapshot.last_error == Some(PlaybackError::CachedSourceUnavailable)
                        && !renewal.cache_retry
                    {
                        renewal.cache_retry = true;
                        2
                    } else if matches!(
                        snapshot.last_error,
                        Some(
                            PlaybackError::NotSeekable
                                | PlaybackError::SeekFailed
                                | PlaybackError::InvalidSeek
                        )
                    ) {
                        3
                    } else if snapshot.state == PlaybackState::Playing {
                        renewal.phase = Phase::Guarding;
                        player.requested = false;
                        0
                    } else if (snapshot.state == PlaybackState::Paused
                        && snapshot.last_error.is_none())
                        || matches!(snapshot.state, PlaybackState::Error | PlaybackState::Ended)
                    {
                        player.renewal = None;
                        player.requested = false;
                        0
                    } else {
                        4
                    }
                }
                Phase::Waiting | Phase::Resolving | Phase::Preparing(_) => 4,
                Phase::Direct if snapshot.state == PlaybackState::Playing => {
                    renewal.phase = Phase::Guarding;
                    0
                }
                Phase::Direct | Phase::Guarding => {
                    if snapshot.last_error.is_some()
                        && snapshot.last_error != Some(PlaybackError::SourceExpired)
                    {
                        player.renewal = None;
                    } else {
                        renewal.position = snapshot.position.or(renewal.position);
                        renewal.confirmed_position =
                            snapshot.position.or(renewal.confirmed_position);
                    }
                    0
                }
            }
        };
        match action {
            1 | 2 => {
                self.queue_source_renewal(action == 2);
                true
            }
            3 => {
                let mut player = self.player.borrow_mut();
                player.renewal = None;
                player.requested = false;
                player.reported_error = snapshot.last_error.map(|error| (player.serial, error));
                let message = "Не удалось восстановить сохранённую позицию. Воспроизведение остаётся на паузе; источник не поддерживает эту перемотку.";
                player.owned_error = Some(message.to_owned());
                drop(player);
                self.widgets
                    .status
                    .set_title("Не удалось продолжить с сохранённого места");
                self.widgets.status.set_subtitle(message);
                self.render_controls();
                true
            }
            4 => true,
            _ => false,
        }
    }
}
