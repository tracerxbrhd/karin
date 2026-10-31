// SPDX-License-Identifier: GPL-3.0-or-later
//! Selection presentation around the accepted player; no transport ownership.

use super::*;
use crate::playback::PlaybackState;

pub(super) struct InfoSelection {
    track: Track,
    context: Option<PlaybackContext>,
    following_current: bool,
}

impl MusicPane {
    pub(in crate::ui) fn track_info_widget(&self) -> gtk::Widget {
        self.controller.track_pane.widget.clone().upcast()
    }

    pub(in crate::ui) fn attach_track_info(&self, split: &adw::OverlaySplitView) {
        self.controller.info_split.replace(Some(split.downgrade()));
        let weak = Rc::downgrade(&self.controller);
        split.connect_collapsed_notify(move |split| {
            if let Some(this) = weak.upgrade() {
                if split.is_collapsed() {
                    let focus = this
                        .window
                        .upgrade()
                        .and_then(|window| gtk::prelude::RootExt::focus(&window));
                    if focus.is_some_and(|focus| {
                        focus == this.track_pane.widget
                            || focus.is_ancestor(&this.track_pane.widget)
                    }) {
                        this.clear_text_focus();
                    }
                    split.set_show_sidebar(false);
                } else {
                    this.sync_info_visibility();
                }
            }
        });
        self.controller.sync_info_visibility();
    }

    pub(in crate::ui) fn preferences_callback(&self) -> Rc<dyn Fn()> {
        let weak = Rc::downgrade(&self.controller);
        Rc::new(move || {
            if let Some(this) = weak.upgrade() {
                this.present_preferences();
            }
        })
    }

    pub(in crate::ui) fn visibility_callback(&self) -> Rc<dyn Fn(bool)> {
        let weak = Rc::downgrade(&self.controller);
        Rc::new(move |visible| {
            if let Some(this) = weak.upgrade() {
                this.music_surface_visible.set(visible);
                this.sync_info_visibility();
            }
        })
    }
}

