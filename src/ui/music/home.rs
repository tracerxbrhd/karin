// SPDX-License-Identifier: GPL-3.0-or-later
//! Modest native starting surface over the existing real music destinations.

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
        let introduction = gtk::Box::new(gtk::Orientation::Vertical, 6);
        introduction.set_widget_name("music-home-introduction");
        introduction.append(&text("Ваша музыка", "title-1"));
        introduction.append(&text(
            "Вернитесь к любимому или найдите новое.",
            "dim-label",
        ));
        home_signedin.append(&introduction);

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

        let home_wave = WaveParts::new();
        home_signedin.append(&home_wave.widget);
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
    pub start: gtk::Button,
    pub cancel: gtk::Button,
    pub toggle: gtk::Button,
    experiment: gtk::Label,
    message: gtk::Label,
    details: gtk::Label,
    spinner: gtk::Spinner,
    actions: gtk::Box,
    transport: gtk::Box,
}

impl WaveParts {
    fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        // Preserve the ordinary, unavailable Home presentation until local
        // experimental mode is explicitly enabled by the caller.
        widget.set_widget_name("music-wave-placeholder");
        widget.add_css_class("music-home-wave");
        widget.set_valign(gtk::Align::Start);
        let icon = gtk::Image::from_icon_name("radio-symbolic");
        icon.set_pixel_size(24);
        icon.set_valign(gtk::Align::Start);
        icon.add_css_class("dim-label");
        widget.append(&icon);

        let labels = gtk::Box::new(gtk::Orientation::Vertical, 4);
        labels.set_hexpand(true);
        labels.append(&text("Моя волна", "heading"));
        let experiment = text(
            "Экспериментальная проверка. Работа с Яндекс Музыкой пока не подтверждена.",
            "caption",
        );
        experiment.set_widget_name("music-wave-experimental-note");
        labels.append(&experiment);
        let progress = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let spinner = gtk::Spinner::new();
        spinner.set_widget_name("music-wave-spinner");
        spinner.set_valign(gtk::Align::Center);
        progress.append(&spinner);
        let message = text("Персональная волна пока не подключена.", "dim-label");
        message.set_widget_name("music-wave-status");
        progress.append(&message);
        labels.append(&progress);
        let details = text("", "caption");
        details.set_widget_name("music-wave-safe-diagnostic");
        details.add_css_class("dim-label");
        labels.append(&details);

