// SPDX-License-Identifier: GPL-3.0-or-later
//! Confirmed account reactions and occurrence-bound use of the existing Next.

use super::*;
use crate::music::interactions::{InteractionError, TrackReaction, TrackReactions};
use interactions::{interaction_diagnostic, interaction_message, like_icon_name};

/// A reaction belongs to the playback cycle selected when the user clicked.
/// The opaque occurrence distinguishes repeated Track IDs and survives queue
/// reindexing; the serial also fences Stop/restart or leaving and returning to
/// the same occurrence. This token is never persisted or sent to the service.
#[derive(PartialEq, Eq)]
pub(super) struct ReactionPlaybackTarget {
    generation: u64,
    occurrence: crate::queue::OccurrenceId,
    serial: u64,
}

pub(super) fn confirmed_dislike_advances(
    kind: TrackReaction,
    enabled: bool,
    result: Result<ReactionRead, InteractionError>,
    submitted: Option<&ReactionPlaybackTarget>,
    current: Option<&ReactionPlaybackTarget>,
) -> bool {
    kind == TrackReaction::Dislike
        && enabled
        && matches!(
            result,
            Ok(ReactionRead::Both(TrackReactions {
                liked: false,
                disliked: true,
            }))
        )
        && submitted.is_some()
        && submitted == current
}

struct ReactionView {
    buttons: [glib::WeakRef<gtk::Button>; 2],
    icons: [glib::WeakRef<gtk::Image>; 2],
    spinners: [glib::WeakRef<gtk::Spinner>; 2],
    feedback: glib::WeakRef<gtk::MenuButton>,
    refresh: glib::WeakRef<gtk::Button>,
    message: glib::WeakRef<gtk::Label>,
}

pub(super) struct ReactionControls {
    generation: u64,
    id: String,
    views: RefCell<Vec<ReactionView>>,
    initialized: Cell<bool>,
    supported: Cell<bool>,
    presentation: Cell<ReactionPresentation>,
}

#[derive(Clone, Copy)]
enum ReactionPresentation {
    Unknown {
        message: &'static str,
        retry: bool,
        animate: bool,
    },
    Confirmed {
        state: ReactionRead,
        mutable: bool,
    },
}

#[derive(Clone, Copy)]
pub(super) enum ReactionRead {
    Both(TrackReactions),
    LikeOnly(bool, InteractionError),
}

fn reaction_action(kind: TrackReaction, marked: bool) -> &'static str {
    match (kind, marked) {
        (TrackReaction::Like, false) => "Поставить отметку «Нравится»",
        (TrackReaction::Like, true) => "Убрать отметку «Нравится»",
        (TrackReaction::Dislike, false) => "Поставить отметку «Не нравится»",
        (TrackReaction::Dislike, true) => "Убрать отметку «Не нравится»",
    }
}

fn dislike_icon_name() -> &'static str {
    "karin-thumb-down-symbolic"
}

impl ReactionView {
    fn apply(&self, presentation: ReactionPresentation, supported: bool) {
        match presentation {
            ReactionPresentation::Unknown {
                message,
                retry,
                animate,
            } => {
                self.unknown(message, retry, animate, supported);
            }
            ReactionPresentation::Confirmed { state, mutable } => match state {
                ReactionRead::Both(state) => {
                    self.render(state.liked, Some(state.disliked), mutable)
                }
                ReactionRead::LikeOnly(liked, _) => self.like_confirmed(liked, mutable),
            },
        }
    }

    fn unknown(&self, message: &str, retry: bool, animate: bool, supported: bool) {
        for index in 0..2 {
            if let Some(button) = self.buttons[index].upgrade() {
                button.set_sensitive(false);
                button.remove_css_class("liked");
                button.update_state(&[gtk::accessible::State::Pressed(
                    gtk::AccessibleTristate::Mixed,
                )]);
                button.set_tooltip_text(Some("Сначала обновите состояние отметок"));
                button.update_property(&[gtk::accessible::Property::Label(if index == 0 {
                    "Отметка «Нравится» не проверена"
                } else {
                    "Отметка «Не нравится» не проверена"
                })]);
                if index == 1 {
                    // An unknown/refused Dislike contract never becomes an
                    // apparently functional control. Previously confirmed
                    // controls stay stable but disabled during their mutation.
                    button.set_visible(supported);
                }
            }
            if let Some(icon) = self.icons[index].upgrade() {
                icon.remove_css_class("accent");
                icon.add_css_class("dim-label");
                icon.set_opacity(if animate { 0.0 } else { 1.0 });
            }
            if let Some(spinner) = self.spinners[index].upgrade() {
                spinner.set_visible(animate);
                if animate {
                    spinner.start();
                } else {
                    spinner.stop();
                }
            }
        }
        if let Some(feedback) = self.feedback.upgrade() {
            feedback.set_visible(retry);
            feedback.set_tooltip_text(Some(message));
        }
        if let Some(refresh) = self.refresh.upgrade() {
            refresh.set_sensitive(retry);
        }
        if let Some(label) = self.message.upgrade() {
            label.set_text(message);
        }
    }

