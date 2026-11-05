// SPDX-License-Identifier: GPL-3.0-or-later
//! Account reactions in the existing footer. No optimistic toggle or radio event.

use super::*;
use crate::music::interactions::{InteractionError, TrackReaction, TrackReactions};
use interactions::{interaction_diagnostic, interaction_message, like_icon_name};

#[derive(Clone)]
struct ReactionControls {
    buttons: [glib::WeakRef<gtk::Button>; 2],
    icons: [glib::WeakRef<gtk::Image>; 2],
    spinners: [glib::WeakRef<gtk::Spinner>; 2],
    feedback: glib::WeakRef<gtk::MenuButton>,
    refresh: glib::WeakRef<gtk::Button>,
    message: glib::WeakRef<gtk::Label>,
    state: Rc<Cell<Option<TrackReactions>>>,
    supported: Rc<Cell<bool>>,
    like_only: Rc<Cell<Option<bool>>>,
    fallback_reason: Rc<Cell<Option<InteractionError>>>,
}

#[derive(Clone, Copy)]
enum ReactionRead {
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
    if let Some(display) = gtk::gdk::Display::default() {
        let theme = gtk::IconTheme::for_display(&display);
        for name in ["thumb-down-symbolic", "action-unavailable-symbolic"] {
            if theme.has_icon(name) {
                return name;
            }
        }
    }
    "process-stop-symbolic"
}

impl ReactionControls {
    fn alive(&self) -> bool {
        self.buttons[0]
            .upgrade()
            .is_some_and(|button| button.root().is_some())
    }

    fn unknown(&self, message: &str, retry: bool) {
        self.unknown_state(message, retry, !retry);
    }

    fn unknown_state(&self, message: &str, retry: bool, animate: bool) {
        self.state.set(None);
        self.like_only.set(None);
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
                    button.set_visible(self.supported.get());
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

    fn confirmed(&self, state: TrackReactions, mutable: bool) {
        self.supported.set(true);
        self.state.set(Some(state));
        self.like_only.set(Some(state.liked));
        self.fallback_reason.set(None);
        self.render(state.liked, Some(state.disliked), mutable);
    }

    fn like_confirmed(&self, liked: bool, reason: InteractionError, mutable: bool) {
        self.supported.set(false);
        self.state.set(None);
        self.like_only.set(Some(liked));
        self.fallback_reason.set(Some(reason));
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

impl Controller {
    pub(super) fn footer_reaction_controls(
        self: &Rc<Self>,
        id: String,
        generation: u64,
    ) -> gtk::Box {
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
            .icon_name("dialog-warning-symbolic")
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
        let widgets = ReactionControls {
            buttons: [buttons[0].downgrade(), buttons[1].downgrade()],
            icons: icons.try_into().expect("two reaction icons"),
            spinners: spinners.try_into().expect("two reaction spinners"),
            feedback: feedback.downgrade(),
            refresh: refresh.downgrade(),
            message: message.downgrade(),
            state: Rc::new(Cell::new(None)),
            supported: Rc::new(Cell::new(false)),
            like_only: Rc::new(Cell::new(None)),
            fallback_reason: Rc::new(Cell::new(None)),
        };
        // Mapping defers the bounded read until render-time borrows unwind.
        // Do not animate before that idle callback: continuous HiDPI redraws
        // must not become a prerequisite for admitting the first request.
        widgets.unknown_state("Проверяем отметки в вашей библиотеке…", false, false);
        let initialized = Rc::new(Cell::new(false));
        let weak = Rc::downgrade(self);
        let read = widgets.clone();
        let read_id = id.clone();
        buttons[0].connect_map(move |_| {
            if initialized.replace(true) {
                return;
            }
            let (weak, widgets, id, initialized) = (
                weak.clone(),
                read.clone(),
                read_id.clone(),
                initialized.clone(),
            );
            glib::idle_add_local_once(move || {
                let Some(this) = weak.upgrade() else { return };
                let current = {
                    let model = this.model.borrow();
                    !model.closed
                        && model.account_valid
                        && model.generation == generation
                        && model.queue.current().is_some_and(|track| track.id == id)
                };
                if current
                    && widgets.alive()
                    && widgets.buttons[0]
                        .upgrade()
                        .is_some_and(|button| button.is_mapped())
                {
                    this.read_footer_reactions(id, widgets);
                } else {
                    initialized.set(false);
                }
            });
        });
        let weak = Rc::downgrade(self);
        let read = widgets.clone();
        let read_id = id.clone();
        refresh.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.read_footer_reactions(read_id.clone(), read.clone());
            }
        });
        for (button, kind) in buttons
            .into_iter()
            .zip([TrackReaction::Like, TrackReaction::Dislike])
        {
            let weak = Rc::downgrade(self);
            let widgets = widgets.clone();
            let id = id.clone();
            button.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.change_footer_reaction(id.clone(), kind, widgets.clone());
                }
            });
        }
        container
    }

    fn read_footer_reactions(self: &Rc<Self>, id: String, widgets: ReactionControls) {
        if !widgets.alive() || !self.model.borrow().account_valid || self.model.borrow().closed {
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
                if widgets.alive() {
                    match result {
                        Ok(ReactionRead::Both(state)) => {
                            widgets.confirmed(state, !this.mutation_busy.get())
                        }
                        Ok(ReactionRead::LikeOnly(liked, reason)) => {
                            widgets.like_confirmed(liked, reason, !this.mutation_busy.get())
                        }
                        Err(error) => widgets.unknown(interaction_message(error), true),
                    }
                }
            },
        );
    }

    fn change_footer_reaction(
        self: &Rc<Self>,
        id: String,
        kind: TrackReaction,
        widgets: ReactionControls,
    ) {
        if !widgets.alive() || !self.model.borrow().account_valid || self.model.borrow().closed {
            return;
        }
        let state = widgets.state.get();
        let marked = match kind {
            TrackReaction::Like => widgets.like_only.get(),
            TrackReaction::Dislike => state.map(|state| state.disliked),
        };
        let Some(marked) = marked else { return };
        let fallback = widgets.fallback_reason.get();
        if self.mutation_busy.replace(true) {
            widgets.unknown(
                "Другая правка ещё выполняется. Обновите отметки позже.",
                true,
            );
            return;
        }
        let enabled = !marked;
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
                if widgets.alive() {
                    match result {
                        Ok(ReactionRead::Both(state)) => widgets.confirmed(state, true),
                        Ok(ReactionRead::LikeOnly(liked, reason)) => {
                            widgets.like_confirmed(liked, reason, true)
                        }
                        Err(error) => widgets.unknown(interaction_message(error), true),
                    }
                }
                // No queue/transport command or recommendation-only radio
                // feedback is implied by a collection reaction.
            },
        );
    }
}