impl Controller {
    pub(super) fn connect_workspace(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.widgets
            .stack
            .connect_visible_child_name_notify(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.refresh_catalog_notice();
                }
            });
        let weak = Rc::downgrade(self);
        self.widgets.cache_notice.connect_button_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.refresh_catalog();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.current_art_button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.toggle_track_info();
            }
        });
        if let Some(window) = self.window.upgrade() {
            let refresh = gio::SimpleAction::new("music-refresh", None);
            let weak = Rc::downgrade(self);
            refresh.connect_activate(move |_, _| {
                if let Some(this) = weak
                    .upgrade()
                    .filter(|this| this.model.borrow().account_valid)
                {
                    this.refresh_catalog();
                }
            });
            window.add_action(&refresh);
            let action = gio::SimpleAction::new("music-info", None);
            action.set_enabled(false);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(this) = weak.upgrade() {
                    this.toggle_track_info();
                }
            });
            window.add_action(&action);
            if let Some(app) = self.app.upgrade() {
                app.set_accels_for_action("win.music-info", &["<Primary>i"]);
                app.set_accels_for_action("win.music-refresh", &["<Primary>r"]);
            }
        }
    }

    pub(super) fn select_track_info(
        &self,
        track: Track,
        context: Option<PlaybackContext>,
        following_current: bool,
    ) {
        self.info_selection.replace(Some(InfoSelection {
            track,
            context,
            following_current,
        }));
        self.sync_track_pane();
    }

    pub(super) fn sync_track_pane(&self) {
        let (ready, current, context, queue_full) = {
            let model = self.model.borrow();
            (
                model.account_valid && !model.closed,
                model.queue.current().cloned(),
                model.queue.context().cloned(),
                model.queue.items().len() >= 500,
            )
        };
        if !ready {
            self.info_selection.borrow_mut().take();
        } else {
            let follows = self
                .info_selection
                .borrow()
                .as_ref()
                .is_none_or(|selection| selection.following_current);
            if follows {
                self.info_selection
                    .replace(current.clone().map(|track| InfoSelection {
                        track,
                        context,
                        following_current: true,
                    }));
            }
        }
        let selected = self.info_selection.borrow();
        let track = selected.as_ref().map(|selection| &selection.track);
        self.track_pane.set_track(track, self.artwork.as_ref());
        self.track_pane.set_navigation_enabled(ready);
        let is_current = selected.as_ref().is_some_and(|selection| {
            selection.following_current
                && current
                    .as_ref()
                    .is_some_and(|current| current.id == selection.track.id)
        });
        let status = if !is_current {
            "Выбранный трек"
        } else {
            match self.music_snapshot().map(|snapshot| snapshot.state) {
                Some(PlaybackState::Playing) => "Сейчас играет",
                Some(PlaybackState::Paused | PlaybackState::Ready) => "Пауза",
                Some(PlaybackState::Loading) => "Подготовка воспроизведения…",
                Some(PlaybackState::Error) => "Не удалось воспроизвести",
                Some(PlaybackState::Ended) => "Воспроизведение завершено",
                _ => "Остановлено",
            }
        };
        self.track_pane.set_playback(
            status,
            ready && track.is_some_and(|track| track.available),
            ready && track.is_some() && !queue_full,
        );
        let context_label = selected
            .as_ref()
            .and_then(|selection| selection.context.as_ref())
            .map(|context| {
                let source = match context.source_kind() {
                    ContextKind::LikedTracks => "Понравившиеся треки",
                    ContextKind::Album => "Альбом",
                    ContextKind::Playlist => "Плейлист",
                    ContextKind::Search => "Результаты поиска",
                    ContextKind::Artist => "Исполнитель",
                    ContextKind::Queue => "Очередь",
                    ContextKind::SingleTrack => "Выбранный трек",
                };
                format!(
                    "{source} · {} из {}{}",
                    context.current_index() + 1,
                    context.items().len(),
                    if context.is_partial() {
                        " · загруженная часть"
                    } else {
                        ""
                    }
                )
            });
        self.track_pane.set_context(context_label.as_deref());
        let available = ready && track.is_some();
        self.widgets
            .current_art_button
            .set_sensitive(ready && current.is_some());
        if let Some(action) = self
            .window
            .upgrade()
            .and_then(|window| window.lookup_action("music-info"))
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(available);
        }
        drop(selected);
        self.sync_info_visibility();
    }

    pub(super) fn sync_info_visibility(&self) {
        let Some(split) = self
            .info_split
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
        else {
            return;
        };
        let available = {
            let model = self.model.borrow();
            model.account_valid
                && !model.closed
                && self.music_surface_visible.get()
                && self.info_selection.borrow().is_some()
        };
        if !available {
            split.set_show_sidebar(false);
        } else if !split.is_collapsed() {
            split.set_show_sidebar(self.track_info_preference());
        }
    }

    fn toggle_track_info(self: &Rc<Self>) {
        if let Some(track) = self.model.borrow().queue.current().cloned() {
            let context = self.model.borrow().queue.context().cloned();
            self.info_selection.replace(Some(InfoSelection {
                track,
                context,
                following_current: true,
            }));
        }
        self.sync_track_pane();
        if self.info_selection.borrow().is_none() {
            return;
        }
        if let Some(split) = self
            .info_split
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
        {
            let show = !split.shows_sidebar();
            if !split.is_collapsed() {
                self.set_track_info_preference(show);
            }
            split.set_show_sidebar(show);
            if show {
                self.track_pane.close.grab_focus();
            }
        }
    }

    pub(super) fn handle_track_pane_action(self: &Rc<Self>, action: track_pane::PaneAction) {
        use track_pane::PaneAction;
        if matches!(action, PaneAction::Close) {
            self.clear_text_focus();
            self.set_track_info_preference(false);
            if let Some(split) = self
                .info_split
                .borrow()
                .as_ref()
                .and_then(glib::WeakRef::upgrade)
            {
                split.set_show_sidebar(false);
            }
            self.widgets.current_art_button.grab_focus();
            return;
        }
        if !self.model.borrow().account_valid || self.model.borrow().closed {
            return;
        }
        let action_track = match &action {
            PaneAction::Play(track) | PaneAction::Enqueue(track) | PaneAction::Details(track) => {
                Some(track)
            }
            _ => None,
        };
        if action_track.is_some_and(|track| self.track_pane.selected().as_ref() != Some(track)) {
            return;
        }
        let context = self
            .info_selection
            .borrow()
            .as_ref()
            .and_then(|selection| selection.context.clone());
        if matches!(
            action,
            PaneAction::Details(_) | PaneAction::Album(_) | PaneAction::Artist(_)
        ) && let Some(split) = self
            .info_split
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
            && split.is_collapsed()
        {
            self.clear_text_focus();
            split.set_show_sidebar(false);
        }
        match action {
            PaneAction::Close => {}
            PaneAction::Play(track) if track.available => {
                if let Some(selection) = self.info_selection.borrow_mut().as_mut() {
                    selection.following_current = true;
                }
                let context = context.or_else(|| {
                    PlaybackContext::new(ContextKind::SingleTrack, None, vec![track], 0, false).ok()
                });
                if let Some(context) = context {
                    self.play_music_context(context);
                }
            }
            PaneAction::Play(_) => {}
            PaneAction::Enqueue(track) => self.enqueue(track),
            PaneAction::Details(track) => self.open_track_context(track.id, context),
            PaneAction::Album(id) => self.open_album(id),
            PaneAction::Artist(artist) => self.open_artist(artist, 0),
        }
    }

    pub(super) fn handle_playlist_action(self: &Rc<Self>, action: playlist_grid::PlaylistAction) {
        if !self.model.borrow().account_valid || self.model.borrow().closed {
            return;
        }
        match action {
            playlist_grid::PlaylistAction::Open(playlist) => self.open_playlist(playlist, 0),
            playlist_grid::PlaylistAction::Create
                if self.can_create_playlist.get() && self.widgets.library_mode.selected() == 1 =>
            {
                self.present_create_playlist()
            }
            playlist_grid::PlaylistAction::Create => {}
            playlist_grid::PlaylistAction::Retry => {
                let page = self.model.borrow().library_page;
                self.load_library_refresh(page);
            }
        }
    }

    pub(super) fn clear_workspace(&self) {
        let dialog = self.playlist_dialog.borrow_mut().take();
        if let Some(dialog) = dialog {
            dialog.close();
        }
        self.info_selection.borrow_mut().take();
        self.track_pane.set_track(None, self.artwork.as_ref());
        self.playlist_grid.clear();
        self.widgets.cache_notice.set_revealed(false);
        self.widgets.cache_hint.set_visible(false);
        self.detail_cache.set(None);
        self.detail_refresh.borrow_mut().take();
        self.catalog.borrow_mut().take();
        self.sync_info_visibility();
    }
}

#[cfg(test)]
mod tests;