    fn like_confirmed(&self, liked: bool, mutable: bool) {
        self.render(liked, None, mutable);
        if let Some(feedback) = self.feedback.upgrade() {
            feedback.set_visible(true);
        }
        if let Some(message) = self.message.upgrade() {
            message.set_text("Отметка «Нравится» проверена. «Не нравится» сейчас недоступно. Можно проверить ещё раз; сохранённые отметки не угадываются.");
        }
    }

    fn render(&self, liked: bool, disliked: Option<bool>, mutable: bool) {
        for (index, (kind, marked)) in [
            (TrackReaction::Like, liked),
            (TrackReaction::Dislike, disliked.unwrap_or(false)),
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(button) = self.buttons[index].upgrade() {
                button.set_visible(index == 0 || disliked.is_some());
                button.set_sensitive(mutable);
                if marked {
                    button.add_css_class("liked");
                } else {
                    button.remove_css_class("liked");
                }
                button.update_state(&[gtk::accessible::State::Pressed(if marked {
                    gtk::AccessibleTristate::True
                } else {
                    gtk::AccessibleTristate::False
                })]);
                let action = reaction_action(kind, marked);
                button.set_tooltip_text(Some(action));
                button.update_property(&[gtk::accessible::Property::Label(action)]);
            }
            if let Some(icon) = self.icons[index].upgrade() {
                icon.set_icon_name(Some(if index == 0 {
                    like_icon_name(Some(marked))
                } else {
                    dislike_icon_name()
                }));
                icon.set_opacity(1.0);
                if marked {
                    icon.add_css_class("accent");
                    icon.remove_css_class("dim-label");
                } else {
                    icon.remove_css_class("accent");
                    icon.add_css_class("dim-label");
                }
            }
            if let Some(spinner) = self.spinners[index].upgrade() {
                spinner.stop();
                spinner.set_visible(false);
            }
        }
        if let Some(feedback) = self.feedback.upgrade() {
            feedback.set_visible(!mutable);
        }
        if let Some(refresh) = self.refresh.upgrade() {
            refresh.set_sensitive(true);
        }
        if let Some(message) = self.message.upgrade() {
            message.set_text(if mutable {
                "Обе отметки проверены в вашей библиотеке Яндекс Музыки."
            } else {
                "Другая правка библиотеки ещё выполняется. После завершения обновите отметки."
            });
        }
    }
}

impl ReactionControls {
    fn new(generation: u64, id: String) -> Self {
        Self {
            generation,
            id,
            views: RefCell::new(Vec::new()),
            initialized: Cell::new(false),
            supported: Cell::new(false),
            presentation: Cell::new(ReactionPresentation::Unknown {
                message: "Проверяем отметки в вашей библиотеке…",
                retry: false,
                animate: false,
            }),
        }
    }

    fn has_mapped_view(&self) -> bool {
        self.views.borrow().iter().any(|view| {
            view.buttons[0]
                .upgrade()
                .is_some_and(|button| button.is_mapped())
        })
    }

    fn update(&self, presentation: ReactionPresentation) {
        self.presentation.set(presentation);
        self.views.borrow_mut().retain(|view| {
            if view.buttons[0].upgrade().is_none() {
                return false;
            }
            view.apply(presentation, self.supported.get());
            true
        });
    }

    fn unknown(&self, message: &'static str, retry: bool) {
        self.update(ReactionPresentation::Unknown {
            message,
            retry,
            animate: !retry,
        });
    }

    fn finish(&self, result: Result<ReactionRead, InteractionError>, mutable: bool) {
        match result {
            Ok(state) => {
                self.supported.set(matches!(state, ReactionRead::Both(_)));
                self.update(ReactionPresentation::Confirmed { state, mutable });
            }
            Err(error) => self.unknown(interaction_message(error), true),
        }
    }
}

