// SPDX-License-Identifier: GPL-3.0-or-later
//! Native music Home and its validated shortcuts into the shared player and catalog.

use super::continuous_loading::FeedSource;
use super::radio::{RadioFailure, RadioPhase, RadioStage, RadioStatus};
use super::*;
use crate::{
    music::{PlaylistId, SearchPlaylist, wave::WaveSource},
    playback::PlaybackState,
};
use std::{collections::VecDeque, time::Duration};

#[cfg(test)]
mod tests;

pub(super) struct HomeParts {
    pub home: gtk::Box,
    pub home_signedin: gtk::Box,
    pub home_hero: gtk::Box,
    pub home_liked: gtk::Button,
    pub home_playlists: gtk::Button,
    pub home_search: gtk::Button,
    pub home_wave: WaveParts,
    pub home_continue: ContinuationParts,
    pub home_recent: RecentParts,
    pub home_queue: gtk::Label,
    pub home_preview: gtk::ListBox,
    pub home_collections: gtk::FlowBox,
}

impl HomeParts {
    pub(super) fn new() -> Self {
        let home = gtk::Box::new(gtk::Orientation::Vertical, 20);
        home.set_widget_name("music-home");
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
            "karin-playlist-symbolic",
            "Плейлисты",
            "Ваши музыкальные подборки",
        );
        let home_search = shortcut(
            "music-home-search",
            "karin-search-symbolic",
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

        let home_continue = ContinuationParts::new();
        home_signedin.append(&home_continue.widget);
        let home_recent = RecentParts::new();
        home_signedin.append(&home_recent.widget);

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
            home_signedin,
            home_hero,
            home_liked,
            home_playlists,
            home_search,
            home_wave,
            home_continue,
            home_recent,
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
    pub settings: gtk::Button,
    pub liked: gtk::Button,
    options: gtk::Box,
    primary_content: adw::ButtonContent,
    message: gtk::Label,
    details: gtk::Label,
    disclosure: gtk::Expander,
    spinner: gtk::Spinner,
}

impl WaveParts {
    fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 8);
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
            .icon_name("karin-player-play-symbolic")
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

        let options = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let settings = gtk::Button::with_label("Настроить");
        settings.set_widget_name("music-wave-settings");
        settings.update_property(&[gtk::accessible::Property::Label("Настроить Мою волну")]);
        options.append(&settings);
        let liked = gtk::Button::with_label("По «Мне нравится»");
        liked.set_widget_name("music-home-liked-wave");
        liked.update_property(&[gtk::accessible::Property::Label(
            "Моя волна по коллекции «Мне нравится»",
        )]);
        options.append(&liked);
        widget.append(&options);

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
            settings,
            liked,
            options,
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
            "karin-player-pause-symbolic"
        } else {
            "karin-player-play-symbolic"
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
        self.options.set_visible(status.authenticated);

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

#[derive(Clone)]
pub(super) struct ContinuationParts {
    widget: gtk::Box,
    pub action: gtk::Button,
    track: gtk::Label,
    status: gtk::Label,
}

impl ContinuationParts {
    fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 8);
        widget.set_widget_name("music-home-continuation");
        widget.set_visible(false);
        widget.append(&text("Продолжить слушать", "title-3"));
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let metadata = gtk::Box::new(gtk::Orientation::Vertical, 4);
        metadata.set_hexpand(true);
        let track = text("", "heading");
        track.set_widget_name("music-home-continuation-track");
        track.set_wrap(false);
        track.set_ellipsize(gtk::pango::EllipsizeMode::End);
        metadata.append(&track);
        let status = text("", "dim-label");
        status.set_wrap(false);
        status.set_ellipsize(gtk::pango::EllipsizeMode::End);
        metadata.append(&status);
        row.append(&metadata);
        let action = gtk::Button::new();
        action.set_widget_name("music-home-continue-playback");
        action.set_valign(gtk::Align::Center);
        action.set_sensitive(false);
        row.append(&action);
        widget.append(&row);
        Self {
            widget,
            action,
            track,
            status,
        }
    }

    pub(super) fn render(
        &self,
        track: Option<&Track>,
        state: PlaybackState,
        available: bool,
        loading: bool,
        position: Option<Duration>,
    ) {
        let Some(track) = track else {
            self.clear();
            return;
        };
        self.widget.set_visible(true);
        if self.track.text() != track.title {
            self.track.set_text(&track.title);
        }
        let status = if loading {
            "Подготовка аудио…".into()
        } else {
            position.map_or_else(String::new, player::music_time)
        };
        if self.status.text() != status {
            self.status.set_text(&status);
        }
        self.status.set_visible(!status.is_empty());
        let action = if state == PlaybackState::Playing {
            "Пауза"
        } else {
            "Продолжить"
        };
        if self.action.label().as_deref() != Some(action) {
            self.action.set_label(action);
        }
        self.action.set_sensitive(available);
    }

    pub(super) fn clear(&self) {
        if self.widget.is_visible() {
            self.widget.set_visible(false);
            self.action.set_sensitive(false);
            self.track.set_text("");
            self.status.set_text("");
        }
    }
}

const MAX_RECENT_COLLECTIONS: usize = 4;

#[derive(Clone)]
enum RecentCollection {
    Playlist(Playlist),
    SearchPlaylist(SearchPlaylist),
    Album { id: String, title: String },
}

