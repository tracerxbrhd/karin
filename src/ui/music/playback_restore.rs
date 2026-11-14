// SPDX-License-Identifier: GPL-3.0-or-later
//! Account-bound continuation of the ordinary queue. This never prepares audio.
use super::*;
use crate::{
    cache::AccountScope,
    playback_session::{LoadStatus, PlaybackSessionStore, PlaybackSnapshot, SaveTicket},
};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

const SAVE_INTERVAL: Duration = Duration::from_secs(5);

pub(super) struct State {
    store: Option<PlaybackSessionStore>,
    scope: Option<AccountScope>,
    generation: u64,
    loading: bool,
    restore_disallowed: bool,
    writes: usize,
    closing: bool,
    suppressed: bool,
    checked: Instant,
    event_due: Option<Instant>,
    last: Option<PlaybackSnapshot>,
    write_retry: bool,
    pub(super) message: Option<&'static str>,
    pub(super) retry: bool,
    delete_retry: bool,
    deletion: Option<(u64, mpsc::Receiver<Result<(), ()>>)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            store: None,
            scope: None,
            generation: 0,
            loading: false,
            restore_disallowed: false,
            writes: 0,
            closing: false,
            suppressed: false,
            checked: Instant::now(),
            event_due: None,
            last: None,
            write_retry: false,
            message: None,
            retry: false,
            delete_retry: false,
            deletion: None,
        }
    }
}
impl State {
    pub(super) fn loading(&self) -> bool {
        self.loading
    }
}

pub(super) enum Deletion {
    Account {
        store: PlaybackSessionStore,
        ticket: SaveTicket,
    },
    UnknownAccount(PlaybackSessionStore),
}
impl Deletion {
    pub(super) fn run(self) -> Result<(), ()> {
        match self {
            Self::UnknownAccount(store) => store.clear_all().map_err(|_| ()),
            Self::Account { store, ticket } => store
                .save_if_current(&ticket, None)
                .and_then(|current| {
                    current
                        .then_some(())
                        .ok_or(crate::playback_session::SnapshotError::Superseded)
                })
                .map_err(|_| ()),
        }
    }
}

impl Controller {
    pub(super) fn bind_playback_account(&self, scope: AccountScope) {
        let mut p = self.persistence.borrow_mut();
        p.playback.scope = Some(scope);
        p.playback.suppressed = false;
    }

    /// Retires UI work before token/account replacement. No disk I/O occurs here.
    pub(super) fn retire_playback_restore(&self) {
        let mut p = self.persistence.borrow_mut();
        let state = &mut p.playback;
        state.generation = state.generation.wrapping_add(1);
        state.loading = false;
        state.restore_disallowed = false;
        state.last = None;
        state.message = None;
        state.retry = false;
        state.delete_retry = false;
        state.suppressed = true;
        if let Some(store) = &state.store {
            let _ = store.invalidate();
        }
    }

