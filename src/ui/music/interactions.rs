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
        }
        if let Some(button) = self.refresh.upgrade() {
            button.set_sensitive(refresh);
        }
        if let Some(label) = self.message.upgrade() {
            label.set_text(message);
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
        }
    }
}

impl Controller {
    /// Called only for an actual loaded track detail. Membership is read when
    /// these controls map; track metadata is never interpreted as a like.
    pub(super) fn like_controls(self: &Rc<Self>, id: String) -> gtk::Box {
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
        self.run(
            Operation::LikeRead,
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
            widgets.unknown(
                "Сначала обновите состояние отметки. Изменение не отправлено.",
                true,
            );
            return;
        };
        if self.mutation_busy.replace(true) {
            widgets.unknown("Другая правка библиотеки ещё выполняется. Изменение не отправлено; обновите состояние позже.", true);
            return;
        }
        // Clear known UI state while the one explicit write and its confirming
        // read are in flight. Repeated clicks cannot enqueue another POST.
        widgets.unknown("Сохранение и проверка отметки…", false);
        self.run(
            Operation::Mutation,
            move |service, token| Ok(service.set_track_liked(&token, &id, !liked)),
            move |this, result| {
                this.mutation_busy.set(false);
                let result = result.unwrap_or_else(|error| Err(InteractionError::Music(error)));
                interaction_diagnostic("like_write", result.as_ref().err().copied());
                // A write may already have reached the service even when its
                // response was lost. Invalidate the local library cache in all
                // outcomes; the next library read must ask Music again.
                this.model.borrow_mut().liked = None;
                this.render_home_preview();
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