impl Controller {
    pub(super) fn sync_current_like(self: &Rc<Self>) {
        let selection = {
            let model = self.model.borrow();
            let track = model
                .queue
                .current()
                .filter(|_| model.account_valid && !model.closed);
            track.map(|track| (model.generation, track.id.clone()))
        };
        if self
            .current_reactions
            .borrow()
            .as_ref()
            .map(|group| (group.generation, &group.id))
            == selection.as_ref().map(|(generation, id)| (*generation, id))
        {
            return;
        }
        let group =
            selection.map(|(generation, id)| Rc::new(ReactionControls::new(generation, id)));
        self.current_reactions.replace(group.clone());
        let mini = self.mini_reactions_host.upgrade();
        for host in [
            &self.widgets.current_like_slot,
            self.now_playing.reactions_host(),
        ]
        .into_iter()
        .chain(mini.as_ref())
        {
            self.render_current_reactions_host(host, group.as_ref());
        }
    }

    pub(super) fn set_mini_reactions_host(self: &Rc<Self>, host: Option<&gtk::Box>) {
        let previous = self.mini_reactions_host.upgrade();
        if previous.as_ref() == host {
            return;
        }
        if let Some(previous) = previous {
            clear_box(&previous);
        }
        self.mini_reactions_host.set(host);
        if let Some(host) = host {
            let group = self.current_reactions.borrow().clone();
            self.render_current_reactions_host(host, group.as_ref());
        }
    }

    pub(super) fn refresh_current_reactions_after_like(self: &Rc<Self>, id: &str) {
        let matches = self
            .current_reactions
            .borrow()
            .as_ref()
            .is_some_and(|group| group.id.split(':').next() == id.split(':').next());
        if matches {
            self.current_reactions.take();
            self.sync_current_like();
        }
    }

    fn render_current_reactions_host(
        self: &Rc<Self>,
        host: &gtk::Box,
        group: Option<&Rc<ReactionControls>>,
    ) {
        clear_box(host);
        host.set_visible(group.is_some());
        if let Some(group) = group {
            host.append(&self.footer_reaction_controls(group));
        }
    }