impl RecentCollection {
    fn playlist_id(&self) -> Option<&PlaylistId> {
        match self {
            Self::Playlist(item) => Some(&item.id),
            Self::SearchPlaylist(item) => Some(&item.id),
            Self::Album { .. } => None,
        }
    }

    fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Album { id, .. }, Self::Album { id: other, .. }) => id == other,
            _ => self
                .playlist_id()
                .is_some_and(|id| Some(id) == other.playlist_id()),
        }
    }

    fn title(&self) -> &str {
        match self {
            Self::Playlist(item) => &item.title,
            Self::SearchPlaylist(item) => &item.title,
            Self::Album { title, .. } => title,
        }
    }
}

#[derive(Default)]
struct RecentHistory {
    generation: Option<u64>,
    items: VecDeque<RecentCollection>,
}

impl RecentHistory {
    fn record(&mut self, generation: u64, collection: RecentCollection) {
        if self.generation != Some(generation) {
            self.items.clear();
            self.generation = Some(generation);
        }
        self.items.retain(|recent| !recent.matches(&collection));
        self.items.push_front(collection);
        self.items.truncate(MAX_RECENT_COLLECTIONS);
    }
}

#[derive(Clone)]
pub(super) struct RecentParts {
    widget: gtk::Box,
    rows: gtk::ListBox,
    history: Rc<RefCell<RecentHistory>>,
}

impl RecentParts {
    fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 8);
        widget.set_widget_name("music-home-recent");
        widget.set_visible(false);
        widget.append(&text("Недавно открывали", "title-3"));
        let rows = gtk::ListBox::new();
        rows.set_selection_mode(gtk::SelectionMode::None);
        rows.add_css_class("boxed-list");
        widget.append(&rows);
        Self {
            widget,
            rows,
            history: Rc::default(),
        }
    }

    pub(super) fn clear(&self) {
        self.history.replace(RecentHistory::default());
        clear_list(&self.rows);
        self.widget.set_visible(false);
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
        let status = self.radio_status();
        self.widgets.home_wave.render(status);
        self.widgets.home_wave.liked.set_sensitive(
            status.authenticated
                && !matches!(status.phase, RadioPhase::Starting | RadioPhase::Finishing)
                && (matches!(status.phase, RadioPhase::Failed { .. })
                    || !self.wave_source_in_use(&WaveSource::LikedCollection)),
        );
    }

    pub(super) fn start_liked_wave_from_home(self: &Rc<Self>) {
        if self.widgets.home_wave.liked.is_sensitive() {
            self.start_wave_from(WaveSource::LikedCollection);
        }
    }

    pub(super) fn record_home_collection(self: &Rc<Self>, source: &FeedSource) {
        if self.widgets.stack.visible_child_name().as_deref() != Some("detail") {
            return;
        }
        let model = self.model.borrow();
        if !model.account_valid
            || model.closed
            || model.token.as_ref().is_none_or(AccessToken::is_expired)
        {
            return;
        }
        let generation = model.generation;
        drop(model);
        let collection = match source {
            FeedSource::Playlist(item) => RecentCollection::Playlist(item.clone()),
            FeedSource::SearchPlaylist(item) => RecentCollection::SearchPlaylist(item.clone()),
            FeedSource::Album(item) => RecentCollection::Album {
                id: item.id.clone(),
                title: item.title.clone(),
            },
            _ => return,
        };
        let recent = &self.widgets.home_recent;
        recent.history.borrow_mut().record(generation, collection);
        clear_list(&recent.rows);
        for item in &recent.history.borrow().items {
            let row = adw::ActionRow::builder()
                .title(item.title())
                .subtitle(if item.playlist_id().is_some() {
                    "Плейлист"
                } else {
                    "Альбом"
                })
                .title_lines(1)
                .use_markup(false)
                .activatable(true)
                .focusable(true)
                .build();
            row.set_widget_name("music-home-recent-collection");
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            let weak = Rc::downgrade(self);
            let item = item.clone();
            row.connect_activated(move |_| {
                let Some(this) = weak.upgrade() else {
                    return;
                };
                let model = this.model.borrow();
                if !model.account_valid
                    || model.closed
                    || model.generation != generation
                    || model.token.as_ref().is_none_or(AccessToken::is_expired)
                {
                    return;
                }
                drop(model);
                match &item {
                    RecentCollection::Playlist(item) => this.open_playlist(item.clone(), 0),
                    RecentCollection::SearchPlaylist(item) => {
                        this.open_search_playlist(item.clone(), 0)
                    }
                    RecentCollection::Album { id, .. } => this.open_album(id.clone()),
                }
            });
            recent.rows.append(&row);
        }
        recent.widget.set_visible(true);
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

pub(super) fn failure_label(failure: RadioFailure) -> &'static str {
    match failure {
        RadioFailure::Authentication => "нужен повторный вход",
        RadioFailure::Forbidden => "доступ отклонён",
        RadioFailure::Restricted => "ограничение сервиса",
        RadioFailure::RateLimited => "ограничение частоты",
        RadioFailure::Network => "сеть недоступна",
        RadioFailure::Timeout => "время ожидания истекло",
        RadioFailure::Http => "запрос отклонён",
        RadioFailure::Unsupported => "неподдерживаемый ответ",
        RadioFailure::SourceNotAccepted => "сервис не подтвердил источник рекомендаций",
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
        RadioFailure::SourceNotAccepted => {
            "Яндекс Музыка не подтвердила выбранный источник рекомендаций."
        }
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
