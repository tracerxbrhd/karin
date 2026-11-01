// SPDX-License-Identifier: GPL-3.0-or-later
//! Confirmed per-track library interactions. No optimistic heart or POST retry.

use std::{cell::Cell, rc::Rc};

use adw::prelude::*;
use gtk::glib;

use super::{Controller, MusicError, Operation, text};
use crate::music::interactions::{InteractionError, LikeState};

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
                icon.set_icon_name(Some("non-starred-symbolic"));
                icon.set_opacity(if refresh { 1.0 } else { 0.0 });
            }
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
            if self.compact {
                icon.set_icon_name(Some(if liked {
                    "starred-symbolic"
                } else {
                    "non-starred-symbolic"
                }));
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
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        container.set_widget_name("music-current-like-controls");
        let button = gtk::Button::new();
        button.set_widget_name("music-current-like");
        button.set_size_request(44, 44);
        button.add_css_class("flat");
        button.set_sensitive(false);
        button.set_tooltip_text(Some("Проверяем отметку «Нравится»"));
        button.update_property(&[gtk::accessible::Property::Label(
            "Отметка «Нравится» не проверена",
        )]);
        let surface = gtk::Overlay::new();
        let icon = gtk::Image::from_icon_name("non-starred-symbolic");
        icon.set_widget_name("music-current-like-icon");
        icon.set_pixel_size(16);
        icon.add_css_class("dim-label");
        surface.set_child(Some(&icon));
        let spinner = gtk::Spinner::new();
        spinner.set_widget_name("music-current-like-spinner");
        spinner.set_halign(gtk::Align::Center);
        spinner.set_valign(gtk::Align::Center);
        spinner.set_can_target(false);
        spinner.set_visible(false);
        surface.add_overlay(&spinner);
        button.set_child(Some(&surface));
        container.append(&button);

        // Uncertain/error states expose an explicit GET-only retry and safe
        // explanation. The main heart never mutates an unconfirmed state.
        let feedback = gtk::MenuButton::builder()
            .icon_name("dialog-warning-symbolic")
            .build();
        feedback.set_widget_name("music-current-like-feedback");
        feedback.set_direction(gtk::ArrowType::Up);
        feedback.add_css_class("flat");
        feedback.set_visible(false);
        feedback.update_property(&[gtk::accessible::Property::Label(
            "Состояние отметки и повторная проверка",
        )]);
        let popover = gtk::Popover::new();
        popover.set_position(gtk::PositionType::Top);
        let feedback_content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        super::super::margins(&feedback_content, 12);
        feedback_content.set_width_request(230);
        let message = text("Проверка отметки в вашей библиотеке…", "caption");
        message.set_max_width_chars(30);
        message.set_widget_name("music-like-message");
        let refresh = gtk::Button::with_label("Обновить отметку");
        refresh.set_widget_name("music-like-refresh");
        refresh.set_sensitive(false);
        feedback_content.append(&message);
        feedback_content.append(&refresh);
        popover.set_child(Some(&feedback_content));
        feedback.set_popover(Some(&popover));
        container.append(&feedback);
        let widgets = LikeWidgets {
            button: button.downgrade(),
            refresh: refresh.downgrade(),
            icon: icon.downgrade(),
            label: glib::WeakRef::new(),
            message: message.downgrade(),
            confirmed: Rc::new(Cell::new(None)),
            compact: true,
            spinner: spinner.downgrade(),
            feedback: feedback.downgrade(),
        };
        let initialized = Rc::new(Cell::new(false));
        let weak = Rc::downgrade(self);
        let read_widgets = widgets.clone();
        let read_id = id.clone();
        button.connect_map(move |_| {
            if initialized.replace(true) {
                return;
            }
            let weak = weak.clone();
            let widgets = read_widgets.clone();
            let id = read_id.clone();
            let initialized = initialized.clone();
            // Mapping can occur synchronously inside a render's immutable
            // model borrow, especially with reduced motion. Start the bounded
            // read only after that native callback has unwound. No widget or
            // controller is kept alive, and retirement invalidates the work.
            glib::idle_add_local_once(move || {
                let Some(this) = weak.upgrade() else {
                    return;
                };
                let current = {
                    let model = this.model.borrow();
                    !model.closed
                        && model.account_valid
                        && model.generation == generation
                        && model.queue.current().is_some_and(|track| track.id == id)
                };
                if current
                    && widgets.alive()
                    && widgets
                        .button
                        .upgrade()
                        .is_some_and(|button| button.is_mapped())
                {
                    this.read_like(id, widgets);
                } else {
                    initialized.set(false);
                }
            });
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
        // Connect map before parenting: an already visible footer can map a
        // freshly inserted button synchronously.
        self.widgets.current_like_slot.append(&container);
    }

    /// Called only for an actual loaded track detail. Membership is read when
    /// these controls map; track metadata is never interpreted as a like.
    pub(super) fn like_controls(self: &Rc<Self>, id: String) -> gtk::Box {
        self.like_controls_presentation(id, false)
    }

    fn like_controls_presentation(self: &Rc<Self>, id: String, compact: bool) -> gtk::Box {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
        container.set_widget_name("music-like-controls");
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let button = gtk::Button::new();
        button.set_widget_name("music-like-toggle");
        button.set_sensitive(false);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let icon = gtk::Image::from_icon_name("starred-symbolic");
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
                    return;
                }
                match result {
                    Ok(state) => widgets.confirmed(state.liked, true),
                    Err(error) => widgets.unknown(interaction_message(error), true),
                }
            },
        );
    }
}

fn interaction_message(error: InteractionError) -> &'static str {
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

fn interaction_diagnostic(operation: &'static str, error: Option<InteractionError>) {
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
