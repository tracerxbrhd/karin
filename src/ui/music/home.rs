// SPDX-License-Identifier: GPL-3.0-or-later
//! Native music Home: My Wave and the existing library destinations.

use super::radio::{RadioFailure, RadioPhase, RadioStage, RadioStatus};
use super::*;
use crate::playback::PlaybackState;

#[cfg(test)]
mod tests;

pub(super) struct HomeParts {
    pub home: gtk::Box,
    pub home_signedout: gtk::Box,
    pub home_signedin: gtk::Box,
    pub home_hero: gtk::Box,
    pub home_liked: gtk::Button,
    pub home_playlists: gtk::Button,
    pub home_search: gtk::Button,
    pub home_wave: WaveParts,
    pub home_queue: gtk::Label,
    pub home_preview: gtk::ListBox,
    pub home_collections: gtk::FlowBox,
}

impl HomeParts {
    pub(super) fn new() -> Self {
        let home = gtk::Box::new(gtk::Orientation::Vertical, 20);
        home.set_widget_name("music-home");
        let home_signedout = gtk::Box::new(gtk::Orientation::Vertical, 16);
        home_signedout.set_margin_top(24);
        let icon = gtk::Image::from_icon_name("audio-x-generic-symbolic");
        icon.set_pixel_size(64);
        icon.set_halign(gtk::Align::Start);
        icon.add_css_class("accent");
        home_signedout.append(&icon);
        home_signedout.append(&text("Ваша музыка — в Karin", "title-1"));
        home_signedout.append(&text(
            "Войдите в Яндекс, чтобы слушать любимые треки и свои плейлисты.",
            "dim-label",
        ));
        home.append(&home_signedout);

        let home_signedin = gtk::Box::new(gtk::Orientation::Vertical, 24);
        home_signedin.set_widget_name("music-home-library");
        let home_wave = WaveParts::new();
        home_signedin.append(&home_wave.widget);

        // These are ordinary existing controller actions, not another library
        // snapshot or an eager read of personalized recommendations.
        let routes = gtk::FlowBox::new();
        routes.set_widget_name("music-home-shortcuts");
        routes.set_selection_mode(gtk::SelectionMode::None);
        routes.set_focusable(false);
        routes.set_homogeneous(true);
        routes.set_min_children_per_line(1);
        routes.set_max_children_per_line(3);
        routes.set_column_spacing(12);
        routes.set_row_spacing(12);
        let home_liked = shortcut(
            "music-home-liked",
            interactions::like_icon_name(None),
            "Нравится",
            "Любимые композиции",
        );
        let home_playlists = shortcut(
            "music-home-playlists",
            "view-list-symbolic",
            "Плейлисты",
            "Ваши музыкальные подборки",
        );
        let home_search = shortcut(
            "music-home-search",
            "system-search-symbolic",
            "Поиск",
            "Треки, альбомы и исполнители",
        );
        for button in [&home_liked, &home_playlists, &home_search] {
            routes.insert(button, -1);
            if let Some(wrapper) = button.parent() {
                wrapper.set_focusable(false);
            }
        }
        home_signedin.append(&routes);

        let home_queue = text("", "caption");
        home_queue.set_widget_name("music-home-queue-summary");
        home_queue.add_css_class("dim-label");
        home_queue.set_visible(false);
        home_signedin.append(&home_queue);
        home.append(&home_signedin);

        // Retain the existing renderer targets without mounting a second full
        // liked list/grid on Home. No new request or stale personal preview is
        // created; future verified sections can be added to the native Box.
        let home_hero = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        home_hero.set_widget_name("music-home-hero");
        let home_preview = list();
        home_preview.set_widget_name("music-home-liked-preview");
        let home_collections = catalog_layout::tile_grid();
        home_collections.set_widget_name("music-home-collections");
        Self {
            home,
            home_signedout,
            home_signedin,
            home_hero,
            home_liked,
            home_playlists,
            home_search,
            home_wave,
            home_queue,
            home_preview,
            home_collections,
        }
    }
}

/// Presentation only: no session identity, service model or network response
/// can enter this widget. The controller supplies fixed categories and the
/// actual snapshot of the same player used by the footer and MPRIS.
#[derive(Clone)]
pub(super) struct WaveParts {
    pub widget: gtk::Box,
    pub primary: gtk::Button,
    pub cancel: gtk::Button,
    primary_content: adw::ButtonContent,
    message: gtk::Label,
    details: gtk::Label,
    disclosure: gtk::Expander,
    spinner: gtk::Spinner,
}