    fn current_reaction_group(&self, group: &Rc<ReactionControls>) -> bool {
        let model = self.model.borrow();
        !model.closed
            && model.account_valid
            && model.generation == group.generation
            && model
                .queue
                .current()
                .is_some_and(|track| track.id == group.id)
            && self
                .current_reactions
                .borrow()
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, group))
    }

    fn footer_reaction_controls(self: &Rc<Self>, widgets: &Rc<ReactionControls>) -> gtk::Box {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        container.set_widget_name("music-current-like-controls");
        container.set_valign(gtk::Align::Center);
        let mut buttons = Vec::new();
        let mut icons = Vec::new();
        let mut spinners = Vec::new();
        for (name, icon_name) in [
            ("music-current-like", like_icon_name(None)),
            ("music-current-dislike", dislike_icon_name()),
        ] {
            let button = gtk::Button::new();
            button.set_widget_name(name);
            button.set_size_request(44, 44);
            button.set_valign(gtk::Align::Center);
            button.add_css_class("flat");
            let surface = gtk::Overlay::new();
            let icon = gtk::Image::from_icon_name(icon_name);
            icon.set_widget_name(&format!("{name}-icon"));
            icon.set_pixel_size(16);
            surface.set_child(Some(&icon));
            let spinner = gtk::Spinner::new();
            spinner.set_widget_name(&format!("{name}-spinner"));
            spinner.set_halign(gtk::Align::Center);
            spinner.set_valign(gtk::Align::Center);
            spinner.set_can_target(false);
            surface.add_overlay(&spinner);
            button.set_child(Some(&surface));
            container.append(&button);
            buttons.push(button);
            icons.push(icon.downgrade());
            spinners.push(spinner.downgrade());
        }
        let feedback = gtk::MenuButton::builder()
            .icon_name("karin-alert-triangle-symbolic")
            .build();
        feedback.set_widget_name("music-current-like-feedback");
        feedback.set_direction(gtk::ArrowType::Up);
        feedback.add_css_class("flat");
        feedback.update_property(&[gtk::accessible::Property::Label(
            "Состояние отметок и повторная проверка",
        )]);
        let popover = gtk::Popover::new();
        popover.set_position(gtk::PositionType::Top);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        super::super::margins(&content, 12);
        content.set_width_request(230);
        let message = text("Проверяем отметки в вашей библиотеке…", "caption");
        message.set_widget_name("music-like-message");
        message.set_max_width_chars(30);
        let refresh = gtk::Button::with_label("Обновить отметки");
        refresh.set_widget_name("music-like-refresh");
        content.append(&message);
        content.append(&refresh);
        popover.set_child(Some(&content));
        feedback.set_popover(Some(&popover));
        container.append(&feedback);
        let view = ReactionView {
            buttons: [buttons[0].downgrade(), buttons[1].downgrade()],
            icons: icons.try_into().expect("two reaction icons"),
            spinners: spinners.try_into().expect("two reaction spinners"),
            feedback: feedback.downgrade(),
            refresh: refresh.downgrade(),
            message: message.downgrade(),
        };
        view.apply(widgets.presentation.get(), widgets.supported.get());
        let mut views = widgets.views.borrow_mut();
        views.retain(|view| view.buttons[0].upgrade().is_some());
        views.push(view);
        drop(views);

        // The first mapped view admits the shared read after render-time
        // borrows unwind. Opening another surface uses its verified result.
        let weak = Rc::downgrade(self);
        let read = widgets.clone();
        buttons[0].connect_map(move |_| {
            if read.initialized.get() {
                return;
            }
            let (weak, widgets) = (weak.clone(), read.clone());
            glib::idle_add_local_once(move || {
                let Some(this) = weak.upgrade() else { return };
                if this.current_reaction_group(&widgets)
                    && widgets.has_mapped_view()
                    && !widgets.initialized.replace(true)
                {
                    this.read_footer_reactions(widgets);
                }
            });
        });
        let weak = Rc::downgrade(self);
        let read = widgets.clone();
        refresh.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.read_footer_reactions(read.clone());
            }
        });
        for (button, kind) in buttons
            .into_iter()
            .zip([TrackReaction::Like, TrackReaction::Dislike])
        {
            let weak = Rc::downgrade(self);
            let widgets = widgets.clone();
            button.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.change_footer_reaction(kind, widgets.clone());
                }
            });
        }
        container
    }

    fn read_footer_reactions(self: &Rc<Self>, widgets: Rc<ReactionControls>) {
        if !self.current_reaction_group(&widgets) {
            return;
        }
        if self.mutation_busy.get() {
            widgets.unknown(
                "Правка библиотеки ещё выполняется. Обновите отметки позже.",
                true,
            );
            return;
        }
        widgets.unknown("Проверяем отметки в вашей библиотеке…", false);
        let id = widgets.id.clone();
        self.run(
            Operation::CurrentLikeRead,
            move |service, token| {
                Ok(match service.track_reactions(&token, &id) {
                    Ok(state) => Ok(ReactionRead::Both(state)),
                    Err(
                        error @ InteractionError::Music(
                            MusicError::AccessDenied | MusicError::Unauthenticated,
                        ),
                    ) => Err(error),
                    Err(reason) => service
                        .is_track_liked(&token, &id)
                        .map(|liked| ReactionRead::LikeOnly(liked, reason)),
                })
            },
            move |this, result| {
                let result = result.unwrap_or_else(|error| Err(InteractionError::Music(error)));
                let diagnostic = match result {
                    Ok(ReactionRead::Both(_)) => None,
                    Ok(ReactionRead::LikeOnly(_, reason)) | Err(reason) => Some(reason),
                };
                interaction_diagnostic("reactions_read", diagnostic);
                if let Err(InteractionError::Music(
                    error @ (MusicError::AccessDenied | MusicError::Unauthenticated),
                )) = result
                {
                    this.show_error(error);
                    return;
                }
                if this.current_reaction_group(&widgets) {
                    widgets.finish(result, !this.mutation_busy.get());
                }
            },
        );
    }

    fn change_footer_reaction(self: &Rc<Self>, kind: TrackReaction, widgets: Rc<ReactionControls>) {
        if !self.current_reaction_group(&widgets) {
            return;
        }
        let (marked, fallback) = match (widgets.presentation.get(), kind) {
            (
                ReactionPresentation::Confirmed {
                    state: ReactionRead::Both(state),
                    mutable: true,
                },
                TrackReaction::Like,
            ) => (state.liked, None),
            (
                ReactionPresentation::Confirmed {
                    state: ReactionRead::Both(state),
                    mutable: true,
                },
                TrackReaction::Dislike,
            ) => (state.disliked, None),
            (
                ReactionPresentation::Confirmed {
                    state: ReactionRead::LikeOnly(liked, reason),
                    mutable: true,
                },
                TrackReaction::Like,
            ) => (liked, Some(reason)),
            _ => return,
        };
        if self.mutation_busy.replace(true) {
            widgets.unknown(
                "Другая правка ещё выполняется. Обновите отметки позже.",
                true,
            );
            return;
        }
        let enabled = !marked;
        let id = widgets.id.clone();
        let playback_target = self.reaction_playback_target();
        let detail_views = self.retire_detail_likes(&id);
        widgets.unknown("Сохраняем и проверяем отметки…", false);
        for operation in [
            Operation::CurrentLikeRead,
            Operation::Library,
            Operation::InitialLibrary,
        ] {
            self.requests.borrow_mut().invalidate(operation);
        }
        self.invalidate_liked_feeds();
        let cache = self.persistent_cache();
        self.run(
            Operation::Mutation,
            move |service, token| {
                let result = if let Some(reason) = fallback {
                    service
                        .set_track_liked(&token, &id, enabled)
                        .map(|state| ReactionRead::LikeOnly(state.liked, reason))
                } else {
                    service
                        .set_track_reaction(&token, &id, kind, enabled)
                        .map(ReactionRead::Both)
                };
                if !token.is_expired()
                    && service.cache_scope(&token).is_ok()
                    && let Some(cache) = cache
                {
                    let _ = cache.invalidate_metadata();
                }
                Ok(result)
            },
            move |this, result| {
                this.mutation_busy.set(false);
                let result = result.unwrap_or_else(|error| Err(InteractionError::Music(error)));
                interaction_diagnostic(
                    match kind {
                        TrackReaction::Like => "like_write",
                        TrackReaction::Dislike => "dislike_write",
                    },
                    result.as_ref().err().copied(),
                );
                this.model.borrow_mut().liked = None;
                for operation in [Operation::Library, Operation::InitialLibrary] {
                    this.requests.borrow_mut().invalidate(operation);
                }
                this.invalidate_liked_feeds();
                this.render_home_preview();
                if let Err(InteractionError::Music(
                    error @ (MusicError::AccessDenied | MusicError::Unauthenticated),
                )) = result
                {
                    this.show_error(error);
                    return;
                }
                detail_views.finish(
                    result.map(|state| match state {
                        ReactionRead::Both(state) => state.liked,
                        ReactionRead::LikeOnly(liked, _) => liked,
                    }),
                    true,
                );
                if this.current_reaction_group(&widgets) {
                    widgets.finish(result, true);
                }
                // `run` admits this FnOnce completion only for the original
                // session and live Mutation ticket. Confirm presentation before
                // using the shared Next path, which owns queue/MPRIS and Wave
                // listening/Skip semantics. GET recovery never reaches here.
                if confirmed_dislike_advances(
                    kind,
                    enabled,
                    result,
                    playback_target.as_ref(),
                    this.reaction_playback_target().as_ref(),
                ) {
                    this.skip_music(true, false);
                }
            },
        );
    }

    pub(super) fn reaction_playback_target(&self) -> Option<ReactionPlaybackTarget> {
        let model = self.model.borrow();
        if model.closed
            || !model.account_valid
            || !(self.music_requested() || self.music_snapshot().is_some())
        {
            return None;
        }
        Some(ReactionPlaybackTarget {
            generation: model.generation,
            occurrence: model.queue.current_occurrence_id()?.clone(),
            serial: self.music_serial(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::{ContextKind, MusicQueue, PlaybackContext, RepeatMode};

    fn queue(kind: ContextKind) -> MusicQueue {
        let track = Track {
            id: "42".into(),
            title: "Synthetic repeated reaction track".into(),
            artists: Vec::new(),
            artist_links: Vec::new(),
            album_id: None,
            album_title: None,
            duration_ms: Some(60_000),
            cover: None,
            explicit: false,
            available: true,
        };
        let mut queue = MusicQueue::new();
        queue.replace_context(PlaybackContext::new(kind, None, vec![track; 3], 1, false).unwrap());
        queue
    }

    fn target(queue: &MusicQueue, generation: u64, serial: u64) -> ReactionPlaybackTarget {
        ReactionPlaybackTarget {
            generation,
            occurrence: queue.current_occurrence_id().unwrap().clone(),
            serial,
        }
    }

    fn confirmed() -> Result<ReactionRead, InteractionError> {
        Ok(ReactionRead::Both(TrackReactions {
            liked: false,
            disliked: true,
        }))
    }

    #[test]
    fn advance_requires_confirmed_add_dislike_not_other_intents_or_recovery() {
        let queue = queue(ContextKind::LikedTracks);
        let submitted = target(&queue, 1, 10);
        let current = target(&queue, 1, 10);
        let permit = |kind, enabled, result| {
            confirmed_dislike_advances(kind, enabled, result, Some(&submitted), Some(&current))
        };
        assert!(permit(TrackReaction::Dislike, true, confirmed()));
        for (kind, enabled) in [
            (TrackReaction::Dislike, false),
            (TrackReaction::Like, true),
            (TrackReaction::Like, false),
        ] {
            assert!(!permit(kind, enabled, confirmed()));
        }
        for result in [
            Err(InteractionError::Music(MusicError::Forbidden)),
            Err(InteractionError::WriteOutcomeUnknown),
            Err(InteractionError::ConfirmationUnavailable),
            Err(InteractionError::ConfirmationMismatch),
            Ok(ReactionRead::Both(TrackReactions {
                liked: false,
                disliked: false,
            })),
            Ok(ReactionRead::Both(TrackReactions {
                liked: true,
                disliked: true,
            })),
            Ok(ReactionRead::LikeOnly(false, InteractionError::Conflict)),
        ] {
            assert!(!permit(TrackReaction::Dislike, true, result));
        }
        assert!(!confirmed_dislike_advances(
            TrackReaction::Dislike,
            true,
            confirmed(),
            None,
            None,
        ));
    }

    #[test]
    fn confirmed_dislike_does_not_advance_equal_id_successor_or_replaced_context() {
        for kind in [ContextKind::LikedTracks, ContextKind::Wave] {
            let mut queue = queue(kind);
            let submitted = target(&queue, 1, 10);
            let original_id = queue.current().unwrap().id.clone();
            queue.next_track().unwrap();
            assert_eq!(queue.current().unwrap().id, original_id);
            // Equal serial is deliberate: exact occurrence identity is required
            // even if a future selection path does not replace the engine.
            let current = target(&queue, 1, 10);
            assert!(!confirmed_dislike_advances(
                TrackReaction::Dislike,
                true,
                confirmed(),
                Some(&submitted),
                Some(&current),
            ));
            let context = queue.context().unwrap().clone();
            queue.replace_context(context);
            assert!(!confirmed_dislike_advances(
                TrackReaction::Dislike,
                true,
                confirmed(),
                Some(&current),
                Some(&target(&queue, 1, 10)),
            ));
        }
    }

    #[test]
    fn confirmed_dislike_survives_unrelated_queue_reindexing() {
        let mut queue = queue(ContextKind::Playlist);
        let submitted = target(&queue, 1, 10);
        queue.remove(0);
        assert_eq!(queue.current_index(), Some(0));
        assert!(confirmed_dislike_advances(
            TrackReaction::Dislike,
            true,
            confirmed(),
            Some(&submitted),
            Some(&target(&queue, 1, 10)),
        ));
    }

    #[test]
    fn playback_cycle_and_account_fence_return_stop_restart_and_repeat_one() {
        let mut queue = queue(ContextKind::Album);
        let submitted = target(&queue, 1, 10);
        queue.next_track().unwrap();
        queue.previous_track().unwrap();
        // A->B->A returns the same occurrence, but never the same intent cycle.
        for (generation, serial) in [(1, 12), (2, 10), (2, 12)] {
            assert!(!confirmed_dislike_advances(
                TrackReaction::Dislike,
                true,
                confirmed(),
                Some(&submitted),
                Some(&target(&queue, generation, serial)),
            ));
        }
        queue.set_repeat_mode(RepeatMode::One);
        queue.advance_eos().unwrap();
        assert_eq!(queue.current_occurrence_id(), Some(&submitted.occurrence));
        assert!(!confirmed_dislike_advances(
            TrackReaction::Dislike,
            true,
            confirmed(),
            Some(&submitted),
            Some(&target(&queue, 1, 11)),
        ));
        // Stop/logout/close have no admissible active target at completion.
        assert!(!confirmed_dislike_advances(
            TrackReaction::Dislike,
            true,
            confirmed(),
            Some(&submitted),
            None,
        ));
    }
}
