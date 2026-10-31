// SPDX-License-Identifier: GPL-3.0-or-later
//! Selection presentation around the accepted player; no transport ownership.

use super::*;
use crate::playback::PlaybackState;

pub(super) struct InfoSelection {
    track: Track,
    context: Option<PlaybackContext>,
    target: InfoTarget,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InfoTarget {
    FollowingCurrent,
    ExplicitSelection,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct CurrentOccurrence {
    kind: ContextKind,
    source: Option<String>,
    index: usize,
    id: String,
}
impl CurrentOccurrence {
    fn from_context(context: &PlaybackContext) -> Self {
        Self {
            kind: context.source_kind(),
            source: context.source_id().map(str::to_owned),
            index: context.current_index(),
            id: context.items()[context.current_index()].id.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InfoPresentation {
    Wide,
    Medium,
    Compact,
}

/// The controller owns intent/target; these weak native references own no music
/// state. GTK Revealer has no backdrop or outside-click dismissal policy.
pub(super) struct InfoHost {
    overlay: glib::WeakRef<gtk::Overlay>,
    reveal: glib::WeakRef<gtk::Revealer>,
    content: glib::WeakRef<gtk::Widget>,
}
impl InfoHost {
    pub(super) fn revealer(&self) -> Option<gtk::Revealer> {
        self.reveal.upgrade()
    }
    pub(super) fn content(&self) -> Option<gtk::Widget> {
        self.content.upgrade()
    }
    pub(super) fn is_open(&self) -> bool {
        self.reveal
            .upgrade()
            .is_some_and(|view| view.reveals_child())
    }
    pub(super) fn presentation(&self) -> InfoPresentation {
        match self.reveal.upgrade().map(|view| view.width_request()) {
            Some(-1) => InfoPresentation::Compact,
            Some(300) => InfoPresentation::Medium,
            _ => InfoPresentation::Wide,
        }
    }
}

impl MusicPane {
    pub(in crate::ui) fn track_info_widget(&self) -> gtk::Widget {
        self.controller.track_pane.widget.clone().upcast()
    }

    pub(in crate::ui) fn attach_track_info(
        &self,
        overlay: &gtk::Overlay,
        reveal: &gtk::Revealer,
        content: &gtk::Widget,
    ) {
        self.controller.info_host.replace(Some(InfoHost {
            overlay: overlay.downgrade(),
            reveal: reveal.downgrade(),
            content: content.downgrade(),
        }));
        let weak = Rc::downgrade(&self.controller);
        reveal.connect_width_request_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.sync_info_visibility();
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
                this.sync_track_pane();
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
                this.show_current_track_info();
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
        self.info_observed.replace(
            self.model
                .borrow()
                .queue
                .context()
                .map(CurrentOccurrence::from_context),
        );
        self.info_selection.replace(Some(InfoSelection {
            track,
            context,
            target: if following_current {
                InfoTarget::FollowingCurrent
            } else {
                InfoTarget::ExplicitSelection
            },
        }));
        self.sync_track_pane();
    }

    pub(super) fn info_target_epoch(&self) -> u64 {
        self.info_target_epoch.get()
    }

    pub(super) fn select_track_info_if_current(
        &self,
        epoch: u64,
        track: Track,
        context: Option<PlaybackContext>,
    ) -> bool {
        if epoch != self.info_target_epoch.get()
            || !self.model.borrow().account_valid
            || self.model.borrow().closed
        {
            return false;
        }
        self.select_track_info(track, context, false);
        true
    }

    /// Called by existing playback selection paths, never by pause/seek/volume.
    /// A replacement/selection can identify a new occurrence even at the same ID.
    pub(super) fn follow_current_occurrence(&self) {
        let (track, context) = {
            let model = self.model.borrow();
            (
                model.queue.current().cloned(),
                model.queue.context().cloned(),
            )
        };
        self.info_target_epoch
            .set(self.info_target_epoch.get().wrapping_add(1));
        self.info_observed
            .replace(context.as_ref().map(CurrentOccurrence::from_context));
        self.info_selection
            .replace(track.map(|track| InfoSelection {
                track,
                context,
                target: InfoTarget::FollowingCurrent,
            }));
    }

    /// Removing another queue row can reindex the same playing occurrence.
    /// A structural queue edit is not a new playback selection.
    pub(super) fn observe_current_occurrence(&self) {
        self.info_observed.replace(
            self.model
                .borrow()
                .queue
                .context()
                .map(CurrentOccurrence::from_context),
        );
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
            self.info_observed.borrow_mut().take();
        } else {
            let observed = context.as_ref().map(CurrentOccurrence::from_context);
            if *self.info_observed.borrow() != observed {
                self.follow_current_occurrence();
            }
            let follows = self
                .info_selection
                .borrow()
                .as_ref()
                .is_none_or(|selection| selection.target == InfoTarget::FollowingCurrent);
            if follows {
                self.info_selection
                    .replace(current.clone().map(|track| InfoSelection {
                        track,
                        context,
                        target: InfoTarget::FollowingCurrent,
                    }));
            }
        }
        let selected = self.info_selection.borrow();
        let track = selected.as_ref().map(|selection| &selection.track);
        self.track_pane.set_track(track, self.artwork.as_ref());
        self.track_pane.set_navigation_enabled(ready);
        let is_current = selected.as_ref().is_some_and(|selection| {
            selection.target == InfoTarget::FollowingCurrent
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
        let available = ready && track.is_some() && self.music_surface_visible.get();
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
        let host = self.info_host.borrow();
        let Some(host) = host.as_ref() else {
            return;
        };
        let (Some(reveal), Some(content), Some(_overlay)) =
            (host.revealer(), host.content(), host.overlay.upgrade())
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
        let open = available && self.track_info_preference();
        let mode = host.presentation();
        let hide_content = open && mode == InfoPresentation::Compact;
        // Clear focus before hiding its native subtree. Besides leaving a
        // usable keyboard destination, this avoids GTK4.14 retaining an old
        // pending focus-move reference over repeated compact/Diagnostics
        // transitions. The outer ToolbarView unmap guard cannot see these
        // independently hidden children.
        let entering_compact = content.is_visible() && hide_content;
        let footer_had_focus = entering_compact
            && self
                .window
                .upgrade()
                .and_then(|window| gtk::prelude::GtkWindowExt::focus(&window))
                .zip(self.widgets.player_main.parent())
                .is_some_and(|(focus, footer)| {
                    focus.is_mapped() && (focus == footer || focus.is_ancestor(&footer))
                });
        if entering_compact {
            #[cfg(test)]
            if let Some(window) = self.window.upgrade() {
                let focus = gtk::prelude::GtkWindowExt::focus(&window);
                eprintln!(
                    "Karin synthetic information focus: phase=before-compact footer={footer_had_focus} kind={} name={} in-content={}",
                    focus.as_ref().map_or("none", |focus| focus.type_().name()),
                    focus
                        .as_ref()
                        .map(|focus| focus.widget_name())
                        .as_deref()
                        .unwrap_or("none"),
                    focus.is_some_and(|focus| focus == content || focus.is_ancestor(&content))
                );
            }
            self.clear_info_focus_within(&content);
        }
        let focus_compact = entering_compact && !footer_had_focus;
        if (self.track_pane.widget.is_visible() && !available) || (reveal.reveals_child() && !open)
        {
            self.clear_info_focus_within(self.track_pane.widget.upcast_ref());
        }
        self.track_pane.widget.set_visible(available);
        self.track_pane
            .set_compact(mode == InfoPresentation::Compact);
        reveal.set_halign(if mode == InfoPresentation::Compact {
            gtk::Align::Fill
        } else {
            gtk::Align::End
        });
        reveal.set_hexpand(mode == InfoPresentation::Compact);
        reveal.set_visible(available);
        reveal.set_can_target(open);
        // Reserve room for every persistent desktop pane. A medium overlay
        // without a reservation covered row menus and transport-side actions.
        // Compact is an explicit subview with the same retained pane.
        content.set_margin_end(match (open, mode) {
            (true, InfoPresentation::Wide) => 340,
            (true, InfoPresentation::Medium) => 316,
            _ => 0,
        });
        content.set_visible(!hide_content);
        if host.is_open() != open {
            reveal.set_reveal_child(open);
        }
        if focus_compact {
            self.focus_compact_information(&reveal, &content);
        }
    }

    fn focus_compact_information(&self, reveal: &gtk::Revealer, content: &gtk::Widget) {
        let window = self.window.clone();
        let reveal = reveal.downgrade();
        let content = content.downgrade();
        let close = self.track_pane.close.downgrade();
        glib::spawn_future_local(async move {
            // A breakpoint can hide the old tree before the subview has a
            // native allocation. Keep only weak references during this bounded
            // request; a later close, resize, dialog or user focus wins.
            for _ in 0..20 {
                glib::timeout_future(std::time::Duration::from_millis(25)).await;
                let (Some(window), Some(reveal), Some(content), Some(close)) = (
                    window.upgrade(),
                    reveal.upgrade(),
                    content.upgrade(),
                    close.upgrade(),
                ) else {
                    return;
                };
                if !window.is_mapped()
                    || window.visible_dialog().is_some()
                    || !reveal.is_visible()
                    || !reveal.reveals_child()
                    || reveal.width_request() != -1
                    || content.is_visible()
                {
                    return;
                }
                let focus = gtk::prelude::GtkWindowExt::focus(&window);
                #[cfg(test)]
                eprintln!(
                    "Karin synthetic information focus: phase=wait-compact kind={} name={} mapped={} in-pane={} revealed={}",
                    focus.as_ref().map_or("none", |focus| focus.type_().name()),
                    focus
                        .as_ref()
                        .map(|focus| focus.widget_name())
                        .as_deref()
                        .unwrap_or("none"),
                    focus.as_ref().is_some_and(|focus| focus.is_mapped()),
                    focus
                        .as_ref()
                        .is_some_and(|focus| focus == reveal.upcast_ref::<gtk::Widget>()
                            || focus.is_ancestor(&reveal)),
                    reveal.is_child_revealed()
                );
                // GTK can automatically assign HeaderBar focus while the old
                // content disappears. That is not deliberate input and must
                // not cancel this one transition request. An already focused
                // pane control remains valid; pre-transition footer focus was
                // excluded before scheduling the request.
                if focus.is_some_and(|focus| {
                    focus.is_mapped()
                        && focus.is_sensitive()
                        && (focus == *reveal.upcast_ref::<gtk::Widget>()
                            || focus.is_ancestor(&reveal))
                }) {
                    return;
                }
                if reveal.is_child_revealed()
                    && close.is_mapped()
                    && close.is_sensitive()
                    && close.grab_focus()
                {
                    return;
                }
            }
        });
    }

    fn clear_info_focus_within(&self, subtree: &gtk::Widget) -> bool {
        let Some(window) = self.window.upgrade() else {
            return false;
        };
        let inside = gtk::prelude::GtkWindowExt::focus(&window)
            .is_some_and(|focus| focus == *subtree || focus.is_ancestor(subtree));
        if inside {
            gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
        }
        inside
    }

    pub(super) fn set_info_open(self: &Rc<Self>, open: bool) {
        self.set_track_info_preference(open);
        self.sync_info_visibility();
    }

    #[cfg(test)]
    pub(super) fn info_is_open(&self) -> bool {
        self.info_host
            .borrow()
            .as_ref()
            .is_some_and(InfoHost::is_open)
    }

    /// The footer cover shows current information; it is not an outside-click
    /// dismissal or a second close control. Ctrl+I retains the explicit toggle.
    fn show_current_track_info(self: &Rc<Self>) {
        if !self.music_surface_visible.get() || self.model.borrow().queue.current().is_none() {
            return;
        }
        self.follow_current_occurrence();
        self.sync_track_pane();
        if self.info_selection.borrow().is_none() {
            return;
        }
        self.set_info_open(true);
        self.track_pane.close.grab_focus();
    }

    fn toggle_track_info(self: &Rc<Self>) {
        if !self.music_surface_visible.get() {
            return;
        }
        self.sync_track_pane();
        if self.info_selection.borrow().is_none() {
            return;
        }
        let show = !self.track_info_preference();
        if show && self.model.borrow().queue.current().is_some() {
            self.follow_current_occurrence();
            self.sync_track_pane();
        }
        self.set_info_open(show);
        if show {
            self.track_pane.close.grab_focus();
        } else {
            self.return_information_focus();
        }
    }

    fn return_information_focus(&self) {
        if !self.widgets.current_art_button.grab_focus() {
            // Explicit metadata may exist before a queue/current cover does.
            // Return to the first real control of the visible music page.
            self.widgets
                .stack
                .child_focus(gtk::DirectionType::TabForward);
        }
    }

    pub(super) fn handle_track_pane_action(self: &Rc<Self>, action: track_pane::PaneAction) {
        use track_pane::PaneAction;
        if matches!(action, PaneAction::Close) {
            self.clear_text_focus();
            self.set_info_open(false);
            self.return_information_focus();
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
        match action {
            PaneAction::Close => {}
            PaneAction::Play(track) if track.available => {
                if let Some(selection) = self.info_selection.borrow_mut().as_mut() {
                    selection.target = InfoTarget::FollowingCurrent;
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
        self.info_observed.borrow_mut().take();
        self.info_target_epoch
            .set(self.info_target_epoch.get().wrapping_add(1));
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