impl WaveParts {
    fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        widget.set_widget_name("music-wave-hero");
        widget.add_css_class("card");
        widget.add_css_class("music-wave-hero");
        widget.set_valign(gtk::Align::Start);
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        let title = text("Моя волна", "music-wave-title");
        title.set_hexpand(true);
        title.set_valign(gtk::Align::Center);
        heading.append(&title);
        heading.append(&wave_motif());
        widget.append(&heading);
        let subtitle = text(
            "Персональная музыка, которая подстраивается под ваш вкус.",
            "music-wave-subtitle",
        );
        subtitle.add_css_class("dim-label");
        widget.append(&subtitle);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        actions.set_widget_name("music-wave-actions");
        actions.set_margin_top(4);
        let primary = gtk::Button::new();
        primary.set_widget_name("music-wave-primary");
        primary.set_halign(gtk::Align::Start);
        primary.add_css_class("suggested-action");
        primary.add_css_class("music-wave-primary");
        let primary_content = adw::ButtonContent::builder()
            .icon_name("media-playback-start-symbolic")
            .label("Слушать")
            .build();
        primary.set_child(Some(&primary_content));
        actions.append(&primary);
        let spinner = gtk::Spinner::new();
        spinner.set_widget_name("music-wave-spinner");
        spinner.set_valign(gtk::Align::Center);
        actions.append(&spinner);
        let cancel = gtk::Button::with_label("Отменить");
        cancel.set_widget_name("music-wave-cancel");
        cancel.set_valign(gtk::Align::Center);
        actions.append(&cancel);
        widget.append(&actions);

        let message = text("", "body");
        message.set_widget_name("music-wave-status");
        widget.append(&message);
        let details = text("", "caption");
        details.set_widget_name("music-wave-safe-diagnostic");
        let disclosure = gtk::Expander::new(Some("Подробности"));
        disclosure.set_widget_name("music-wave-error-details");
        disclosure.set_child(Some(&details));
        widget.append(&disclosure);
        let parts = Self {
            widget,
            primary,
            cancel,
            primary_content,
            message,
            details,
            disclosure,
            spinner,
        };
        parts.render(RadioStatus {
            phase: RadioPhase::Ready,
            playback: None,
            authenticated: false,
        });
        parts
    }

    pub(super) fn render(&self, status: RadioStatus) {
        let failed = matches!(status.phase, RadioPhase::Failed { .. });
        let transport = matches!(status.phase, RadioPhase::Active | RadioPhase::LoadingNext);
        let playing = status.playback == Some(PlaybackState::Playing);
        let resumable = matches!(
            status.playback,
            Some(PlaybackState::Paused | PlaybackState::Ready)
        );
        let starting = status.phase == RadioPhase::Starting;
        let finishing = status.phase == RadioPhase::Finishing;
        let preparing = transport && !playing && !resumable;
        let busy = starting || finishing || preparing;
        self.spinner.set_spinning(busy);
        self.spinner.set_visible(busy);
        let label = if failed {
            "Начать новую волну"
        } else if transport && playing {
            "Пауза"
        } else if transport && resumable {
            "Продолжить"
        } else if starting || preparing {
            "Загрузка…"
        } else {
            "Слушать"
        };
        self.primary_content.set_label(label);
        self.primary_content.set_icon_name(if transport && playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        });
        self.primary
            .update_property(&[gtk::accessible::Property::Label(label)]);
        self.primary.set_sensitive(
            status.authenticated
                && !busy
                && (matches!(status.phase, RadioPhase::Ready | RadioPhase::Failed { .. })
                    || (transport && (playing || resumable))),
        );
        self.primary
            .set_tooltip_text(Some(if !status.authenticated {
                "Войдите в Яндекс, чтобы слушать музыку"
            } else {
                label
            }));
        // Cancel is a startup operation. Active radio has no separate Finish or
        // Stop affordance: the common player/source change owns that lifecycle.
        self.cancel.set_visible(starting);
        self.cancel.set_sensitive(starting);

        let message = match status.phase {
            RadioPhase::Ready if !status.authenticated => "Войдите в Яндекс, чтобы слушать музыку.",
            RadioPhase::Starting => "Подбираем первые треки…",
            RadioPhase::LoadingNext if preparing => "Подбираем следующие треки…",
            RadioPhase::Finishing => "Готовимся к следующему запуску…",
            RadioPhase::Failed { failure, .. } => failure_message(failure),
            _ => "",
        };
        self.message.set_text(message);
        self.message.set_visible(!message.is_empty());
        if failed {
            self.message.add_css_class("error");
        } else {
            self.message.remove_css_class("error");
        }
        if let RadioPhase::Failed { stage, failure } = status.phase {
            // Closed local enums only; no response body or service identity.
            self.details.set_text(&format!(
                "Этап: {} · Причина: {}",
                stage_label(stage),
                failure_label(failure)
            ));
        } else {
            self.details.set_text("");
            self.disclosure.set_expanded(false);
        }
        self.disclosure.set_visible(failed);
    }
}