    pub(super) fn start_playback_restore(self: &Rc<Self>, scope: AccountScope) {
        {
            let model = self.model.borrow();
            if !model.account_valid || !model.queue.items().is_empty() {
                return;
            }
        }
        let (store, generation, serial) = {
            let mut p = self.persistence.borrow_mut();
            if !p.enabled
                || !p.ready
                || p.closed
                || p.values.forget_pending
                || !p.values.remember_login
                || p.playback.closing
                || p.playback.loading
                || p.playback.restore_disallowed
            {
                return;
            }
            let store = match p
                .playback
                .store
                .clone()
                .map(Ok)
                .unwrap_or_else(PlaybackSessionStore::for_application)
            {
                Ok(store) => store,
                Err(_) => {
                    p.playback.message = Some(
                        "Не удалось открыть сохранённую очередь. Текущий сеанс работает без восстановления.",
                    );
                    p.playback.retry = true;
                    drop(p);
                    self.render_persistence();
                    return;
                }
            };
            p.playback.store = Some(store.clone());
            p.playback.scope = Some(scope.clone());
            p.playback.generation = p.playback.generation.wrapping_add(1);
            p.playback.loading = true;
            p.playback.suppressed = false;
            p.playback.retry = false;
            p.playback.message = Some("Проверяем сохранённую очередь…");
            (store, p.playback.generation, self.music_serial())
        };
        let (token, service, session) = {
            let model = self.model.borrow();
            let Some(token) = model.token.clone().filter(|token| !token.is_expired()) else {
                let mut p = self.persistence.borrow_mut();
                p.playback.loading = false;
                p.playback.message = None;
                return;
            };
            let Some(service) = self.service.clone() else {
                let mut p = self.persistence.borrow_mut();
                p.playback.loading = false;
                p.playback.message = None;
                return;
            };
            (token, service, model.generation)
        };
        self.render_persistence();
        let expected_scope = scope.clone();
        self.persistence_job(
            move || {
                let loaded = store.load(&scope).map_err(|_| ())?;
                let status = loaded.status;
                let Some(snapshot) = loaded.snapshot else { return Ok::<_, ()>((status, None)); };
                let tracks = service.restore_tracks(&token, &snapshot.ids()).map_err(|_| ())?;
                let restored = snapshot.restore(&tracks).map_err(|_| ())?;
                Ok::<_, ()>((status, Some((snapshot, restored))))
            },
            move |this, result| {
                {
                    let mut p = this.persistence.borrow_mut();
                    if p.playback.generation != generation || p.playback.scope.as_ref() != Some(&expected_scope) {
                        return;
                    }
                    p.playback.loading = false;
                    p.playback.message = None;
                }
                // An explicit user selection wins over a slow restore, including
                // a new selection of the same service ID at another occurrence.
                let admissible = {
                    let model = this.model.borrow();
                    model.generation == session && model.account_valid && model.queue.items().is_empty()
                        && this.music_serial() == serial
                        && this.persistence.borrow().scope.as_ref() == Some(&expected_scope)
                };
                if !admissible { this.render_persistence(); return; }
                match result {
                    Ok(Ok((status, outcome))) => {
                        let mut p = this.persistence.borrow_mut();
                        p.playback.retry = false;
                        p.playback.checked = Instant::now();
                        if let Some((snapshot, restored)) = outcome {
                            let collection = matches!(snapshot.origin(),
                                crate::queue::PlaybackOrigin::LikedTracks
                                | crate::queue::PlaybackOrigin::Playlist { .. }
                                | crate::queue::PlaybackOrigin::Album { .. }
                                | crate::queue::PlaybackOrigin::Artist { .. });
                            p.playback.last = Some(snapshot);
                            let position = restored.position;
                            let omitted = restored.omitted;
                            let empty = restored.queue.items().is_empty();
                            p.playback.message = Some(if empty {
                                "Сохранённые треки больше недоступны. Выберите музыку заново."
                            } else if restored.current_changed {
                                "Прежний трек недоступен. Восстановлена сохранённая очередь с другим выбранным треком."
                            } else if omitted != 0 {
                                "Восстановлена сохранённая очередь. Недоступные треки пропущены; состав исходного раздела мог измениться."
                            } else if collection {
                                "Восстановлена сохранённая очередь. Нажмите «Воспроизвести», чтобы продолжить; состав исходного раздела мог измениться."
                            } else {
                                "Восстановлена сохранённая очередь. Нажмите «Воспроизвести», чтобы продолжить."
                            });
                            drop(p);
                            this.model.borrow_mut().queue = restored.queue;
                            this.present_restored_playback(position);
                            this.follow_current_occurrence();
                            this.render_queue();
                            this.render_music_player();
                        } else {
                            p.playback.last = None;
                            p.playback.message = (status == LoadStatus::RecoveredCorrupt).then_some(
                                "Повреждённая сохранённая очередь пропущена. Текущий сеанс работает как обычно.");
                        }
                    }
                    _ => {
                        let mut p = this.persistence.borrow_mut();
                        p.playback.message = Some("Не удалось проверить сохранённую очередь. Повторите восстановление или выберите музыку заново.");
                        p.playback.retry = true;
                    }
                }
                this.render_persistence();
            },
        );
    }

    pub(super) fn retry_playback_restore(self: &Rc<Self>) {
        if self.persistence.borrow().playback.delete_retry {
            self.stop_playback_continuation();
            return;
        }
        let scope = self.persistence.borrow().scope.clone();
        if let Some(scope) = scope {
            self.persistence.borrow_mut().playback.restore_disallowed = false;
            self.start_playback_restore(scope);
        }
    }

    /// Reserves the durable deletion before logout clears the current account.
    pub(super) fn prepare_forget_playback(
        &self,
        forget_unknown_account: bool,
    ) -> Result<Option<Deletion>, ()> {
        self.retire_playback_restore();
        let mut p = self.persistence.borrow_mut();
        let state = &mut p.playback;
        let scope = state.scope.clone();
        if scope.is_none() && !forget_unknown_account {
            return Ok(None);
        }
        let store = state
            .store
            .clone()
            .map(Ok)
            .unwrap_or_else(PlaybackSessionStore::for_application)
            .map_err(|_| ())?;
        state.store = Some(store.clone());
        if let Some(scope) = scope {
            let ticket = store.reserve_save(&scope).map_err(|_| ())?;
            Ok(Some(Deletion::Account { store, ticket }))
        } else {
            store.invalidate().map_err(|_| ())?;
            Ok(Some(Deletion::UnknownAccount(store)))
        }
    }

