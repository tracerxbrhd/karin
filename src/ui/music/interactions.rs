// SPDX-License-Identifier: GPL-3.0-or-later
//! Confirmed per-track library interactions. No optimistic heart or POST retry.

use std::{cell::Cell, rc::Rc};

use adw::prelude::*;
use gtk::glib;

use super::{Controller, MusicError, Operation, text};
use crate::music::interactions::{InteractionError, LikeState};

/// Prefer an actual native-theme heart; Adwaita and Breeze use different
/// published names. Unusual themes retain a functional native fallback rather
/// than an absent glyph. This is presentation only, never membership evidence.
pub(super) fn like_icon_name(liked: Option<bool>) -> &'static str {
    if let Some(display) = gtk::gdk::Display::default() {
        let theme = gtk::IconTheme::for_display(&display);
        for name in ["emote-love-symbolic", "love-symbolic"] {
            if theme.has_icon(name) {
                return name;
            }
        }
    }
    if liked == Some(true) {
        "starred-symbolic"
    } else {
        "non-starred-symbolic"
    }
}

#[derive(Clone)]
struct LikeWidgets {
    button: glib::WeakRef<gtk::Button>,
    refresh: glib::WeakRef<gtk::Button>,
    icon: glib::WeakRef<gtk::Image>,
    label: glib::WeakRef<gtk::Label>,
    message: glib::WeakRef<gtk::Label>,
    confirmed: Rc<Cell<Option<bool>>>,
    compact: bool,
    spinner: glib::WeakRef<gtk::Spinner>,
    feedback: glib::WeakRef<gtk::MenuButton>,
}

/// Weak presentation subscriptions, never a membership cache. A confirmed write
/// retires in-flight detail reads before updating all surviving views of its ID.
pub(super) struct DetailLike {
    generation: u64,
    id: String,
    widgets: LikeWidgets,
}

pub(super) struct RetiredDetailLikes(Vec<LikeWidgets>);

impl RetiredDetailLikes {
    pub(super) fn finish(&self, result: Result<bool, InteractionError>, mutable: bool) {
        for widgets in self.0.iter().filter(|widgets| widgets.alive()) {
            match result {
                Ok(liked) => widgets.confirmed(liked, mutable),
                Err(error) => widgets.unknown(interaction_message(error), true),
            }
        }
    }
}

impl LikeWidgets {
    fn alive(&self) -> bool {
        self.button
            .upgrade()
            .is_some_and(|button| button.root().is_some())
    }

    fn unknown(&self, message: &str, refresh: bool) {
        self.confirmed.set(None);
        if let Some(button) = self.button.upgrade() {
            button.set_sensitive(false);
            button.remove_css_class("liked");
            button.update_state(&[gtk::accessible::State::Pressed(
                gtk::AccessibleTristate::Mixed,
            )]);
            button.set_tooltip_text(Some("Сначала обновите состояние отметки"));
            button.update_property(&[gtk::accessible::Property::Label(
                "Отметка «Нравится» не проверена",
            )]);
        }
        if let Some(label) = self.label.upgrade() {
            label.set_text("Отметка не проверена");
        }
        if let Some(icon) = self.icon.upgrade() {
            icon.remove_css_class("accent");
            icon.add_css_class("dim-label");
            if self.compact {
                icon.set_opacity(if refresh { 1.0 } else { 0.0 });
            }
            icon.set_icon_name(Some(like_icon_name(None)));
        }
        if let Some(spinner) = self.spinner.upgrade() {
            spinner.set_visible(!refresh);
            if refresh {
                spinner.stop();
            } else {
                spinner.start();
            }
        }
        if let Some(feedback) = self.feedback.upgrade() {
            feedback.set_visible(refresh);
            feedback.set_tooltip_text(Some(message));
        }
        if let Some(button) = self.refresh.upgrade() {
            button.set_sensitive(refresh);
        }
        if let Some(label) = self.message.upgrade() {
            label.set_text(message);
            label.set_visible(true);
        }
    }