/// Original static decoration. It never animates or claims playback activity.
fn wave_motif() -> gtk::Box {
    let motif = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    motif.set_widget_name("music-wave-motif");
    motif.add_css_class("music-wave-motif");
    motif.set_valign(gtk::Align::Center);
    motif.set_halign(gtk::Align::End);
    motif.set_can_target(false);
    for height in [10, 20, 32, 44, 32, 20, 10] {
        let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bar.set_size_request(4, height);
        bar.set_valign(gtk::Align::Center);
        motif.append(&bar);
    }
    motif
}

impl Controller {
    pub(super) fn render_wave(&self) {
        self.widgets.home_wave.render(self.radio_status());
    }
}

fn stage_label(stage: RadioStage) -> &'static str {
    match stage {
        RadioStage::Session => "создание сессии",
        RadioStage::Tracks => "получение партии",
        RadioStage::Normalization => "проверка треков",
        RadioStage::Delivery => "получение аудио",
        RadioStage::Playback => "воспроизведение",
        RadioStage::Feedback => "событие прослушивания",
        RadioStage::Local => "локальное управление",
    }
}

fn failure_label(failure: RadioFailure) -> &'static str {
    match failure {
        RadioFailure::Authentication => "нужен повторный вход",
        RadioFailure::Forbidden => "доступ отклонён",
        RadioFailure::Restricted => "ограничение сервиса",
        RadioFailure::RateLimited => "ограничение частоты",
        RadioFailure::Network => "сеть недоступна",
        RadioFailure::Timeout => "время ожидания истекло",
        RadioFailure::Http => "запрос отклонён",
        RadioFailure::Unsupported => "неподдерживаемый ответ",
        RadioFailure::Empty => "пустая партия",
        RadioFailure::Unavailable => "нет доступного аудио",
        RadioFailure::Busy => "операция уже выполняется",
        RadioFailure::OutcomeUnknown => "результат записи неизвестен",
        RadioFailure::Capacity => "локальный предел очереди",
    }
}

fn failure_message(failure: RadioFailure) -> &'static str {
    match failure {
        RadioFailure::Authentication => {
            "Музыкальная сессия больше не подтверждена. Войдите заново."
        }
        RadioFailure::Forbidden | RadioFailure::Restricted => {
            "Яндекс Музыка не разрешила эту операцию с волной."
        }
        RadioFailure::RateLimited => "Сервис ограничил частоту запросов. Попробуйте позднее.",
        RadioFailure::Network | RadioFailure::Timeout => {
            "Не удалось связаться с Яндекс Музыкой. Проверьте подключение."
        }
        RadioFailure::Http => "Яндекс Музыка отклонила запрос волны.",
        RadioFailure::Unsupported => "Не удалось обработать ответ сервиса.",
        RadioFailure::Empty => "В этой партии сервис не вернул доступных треков.",
        RadioFailure::Unavailable => "Не удалось подготовить аудио для трека волны.",
        RadioFailure::Busy => "Предыдущая операция ещё выполняется.",
        RadioFailure::OutcomeUnknown => {
            "Неизвестно, принят ли запрос. Автоматического повтора не будет."
        }
        RadioFailure::Capacity => "Достигнут локальный предел волны. Можно начать новую сессию.",
    }
}

fn shortcut(name: &str, icon: &str, title: &str, description: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.add_css_class("music-home-shortcut");
    button.set_width_request(200);
    button.set_height_request(72);
    button.set_hexpand(true);
    button.set_tooltip_text(Some(title));
    button.update_property(&[gtk::accessible::Property::Label(title)]);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    super::super::margins(&content, 12);
    let icon = gtk::Image::from_icon_name(icon);
    icon.set_pixel_size(24);
    icon.set_valign(gtk::Align::Center);
    content.append(&icon);
    let labels = gtk::Box::new(gtk::Orientation::Vertical, 4);
    labels.set_hexpand(true);
    labels.set_valign(gtk::Align::Center);
    labels.append(&text(title, "heading"));
    let subtitle = text(description, "dim-label");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.set_wrap(false);
    subtitle.set_width_chars(1);
    subtitle.set_tooltip_text(Some(description));
    labels.append(&subtitle);
    content.append(&labels);
    button.set_child(Some(&content));
    button
}