    pub(super) fn allow_playback_continuation(&self) {
        self.note_playback_intent();
        {
            let mut p = self.persistence.borrow_mut();
            p.playback.generation = p.playback.generation.wrapping_add(1);
            p.playback.suppressed = false;
            p.playback.message = None;
            p.playback.delete_retry = false;
            p.playback.retry = false;
        }
        self.render_persistence();
    }

    /// Explicit queue/mode intent supersedes an asynchronous startup read.
    /// The bounded debounce coalesces keyboard repeats and manual queue edits.
    pub(super) fn note_playback_intent(&self) {
        let mut p = self.persistence.borrow_mut();
        let cancelled = p.playback.loading || (p.playback.retry && !p.playback.delete_retry);
        p.playback.restore_disallowed = true;
        if cancelled {
            p.playback.generation = p.playback.generation.wrapping_add(1);
            p.playback.loading = false;
            p.playback.retry = false;
            p.playback.message = None;
        }
        p.playback.event_due = Some(Instant::now() + Duration::from_millis(750));
        drop(p);
        if cancelled {
            self.render_persistence();
        }
    }

    /// Explicit Stop deletes continuation; it does not destroy the visible queue.
    pub(super) fn stop_playback_continuation(&self) {
        {
            let p = self.persistence.borrow();
            if p.playback.closing || (p.playback.suppressed && p.playback.deletion.is_some()) {
                return;
            }
        }
        match self.prepare_forget_playback(false) {
            Ok(Some(task)) => {
                let (tx, rx) = mpsc::channel();
                let mut p = self.persistence.borrow_mut();
                p.playback.deletion = Some((p.playback.generation, rx));
                p.playback.writes += 1;
                drop(p);
                self.playback_storage_job(move || {
                    let _ = tx.send(task.run());
                });
            }
            Ok(None) => {}
            Err(()) => {
                let mut p = self.persistence.borrow_mut();
                p.playback.message = Some(
                    "Не удалось удалить сохранённую позицию. Повторите очистку сохранённого входа в настройках.",
                );
                p.playback.delete_retry = true;
                p.playback.retry = true;
            }
        }
        self.render_persistence();
    }

    pub(super) fn tick_playback_restore(self: &Rc<Self>) {
        let deletion = {
            let p = self.persistence.borrow();
            p.playback
                .deletion
                .as_ref()
                .and_then(|(generation, rx)| match rx.try_recv() {
                    Ok(result) => Some((*generation, result)),
                    Err(mpsc::TryRecvError::Disconnected) => Some((*generation, Err(()))),
                    Err(mpsc::TryRecvError::Empty) => None,
                })
        };
        if let Some((generation, result)) = deletion {
            let mut p = self.persistence.borrow_mut();
            p.playback.deletion = None;
            p.playback.writes = p.playback.writes.saturating_sub(1);
            if result.is_err() {
                p.playback.write_retry = true;
            }
            if generation == p.playback.generation && result.is_err() {
                p.playback.message =
                    Some("Не удалось удалить сохранённую позицию. Повторите очистку.");
                p.playback.delete_retry = true;
                p.playback.retry = true;
            }
            drop(p);
            self.render_persistence();
        }
        let due = {
            let mut p = self.persistence.borrow_mut();
            if p.playback.checked.elapsed() < SAVE_INTERVAL
                && !p
                    .playback
                    .event_due
                    .is_some_and(|due| Instant::now() >= due)
            {
                false
            } else {
                p.playback.checked = Instant::now();
                p.playback.event_due = None;
                true
            }
        };
        if due {
            self.save_playback_continuation(false);
        }
    }

    fn snapshot_for_save(&self) -> Result<Option<PlaybackSnapshot>, ()> {
        let model = self.model.borrow();
        let snapshot = self.music_snapshot();
        let actual = snapshot.as_ref().and_then(|snapshot| snapshot.position);
        let position = if snapshot.as_ref().is_some_and(|snapshot| {
            matches!(
                snapshot.state,
                crate::playback::PlaybackState::Playing
                    | crate::playback::PlaybackState::Paused
                    | crate::playback::PlaybackState::Ended
            )
        }) {
            actual.or_else(|| self.continuation_position())
        } else {
            self.continuation_position().or(actual)
        }
        .unwrap_or_default();
        PlaybackSnapshot::capture(&model.queue, position).map_err(|_| ())
    }