    fn confirmed(&self, liked: bool, mutable: bool) {
        self.confirmed.set(Some(liked));
        let action = if liked {
            "Убрать отметку «Нравится»"
        } else {
            "Поставить отметку «Нравится»"
        };
        if let Some(button) = self.button.upgrade() {
            button.set_sensitive(mutable);
            if liked {
                button.add_css_class("liked");
            } else {
                button.remove_css_class("liked");
            }
            button.update_state(&[gtk::accessible::State::Pressed(if liked {
                gtk::AccessibleTristate::True
            } else {
                gtk::AccessibleTristate::False
            })]);
            button.set_tooltip_text(Some(action));
            button.update_property(&[gtk::accessible::Property::Label(action)]);
        }
        if let Some(label) = self.label.upgrade() {
            label.set_text(if liked {
                "В любимом"
            } else {
                "Нравится"
            });
        }
        if let Some(icon) = self.icon.upgrade() {
            icon.set_icon_name(Some(like_icon_name(Some(liked))));
            if self.compact {
                icon.set_opacity(1.0);
            }
            if liked {
                icon.remove_css_class("dim-label");
                icon.add_css_class("accent");
            } else {
                icon.remove_css_class("accent");
                icon.add_css_class("dim-label");
            }
        }
        if let Some(button) = self.refresh.upgrade() {
            button.set_sensitive(true);
        }
        if let Some(spinner) = self.spinner.upgrade() {
            spinner.stop();
            spinner.set_visible(false);
        }
        if let Some(feedback) = self.feedback.upgrade() {
            feedback.set_visible(!mutable);
        }
        if let Some(message) = self.message.upgrade() {
            message.set_text(if mutable {
                if liked {
                    "Отметка подтверждена библиотекой Яндекс Музыки."
                } else {
                    "В библиотеке нет отметки «Нравится» для этого трека."
                }
            } else {
                "Другая правка библиотеки ещё выполняется. После завершения обновите состояние."
            });
            // A confirmed footer action needs no explanatory paragraph.
            // Pending, uncertain and refused operations retain their message.
            message.set_visible(!self.compact || !mutable);
        }
    }
}

impl Controller {
    pub(super) fn retire_detail_likes(&self, id: &str) -> RetiredDetailLikes {
        let generation = self.model.borrow().generation;
        let (views, pending_other) = {
            let mut entries = self.detail_like_controls.borrow_mut();
            entries.retain(|entry| entry.generation == generation && entry.widgets.alive());
            let matching = |entry: &&DetailLike| entry.id.split(':').next() == id.split(':').next();
            let views = entries
                .iter()
                .filter(matching)
                .map(|entry| entry.widgets.clone())
                .collect::<Vec<_>>();
            let pending = entries
                .iter()
                .filter(|entry| !matching(entry) && entry.widgets.confirmed.get().is_none())
                .map(|entry| entry.widgets.clone())
                .collect::<Vec<_>>();
            (views, pending)
        };
        if !views.is_empty() {
            self.requests.borrow_mut().invalidate(Operation::LikeRead);
            for widgets in pending_other {
                widgets.unknown(
                    "Проверка отложена из-за правки библиотеки. Обновите отметку.",
                    true,
                );
            }
        }
        for widgets in &views {
            widgets.unknown("Сохраняем и проверяем отметку…", false);
        }
        RetiredDetailLikes(views)
    }

    /// A direct confirmed action, looked up once only when the active footer
    /// maps. Queue selection alone leaves the hidden footer and network idle.
    /// Identity is account generation + track, not a duplicate queue occurrence.
    pub(super) fn sync_current_like(self: &Rc<Self>) {
        let key = {
            let model = self.model.borrow();
            if model.account_valid && !model.closed {
                model
                    .queue
                    .current()
                    .map(|track| (model.generation, track.id.clone()))
            } else {
                None
            }
        };
        if *self.current_like_key.borrow() == key {
            return;
        }
        self.current_like_key.replace(key.clone());
        super::clear_box(&self.widgets.current_like_slot);
        self.widgets.current_like_slot.set_visible(key.is_some());
        let Some((generation, id)) = key else { return };
        let container = self.footer_reaction_controls(id, generation);
        // Connect map before parenting: a visible footer can map synchronously.
        self.widgets.current_like_slot.append(&container);
    }

    /// Full-size presentation retained by native confirmation/retirement
    /// fixtures; the ordinary selected-track menu uses the compact wrapper.
    #[cfg(test)]
    pub(super) fn like_controls(self: &Rc<Self>, id: String) -> gtk::Box {
        self.like_controls_presentation(id, false)
    }