        // Keep a possible fresh-session action above the existing-session
        // controls: even an error while audio finishes must fit compact Home.
        let actions = gtk::Box::new(gtk::Orientation::Vertical, 8);
        actions.set_margin_top(8);
        let start = gtk::Button::with_label("Запустить волну");
        start.set_widget_name("music-wave-start");
        start.set_halign(gtk::Align::Start);
        start.add_css_class("suggested-action");
        start.set_tooltip_text(Some("Начать новую экспериментальную радиосессию"));
        actions.append(&start);
        let transport = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let toggle = gtk::Button::with_label("Продолжить");
        toggle.set_widget_name("music-wave-toggle");
        transport.append(&toggle);
        let cancel = gtk::Button::with_label("Отменить");
        cancel.set_widget_name("music-wave-cancel");
        transport.append(&cancel);
        actions.append(&transport);
        labels.append(&actions);
        widget.append(&labels);
        let parts = Self {
            widget,
            start,
            cancel,
            toggle,
            experiment,
            message,
            details,
            spinner,
            actions,
            transport,
        };
        parts.render(RadioStatus {
            phase: RadioPhase::Disabled,
            playback: None,
            authenticated: false,
        });
        parts
    }

    pub(super) fn render(&self, status: RadioStatus) {
        let enabled = status.phase != RadioPhase::Disabled;
        self.experiment.set_visible(enabled);
        let busy = matches!(
            status.phase,
            RadioPhase::Starting | RadioPhase::LoadingNext | RadioPhase::Finishing
        );
        self.spinner.set_spinning(busy);
        self.spinner.set_visible(busy);
        let message = match status.phase {
            RadioPhase::Disabled => "Персональная волна пока не подключена.",
            RadioPhase::Ready if !status.authenticated => {
                "Войдите в Яндекс, чтобы выполнить локальную проверку волны."
            }
            RadioPhase::Ready => "Запустите персональное радио с текущим музыкальным аккаунтом.",
            RadioPhase::Starting => "Создаём радиосессию и получаем первые треки…",
            RadioPhase::LoadingNext => "Получаем следующую партию треков…",
            RadioPhase::Finishing => "Завершаем радиосессию локально…",
            RadioPhase::Active => match status.playback {
                Some(PlaybackState::Playing) => "Волна играет в плеере Karin.",
                Some(PlaybackState::Paused) => "Волна на паузе.",
                Some(PlaybackState::Loading | PlaybackState::Ready) => {
                    "Подготавливаем воспроизведение…"
                }
                _ => "Радиосессия активна. Управление воспроизведением — в плеере Karin.",
            },
            RadioPhase::Failed { failure, .. } => failure_message(failure),
        };
        self.message.set_text(message);
        let failed = matches!(status.phase, RadioPhase::Failed { .. });
        if failed {
            self.message.remove_css_class("dim-label");
            self.message.add_css_class("error");
        } else {
            self.message.remove_css_class("error");
            self.message.add_css_class("dim-label");
        }
        if let RadioPhase::Failed { stage, failure } = status.phase {
            // Both substitutions come exclusively from closed local enums.
            // Never format an API error/response or a service/session identity.
            self.details.set_text(&format!(
                "Этап: {} · Причина: {}",
                stage_label(stage),
                failure_label(failure)
            ));
        } else {
            self.details.set_text("");
        }
        self.details.set_visible(failed);

        let can_start = enabled
            && status.authenticated
            && matches!(status.phase, RadioPhase::Ready | RadioPhase::Failed { .. });
        self.start.set_visible(can_start);
        self.start.set_sensitive(can_start);
        self.start.set_label(if failed {
            "Начать новую волну"
        } else {
            "Запустить волну"
        });
        let has_transport = enabled
            && status.authenticated
            && matches!(
                status.playback,
                Some(PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ready)
            );
        self.toggle.set_visible(has_transport);
        self.toggle
            .set_sensitive(has_transport && status.phase != RadioPhase::Finishing);
        self.toggle
            .set_label(if status.playback == Some(PlaybackState::Playing) {
                "Пауза"
            } else {
                "Продолжить"
            });
        let can_cancel = enabled
            && (matches!(
                status.phase,
                RadioPhase::Starting
                    | RadioPhase::Active
                    | RadioPhase::LoadingNext
                    | RadioPhase::Finishing
            ) || failed);
        self.cancel.set_visible(can_cancel);
        self.cancel
            .set_sensitive(can_cancel && status.phase != RadioPhase::Finishing);
        self.cancel
            .set_label(if status.phase == RadioPhase::Starting {
                "Отменить"
            } else {
                "Завершить волну"
            });
        self.transport.set_visible(has_transport || can_cancel);
        self.actions
            .set_visible(can_start || has_transport || can_cancel);
    }
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
        RadioFailure::Unsupported => "Ответ сервиса не соответствует проверяемому контракту волны.",
        RadioFailure::Empty => "В этой партии сервис не вернул доступных треков.",
        RadioFailure::Unavailable => "Не удалось подготовить аудио для трека волны.",
        RadioFailure::Busy => "Предыдущая операция ещё выполняется.",
        RadioFailure::OutcomeUnknown => {
            "Неизвестно, принял ли сервис событие. Этот запрос не будет повторён автоматически."
        }
        RadioFailure::Capacity => "Достигнут локальный предел волны. Можно начать новую сессию.",
    }
}

fn shortcut(name: &str, icon: &str, title: &str, description: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.add_css_class("music-home-shortcut");
    button.set_width_request(216);
    button.set_height_request(84);
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