    pub(super) fn save_playback_continuation(self: &Rc<Self>, force: bool) {
        if !self.model.borrow().account_valid {
            return;
        }
        let Ok(snapshot) = self.snapshot_for_save() else {
            self.persistence.borrow_mut().playback.message =
                Some("Текущая очередь не может быть сохранена. Воспроизведение продолжается.");
            self.render_persistence();
            return;
        };
        let task = {
            let mut p = self.persistence.borrow_mut();
            if !p.enabled
                || !p.ready
                || p.closed
                || p.values.forget_pending
                || !p.values.remember_login
                || p.playback.closing
                || p.playback.loading
                || p.playback.suppressed
                || p.playback.retry
            {
                return;
            }
            if p.playback.writes != 0
                || (!force && !p.playback.write_retry && p.playback.last == snapshot)
            {
                return;
            }
            let Some(scope) = p.scope.clone() else {
                return;
            };
            let store = match p
                .playback
                .store
                .clone()
                .map(Ok)
                .unwrap_or_else(PlaybackSessionStore::for_application)
            {
                Ok(value) => value,
                Err(_) => return,
            };
            let Ok(ticket) = store.reserve_save(&scope) else {
                return;
            };
            p.playback.store = Some(store.clone());
            p.playback.scope = Some(scope);
            p.playback.last = snapshot.clone();
            p.playback.write_retry = false;
            p.playback.writes += 1;
            (store, ticket, p.playback.generation)
        };
        self.persistence_job(move || task.0.save_if_current(&task.1, snapshot.as_ref()), move |this, result| {
            let mut p = this.persistence.borrow_mut();
            p.playback.writes = p.playback.writes.saturating_sub(1);
            if !matches!(result, Ok(Ok(_))) { p.playback.write_retry = true; }
            if p.playback.generation != task.2 { return; }
            if !matches!(result, Ok(Ok(_))) {
                p.playback.last = None;
                p.playback.message = Some("Не удалось сохранить очередь. Воспроизведение продолжается; следующая попытка будет выполнена автоматически.");
            }
            drop(p);
            this.render_persistence();
        });
    }

    /// Capture before stop_engine/queue.clear; the application hold and work
    /// fence keep this transaction alive after the last GTK window closes.
    pub(super) fn close_playback_restore(&self) {
        let snapshot = self.snapshot_for_save();
        let confirmation = self.pending_playback_save_confirmation();
        let task = {
            let mut p = self.persistence.borrow_mut();
            p.playback.closing = true;
            p.playback.generation = p.playback.generation.wrapping_add(1);
            if !self.model.borrow().account_valid
                || !p.enabled
                || !p.ready
                || (p.values.forget_pending && confirmation.is_none())
                || !p.values.remember_login
                || p.playback.suppressed
                || p.playback.loading
                || p.playback.retry
            {
                return;
            }
            let Some(scope) = p.scope.clone() else {
                return;
            };
            let Ok(snapshot) = snapshot else {
                return;
            };
            // An empty queue during interactive credential setup has not
            // replaced the previous continuation. A user-selected ordinary
            // queue, however, must survive a fast close after that save wins.
            if confirmation.is_some() && snapshot.is_none() {
                return;
            }
            let store = match p
                .playback
                .store
                .clone()
                .map(Ok)
                .unwrap_or_else(PlaybackSessionStore::for_application)
            {
                Ok(value) => value,
                Err(_) => return,
            };
            let Ok(ticket) = store.reserve_save(&scope) else {
                return;
            };
            p.playback.scope = Some(scope);
            (store, ticket, snapshot)
        };
        self.confirmed_playback_storage_job(confirmation, move || {
            let _ = task.0.save_if_current(&task.1, task.2.as_ref());
        });
    }

    #[cfg(test)]
    pub(super) fn playback_storage_idle_for_test(&self) -> bool {
        let p = self.persistence.borrow();
        !p.playback.loading && p.playback.writes == 0
    }
    #[cfg(test)]
    pub(super) fn flush_playback_for_test(self: &Rc<Self>) {
        self.save_playback_continuation(true);
    }
    #[cfg(test)]
    pub(super) fn restored_playback_position_for_test(&self) -> Option<Duration> {
        self.continuation_position()
    }
    #[cfg(test)]
    pub(super) fn seed_playback_write_retry_for_test(&self) {
        self.persistence.borrow_mut().playback.write_retry = true;
    }
    #[cfg(test)]
    pub(super) fn playback_restore_retry_for_test(&self) -> bool {
        let p = self.persistence.borrow();
        p.playback.retry && !p.playback.delete_retry
    }
}