    /// The selected track's native action menu reuses the confirmed Like path;
    /// opening metadata alone must not start membership reads for every row.
    pub(super) fn selected_like_controls(self: &Rc<Self>, id: String) -> gtk::Box {
        self.like_controls_presentation(id, true)
    }

    fn like_controls_presentation(self: &Rc<Self>, id: String, compact: bool) -> gtk::Box {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
        container.set_widget_name("music-like-controls");
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let button = gtk::Button::new();
        button.set_widget_name("music-like-toggle");
        button.set_sensitive(false);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let icon = gtk::Image::from_icon_name(like_icon_name(None));
        icon.add_css_class("dim-label");
        let label = text("Проверка отметки…", "");
        content.append(&icon);
        content.append(&label);
        button.set_child(Some(&content));
        actions.append(&button);
        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_widget_name("music-like-refresh");
        refresh.set_tooltip_text(Some("Обновить отметку «Нравится»"));
        refresh.update_property(&[gtk::accessible::Property::Label(
            "Обновить отметку «Нравится»",
        )]);
        actions.append(&refresh);
        container.append(&actions);
        let message = text("Проверка отметки в вашей библиотеке…", "caption");
        message.set_widget_name("music-like-message");
        container.append(&message);
        let widgets = LikeWidgets {
            button: button.downgrade(),
            refresh: refresh.downgrade(),
            icon: icon.downgrade(),
            label: label.downgrade(),
            message: message.downgrade(),
            confirmed: Rc::new(Cell::new(None)),
            compact,
            spinner: glib::WeakRef::new(),
            feedback: glib::WeakRef::new(),
        };
        {
            let mut entries = self.detail_like_controls.borrow_mut();
            entries.retain(|entry| entry.widgets.alive());
            // Only the current detail is ordinarily mapped. Bound retained
            // weak subscriptions even if a synthetic host mounts many views.
            if entries.len() == 64 {
                entries.remove(0);
            }
            entries.push(DetailLike {
                generation: self.model.borrow().generation,
                id: id.clone(),
                widgets: widgets.clone(),
            });
        }
        let initialized = Cell::new(false);
        let weak = Rc::downgrade(self);
        let read_widgets = widgets.clone();
        let read_id = id.clone();
        container.connect_map(move |_| {
            if !initialized.replace(true)
                && let Some(this) = weak.upgrade()
            {
                this.read_like(read_id.clone(), read_widgets.clone());
            }
        });
        let weak = Rc::downgrade(self);
        let read_widgets = widgets.clone();
        let read_id = id.clone();
        refresh.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.read_like(read_id.clone(), read_widgets.clone());
            }
        });
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.change_like(id.clone(), widgets.clone());
            }
        });
        container
    }

    fn read_like(self: &Rc<Self>, id: String, widgets: LikeWidgets) {
        if !widgets.alive() || !self.model.borrow().account_valid || self.model.borrow().closed {
            return;
        }
        if self.mutation_busy.get() {
            widgets.unknown(
                "Правка библиотеки ещё выполняется. Обновите состояние после её завершения.",
                true,
            );
            return;
        }
        widgets.unknown("Проверка отметки в вашей библиотеке…", false);
        let operation = if widgets.compact {
            Operation::CurrentLikeRead
        } else {
            Operation::LikeRead
        };
        self.run(
            operation,
            move |service, token| {
                Ok(service
                    .is_track_liked(&token, &id)
                    .map(|liked| LikeState { liked }))
            },
            move |this, result| {
                let result = result.unwrap_or_else(|error| Err(InteractionError::Music(error)));
                interaction_diagnostic("like_read", result.as_ref().err().copied());
                if let Err(InteractionError::Music(
                    error @ (MusicError::AccessDenied | MusicError::Unauthenticated),
                )) = result
                {
                    this.show_error(error);
                    return;
                }
                if !widgets.alive() {
                    return;
                }
                match result {
                    Ok(state) => widgets.confirmed(state.liked, !this.mutation_busy.get()),
                    Err(error) => widgets.unknown(interaction_message(error), true),
                }
            },
        );
    }

    fn change_like(self: &Rc<Self>, id: String, widgets: LikeWidgets) {
        if !widgets.alive() || !self.model.borrow().account_valid || self.model.borrow().closed {
            return;
        }
        let Some(liked) = widgets.confirmed.get() else {
            // A disabled/late activation must not erase the first uncertain
            // write outcome or imply that the preceding write was never sent.
            return;
        };
        if self.mutation_busy.replace(true) {
            widgets.unknown("Другая правка библиотеки ещё выполняется. Изменение не отправлено; обновите состояние позже.", true);
            return;
        }
        // Clear known UI state while the one explicit write and its confirming
        // read are in flight. Repeated clicks cannot enqueue another POST.
        widgets.unknown("Сохранение и проверка отметки…", false);
        let detail_views = self.retire_detail_likes(&id);
        self.requests.borrow_mut().invalidate(Operation::Library);
        self.requests
            .borrow_mut()
            .invalidate(Operation::InitialLibrary);
        self.invalidate_liked_feeds();
        let cache = self.persistent_cache();
        let changed_id = id.clone();
        self.run(
            Operation::Mutation,
            move |service, token| {
                let result = service.set_track_liked(&token, &id, !liked);
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
                interaction_diagnostic("like_write", result.as_ref().err().copied());
                // A write may already have reached the service even when its
                // response was lost. Invalidate the local library cache in all
                // outcomes; the next library read must ask Music again.
                this.model.borrow_mut().liked = None;
                this.requests.borrow_mut().invalidate(Operation::Library);
                this.requests
                    .borrow_mut()
                    .invalidate(Operation::InitialLibrary);
                this.invalidate_liked_feeds();
                this.render_home_preview();
                if let Err(InteractionError::Music(
                    error @ (MusicError::AccessDenied | MusicError::Unauthenticated),
                )) = result
                {
                    this.show_error(error);
                    return;
                }
                // A different visible detail control can change the same
                // account-global mark. Retire the footer's old membership and
                // let its visible replacement read current state, without ever
                // repeating a write or borrowing detail display fields.
                if !widgets.compact
                    && this
                        .current_like_key
                        .borrow()
                        .as_ref()
                        .is_some_and(|(_, current)| {
                            current.split(':').next() == changed_id.split(':').next()
                        })
                {
                    this.current_like_key.replace(None);
                    this.sync_current_like();
                }
                if !widgets.alive() {
                    detail_views.finish(result.map(|state| state.liked), true);
                    return;
                }
                detail_views.finish(result.map(|state| state.liked), true);
                match result {
                    Ok(state) => widgets.confirmed(state.liked, true),
                    Err(error) => widgets.unknown(interaction_message(error), true),
                }
            },
        );
    }
}

pub(super) fn interaction_message(error: InteractionError) -> &'static str {
    match error {
        InteractionError::Conflict => {
            "Библиотека изменилась. Обновите состояние перед новой правкой."
        }
        InteractionError::WriteOutcomeUnknown => {
            "Изменение могло сохраниться, но надёжный ответ не получен. Обновите состояние; запрос не повторяется автоматически."
        }
        InteractionError::ConfirmationUnavailable => {
            "Запрос отправлен, но результат не удалось проверить. Обновите состояние; изменение не считается подтверждённым."
        }
        InteractionError::ConfirmationMismatch => {
            "Библиотека не подтвердила изменение. Обновите состояние перед новой правкой."
        }
        InteractionError::Music(error) => super::music_error_message(error).1,
    }
}

pub(super) fn interaction_diagnostic(operation: &'static str, error: Option<InteractionError>) {
    use std::io::Write;
    let (category, http) = match error {
        None => ("confirmed", None),
        Some(InteractionError::Music(error)) => (error.category(), error.http_status()),
        Some(InteractionError::Conflict) => ("conflict", Some(409)),
        Some(InteractionError::WriteOutcomeUnknown) => ("write-outcome-unknown", None),
        Some(InteractionError::ConfirmationUnavailable) => ("confirmation-unavailable", None),
        Some(InteractionError::ConfirmationMismatch) => ("confirmation-mismatch", None),
    };
    let _ = writeln!(
        std::io::stderr().lock(),
        "Karin library: operation={} outcome={} http={}",
        operation,
        category,
        http.map(|status| status.to_string())
            .unwrap_or_else(|| "none".into())
    );
}

#[cfg(test)]
mod tests;
