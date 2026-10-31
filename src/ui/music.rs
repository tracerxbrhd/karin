// SPDX-License-Identifier: GPL-3.0-or-later
//! Native account/catalog presentation. Only typed application results reach GTK.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use adw::prelude::*;
use gtk::{gio, glib};

mod account;
mod catalog_view;
#[cfg(test)]
mod diagnostic_tests;
mod interactions;
#[cfg(test)]
mod navigation_tests;
mod persistence;
mod player;
mod playlist_creation;
mod playlist_grid;
mod queue_view;
mod requests;
#[cfg(test)]
mod row_activation_tests;
#[cfg(test)]
mod search_tests;
mod search_view;
mod system_media;
#[cfg(test)]
mod test_input;
mod track_pane;
mod workspace;
use requests::{Operation, Requests};
type PendingJob = Box<dyn FnOnce(&Rc<Controller>, u64)>;

/// The row owns its typed action; removing/replacing it also removes the
/// action's captured metadata. The list has one user-activation dispatcher.
enum RowAction {
    Track(PlaybackContext),
    SearchPlaylist(crate::music::SearchPlaylist),
    Album(String),
    Artist(Artist),
    Queue(usize),
}

use super::auth::LoginDialog;
use crate::auth::AuthorizationCode;
#[cfg(feature = "auth-webkit")]
use crate::auth::SessionCookies;
use crate::{
    artwork::{ArtworkLoader, CoverArt},
    auth::{AuthAttempt, AuthError, AuthProvider, BrowserMode},
    music::{Album, Artist, MusicError, MusicService, Page, Playlist, SearchKind, Track},
    queue::{ContextKind, MusicQueue, PlaybackContext},
    session::AccessToken,
};
pub(super) struct MusicPane {
    pub widget: gtk::Box,
    pub player_bar: gtk::Box,
    controller: Rc<Controller>,
}

struct Model {
    token: Option<AccessToken>,
    account_valid: bool,
    busy: bool,
    auth_busy: bool,
    closed: bool,
    generation: u64,
    view_generation: u64,
    detail_origin: String,
    library_touched: bool,
    delivery_diagnostic: bool,
    delivery_cancellation: Option<crate::stream::DeliveryCancellation>,
    library_page: u32,
    search_page: u32,
    search_scope: Option<SearchKind>,
    queue: MusicQueue,
    attempt: Option<Arc<AuthAttempt>>,
    liked: Option<Result<Page<Track>, MusicError>>,
    playlists: Option<Result<Page<Playlist>, MusicError>>,
    library_origins: Vec<(u32, u32, crate::catalog_cache::Provenance, bool)>,
    library_displaying: Option<(u32, u32)>,
}

#[derive(Clone)]
struct Widgets {
    account_bar: gtk::Box,
    account_menu: gtk::MenuButton,
    account_open: gtk::Button,
    account_launch_error: gtk::Label,
    account: gtk::Label,
    notice: gtk::Label,
    login: gtk::Button,
    logout: gtk::Button,
    spinner: gtk::Spinner,
    status: adw::ActionRow,
    status_revealer: gtk::Revealer,
    home_signedout: gtk::Box,
    home_signedin: gtk::Box,
    home_liked: gtk::Button,
    home_playlists: gtk::Button,
    home_search: gtk::Button,
    home_queue: gtk::Label,
    home_preview: gtk::ListBox,
    stack: gtk::Stack,
    library_mode: gtk::DropDown,
    library_refresh: gtk::Button,
    library_rows: gtk::ListBox,
    cache_notice: adw::Banner,
    cache_hint: gtk::Label,
    library_previous: gtk::Button,
    library_next: gtk::Button,
    search: gtk::SearchEntry,
    search_sections: gtk::Box,
    search_all: gtk::Button,
    search_scope_title: gtk::Label,
    search_button: gtk::Button,
    search_feedback: gtk::Box,
    search_rows: gtk::ListBox,
    search_previous: gtk::Button,
    search_next: gtk::Button,
    detail: gtk::Box,
    queue_rows: gtk::ListBox,
    queue_count: gtk::Label,
    current: gtk::Label,
    current_artist: gtk::Label,
    current_art: gtk::Box,
    current_art_button: gtk::Button,
    player_main: gtk::Box,
    position: gtk::Label,
    player_status: gtk::Label,
    play: gtk::Button,
    stop: gtk::Button,
    progress: gtk::Scale,
    volume: gtk::Scale,
    previous: gtk::Button,
    next: gtk::Button,
    clear_queue: gtk::Button,
}

type DetailRefresh = Rc<dyn Fn(&Rc<Controller>)>;

struct Controller {
    app: glib::WeakRef<adw::Application>,
    service: Option<MusicService>,
    provider: Arc<dyn AuthProvider>,
    launch_browser: fn(&str) -> Result<(), AuthError>,
    window: glib::WeakRef<adw::ApplicationWindow>,
    login_dialog: RefCell<Option<LoginDialog>>,
    retiring_logins: RefCell<Vec<glib::WeakRef<adw::Dialog>>>,
    logout_confirmation: RefCell<Option<account::LogoutConfirmation>>,
    model: RefCell<Model>,
    widgets: Widgets,
    requests: RefCell<Requests<PendingJob>>,
    mutation_busy: Cell<bool>,
    artwork: Option<ArtworkLoader>,
    player: RefCell<player::MusicPlayback>,
    system_media: RefCell<system_media::SystemMedia>,
    persistence: RefCell<persistence::State>,
    playlist_grid: playlist_grid::PlaylistGrid,
    track_pane: track_pane::TrackPane,
    info_selection: RefCell<Option<workspace::InfoSelection>>,
    info_split: RefCell<Option<glib::WeakRef<adw::OverlaySplitView>>>,
    catalog: RefCell<Option<crate::catalog_cache::CachedCatalog>>,
    playlist_dialog: RefCell<Option<adw::Dialog>>,
    can_create_playlist: Cell<bool>,
    detail_refresh: RefCell<Option<DetailRefresh>>,
    music_surface_visible: Cell<bool>,
    detail_cache: Cell<Option<(crate::catalog_cache::Provenance, bool)>>,
}

struct OfflineProvider;

impl AuthProvider for OfflineProvider {
    fn begin(&self) -> Result<Arc<AuthAttempt>, AuthError> {
        Err(AuthError::OfflineFixture)
    }

    fn exchange(&self, _: &AuthAttempt, _: AuthorizationCode) -> Result<AccessToken, AuthError> {
        Err(AuthError::OfflineFixture)
    }
}

impl MusicPane {
    pub fn new(
        app: &adw::Application,
        window: &adw::ApplicationWindow,
        experimental: bool,
    ) -> Self {
        Self::with_service(app, window, experimental, MusicService::new().ok())
    }

    /// Explicit visual/structural diagnostic: never contact a real OAuth service.
    /// Ordinary application startup always uses `new` and its live provider.
    pub fn new_offline(
        app: &adw::Application,
        window: &adw::ApplicationWindow,
        _experimental: bool,
    ) -> Self {
        Self::with_provider(
            app,
            window,
            MusicService::new().ok(),
            Arc::new(OfflineProvider),
            launch_system_browser,
        )
    }

    fn with_service(
        app: &adw::Application,
        window: &adw::ApplicationWindow,
        _experimental: bool,
        service: Option<MusicService>,
    ) -> Self {
        Self::with_provider_state(
            app,
            window,
            service,
            crate::auth::public_provider(),
            launch_system_browser,
            persistence::State::system(),
        )
    }

    fn with_provider(
        app: &adw::Application,
        window: &adw::ApplicationWindow,
        service: Option<MusicService>,
        provider: Arc<dyn AuthProvider>,
        launch_browser: fn(&str) -> Result<(), AuthError>,
    ) -> Self {
        Self::with_provider_state(
            app,
            window,
            service,
            provider,
            launch_browser,
            persistence::State::disabled(),
        )
    }

    fn with_provider_state(
        app: &adw::Application,
        window: &adw::ApplicationWindow,
        service: Option<MusicService>,
        provider: Arc<dyn AuthProvider>,
        launch_browser: fn(&str) -> Result<(), AuthError>,
        persistence_state: persistence::State,
    ) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        widget.set_widget_name("music-pane");
        widget.append(&persistence_state.banner);
        let controller_link: Rc<RefCell<std::rc::Weak<Controller>>> = Rc::default();
        let artwork = ArtworkLoader::new().ok();
        let track_pane = track_pane::TrackPane::new({
            let link = controller_link.clone();
            Rc::new(move |action| {
                if let Some(this) = link.borrow().upgrade() {
                    this.handle_track_pane_action(action);
                }
            })
        });
        let playlist_grid = playlist_grid::PlaylistGrid::new(artwork.clone(), {
            let link = controller_link.clone();
            Rc::new(move |action| {
                if let Some(this) = link.borrow().upgrade() {
                    this.handle_playlist_action(action);
                }
            })
        });
        let account_bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        account_bar.set_widget_name("music-account-bar");
        let account_icon = gtk::Image::from_icon_name("avatar-default-symbolic");
        account_icon.set_pixel_size(20);
        account_icon.add_css_class("dim-label");
        account_bar.append(&account_icon);
        account_bar.append(&text("Ваш аккаунт Яндекса", "heading"));
        let account_controls = account::AccountMenu::new();
        let account = account_controls.name.clone();
        account.set_widget_name("music-account");
        let account_menu = account_controls.menu;
        let account_open = account_controls.open_account;
        let account_launch_error = account_controls.launch_error;
        let logout = account_controls.logout;
        let login = gtk::Button::with_label("Войти");
        login.set_widget_name("music-login");
        login.add_css_class("suggested-action");
        account_bar.append(&login);
        widget.append(&account_bar);
        let notice = text(public_notice(), "dim-label");
        notice.set_visible(false);
        // Error feedback is a native inline component; ordinary successful
        // navigation never fills the music workspace with status cards.
        let progress = adw::PreferencesGroup::new();
        let status = adw::ActionRow::builder()
            .title("Выполните вход для доступа к библиотеке и поиску.")
            .title_lines(0)
            .subtitle_lines(0)
            .build();
        status.set_widget_name("music-status");
        let spinner = gtk::Spinner::new();
        spinner.set_valign(gtk::Align::Center);
        status.add_suffix(&spinner);
        progress.add(&status);
        let status_revealer = gtk::Revealer::new();
        status_revealer.set_child(Some(&progress));
        status_revealer.set_reveal_child(false);
        status_revealer.set_visible(false);
        widget.append(&status_revealer);

        let stack = gtk::Stack::new();
        // Measure the visible page. GTK 4.14 otherwise measures hidden wrapped
        // pages at their minimum height, forcing their natural width on the
        // entire window even when the visible page fits a compact allocation.
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);
        stack.set_widget_name("music-sections");
        // The window owns one adaptive navigation surface. Internal pages
        // expose this stack to that surface and to keyboard actions.
        widget.append(&stack);
        let home = gtk::Box::new(gtk::Orientation::Vertical, 24);
        let home_signedout = gtk::Box::new(gtk::Orientation::Vertical, 16);
        let welcome_icon = gtk::Image::from_icon_name("audio-x-generic-symbolic");
        welcome_icon.set_pixel_size(72);
        welcome_icon.set_halign(gtk::Align::Start);
        welcome_icon.add_css_class("accent");
        home_signedout.append(&welcome_icon);
        home_signedout.append(&text("Ваша музыка — в Karin", "title-1"));
        home_signedout.append(&text("Войдите в Яндекс, чтобы открыть понравившиеся треки, личные плейлисты и поиск по каталогу.", "dim-label"));
        home_signedout.append(&text(
            "Karin может запомнить вход в защищённом хранилище. Управляйте этим в настройках.",
            "caption",
        ));
        home.append(&home_signedout);
        let home_signedin = gtk::Box::new(gtk::Orientation::Vertical, 18);
        home_signedin.append(&text("Ваша музыка", "title-1"));
        home_signedin.append(&text(
            "Вернитесь к любимым трекам или найдите что-то новое.",
            "dim-label",
        ));
        let quick = gtk::FlowBox::new();
        quick.set_selection_mode(gtk::SelectionMode::None);
        quick.set_min_children_per_line(1);
        quick.set_max_children_per_line(3);
        quick.set_column_spacing(12);
        quick.set_row_spacing(12);
        quick.set_homogeneous(true);
        let home_liked = destination("starred-symbolic", "Любимые треки", "Ваша коллекция");
        let home_playlists = destination("view-list-symbolic", "Мои плейлисты", "Ваши подборки");
        let home_search = destination("system-search-symbolic", "Поиск", "Весь каталог");
        for button in [&home_liked, &home_playlists, &home_search] {
            quick.insert(button, -1);
        }
        home_signedin.append(&quick);
        let preview_heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        preview_heading.append(&text("Из любимого", "title-3"));
        let all_liked = gtk::Button::with_label("Все треки");
        all_liked.add_css_class("flat");
        all_liked.set_tooltip_text(Some("Открыть понравившиеся треки в библиотеке"));
        let target = home_liked.downgrade();
        all_liked.connect_clicked(move |_| {
            if let Some(target) = target.upgrade() {
                target.emit_clicked();
            }
        });
        preview_heading.append(&all_liked);
        home_signedin.append(&preview_heading);
        let home_preview = list();
        home_preview.set_widget_name("music-home-liked-preview");
        set_empty(&home_preview, "Любимые треки появятся после входа.");
        home_signedin.append(&home_preview);
        let home_queue = text("Очередь пуста", "heading");
        home_signedin.append(&home_queue);
        let open_queue = gtk::Button::with_label("Открыть очередь");
        open_queue.set_action_name(Some("win.music-queue"));
        open_queue.set_halign(gtk::Align::Start);
        home_signedin.append(&open_queue);
        home.append(&home_signedin);
        stack.add_titled(&home, Some("home"), "Главная");
        stack.set_visible_child_name("home");

        let library = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let library_mode = gtk::DropDown::from_strings(&[
            "Понравившиеся треки",
            "Мои плейлисты",
            "Понравившиеся плейлисты",
        ]);
        library_mode.set_hexpand(true);
        library_mode.update_property(&[gtk::accessible::Property::Label("Раздел библиотеки")]);
        let library_controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        library_controls.append(&library_mode);
        let library_refresh = icon_button("view-refresh-symbolic", "Обновить библиотеку");
        library_controls.append(&library_refresh);
        library.append(&library_controls);
        let library_rows = list();
        set_empty(&library_rows, "Ваша библиотека появится после входа.");
        library.append(&library_rows);
        playlist_grid.widget.set_visible(false);
        library.append(&playlist_grid.widget);
        let library_pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let library_previous = gtk::Button::with_label("Назад");
        library_previous.set_tooltip_text(Some("Предыдущая страница библиотеки"));
        library_previous.set_visible(false);
        let library_next = gtk::Button::with_label("Далее");
        library_next.set_tooltip_text(Some("Следующая страница библиотеки"));
        library_next.set_halign(gtk::Align::Start);
        library_next.set_visible(false);
        library_pages.append(&library_previous);
        library_pages.append(&library_next);
        library.append(&library_pages);
        stack.add_titled(&library, Some("library"), "Библиотека");

        // The HeaderBar already names this page. Give the entry and results
        // priority; wide windows place both control groups on one native row.
        let search_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let search_tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Что хотите послушать?"));
        search.set_hexpand(true);
        search.set_width_request(120);
        search.set_widget_name("music-search");
        search.update_property(&[gtk::accessible::Property::Label("Поиск музыки")]);
        let search_button = gtk::Button::with_label("Найти");
        search_button.set_widget_name("music-search-button");
        search_button.add_css_class("suggested-action");
        search_tools.append(&search);
        search_tools.append(&search_button);
        search_page.append(&search_tools);
        let scope_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let search_all = gtk::Button::with_label("Все результаты");
        search_all.set_widget_name("music-search-all");
        search_all.set_icon_name("go-previous-symbolic");
        search_all.set_tooltip_text(Some("Вернуться ко всем результатам поиска"));
        search_all.set_visible(false);
        let search_scope_title = text("", "heading");
        search_scope_title.set_wrap(false);
        search_scope_title.set_hexpand(false);
        search_scope_title.set_visible(false);
        scope_header.append(&search_all);
        scope_header.append(&search_scope_title);
        search_tools.prepend(&scope_header);
        let search_feedback = gtk::Box::new(gtk::Orientation::Vertical, 8);
        search_feedback.set_widget_name("music-search-feedback");
        search_feedback.set_visible(false);
        search_page.append(&search_feedback);
        let search_sections = gtk::Box::new(gtk::Orientation::Vertical, 20);
        search_sections.set_widget_name("music-search-sections");
        let search_rows = list();
        set_empty(
            &search_rows,
            "Найдите музыку по названию, исполнителю или настроению.",
        );
        search_sections.append(&search_rows);
        search_page.append(&search_sections);
        let search_pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let search_previous = gtk::Button::with_label("Назад");
        search_previous.set_tooltip_text(Some("Предыдущая страница поиска"));
        search_previous.set_visible(false);
        let search_next = gtk::Button::with_label("Далее");
        search_next.set_tooltip_text(Some("Следующая страница поиска"));
        search_next.set_visible(false);
        search_pages.append(&search_previous);
        search_pages.append(&search_next);
        search_page.append(&search_pages);
        stack.add_titled(&search_page, Some("search"), "Поиск");

        let queue_page = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let queue_tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let queue_count = text("Очередь пуста", "dim-label");
        queue_tools.append(&queue_count);
        let clear_queue = gtk::Button::with_label("Очистить очередь");
        clear_queue.set_halign(gtk::Align::Start);
        queue_tools.append(&clear_queue);
        queue_page.append(&queue_tools);
        let queue_rows = list();
        queue_page.append(&queue_rows);
        stack.add_titled(&queue_page, Some("queue"), "Очередь");

        let detail = gtk::Box::new(gtk::Orientation::Vertical, 12);
        stack.add_named(&detail, Some("detail"));
        let player_bar = gtk::Box::new(gtk::Orientation::Vertical, 6);
        player_bar.set_widget_name("music-player-bar");
        super::margins(&player_bar, 12);
        let player_top = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        player_top.set_widget_name("music-player-main");
        let player_metadata = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        player_metadata.set_hexpand(true);
        let player_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        player_actions.set_halign(gtk::Align::Center);
        let current_art = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let current_art_button = gtk::Button::builder()
            .has_frame(false)
            .child(&current_art)
            .tooltip_text("Сведения о треке")
            .sensitive(false)
            .build();
        current_art_button.add_css_class("flat");
        current_art_button.set_widget_name("music-current-info");
        player_metadata.append(&current_art_button);
        let playing = gtk::Box::new(gtk::Orientation::Vertical, 2);
        playing.set_hexpand(true);
        let current = text("Трек не выбран", "heading");
        current.set_widget_name("music-current-title");
        current.set_ellipsize(gtk::pango::EllipsizeMode::End);
        current.set_lines(1);
        current.set_wrap(false);
        current.set_width_chars(1);
        let current_artist = text("Выберите трек", "dim-label");
        current_artist.set_widget_name("music-current-artist");
        current_artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
        current_artist.set_lines(1);
        current_artist.set_wrap(false);
        current_artist.set_width_chars(1);
        playing.append(&current);
        playing.append(&current_artist);
        let player_status = text("", "caption");
        player_status.set_width_chars(1);
        player_status.set_visible(false);
        playing.append(&player_status);
        player_metadata.append(&playing);
        player_top.append(&player_metadata);
        let previous = icon_button("media-skip-backward-symbolic", "Предыдущий трек");
        let play = icon_button(
            "media-playback-start-symbolic",
            "Воспроизвести выбранный трек",
        );
        play.set_widget_name("music-play");
        play.set_sensitive(false);
        play.add_css_class("circular");
        play.add_css_class("suggested-action");
        play.set_size_request(44, 44);
        let stop = icon_button("media-playback-stop-symbolic", "Остановить воспроизведение");
        stop.set_widget_name("music-stop");
        stop.set_sensitive(false);
        let next = icon_button("media-skip-forward-symbolic", "Следующий трек");
        player_actions.append(&previous);
        player_actions.append(&play);
        player_actions.append(&next);
        player_actions.append(&stop);
        let queue = icon_button("view-list-symbolic", "Открыть очередь");
        queue.set_action_name(Some("win.music-queue"));
        player_actions.append(&queue);

        let timeline = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let progress = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        progress.set_draw_value(false);
        progress.set_hexpand(true);
        progress.set_sensitive(false);
        progress.set_widget_name("music-progress");
        progress.set_tooltip_text(Some("Позиция воспроизведения"));
        progress.update_property(&[gtk::accessible::Property::Label("Позиция воспроизведения")]);
        let position = text("— / —", "caption");
        position.set_hexpand(false);
        position.set_wrap(false);
        timeline.append(&progress);
        timeline.append(&position);
        let volume_icon = gtk::Image::from_icon_name("audio-volume-high-symbolic");
        volume_icon.set_pixel_size(16);
        player_actions.append(&volume_icon);
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        volume.set_draw_value(false);
        volume.set_value(0.7);
        volume.set_widget_name("music-volume");
        volume.set_width_request(72);
        volume.set_sensitive(false);
        volume.set_tooltip_text(Some("Громкость"));
        volume.update_property(&[gtk::accessible::Property::Label(
            "Громкость воспроизведения",
        )]);
        player_actions.append(&volume);
        player_top.append(&player_actions);
        player_bar.append(&player_top);
        player_bar.append(&timeline);

        let cache_notice = adw::Banner::builder()
            .title("Сохранённые данные могут быть устаревшими.")
            .button_label("Обновить")
            .revealed(false)
            .build();
        widget.insert_child_after(&cache_notice, Some(&status_revealer));
        let cache_hint = text("Сохранённые сведения", "caption");
        cache_hint.add_css_class("dim-label");
        cache_hint.set_visible(false);
        widget.insert_child_after(&cache_hint, Some(&cache_notice));
        let widgets = Widgets {
            account_bar,
            account_menu,
            account_open,
            account_launch_error,
            account,
            notice,
            login,
            logout,
            spinner,
            status,
            status_revealer,
            home_signedout,
            home_signedin,
            home_liked,
            home_playlists,
            home_search,
            home_queue,
            home_preview,
            stack,
            library_mode,
            library_refresh,
            library_rows,
            cache_notice,
            cache_hint,
            library_previous,
            library_next,
            search,
            search_sections,
            search_all,
            search_scope_title,
            search_button,
            search_feedback,
            search_rows,
            search_previous,
            search_next,
            detail,
            queue_rows,
            queue_count,
            current,
            current_artist,
            current_art,
            current_art_button,
            player_main: player_top,
            position,
            player_status,
            play,
            stop,
            progress,
            volume,
            previous,
            next,
            clear_queue,
        };
        let controller = Rc::new(Controller {
            app: app.downgrade(),
            window: window.downgrade(),
            service,
            provider,
            launch_browser,
            login_dialog: RefCell::new(None),
            retiring_logins: RefCell::new(Vec::new()),
            logout_confirmation: RefCell::new(None),
            model: RefCell::new(Model {
                token: None,
                account_valid: false,
                busy: false,
                auth_busy: false,
                closed: false,
                generation: 0,
                view_generation: 0,
                detail_origin: "library".into(),
                library_touched: false,
                delivery_diagnostic: false,
                delivery_cancellation: None,
                library_page: 0,
                search_page: 0,
                search_scope: None,
                queue: MusicQueue::new(),
                attempt: None,
                liked: None,
                playlists: None,
                library_origins: Vec::new(),
                library_displaying: None,
            }),
            widgets,
            requests: RefCell::new(Requests::default()),
            mutation_busy: Cell::new(false),
            artwork,
            player: RefCell::new(player::MusicPlayback::default()),
            system_media: RefCell::new(system_media::SystemMedia::default()),
            persistence: RefCell::new(persistence_state),
            playlist_grid,
            track_pane,
            info_selection: RefCell::new(None),
            info_split: RefCell::new(None),
            catalog: RefCell::new(None),
            playlist_dialog: RefCell::new(None),
            can_create_playlist: Cell::new(true),
            detail_refresh: RefCell::new(None),
            music_surface_visible: Cell::new(true),
            detail_cache: Cell::new(None),
        });
        *controller_link.borrow_mut() = Rc::downgrade(&controller);
        if let (Some(artwork), Some(cache)) = (&controller.artwork, controller.persistent_cache()) {
            artwork.set_persistent_cache(Some(cache));
        }
        controller.connect(window);
        controller.connect_workspace();
        controller.connect_music_player();
        controller.connect_system_media();
        controller.render_controls();
        controller.render_queue();
        controller.start_persistence();
        Self {
            widget,
            player_bar,
            controller,
        }
    }

    /// Explicit owner-local metadata-only delivery experiment; never enabled
    /// by ordinary startup or automated smoke/capture modes.
    pub fn enable_delivery_diagnostic(&self) {
        self.controller.model.borrow_mut().delivery_diagnostic = true;
    }

    pub fn section_stack(&self) -> gtk::Stack {
        self.controller.widgets.stack.clone()
    }

    pub fn navigation_target(&self, action: &str) -> gtk::Widget {
        match action {
            "music-search" => self.controller.widgets.search.clone().upcast(),
            "music-library" => self.controller.widgets.library_mode.clone().upcast(),
            "music-home" => self.controller.widgets.home_liked.clone().upcast(),
            "music-queue" => self.controller.widgets.queue_rows.clone().upcast(),
            _ => self.controller.widgets.login.clone().upcast(),
        }
    }

    pub fn primary_action(&self) -> gtk::Button {
        self.controller.widgets.login.clone()
    }

    /// Account actions belong to the window header, never the content's Back
    /// navigation or a direct destructive icon in the music workspace.
    pub fn account_menu(&self) -> gtk::MenuButton {
        self.controller.widgets.account_menu.clone()
    }

    pub fn player_main(&self) -> gtk::Box {
        self.controller.widgets.player_main.clone()
    }

    pub fn close(&self) {
        self.controller.close();
    }

    pub async fn exercise_login_for_smoke(&self) -> Result<(), String> {
        let window = self
            .controller
            .window
            .upgrade()
            .ok_or("Login window is unavailable")?;
        if window.visible_dialog().is_some() || window.dialogs().n_items() != 0 {
            return Err("A previous native dialog is still active".into());
        }
        // Cancel before the first main-loop yield, then reopen while the old
        // dialog is still preparing its native sheet. Its later closed signal
        // must neither leave an orphan modal nor cancel the new attempt.
        self.controller.open_login();
        let cancelled = self
            .controller
            .login_dialog
            .borrow()
            .as_ref()
            .cloned()
            .ok_or("Immediate login dialog was not created")?;
        // This first sheet is already natively presented, though not ready.
        // Native close is valid here and routes through close-attempt.
        if cancelled.dialog.close() {
            return Err("Native dialog close bypassed presentation readiness".into());
        }
        self.controller.open_login();
        let pending_cancelled = self
            .controller
            .login_dialog
            .borrow()
            .as_ref()
            .cloned()
            .ok_or("Pending login dialog was not created")?;
        if window.dialogs().n_items() > 1
            || (cancelled.dialog.parent().is_some() && pending_cancelled.dialog.parent().is_some())
        {
            return Err("A replacement login shadowed a retiring native dialog".into());
        }
        // A queued logical dialog has no native sheet yet. Its ordinary Cancel
        // control must discard it; calling native close() would be invalid.
        pending_cancelled.cancel.emit_clicked();
        self.controller.open_login();
        let reopened = self
            .controller
            .login_dialog
            .borrow()
            .as_ref()
            .cloned()
            .ok_or("Login dialog could not be reopened")?;
        let mut result = Err("Login dialog did not settle".to_owned());
        for _ in 0..40 {
            glib::timeout_future(std::time::Duration::from_millis(25)).await;
            if window.dialogs().n_items() > 1 {
                result = Err("More than one native login dialog was presented".into());
                break;
            }
            if self.controller.model.borrow().busy {
                continue;
            }
            let dialog = self.controller.login_dialog.borrow().as_ref().cloned();
            let Some(dialog) = dialog else {
                break;
            };
            if dialog.dialog != reopened.dialog {
                result = Err("Late dialog close replaced the new attempt".into());
                break;
            }
            if !dialog.presentation_ready()
                || !dialog.dialog.is_mapped()
                || !dialog.cancel.is_sensitive()
                || cancelled.dialog.parent().is_some()
                || pending_cancelled.dialog.parent().is_some()
            {
                continue;
            }
            result = super::check_widget_geometry(dialog.dialog.upcast_ref(), window.upcast_ref());
            if result.is_ok() {
                break;
            }
        }
        self.controller.cancel_login();
        // Dialog::closed marks the beginning of the close animation in 1.5.
        // The native host and parent must both release it before another resize
        // or the normal window-close/resource checks can safely proceed.
        let mut removed = false;
        for _ in 0..80 {
            if window.visible_dialog().is_none()
                && window.dialogs().n_items() == 0
                && cancelled.dialog.parent().is_none()
                && pending_cancelled.dialog.parent().is_none()
                && reopened.dialog.parent().is_none()
            {
                removed = true;
                break;
            }
            glib::timeout_future(std::time::Duration::from_millis(25)).await;
        }
        if !removed {
            return Err("Canceled login dialogs remain in the native window host".into());
        }
        if self.controller.model.borrow().account_valid
            || self.controller.model.borrow().token.is_some()
            || self.controller.login_dialog.borrow().is_some()
            || !self
                .controller
                .widgets
                .status
                .subtitle()
                .is_none_or(|value| value.is_empty())
        {
            return Err("Blocked login/cancellation retained session state".into());
        }
        result
    }

    pub fn verify_layout(&self) -> Result<(), String> {
        if self.widget.is_mapped() && self.widget.width() <= 0 {
            return Err("Music page has no width".into());
        }
        if self.player_bar.is_mapped() && self.player_bar.width() <= 0 {
            return Err("Music player bar has no width".into());
        }
        Ok(())
    }
}

impl Drop for MusicPane {
    fn drop(&mut self) {
        self.controller.close();
    }
}

impl Controller {
    fn open_login(self: &Rc<Self>) {
        if self.model.borrow().closed || self.model.borrow().account_valid {
            return;
        }
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            if dialog.dialog.parent().is_some()
                && let Some(window) = self.window.upgrade()
            {
                window.present();
            }
            return;
        }
        if !self.prepare_interactive_login() {
            return;
        }
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let dialog = LoginDialog::new();
        let weak = Rc::downgrade(self);
        dialog.dialog.connect_closed(move |closed| {
            if let Some(this) = weak.upgrade() {
                this.cancel_dialog(closed);
            }
        });
        let weak = Rc::downgrade(self);
        dialog.dialog.connect_close_attempt(move |closing| {
            if let Some(this) = weak.upgrade() {
                this.cancel_dialog(closing);
            }
        });
        let weak = Rc::downgrade(self);
        let owner = dialog.dialog.downgrade();
        dialog.cancel.connect_clicked(move |_| {
            if let (Some(this), Some(owner)) = (weak.upgrade(), owner.upgrade()) {
                this.cancel_dialog(&owner);
            }
        });
        let weak = Rc::downgrade(self);
        let owner = dialog.dialog.downgrade();
        dialog.retry.connect_clicked(move |_| {
            if let (Some(this), Some(owner)) = (weak.upgrade(), owner.upgrade()) {
                let current = this
                    .login_dialog
                    .borrow()
                    .as_ref()
                    .is_some_and(|current| current.dialog == owner);
                if current {
                    this.start_login();
                }
            }
        });
        *self.login_dialog.borrow_mut() = Some(dialog.clone());
        let generation = {
            let mut model = self.model.borrow_mut();
            model.auth_busy = true;
            model.busy = true;
            model.generation
        };
        self.render_controls();
        if self.login_host_ready(&window) {
            dialog.dialog.present(Some(&window));
            dialog.cancel.grab_focus();
            self.start_login();
            return;
        }
        // Libadwaita 1.5 keeps a closing sheet parented after logical closure.
        // Presenting another sheet then shadows the retiring dialog and can
        // register a native focus weak pointer whose owner is being disposed.
        // Observe host removal and unparenting; never overlap our login sheets.
        let weak = Rc::downgrade(self);
        let pending = dialog.dialog.downgrade();
        window.add_tick_callback(move |window, _| {
            let (Some(this), Some(dialog)) = (weak.upgrade(), pending.upgrade()) else {
                return glib::ControlFlow::Break;
            };
            let current = {
                let model = this.model.borrow();
                !model.closed
                    && model.generation == generation
                    && this
                        .login_dialog
                        .borrow()
                        .as_ref()
                        .is_some_and(|current| current.dialog == dialog)
            };
            if !current {
                return glib::ControlFlow::Break;
            }
            if !this.login_host_ready(window) {
                return glib::ControlFlow::Continue;
            }
            dialog.present(Some(window));
            if let Some(current) = this.login_dialog.borrow().as_ref() {
                current.cancel.grab_focus();
            }
            this.start_login();
            glib::ControlFlow::Break
        });
    }

    fn login_host_ready(&self, window: &adw::ApplicationWindow) -> bool {
        let mut retiring = self.retiring_logins.borrow_mut();
        retiring.retain(|dialog| {
            dialog
                .upgrade()
                .is_some_and(|dialog| dialog.parent().is_some())
        });
        retiring.is_empty() && window.dialogs().n_items() == 0
    }

    fn dismiss_login(&self) {
        let dialog = self.login_dialog.borrow_mut().take();
        if let Some(dialog) = dialog {
            self.retiring_logins
                .borrow_mut()
                .push(dialog.dialog.downgrade());
            dialog.request_close();
        }
    }

    fn cancel_dialog(&self, dialog: &adw::Dialog) {
        let current = self
            .login_dialog
            .borrow()
            .as_ref()
            .is_some_and(|current| current.dialog == *dialog);
        if current {
            self.cancel_login();
        }
    }

    fn cancel_login(&self) {
        if self.login_dialog.borrow().is_none() {
            return;
        }
        self.stop_music();
        self.requests.borrow_mut().invalidate_all();
        self.cancel_delivery();
        if let Some(artwork) = &self.artwork {
            artwork.clear();
        }
        {
            let mut model = self.model.borrow_mut();
            model.generation += 1;
            model.view_generation += 1;
            model.auth_busy = false;
            model.busy = false;
            if let Some(attempt) = model.attempt.take() {
                attempt.cancel();
            }
            if let Some(token) = model.token.take() {
                token.invalidate();
            }
            model.account_valid = false;
        }
        if let Some(service) = &self.service {
            service.clear_session();
        }
        self.dismiss_login();
        self.reset_status();
        self.render_controls();
        self.widgets.login.grab_focus();
    }

    fn reset_status(&self) {
        self.widgets
            .status
            .set_title("Выполните вход для доступа к библиотеке и поиску.");
        self.widgets.status.set_subtitle("");
    }

    fn start_login(self: &Rc<Self>) {
        if self
            .login_dialog
            .borrow()
            .as_ref()
            .is_none_or(|dialog| dialog.dialog.parent().is_none())
        {
            return;
        }
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let generation = {
            let mut model = self.model.borrow_mut();
            if model.closed {
                return;
            }
            if let Some(attempt) = model.attempt.take() {
                attempt.cancel();
            }
            if let Some(token) = model.token.take() {
                token.invalidate();
            }
            model.generation += 1;
            model.view_generation += 1;
            model.account_valid = false;
            model.auth_busy = true;
            model.busy = true;
            model.generation
        };
        if let Some(service) = &self.service {
            service.clear_session();
        }
        self.reset_status();
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            dialog.shutdown();
            dialog.loading("Подготовка входа…");
        }
        self.render_controls();
        let provider = self.provider.clone();
        let weak = Rc::downgrade(self);
        let hold = app.hold();
        glib::spawn_future_local(async move {
            let _hold = hold;
            let result = gio::spawn_blocking(move || provider.begin()).await;
            let Some(this) = weak.upgrade() else {
                if let Ok(Ok(attempt)) = result {
                    attempt.cancel();
                }
                return;
            };
            if !this.current_attempt(generation) {
                if let Ok(Ok(attempt)) = result {
                    attempt.cancel();
                }
                return;
            }
            match result {
                Ok(Ok(attempt)) => this.present_attempt(attempt, generation),
                Ok(Err(error)) => this.auth_failed(error),
                Err(_) => this.auth_failed(AuthError::Configuration),
            }
        });
    }

    fn current_attempt(&self, generation: u64) -> bool {
        let model = self.model.borrow();
        !model.closed && model.generation == generation && self.login_dialog.borrow().is_some()
    }

    fn present_attempt(self: &Rc<Self>, attempt: Arc<AuthAttempt>, generation: u64) {
        self.model.borrow_mut().attempt = Some(attempt.clone());
        let url = attempt.authorization_url();
        match attempt.browser_mode() {
            BrowserMode::System => {
                if (self.launch_browser)(&url).is_err() {
                    self.auth_failed(AuthError::BrowserLoad);
                    return;
                }
                if let Some(dialog) = self.login_dialog.borrow().as_ref() {
                    dialog.system_browser();
                }
                self.wait_for_callback(attempt, generation);
            }
            BrowserMode::Embedded => {
                #[cfg(feature = "auth-webkit")]
                self.present_embedded(attempt, generation, &url);
                #[cfg(not(feature = "auth-webkit"))]
                self.auth_failed(AuthError::Configuration);
            }
        }
    }

    #[cfg(feature = "auth-webkit")]
    fn present_embedded(self: &Rc<Self>, attempt: Arc<AuthAttempt>, generation: u64, url: &str) {
        let expiry_attempt = Arc::downgrade(&attempt);
        let weak = Rc::downgrade(self);
        let callback_attempt = attempt.clone();
        let on_redirect = Rc::new(move |code| {
            if let Some(this) = weak.upgrade()
                && this.current_attempt(generation)
            {
                this.exchange_code(callback_attempt.clone(), code, generation);
            }
        });
        let weak = Rc::downgrade(self);
        let session_attempt = attempt.clone();
        let on_session = Rc::new(move |cookies| {
            if let Some(this) = weak.upgrade()
                && this.current_attempt(generation)
            {
                this.exchange_session(session_attempt.clone(), cookies, generation);
            }
        });
        let weak = Rc::downgrade(self);
        let on_error = Rc::new(move |error| {
            if let Some(this) = weak.upgrade()
                && this.current_attempt(generation)
            {
                this.auth_failed(error);
            }
        });
        let weak = Rc::downgrade(self);
        let on_loading = Rc::new(move |loading| {
            if let Some(this) = weak.upgrade()
                && this.current_attempt(generation)
                && let Some(dialog) = this.login_dialog.borrow().as_ref()
            {
                dialog.browser_loading(loading);
            }
        });
        let weak = Rc::downgrade(self);
        let on_origin = Rc::new(move |origin: &str| {
            if let Some(this) = weak.upgrade()
                && this.current_attempt(generation)
                && let Some(dialog) = this.login_dialog.borrow().as_ref()
            {
                dialog.set_origin(origin);
            }
        });
        let webview = super::auth::webkit::AuthWebView::new(
            attempt,
            on_redirect,
            on_session,
            on_error,
            on_loading,
            on_origin,
        );
        webview.load(url);
        if self.current_attempt(generation) {
            if let Some(dialog) = self.login_dialog.borrow().as_ref() {
                dialog.set_webview(webview);
            }
        } else {
            webview.shutdown();
        }
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            loop {
                let delay = {
                    let Some(attempt) = expiry_attempt.upgrade() else {
                        return;
                    };
                    if attempt.is_cancelled() {
                        return;
                    }
                    attempt
                        .time_remaining()
                        .min(std::time::Duration::from_secs(1))
                };
                // Do not retain PKCE material or the callback listener during
                // the timer wait after cancellation, failure or dialog closure.
                glib::timeout_future(delay).await;
                let Some(this) = weak.upgrade() else {
                    return;
                };
                if !this.current_attempt(generation) {
                    return;
                }
                let Some(attempt) = expiry_attempt.upgrade() else {
                    return;
                };
                if attempt.is_cancelled() {
                    return;
                }
                if attempt.expired() {
                    this.auth_failed(AuthError::Expired);
                    return;
                }
            }
        });
    }

    fn wait_for_callback(self: &Rc<Self>, attempt: Arc<AuthAttempt>, generation: u64) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let hold = app.hold();
        let weak = Rc::downgrade(self);
        let waiting = attempt.clone();
        glib::spawn_future_local(async move {
            let _hold = hold;
            let result = gio::spawn_blocking(move || waiting.receive_callback()).await;
            let Some(this) = weak.upgrade() else {
                return;
            };
            if !this.current_attempt(generation) {
                return;
            }
            match result {
                Ok(Ok(code)) => this.exchange_code(attempt, code, generation),
                Ok(Err(error)) => this.auth_failed(error),
                Err(_) => this.auth_failed(AuthError::Configuration),
            }
        });
    }

    fn exchange_code(
        self: &Rc<Self>,
        attempt: Arc<AuthAttempt>,
        code: AuthorizationCode,
        generation: u64,
    ) {
        if let Some(window) = self.window.upgrade() {
            window.present();
        }
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            dialog.set_origin("");
            dialog.loading("Проверка разрешения приложения…");
        }
        let provider = self.provider.clone();
        self.run_auth_worker(generation, move || provider.exchange(&attempt, code));
    }

    #[cfg(feature = "auth-webkit")]
    fn exchange_session(
        self: &Rc<Self>,
        attempt: Arc<AuthAttempt>,
        cookies: SessionCookies,
        generation: u64,
    ) {
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            dialog.set_origin("");
            dialog.loading("Получение разрешения Яндекс Музыки…");
        }
        let provider = self.provider.clone();
        self.run_auth_worker(generation, move || {
            provider.exchange_session(&attempt, cookies)
        });
    }

    fn run_auth_worker(
        self: &Rc<Self>,
        generation: u64,
        work: impl FnOnce() -> Result<AccessToken, AuthError> + Send + 'static,
    ) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let hold = app.hold();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let _hold = hold;
            let result = gio::spawn_blocking(work).await;
            let Some(this) = weak.upgrade() else {
                if let Ok(Ok(token)) = result {
                    token.invalidate();
                }
                return;
            };
            if !this.current_attempt(generation) {
                if let Ok(Ok(token)) = result {
                    token.invalidate();
                }
                return;
            }
            match result {
                Ok(Ok(token)) => this.authenticate(token),
                Ok(Err(error)) => this.auth_failed(error),
                Err(_) => this.auth_failed(AuthError::Configuration),
            }
        });
    }

    fn auth_failed(&self, error: AuthError) {
        {
            let mut model = self.model.borrow_mut();
            model.generation += 1;
            if let Some(attempt) = model.attempt.take() {
                attempt.cancel();
            }
            if let Some(token) = model.token.take() {
                token.invalidate();
            }
            model.account_valid = false;
            model.auth_busy = false;
            model.busy = false;
        }
        if let Some(service) = &self.service {
            service.clear_session();
        }
        self.reset_status();
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            dialog.unavailable("Не удалось выполнить вход", &error.to_string());
        }
        self.render_controls();
    }

    fn clear_text_focus(&self) {
        if let Some(root) = self.widgets.stack.root()
            && let Ok(window) = root.downcast::<gtk::Window>()
        {
            gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
        }
    }

    fn connect(self: &Rc<Self>, window: &adw::ApplicationWindow) {
        let weak = Rc::downgrade(self);
        window.connect_close_request(move |_| {
            if let Some(this) = weak.upgrade() {
                this.close();
            }
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(self);
        self.widgets.login.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.open_login();
            }
        });
        self.connect_account_menu();
        for (button, mode) in [
            (&self.widgets.home_liked, 0),
            (&self.widgets.home_playlists, 1),
        ] {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.widgets.stack.set_visible_child_name("library");
                    this.widgets.library_mode.set_selected(mode);
                    this.show_library();
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.widgets.home_search.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.widgets.stack.set_visible_child_name("search");
                this.widgets.search.grab_focus();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.status.connect_subtitle_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.render_controls();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.library_mode.connect_selected_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.show_library();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.library_refresh.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let page = this.model.borrow().library_page;
                this.load_library_refresh(page);
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.library_previous.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let page = this.model.borrow().library_page.saturating_sub(1);
                this.load_library(page);
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.library_next.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let page = this.model.borrow().library_page.saturating_add(1);
                this.load_library(page);
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.search_button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.search(0);
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.search.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.search(0);
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.search.connect_changed(move |_| {
            if let Some(this) = weak.upgrade() {
                this.invalidate_search();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.search_all.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.model.borrow_mut().search_scope = None;
                this.search(0);
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.search_previous.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let page = this.model.borrow().search_page.saturating_sub(1);
                this.search(page);
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.search_next.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let page = this.model.borrow().search_page.saturating_add(1);
                this.search(page);
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets
            .stack
            .connect_visible_child_name_notify(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.model.borrow_mut().view_generation += 1;
                    this.requests.borrow_mut().invalidate(Operation::Detail);
                    this.requests.borrow_mut().invalidate(Operation::Delivery);
                    this.cancel_delivery();
                }
            });
        let weak = Rc::downgrade(self);
        self.widgets.clear_queue.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.stop_music();
                this.model.borrow_mut().queue.clear();
                this.render_queue();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.previous.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.skip_music(false, false);
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.next.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.skip_music(true, false);
            }
        });
        for (name, page, accelerator) in [
            ("music-home", "home", "<Primary>h"),
            ("music-library", "library", "<Primary>l"),
            ("music-search", "search", "<Primary>f"),
            ("music-queue", "queue", "<Primary>j"),
        ] {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(this) = weak.upgrade() {
                    this.widgets.stack.set_visible_child_name(page);
                    if page == "search" {
                        this.widgets.search.grab_focus();
                    }
                }
            });
            window.add_action(&action);
            if let Some(app) = self.app.upgrade() {
                app.set_accels_for_action(&format!("win.{name}"), &[accelerator]);
            }
        }
        let back = gio::SimpleAction::new("music-back", None);
        let weak = Rc::downgrade(self);
        back.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.back_from_detail();
            }
        });
        window.add_action(&back);
        if let Some(app) = self.app.upgrade() {
            app.set_accels_for_action("win.music-back", &["<Alt>Left"]);
        }
    }

    fn render_controls(&self) {
        let model = self.model.borrow();
        let ready = model.account_valid && !model.closed;
        self.widgets.home_signedin.set_visible(model.account_valid);
        self.widgets
            .home_signedout
            .set_visible(!model.account_valid);
        self.widgets.home_liked.set_sensitive(ready);
        self.widgets.home_playlists.set_sensitive(ready);
        self.widgets.home_search.set_sensitive(ready);
        self.widgets.account_bar.set_visible(!model.account_valid);
        self.widgets.account_menu.set_visible(model.account_valid);
        self.widgets.account_menu.set_sensitive(ready);
        self.widgets.library_mode.set_sensitive(ready);
        self.widgets.library_refresh.set_sensitive(ready);
        // Independent lanes accept actions during unrelated work. Repeated
        // requests in one lane retain only the latest pending action.
        self.widgets
            .search
            .set_sensitive(model.account_valid && !model.closed);
        self.widgets.search_button.set_sensitive(ready);
        self.widgets.library_next.set_sensitive(ready);
        self.widgets.library_previous.set_sensitive(ready);
        self.widgets.search_next.set_sensitive(ready);
        self.widgets.search_previous.set_sensitive(ready);
        self.widgets.login.set_sensitive(
            !model.account_valid
                && !model.auth_busy
                && !model.closed
                && !self.persistence_blocks_login(),
        );
        self.widgets
            .login
            .set_tooltip_text(Some("Войти в Яндекс Музыку"));
        self.widgets.login.set_visible(!model.account_valid);
        self.widgets
            .logout
            .set_sensitive(model.token.is_some() && !model.closed);
        // Search owns its one inline loading/error area. A second global card
        // would repeat the same state and push the actual results down.
        let visible_work = (model.auth_busy
            || self.requests.borrow().busy_except(Operation::Search))
            && (model.account_valid || self.login_dialog.borrow().is_some());
        self.widgets.spinner.set_spinning(visible_work);
        self.widgets.spinner.set_visible(visible_work);
        let has_error = self
            .widgets
            .status
            .subtitle()
            .is_some_and(|message| !message.is_empty());
        let show_status = model.account_valid && (visible_work || has_error);
        self.widgets.status_revealer.set_visible(show_status);
        self.widgets.status_revealer.set_reveal_child(show_status);
    }

    fn close(&self) {
        // close-request, explicit close and the pane's Drop can all arrive.
        // A repeated close must not render children after native disposal.
        if self.model.borrow().closed {
            return;
        }
        self.close_persistence();
        self.clear_workspace();
        self.dismiss_logout_confirmation();
        self.stop_music();
        self.clear_system_media();
        self.mutation_busy.set(false);
        self.clear_text_focus();
        self.requests.borrow_mut().invalidate_all();
        self.cancel_delivery();
        if let Some(artwork) = &self.artwork {
            artwork.clear();
        }
        let mut model = self.model.borrow_mut();
        if model.closed {
            return;
        }
        model.closed = true;
        model.generation += 1;
        if let Some(token) = model.token.take() {
            token.invalidate();
        }
        model.queue.clear();
        model.liked = None;
        model.playlists = None;
        if let Some(attempt) = model.attempt.take() {
            attempt.cancel();
        }
        drop(model);
        self.dismiss_login();
        self.shutdown_system_media();
        if let Some(service) = &self.service {
            service.clear_session();
        }
    }

    fn logout(self: &Rc<Self>) {
        self.begin_forget_session();
        self.dismiss_logout_confirmation();
        self.stop_music();
        self.clear_system_media();
        self.mutation_busy.set(false);
        self.clear_text_focus();
        self.requests.borrow_mut().invalidate_all();
        self.cancel_delivery();
        if let Some(artwork) = &self.artwork {
            artwork.clear();
        }
        {
            let mut model = self.model.borrow_mut();
            model.generation += 1;
            model.view_generation += 1;
            if let Some(token) = model.token.take() {
                token.invalidate();
            }
            model.account_valid = false;
            model.auth_busy = false;
            model.busy = false;
            model.queue.clear();
            model.liked = None;
            model.playlists = None;
            model.library_origins.clear();
            model.library_displaying = None;
            model.library_page = 0;
            model.search_page = 0;
            model.search_scope = None;
            model.library_touched = false;
            if let Some(attempt) = model.attempt.take() {
                attempt.cancel();
            }
        }
        self.dismiss_login();
        self.clear_workspace();
        if let Some(service) = &self.service {
            service.clear_session();
        }
        self.widgets.account.set_text("Музыка вашего аккаунта");
        self.widgets.notice.set_text(
            "Воспроизведение остановлено. Удаление сохранённого входа проверяется отдельно; разрешение Яндекса не отзывается.",
        );
        self.reset_status();
        self.widgets.search.set_text("");
        set_empty(
            &self.widgets.library_rows,
            "Ваша библиотека появится после входа.",
        );
        set_empty(&self.widgets.search_rows, "Введите запрос после входа.");
        self.clear_search_feedback();
        self.render_home_preview();
        clear_box(&self.widgets.detail);
        self.widgets.library_next.set_visible(false);
        self.widgets.library_previous.set_visible(false);
        self.widgets.search_next.set_visible(false);
        self.widgets.search_previous.set_visible(false);
        self.widgets.stack.set_visible_child_name("library");
        self.widgets.library_mode.set_selected(0);
        self.render_controls();
        self.render_queue();
    }

    fn cancel_delivery(&self) {
        if let Some(cancellation) = self.model.borrow_mut().delivery_cancellation.take() {
            cancellation.cancel();
        }
    }

    fn invalidate_search(&self) {
        self.clear_search_feedback();
        self.requests.borrow_mut().invalidate(Operation::Search);
        let mut model = self.model.borrow_mut();
        model.view_generation += 1;
        model.search_page = 0;
        model.search_scope = None;
        drop(model);
        self.reset_search_sections();
        self.widgets.search_next.set_visible(false);
        self.widgets.search_previous.set_visible(false);
        if self.model.borrow().account_valid {
            set_empty(
                &self.widgets.search_rows,
                "Запрос изменён. Нажмите Enter или «Найти».",
            );
        }
    }

    fn sync_busy(&self) {
        let running = self.requests.borrow().busy();
        let mut model = self.model.borrow_mut();
        model.busy = model.auth_busy || running;
    }

    fn run<R: Send + 'static>(
        self: &Rc<Self>,
        operation: Operation,
        work: impl FnOnce(MusicService, AccessToken) -> Result<R, MusicError> + Send + 'static,
        done: impl FnOnce(&Rc<Self>, Result<R, MusicError>) + 'static,
    ) {
        if matches!(operation, Operation::Search | Operation::Detail) {
            write_catalog_line(catalog_line(
                operation,
                CatalogPhase::Requested,
                "requested",
                None,
                None,
            ));
        }
        let generation = self.model.borrow().generation;
        let job: PendingJob = Box::new(move |this, serial| {
            this.start_job(operation, serial, generation, work, done);
        });
        let next = self.requests.borrow_mut().submit(operation, job);
        self.sync_busy();
        self.render_controls();
        if let Some((serial, job)) = next {
            job(self, serial);
        } else {
            catalog_event(operation, "queued_latest", None);
        }
    }

    fn start_job<R: Send + 'static>(
        self: &Rc<Self>,
        operation: Operation,
        serial: u64,
        generation: u64,
        work: impl FnOnce(MusicService, AccessToken) -> Result<R, MusicError> + Send + 'static,
        done: impl FnOnce(&Rc<Self>, Result<R, MusicError>) + 'static,
    ) {
        let (Some(app), Some(service)) = (self.app.upgrade(), self.service.clone()) else {
            done(self, Err(MusicError::Configuration));
            self.finish_job(operation, serial);
            return;
        };
        let token = {
            let model = self.model.borrow();
            if model.closed || model.generation != generation {
                drop(model);
                self.finish_job(operation, serial);
                return;
            }
            model.token.clone()
        };
        let Some(token) = token else {
            done(self, Err(MusicError::Unauthenticated));
            self.finish_job(operation, serial);
            return;
        };
        if token.is_expired() {
            self.logout();
            self.widgets
                .status
                .set_title("Срок действия сеанса истёк. Войдите снова.");
            self.finish_job(operation, serial);
            return;
        }
        let hold = app.hold();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let _hold = hold;
            let result = gio::spawn_blocking(move || work(service, token)).await;
            let Some(this) = weak.upgrade() else {
                return;
            };
            let same_session = {
                let model = this.model.borrow();
                !model.closed && model.generation == generation
            };
            let current = same_session && this.requests.borrow().current(operation, serial);
            if !current {
                catalog_event(operation, "stale_result", None);
            }
            let result = result.unwrap_or(Err(MusicError::Configuration));
            if current {
                if !matches!(operation, Operation::LikeRead | Operation::Mutation)
                    && (operation != Operation::Search || result.is_err())
                {
                    catalog_diagnostic(operation, result.as_ref().err().copied());
                }
                done(&this, result);
            } else if same_session
                && matches!(
                    result,
                    Err(MusicError::AccessDenied | MusicError::Unauthenticated)
                )
            {
                this.show_error(MusicError::AccessDenied);
            }
            // Always release the exact running ticket, even after logout or
            // view invalidation. A late ticket cannot release a newer job.
            this.finish_job(operation, serial);
        });
    }

    fn finish_job(self: &Rc<Self>, operation: Operation, serial: u64) {
        let next = self.requests.borrow_mut().finish(operation, serial);
        self.sync_busy();
        self.render_controls();
        if let Some((serial, job)) = next {
            job(self, serial);
        }
    }

    fn show_error(self: &Rc<Self>, error: MusicError) {
        if matches!(
            error,
            MusicError::AccessDenied | MusicError::Unauthenticated
        ) {
            self.logout();
        }
        let (title, message) = music_error_message(error);
        self.widgets.status.set_title(title);
        self.widgets.status.set_subtitle(message);
    }

    fn show_library(self: &Rc<Self>) {
        if !self.model.borrow().account_valid {
            return;
        }
        self.requests.borrow_mut().invalidate(Operation::Library);
        if self.widgets.library_mode.selected() == 2 {
            self.load_library(0);
            return;
        }
        let playlists = self.widgets.library_mode.selected() == 1;
        if playlists {
            let cached = self.model.borrow().playlists.clone();
            if let Some(result) = cached {
                self.render_playlists(result, 0);
                return;
            }
        } else {
            let cached = self.model.borrow().liked.clone();
            if let Some(result) = cached {
                self.render_liked(result, 0);
                return;
            }
        }
        self.load_library(0);
    }

    fn render_playlists(self: &Rc<Self>, result: Result<Page<Playlist>, MusicError>, page: u32) {
        self.widgets.library_rows.set_visible(false);
        self.playlist_grid.widget.set_visible(true);
        match result {
            Ok(result) => {
                self.model.borrow_mut().library_page = page;
                self.model.borrow_mut().library_displaying =
                    Some((self.widgets.library_mode.selected(), page));
                self.playlist_grid.set_playlists(
                    &result.items,
                    self.widgets.library_mode.selected() == 1
                        && self.can_create_playlist.get()
                        && self.model.borrow().account_valid,
                );
                self.widgets.library_next.set_visible(result.has_more);
                self.widgets.library_previous.set_visible(page > 0);
                self.refresh_catalog_notice();
                if self.widgets.stack.visible_child_name().as_deref() == Some("library") {
                    self.widgets.status.set_title(&format!(
                        "{} · страница {}",
                        if self.widgets.library_mode.selected() == 2 {
                            "Понравившиеся плейлисты"
                        } else {
                            "Мои плейлисты"
                        },
                        page + 1
                    ));
                    self.widgets.status.set_subtitle("");
                }
            }
            Err(error) => {
                self.model.borrow_mut().library_displaying = None;
                self.playlist_grid.set_error();
                self.show_error(error);
            }
        }
    }

    fn render_home_preview(self: &Rc<Self>) {
        let (valid, result) = {
            let model = self.model.borrow();
            (model.account_valid, model.liked.clone())
        };
        if !valid {
            set_empty(
                &self.widgets.home_preview,
                "Любимые треки появятся после входа.",
            );
            return;
        }
        match result {
            Some(Ok(page)) if !page.items.is_empty() => {
                self.tracks_from(
                    &self.widgets.home_preview,
                    page.items,
                    ContextKind::LikedTracks,
                    None,
                    page.has_more,
                    0..5,
                );
            }
            Some(Ok(_)) => set_empty(
                &self.widgets.home_preview,
                "Понравившихся треков пока нет. Их можно найти через поиск.",
            ),
            Some(Err(error)) => {
                let (title, _) = music_error_message(error);
                set_empty(
                    &self.widgets.home_preview,
                    &format!("{title}. Откройте библиотеку и повторите обновление."),
                );
            }
            None => set_empty(
                &self.widgets.home_preview,
                "Откройте «Все треки», чтобы обновить этот раздел.",
            ),
        }
        self.refresh_catalog_notice();
    }

    fn render_liked(self: &Rc<Self>, result: Result<Page<Track>, MusicError>, page: u32) {
        self.widgets.library_rows.set_visible(true);
        self.playlist_grid.widget.set_visible(false);
        match result {
            Ok(result) => {
                self.model.borrow_mut().library_page = page;
                self.model.borrow_mut().library_displaying = Some((0, page));
                self.tracks_from(
                    &self.widgets.library_rows,
                    result.items,
                    ContextKind::LikedTracks,
                    None,
                    page > 0 || result.has_more,
                    0..usize::MAX,
                );
                self.widgets.library_next.set_visible(result.has_more);
                self.widgets.library_previous.set_visible(page > 0);
                self.refresh_catalog_notice();
                if self.widgets.stack.visible_child_name().as_deref() == Some("library") {
                    self.widgets
                        .status
                        .set_title(&format!("Понравившиеся треки · страница {}", page + 1));
                    self.widgets.status.set_subtitle("");
                }
            }
            Err(error) => {
                self.model.borrow_mut().library_displaying = None;
                set_empty(
                    &self.widgets.library_rows,
                    "Не удалось загрузить треки. Повторите обновление.",
                );
                self.show_error(error);
            }
        }
        if page == 0 {
            self.render_home_preview();
        }
    }

    fn load_initial_library(self: &Rc<Self>) {
        self.load_initial_library_policy(false);
    }

    fn load_initial_library_policy(self: &Rc<Self>, force: bool) {
        self.render_home_preview();
        let catalog = self.cached_catalog();
        self.run(
            Operation::InitialLibrary,
            move |service, token| {
                let liked = if let Some(catalog) = &catalog {
                    catalog.liked_tracks(&service, &token, 0, force)
                } else {
                    service.liked_tracks(&token, 0).map(catalog_view::network)
                };
                let playlists = if let Some(catalog) = &catalog {
                    catalog.playlists(&service, &token, 0, force)
                } else {
                    service.playlists(&token, 0).map(catalog_view::network)
                };
                Ok((liked, playlists))
            },
            move |this, result| match result {
                Ok((liked, playlists)) => {
                    let retired = liked
                        .as_ref()
                        .ok()
                        .and_then(|reply| reply.lease.as_ref())
                        .is_some_and(|lease| !lease.is_current())
                        || playlists
                            .as_ref()
                            .ok()
                            .and_then(|reply| reply.lease.as_ref())
                            .is_some_and(|lease| !lease.is_current());
                    if retired && !force {
                        this.load_initial_library_policy(true);
                        return;
                    }
                    let liked_origin = liked
                        .as_ref()
                        .map_or(crate::catalog_cache::Provenance::Network, |reply| {
                            reply.provenance
                        });
                    let playlists_origin = playlists
                        .as_ref()
                        .map_or(crate::catalog_cache::Provenance::Network, |reply| {
                            reply.provenance
                        });
                    let liked_cache_error = liked
                        .as_ref()
                        .is_ok_and(|reply| reply.cache_error.is_some());
                    let playlists_cache_error = playlists
                        .as_ref()
                        .is_ok_and(|reply| reply.cache_error.is_some());
                    let liked = liked.and_then(|reply| {
                        if reply
                            .lease
                            .as_ref()
                            .is_some_and(|lease| !lease.is_current())
                        {
                            Err(MusicError::Network)
                        } else {
                            Ok(reply.value)
                        }
                    });
                    let playlists = playlists.and_then(|reply| {
                        if reply
                            .lease
                            .as_ref()
                            .is_some_and(|lease| !lease.is_current())
                        {
                            Err(MusicError::Network)
                        } else {
                            Ok(reply.value)
                        }
                    });
                    if liked
                        .as_ref()
                        .err()
                        .into_iter()
                        .chain(playlists.as_ref().err())
                        .any(|error| {
                            matches!(
                                error,
                                MusicError::AccessDenied | MusicError::Unauthenticated
                            )
                        })
                    {
                        this.logout();
                        this.show_error(MusicError::AccessDenied);
                        return;
                    }
                    let mut model = this.model.borrow_mut();
                    let save_liked = model.liked.is_none();
                    let save_playlists = model.playlists.is_none();
                    if save_liked {
                        model.liked = Some(liked);
                    }
                    if save_playlists {
                        model.playlists = Some(playlists);
                    }
                    let render = !model.library_touched;
                    drop(model);
                    if save_liked {
                        this.remember_library_origin(0, 0, liked_origin, liked_cache_error);
                    }
                    if save_playlists {
                        this.remember_library_origin(1, 0, playlists_origin, playlists_cache_error);
                    }
                    if render {
                        this.show_library();
                    }
                    this.render_home_preview();
                }
                Err(error) => this.show_error(error),
            },
        );
    }

    fn load_library(self: &Rc<Self>, page: u32) {
        self.load_library_policy(page, false);
    }

    pub(super) fn load_library_refresh(self: &Rc<Self>, page: u32) {
        self.load_library_policy(page, true);
    }

    fn load_library_policy(self: &Rc<Self>, page: u32, force: bool) {
        if !self.model.borrow().account_valid || self.model.borrow().closed {
            return;
        }
        self.model.borrow_mut().library_touched = true;
        self.model.borrow_mut().library_displaying = None;
        self.widgets.status.set_title("Загрузка библиотеки…");
        self.widgets.status.set_subtitle("");
        self.widgets.cache_notice.set_revealed(false);
        let mode = self.widgets.library_mode.selected();
        self.widgets.library_rows.set_visible(mode == 0);
        self.playlist_grid.widget.set_visible(mode != 0);
        if mode == 0 {
            set_empty(&self.widgets.library_rows, "Загрузка библиотеки…");
        } else {
            self.playlist_grid.set_loading();
        }
        self.widgets.library_next.set_visible(false);
        self.widgets.library_previous.set_visible(false);
        if mode == 2 {
            self.run_catalog(
                Operation::Library,
                Some((mode, page)),
                force,
                move |service, token, catalog, force| match catalog {
                    Some(catalog) => catalog.liked_playlists(&service, &token, page, force),
                    None => service
                        .liked_playlists(&token, page)
                        .map(catalog_view::network),
                },
                move |this, result| this.render_playlists(result, page),
            );
        } else if mode == 1 {
            self.run_catalog(
                Operation::Library,
                Some((mode, page)),
                force,
                move |service, token, catalog, force| match catalog {
                    Some(catalog) => catalog.playlists(&service, &token, page, force),
                    None => service.playlists(&token, page).map(catalog_view::network),
                },
                move |this, result| {
                    if page == 0 {
                        this.model.borrow_mut().playlists = Some(result.clone());
                    }
                    this.render_playlists(result, page);
                },
            );
        } else {
            self.run_catalog(
                Operation::Library,
                Some((mode, page)),
                force,
                move |service, token, catalog, force| match catalog {
                    Some(catalog) => catalog.liked_tracks(&service, &token, page, force),
                    None => service
                        .liked_tracks(&token, page)
                        .map(catalog_view::network),
                },
                move |this, result| {
                    if page == 0 {
                        this.model.borrow_mut().liked = Some(result.clone());
                    }
                    this.render_liked(result, page);
                },
            );
        }
    }

    fn clear_search_feedback(&self) {
        clear_box(&self.widgets.search_feedback);
        self.widgets.search_feedback.set_visible(false);
    }

    fn search_error(self: &Rc<Self>, error: MusicError, page: u32) {
        self.clear_search_feedback();
        clear_list(&self.widgets.search_rows);
        if matches!(
            error,
            MusicError::AccessDenied | MusicError::Unauthenticated
        ) {
            self.show_error(error);
            return;
        }
        let message = friendly_search_error(error);
        let group = adw::PreferencesGroup::new();
        let row = adw::ActionRow::builder()
            .title("Не удалось выполнить поиск")
            .subtitle(message)
            .title_lines(0)
            .subtitle_lines(0)
            .build();
        row.set_widget_name("music-search-error");
        let retry = gtk::Button::with_label("Повторить поиск");
        retry.set_widget_name("music-search-retry");
        retry.set_valign(gtk::Align::Center);
        retry.add_css_class("suggested-action");
        let weak = Rc::downgrade(self);
        retry.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.search(page);
            }
        });
        row.add_suffix(&retry);
        group.add(&row);
        self.widgets.search_feedback.append(&group);
        // Diagnostics are explicitly disclosed, collapsed and independently
        // selectable. The everyday error contains no protocol vocabulary.
        let details = gtk::Expander::new(Some("Сведения для диагностики"));
        details.set_widget_name("music-search-diagnostics");
        details.set_expanded(false);
        let support = text(
            &format!(
                "Категория: {}\nЭтап: {}\nФорма: {}\nHTTP: {}",
                error.category(),
                error.diagnostic_stage().unwrap_or("none"),
                error.diagnostic_shape().unwrap_or("none"),
                error
                    .http_status()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".into()),
            ),
            "monospace",
        );
        support.set_selectable(true);
        details.set_child(Some(&support));
        self.widgets.search_feedback.append(&details);
        self.widgets.search_feedback.set_visible(true);
    }

    fn tracks_from(
        self: &Rc<Self>,
        list: &gtk::ListBox,
        tracks: Vec<Track>,
        kind: ContextKind,
        source: Option<String>,
        partial: bool,
        range: std::ops::Range<usize>,
    ) {
        clear_list(list);
        if tracks.is_empty() {
            set_empty(list, "Треков пока нет.");
            return;
        }
        let Ok(context) = PlaybackContext::new(kind, source, tracks, range.start, partial) else {
            set_empty(
                list,
                "Этот источник превышает размер очереди. Откройте отдельную страницу.",
            );
            return;
        };
        for (index, track) in context
            .items()
            .iter()
            .enumerate()
            .skip(range.start)
            .take(range.end.saturating_sub(range.start))
        {
            if let Some(selected) = context.with_current_index(index) {
                self.track_row_context(list, track.clone(), selected);
            }
        }
    }

    fn track_row_context(
        self: &Rc<Self>,
        list: &gtk::ListBox,
        track: Track,
        context: PlaybackContext,
    ) {
        let mut subtitle = track.artists.join(", ");
        if track.explicit {
            subtitle.push_str(" · Explicit");
        }
        if !track.available {
            subtitle.push_str(" · Недоступен");
        }
        let (row, body) = item(&track.title, &subtitle);
        row.set_tooltip_text(Some("Воспроизвести трек"));
        row.update_property(&[gtk::accessible::Property::Label(&format!(
            "Воспроизвести: {}",
            track.title
        ))]);
        row.set_activatable(track.available);
        body.prepend(&cover_widget(
            self.artwork.as_ref(),
            track.cover.as_ref(),
            40,
        ));
        if let Some(duration) = track.duration_ms {
            let label = text(&format_duration(duration), "caption");
            label.set_hexpand(false);
            label.set_wrap(false);
            body.append(&label);
        }
        let play = self.context_play_button(&track, true, context.clone());
        play.add_css_class("suggested-action");
        body.append(&play);
        body.append(&self.track_menu_context(&track, Some(context.clone())));
        self.row_action(&row, RowAction::Track(context));
        row.set_activatable(track.available);
        list.append(&row);
    }

    fn album_row(self: &Rc<Self>, list: &gtk::ListBox, album: Album) {
        let (row, body) = item(&album.title, &album.artists.join(", "));
        body.prepend(&cover_widget(
            self.artwork.as_ref(),
            album.cover.as_ref(),
            56,
        ));
        body.append(&gtk::Image::from_icon_name("go-next-symbolic"));
        self.row_action(&row, RowAction::Album(album.id));
        list.append(&row);
    }

    fn artist_row(self: &Rc<Self>, list: &gtk::ListBox, artist: Artist) {
        let (row, body) = item(&artist.name, "Открыть треки исполнителя");
        body.prepend(&cover_widget(
            self.artwork.as_ref(),
            artist.cover.as_ref(),
            56,
        ));
        body.append(&gtk::Image::from_icon_name("go-next-symbolic"));
        self.row_action(&row, RowAction::Artist(artist));
        list.append(&row);
    }

    fn row_action(self: &Rc<Self>, row: &gtk::ListBoxRow, action: RowAction) {
        row.set_activatable(true);
        let group = gio::SimpleActionGroup::new();
        let activate = gio::SimpleAction::new("activate", None);
        let weak = Rc::downgrade(self);
        activate.connect_activate(move |_, _| {
            let Some(this) = weak.upgrade() else { return };
            if this.model.borrow().closed || !this.model.borrow().account_valid {
                return;
            }
            match &action {
                RowAction::Track(context) => this.play_music_context(context.clone()),
                RowAction::SearchPlaylist(playlist) => {
                    this.open_search_playlist(playlist.clone(), 0)
                }
                RowAction::Album(id) => this.open_album(id.clone()),
                RowAction::Artist(artist) => this.open_artist(artist.clone(), 0),
                RowAction::Queue(index) => {
                    this.play_music_queue(*index);
                }
            }
        });
        group.add_action(&activate);
        // Do not set the row's GtkActionable action-name: the list's actual
        // user-activation signal is the single dispatch path, for mouse and
        // keyboard alike. Nested buttons retain their independent actions.
        row.insert_action_group("catalog-row", Some(&group));
    }

    fn detail_loading(self: &Rc<Self>, title: &str, subtitle: &str) {
        self.detail_cache.set(None);
        self.widgets.cache_notice.set_revealed(false);
        self.widgets.cache_hint.set_visible(false);
        self.requests.borrow_mut().invalidate(Operation::LikeRead);
        if let Some(origin) = self.widgets.stack.visible_child_name()
            && origin != "detail"
        {
            self.model.borrow_mut().detail_origin = origin.to_string();
        }
        self.cancel_delivery();
        self.requests.borrow_mut().invalidate(Operation::Delivery);
        self.detail_header(title, subtitle);
        self.widgets.status.set_title("Загрузка выбранных данных…");
        self.widgets.status.set_subtitle("");
        let loading = adw::StatusPage::builder()
            .title("Загрузка…")
            .description("Можно вернуться назад, не дожидаясь ответа.")
            .build();
        let spinner = gtk::Spinner::new();
        spinner.set_spinning(true);
        spinner.set_halign(gtk::Align::Center);
        loading.set_child(Some(&spinner));
        self.widgets.detail.append(&loading);
    }

    fn detail_header(self: &Rc<Self>, title: &str, subtitle: &str) {
        clear_box(&self.widgets.detail);
        let origin = self.model.borrow().detail_origin.clone();
        let label = match origin.as_str() {
            "search" => "Назад к поиску",
            "queue" => "Назад к очереди",
            "home" => "Назад на главную",
            _ => "Назад в библиотеку",
        };
        let back = gtk::Button::with_label(label);
        back.set_widget_name("music-detail-back");
        let back_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        back_content.append(&gtk::Image::from_icon_name("go-previous-symbolic"));
        back_content.append(&gtk::Label::new(Some(label)));
        back.set_child(Some(&back_content));
        back.update_property(&[gtk::accessible::Property::Label(label)]);
        back.set_halign(gtk::Align::Start);
        let weak = Rc::downgrade(self);
        back.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.back_from_detail();
            }
        });
        self.widgets.detail.append(&back);
        self.widgets.detail.append(&text(title, "title-2"));
        self.widgets.detail.append(&text(subtitle, "dim-label"));
        self.widgets.stack.set_visible_child_name("detail");
    }

    fn back_from_detail(&self) {
        if self.widgets.stack.visible_child_name().as_deref() != Some("detail") {
            return;
        }
        self.cancel_delivery();
        self.requests.borrow_mut().invalidate(Operation::Detail);
        self.model.borrow_mut().view_generation += 1;
        let origin = self.model.borrow().detail_origin.clone();
        self.widgets.stack.set_visible_child_name(&origin);
        match origin.as_str() {
            "search" => {
                self.widgets.search.grab_focus();
            }
            "queue" => {
                if let Some(row) = self.widgets.queue_rows.first_child() {
                    row.grab_focus();
                }
            }
            "home" => {
                self.widgets.home_liked.grab_focus();
            }
            _ => {
                self.widgets.library_mode.grab_focus();
            }
        }
    }

    fn detail_cover(&self, cover: Option<&CoverArt>) {
        let Some(back) = self.widgets.detail.first_child() else {
            return;
        };
        let Some(title) = back.next_sibling() else {
            return;
        };
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        header.append(&cover_widget(self.artwork.as_ref(), cover, 96));
        self.widgets.detail.remove(&title);
        title.set_valign(gtk::Align::Center);
        header.append(&title);
        self.widgets.detail.insert_child_after(&header, Some(&back));
    }

    fn track_menu_context(
        self: &Rc<Self>,
        track: &Track,
        context: Option<PlaybackContext>,
    ) -> gtk::MenuButton {
        let button = gtk::MenuButton::builder().label("Ещё").build();
        button.set_valign(gtk::Align::Center);
        button.set_tooltip_text(Some("Действия с треком"));
        button.update_property(&[gtk::accessible::Property::Label("Действия с треком")]);
        let popover = gtk::Popover::new();
        let actions = gtk::Box::new(gtk::Orientation::Vertical, 4);
        super::margins(&actions, 6);
        let detail = gtk::Button::with_label("Открыть сведения о треке");
        detail.add_css_class("flat");
        let weak = Rc::downgrade(self);
        let close = popover.downgrade();
        let id = track.id.clone();
        let detail_context = context.clone();
        detail.connect_clicked(move |_| {
            if let Some(popover) = close.upgrade() {
                popover.popdown();
            }
            if let Some(this) = weak.upgrade() {
                this.open_track_context(id.clone(), detail_context.clone());
            }
        });
        actions.append(&detail);
        let play = gtk::Button::with_label("Воспроизвести");
        play.add_css_class("flat");
        play.set_sensitive(track.available);
        let weak = Rc::downgrade(self);
        let close = popover.downgrade();
        let selected = track.clone();
        play.connect_clicked(move |_| {
            if let Some(popover) = close.upgrade() {
                popover.popdown();
            }
            if let Some(this) = weak.upgrade() {
                if let Some(context) = context.clone() {
                    this.play_music_context(context);
                } else {
                    this.play_music_track(selected.clone());
                }
            }
        });
        actions.append(&play);
        let add = gtk::Button::with_label("Добавить в очередь");
        add.add_css_class("flat");
        let weak = Rc::downgrade(self);
        let close = popover.downgrade();
        let queued = track.clone();
        add.connect_clicked(move |_| {
            if let Some(popover) = close.upgrade() {
                popover.popdown();
            }
            if let Some(this) = weak.upgrade() {
                this.enqueue(queued.clone());
            }
        });
        actions.append(&add);
        if let Some(id) = &track.album_id {
            let album = gtk::Button::with_label("Открыть альбом");
            album.add_css_class("flat");
            let weak = Rc::downgrade(self);
            let close = popover.downgrade();
            let id = id.clone();
            album.connect_clicked(move |_| {
                if let Some(popover) = close.upgrade() {
                    popover.popdown();
                }
                if let Some(this) = weak.upgrade() {
                    this.open_album(id.clone());
                }
            });
            actions.append(&album);
        }
        popover.set_child(Some(&actions));
        button.set_popover(Some(&popover));
        button
    }

    fn detail_error(self: &Rc<Self>, error: MusicError, retry: impl Fn(&Rc<Self>) + 'static) {
        if matches!(
            error,
            MusicError::AccessDenied | MusicError::Unauthenticated
        ) {
            self.show_error(error);
            return;
        }
        // Retain the selected title and Back action; replace only the content.
        let subtitle = self
            .widgets
            .detail
            .first_child()
            .and_then(|child| child.next_sibling())
            .and_then(|child| child.next_sibling())
            .and_downcast::<gtk::Label>();
        if let Some(subtitle) = subtitle {
            subtitle.set_text("Не удалось загрузить данные.");
        }
        let mut content = self.widgets.detail.first_child();
        for _ in 0..3 {
            content = content.and_then(|child| child.next_sibling());
        }
        while let Some(child) = content {
            content = child.next_sibling();
            self.widgets.detail.remove(&child);
        }
        let (title, message) = music_error_message(error);
        let failure = adw::StatusPage::builder()
            .title(title)
            .description(message)
            .icon_name("dialog-warning-symbolic")
            .build();
        let button = gtk::Button::with_label("Повторить");
        button.set_halign(gtk::Align::Center);
        button.add_css_class("suggested-action");
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                retry(&this);
            }
        });
        failure.set_child(Some(&button));
        self.widgets.detail.append(&failure);
    }

    fn open_track_context(self: &Rc<Self>, id: String, context: Option<PlaybackContext>) {
        self.open_track_policy(id, context, false);
    }

    fn open_track_policy(
        self: &Rc<Self>,
        id: String,
        context: Option<PlaybackContext>,
        force: bool,
    ) {
        let retry_context = context.clone();
        self.detail_loading("Сведения о треке", "Загрузка трека…");
        let refresh_id = id.clone();
        let refresh_context = context.clone();
        self.detail_refresh.replace(Some(Rc::new(move |this| {
            this.open_track_policy(refresh_id.clone(), refresh_context.clone(), true)
        })));
        let retry_id = id.clone();
        self.run_catalog(
            Operation::Detail,
            None,
            force,
            move |service, token, catalog, force| match catalog {
                Some(catalog) => catalog.track(&service, &token, &id, force),
                None => service.track(&token, &id).map(catalog_view::network),
            },
            move |this, result| match result {
                Ok(track) => {
                    this.select_track_info(track.clone(), context.clone(), false);
                    this.detail_header(&track.title, &track.artists.join(", "));
                    this.detail_cover(track.cover.as_ref());
                    let duration = track
                        .duration_ms
                        .map(|ms| format!("{}:{:02}", ms / 60_000, (ms / 1_000) % 60))
                        .unwrap_or_else(|| "неизвестна".into());
                    this.widgets
                        .detail
                        .append(&text(&format!("Продолжительность: {duration}"), ""));
                    if !track.available {
                        this.widgets
                            .detail
                            .append(&text("Сервис пометил трек как недоступный.", "dim-label"));
                    }
                    let button = if let Some(context) = context.clone() {
                        this.context_play_button(&track, false, context)
                    } else {
                        this.track_play_button(&track, false)
                    };
                    this.widgets.detail.append(&button);
                    let add = gtk::Button::with_label("Добавить в очередь");
                    add.set_halign(gtk::Align::Start);
                    let weak = Rc::downgrade(this);
                    let queued = track.clone();
                    add.connect_clicked(move |_| {
                        if let Some(this) = weak.upgrade() {
                            this.enqueue(queued.clone());
                        }
                    });
                    this.widgets.detail.append(&add);
                    this.widgets
                        .detail
                        .append(&this.like_controls(track.id.clone()));
                    if this.model.borrow().delivery_diagnostic {
                        this.delivery_action(track.id.clone());
                    }
                    if let Some(album_id) = track.album_id {
                        let album = gtk::Button::with_label(
                            track.album_title.as_deref().unwrap_or("Открыть альбом"),
                        );
                        album.set_halign(gtk::Align::Start);
                        if let Some(label) = album.child().and_downcast::<gtk::Label>() {
                            label.set_wrap(true);
                            label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                        }
                        let weak = Rc::downgrade(this);
                        album.connect_clicked(move |_| {
                            if let Some(this) = weak.upgrade() {
                                this.open_album(album_id.clone());
                            }
                        });
                        this.widgets.detail.append(&album);
                    }
                    this.widgets.status.set_title("Сведения о треке получены.");
                }
                Err(error) => this.detail_error(error, move |this| {
                    this.open_track_context(retry_id.clone(), retry_context.clone())
                }),
            },
        );
    }

    fn open_album(self: &Rc<Self>, id: String) {
        self.open_album_policy(id, false, 0);
    }

    fn open_album_policy(self: &Rc<Self>, id: String, force: bool, page: usize) {
        self.detail_loading("Альбом", "Загрузка альбома…");
        let refresh_id = id.clone();
        self.detail_refresh.replace(Some(Rc::new(move |this| {
            this.open_album_policy(refresh_id.clone(), true, page)
        })));
        let retry_id = id.clone();
        self.run_catalog(
            Operation::Detail,
            None,
            force,
            move |service, token, catalog, force| match catalog {
                Some(catalog) => catalog.album(&service, &token, &id, force),
                None => service.album(&token, &id).map(catalog_view::network),
            },
            move |this, result| match result {
                Ok(album) => {
                    this.render_album(Rc::new(album), page);
                    this.widgets.status.set_title("Альбом получен.");
                }
                Err(error) => this.detail_error(error, move |this| {
                    this.open_album_policy(retry_id.clone(), true, page)
                }),
            },
        );
    }

    fn render_album(self: &Rc<Self>, album: Rc<Album>, page: usize) {
        const ROWS: usize = 50;
        let page = page.min(album.tracks.len().saturating_sub(1) / ROWS);
        let id = album.id.clone();
        self.detail_refresh.replace(Some(Rc::new(move |this| {
            this.open_album_policy(id.clone(), true, page)
        })));
        self.detail_header(&album.title, &album.artists.join(", "));
        self.detail_cover(album.cover.as_ref());
        let rows = list();
        let offset = page.saturating_mul(ROWS);
        if album.tracks.len() <= 500 {
            self.tracks_from(
                &rows,
                album.tracks.clone(),
                ContextKind::Album,
                Some(album.id.clone()),
                false,
                offset..offset.saturating_add(ROWS),
            );
        } else {
            self.tracks_from(
                &rows,
                album
                    .tracks
                    .iter()
                    .skip(offset)
                    .take(ROWS)
                    .cloned()
                    .collect(),
                ContextKind::Album,
                Some(album.id.clone()),
                true,
                0..ROWS,
            );
        }
        self.widgets.detail.append(&rows);
        let pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        if page > 0 {
            let back = gtk::Button::with_label("Предыдущие");
            let weak = Rc::downgrade(self);
            let album = album.clone();
            back.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.render_album(album.clone(), page - 1);
                }
            });
            pages.append(&back);
        }
        if offset.saturating_add(ROWS) < album.tracks.len() {
            let next = gtk::Button::with_label("Следующие");
            let weak = Rc::downgrade(self);
            next.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.render_album(album.clone(), page + 1);
                }
            });
            pages.append(&next);
        }
        self.widgets.detail.append(&pages);
    }

    fn open_playlist(self: &Rc<Self>, playlist: Playlist, page: u32) {
        self.open_playlist_policy(playlist, page, false);
    }

    fn open_playlist_policy(self: &Rc<Self>, playlist: Playlist, page: u32, force: bool) {
        let id = playlist.id.clone();
        self.detail_loading(
            &playlist.title,
            &format!("Загрузка плейлиста · страница {}", page + 1),
        );
        let refresh_playlist = playlist.clone();
        self.detail_refresh.replace(Some(Rc::new(move |this| {
            this.open_playlist_policy(refresh_playlist.clone(), page, true)
        })));
        self.detail_cover(playlist.cover.as_ref());
        self.run_catalog(
            Operation::Detail,
            None,
            force,
            move |service, token, catalog, force| match catalog {
                Some(catalog) => catalog.playlist_tracks(&service, &token, &id, page, force),
                None => service
                    .playlist_tracks(&token, &id, page)
                    .map(catalog_view::network),
            },
            move |this, result| match result {
                Ok(result) => {
                    this.detail_header(
                        &playlist.title,
                        &format!("Плейлист · страница {}", page + 1),
                    );
                    this.detail_cover(playlist.cover.as_ref());
                    let rows = list();
                    this.tracks_from(
                        &rows,
                        result.items,
                        ContextKind::Playlist,
                        Some(format!("{}:{}", playlist.id.owner, playlist.id.kind)),
                        page > 0 || result.has_more,
                        0..usize::MAX,
                    );
                    this.widgets.detail.append(&rows);
                    let pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    if page > 0 {
                        let previous = gtk::Button::with_label("Предыдущая страница");
                        let weak = Rc::downgrade(this);
                        let playlist = playlist.clone();
                        previous.connect_clicked(move |_| {
                            if let Some(this) = weak.upgrade() {
                                this.open_playlist(playlist.clone(), page - 1);
                            }
                        });
                        pages.append(&previous);
                    }
                    if result.has_more {
                        let next = gtk::Button::with_label("Следующая страница");
                        next.set_halign(gtk::Align::Start);
                        let weak = Rc::downgrade(this);
                        next.connect_clicked(move |_| {
                            if let Some(this) = weak.upgrade() {
                                this.open_playlist(playlist.clone(), page.saturating_add(1));
                            }
                        });
                        pages.append(&next);
                    }
                    this.widgets.detail.append(&pages);
                    this.widgets.status.set_title("Плейлист получен.");
                    this.widgets.status.set_subtitle("");
                }
                Err(error) => this.detail_error(error, move |this| {
                    this.open_playlist(playlist.clone(), page)
                }),
            },
        );
    }

    fn open_artist(self: &Rc<Self>, artist: Artist, page: u32) {
        self.open_artist_policy(artist, page, false);
    }

    fn open_artist_policy(self: &Rc<Self>, artist: Artist, page: u32, force: bool) {
        self.detail_loading(&artist.name, "Загрузка треков исполнителя…");
        let refresh_artist = artist.clone();
        self.detail_refresh.replace(Some(Rc::new(move |this| {
            this.open_artist_policy(refresh_artist.clone(), page, true)
        })));
        self.detail_cover(artist.cover.as_ref());
        let id = artist.id.clone();
        self.run_catalog(
            Operation::Detail,
            None,
            force,
            move |service, token, catalog, force| match catalog {
                Some(catalog) => catalog.artist_tracks(&service, &token, &id, page, force),
                None => service
                    .artist_tracks(&token, &id, page)
                    .map(catalog_view::network),
            },
            move |this, result| match result {
                Ok(result) => {
                    this.detail_header(
                        &artist.name,
                        &format!("Треки исполнителя · страница {}", page + 1),
                    );
                    this.detail_cover(artist.cover.as_ref());
                    let rows = list();
                    this.tracks_from(
                        &rows,
                        result.items,
                        ContextKind::Artist,
                        Some(artist.id.clone()),
                        page > 0 || result.has_more,
                        0..usize::MAX,
                    );
                    this.widgets.detail.append(&rows);
                    let pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    if page > 0 {
                        let previous = gtk::Button::with_label("Предыдущая страница");
                        let weak = Rc::downgrade(this);
                        let artist = artist.clone();
                        previous.connect_clicked(move |_| {
                            if let Some(this) = weak.upgrade() {
                                this.open_artist(artist.clone(), page - 1);
                            }
                        });
                        pages.append(&previous);
                    }
                    if result.has_more {
                        let next = gtk::Button::with_label("Следующая страница");
                        let weak = Rc::downgrade(this);
                        next.connect_clicked(move |_| {
                            if let Some(this) = weak.upgrade() {
                                this.open_artist(artist.clone(), page.saturating_add(1));
                            }
                        });
                        pages.append(&next);
                    }
                    this.widgets.detail.append(&pages);
                    this.widgets.status.set_title("Треки исполнителя получены.");
                    this.widgets.status.set_subtitle("");
                }
                Err(error) => {
                    this.detail_error(error, move |this| this.open_artist(artist.clone(), page))
                }
            },
        );
    }

    fn delivery_action(self: &Rc<Self>, id: String) {
        let action = gtk::Button::with_label("Проверить доставку аудио без загрузки");
        action.set_widget_name("music-delivery-probe");
        action.set_halign(gtk::Align::Start);
        let report = text(
            "Локальный opt-in эксперимент: проверяется только информация о доставке. Аудио не загружается.",
            "dim-label",
        );
        report.set_widget_name("music-delivery-report");
        self.widgets.detail.append(&action);
        self.widgets.detail.append(&report);
        let weak = Rc::downgrade(self);
        action.connect_clicked(move |action| {
            let Some(this) = weak.upgrade() else {
                return;
            };
            this.cancel_delivery();
            let cancellation = crate::stream::DeliveryCancellation::new();
            this.model.borrow_mut().delivery_cancellation = Some(cancellation.clone());
            let id = id.clone();
            let label = report.downgrade();
            let button = action.downgrade();
            action.set_sensitive(false);
            report.set_text("Проверка информации о доставке…");
            this.run(
                Operation::Delivery,
                move |service, token| {
                    Ok(match crate::stream::DeliveryProbe::new() {
                        Ok(probe) => probe.probe(&service, &token, &id, &cancellation),
                        Err(report) => report,
                    })
                },
                move |this, result| {
                    this.model.borrow_mut().delivery_cancellation = None;
                    if let Some(label) = label.upgrade() {
                        match result {
                            Ok(report) => label.set_text(&report.to_string()),
                            Err(error) => label.set_text(music_error_message(error).0),
                        }
                    }
                    if let Some(button) = button.upgrade() {
                        button.set_sensitive(true);
                    }
                },
            );
        });
    }

    fn enqueue(self: &Rc<Self>, track: Track) {
        let result = self.model.borrow_mut().queue.append(track);
        match result {
            Ok(_) => self.widgets.status.set_title("Трек добавлен в очередь."),
            Err(_) => self
                .widgets
                .status
                .set_title("Очередь заполнена. Удалите ненужные треки."),
        }
        self.render_queue();
    }
}

fn destination(icon: &str, title: &str, subtitle: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("card");
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    super::margins(&content, 18);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(32);
    image.set_halign(gtk::Align::Start);
    image.add_css_class("accent");
    content.append(&image);
    content.append(&text(title, "heading"));
    content.append(&text(subtitle, "caption"));
    button.set_child(Some(&content));
    button.set_hexpand(true);
    button
}

fn cover_widget(
    loader: Option<&ArtworkLoader>,
    cover: Option<&CoverArt>,
    size: i32,
) -> gtk::Widget {
    match loader {
        Some(loader) => super::artwork::thumbnail(loader, cover, size).upcast(),
        None => {
            let fallback = gtk::Image::from_icon_name("audio-x-generic-symbolic");
            fallback.set_pixel_size(size / 2);
            fallback.set_size_request(size, size);
            fallback.add_css_class("dim-label");
            fallback.upcast()
        }
    }
}

fn track_count_text(count: u64) -> String {
    let word = if (11..=14).contains(&(count % 100)) {
        "треков"
    } else {
        match count % 10 {
            1 => "трек",
            2..=4 => "трека",
            _ => "треков",
        }
    };
    format!("{count} {word}")
}

fn format_duration(ms: u64) -> String {
    format!("{}:{:02}", ms / 60_000, (ms / 1_000) % 60)
}

fn public_notice() -> &'static str {
    "Войдите в свой аккаунт Яндекса, чтобы открыть библиотеку и поиск. Прототип проверит доступ к Яндекс Музыке после входа."
}
fn text(value: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(value));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_hexpand(true);
    if !class.is_empty() {
        label.add_css_class(class);
    }
    label
}
fn icon_button(icon: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(label));
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button.set_valign(gtk::Align::Center);
    button
}
fn list() -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.connect_row_activated(|_, row| {
        if row.is_activatable() {
            let _ = row.activate_action("catalog-row.activate", None);
        }
    });
    list.add_css_class("boxed-list");
    list
}
fn clear_list(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}
fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}
fn set_empty(list: &gtk::ListBox, message: &str) {
    clear_list(list);
    let (row, _) = item(message, "");
    row.set_activatable(false);
    row.set_focusable(false);
    list.append(&row);
}
fn item(title: &str, subtitle: &str) -> (gtk::ListBoxRow, gtk::Box) {
    let row = gtk::ListBoxRow::new();
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    body.set_margin_start(12);
    body.set_margin_end(12);
    body.set_margin_top(8);
    body.set_margin_bottom(8);
    let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
    labels.set_hexpand(true);
    labels.append(&text(title, "heading"));
    if !subtitle.is_empty() {
        labels.append(&text(subtitle, "dim-label"));
    }
    body.append(&labels);
    row.set_child(Some(&body));
    (row, body)
}

impl Controller {
    fn authenticate(self: &Rc<Self>, token: AccessToken) {
        {
            let mut model = self.model.borrow_mut();
            if model.closed {
                token.invalidate();
                return;
            }
            if let Some(previous) = model.token.replace(token) {
                previous.invalidate();
            }
            model.account_valid = false;
            model.auth_busy = false;
            model.busy = false;
        }
        if self.service.is_none() {
            self.auth_failed(AuthError::Configuration);
            return;
        }
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            dialog.loading("Проверка доступа к Яндекс Музыке…");
        }
        self.run(Operation::Account, |service, token| {
            let account = service.account(&token)?;
            let scope = service.cache_scope(&token)?;
            Ok((account, scope))
        }, |this, result| match result {
            Ok((account, scope)) => {
                {
                    let mut model = this.model.borrow_mut();
                    model.generation += 1;
                    model.account_valid = true;
                    if let Some(attempt) = model.attempt.take() { attempt.cancel(); }
                }
                this.widgets.account.set_text(&account.display_name);
                this.widgets.notice.set_text("Аккаунт Яндекс Музыки проверен. Сохранением входа можно управлять в настройках.");
                this.can_create_playlist.set(true);
                this.session_validated(scope);
                this.dismiss_login();
                this.render_controls();
                this.load_initial_library();
            }
            Err(error) => {
                this.session_validation_failed(error);
                let (title, message) = music_error_message(error);
                {
                    let mut model = this.model.borrow_mut();
                    model.generation += 1;
                    if let Some(token) = model.token.take() { token.invalidate(); }
                    if let Some(attempt) = model.attempt.take() { attempt.cancel(); }
                    model.account_valid = false;
                    model.auth_busy = false;
            model.busy = false;
                }
                if let Some(service) = &this.service { service.clear_session(); }
                this.reset_status();
                if let Some(dialog) = this.login_dialog.borrow().as_ref() {
                    let status = error.http_status().map(|status| format!(" · HTTP {status}")).unwrap_or_default();
                    dialog.unavailable(title, &format!("Шаг: проверка аккаунта Яндекс Музыки. {message}{status}"));
                }
                this.render_controls();
            }
        });
    }
}

fn launch_system_browser(url: &str) -> Result<(), AuthError> {
    gio::AppInfo::launch_default_for_uri(url, None::<&gio::AppLaunchContext>)
        .map_err(|_| AuthError::BrowserLoad)
}

fn catalog_diagnostic(operation: Operation, error: Option<MusicError>) {
    let mut line = catalog_line(
        operation,
        CatalogPhase::Result,
        error.map(MusicError::category).unwrap_or("success"),
        error.and_then(MusicError::http_status),
        error.and_then(MusicError::diagnostic_stage),
    );
    if let Some(shape) = error.and_then(MusicError::diagnostic_shape) {
        use std::fmt::Write;
        let _ = write!(line, " shape={shape}");
    }
    write_catalog_line(line);
}

fn catalog_event(operation: Operation, category: &'static str, http: Option<u16>) {
    write_catalog_line(catalog_line(
        operation,
        CatalogPhase::State,
        category,
        http,
        None,
    ));
}

#[derive(Clone, Copy)]
enum CatalogPhase {
    Requested,
    Result,
    State,
}

fn catalog_line(
    operation: Operation,
    phase: CatalogPhase,
    category: &'static str,
    http: Option<u16>,
    stage: Option<&'static str>,
) -> String {
    let event = match (operation, phase) {
        (Operation::Detail, CatalogPhase::Requested) => "detail-requested",
        (Operation::Detail, CatalogPhase::Result) => "detail-result",
        (Operation::Search, CatalogPhase::Requested) => "search-requested",
        (Operation::Search, CatalogPhase::Result) => "search-result",
        (_, CatalogPhase::Requested) => "requested",
        (_, CatalogPhase::Result) => "result",
        (_, CatalogPhase::State) => "state",
    };
    format!(
        "Karin catalog: operation={} outcome={} http={} stage={} event={}",
        operation.category(),
        category,
        http.map(|value| value.to_string())
            .unwrap_or_else(|| "none".into()),
        stage.unwrap_or("none"),
        event,
    )
}

fn write_catalog_line(line: String) {
    use std::io::Write;
    // All values are local finite categories or a numeric status. Never format
    // provider errors, responses, queries, collection names, IDs or URLs.
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

fn music_error_message(error: MusicError) -> (&'static str, &'static str) {
    match error {
        MusicError::SearchResponse { kind, .. } => {
            use crate::music::SearchFailureKind;
            music_error_message(match kind {
                SearchFailureKind::Contract => MusicError::InvalidResponse,
                SearchFailureKind::ResponseTooLarge => MusicError::ResponseTooLarge,
                SearchFailureKind::Network => MusicError::Network,
                SearchFailureKind::Timeout => MusicError::Timeout,
            })
        }
        MusicError::Forbidden => (
            "Music API отклонил запрос",
            "Причина запрета не установлена. Это не означает, что вы ввели неверный пароль Яндекса.",
        ),
        MusicError::MissingScopes => (
            "Music API требует дополнительные разрешения",
            "Сервис сообщил об отсутствующих разрешениях для этого запроса. Это не означает неверный пароль Яндекса.",
        ),
        MusicError::AccessDenied | MusicError::Unauthenticated => (
            "Сеанс больше не действует",
            "Выполните вход снова, чтобы проверить доступ к Музыке.",
        ),
        MusicError::Restricted => (
            "Доступ ограничен сервисом",
            "Яндекс Музыка ограничила доступ к этим данным.",
        ),
        MusicError::RateLimited => (
            "Слишком много запросов",
            "Подождите и повторите запрос вручную.",
        ),
        MusicError::Network => (
            "Нет соединения с Музыкой",
            "Проверьте подключение к сети и повторите запрос.",
        ),
        MusicError::Timeout => ("Сервис не ответил вовремя", "Повторите запрос позже."),
        MusicError::Configuration => (
            "Сетевой клиент недоступен",
            "Не удалось подготовить подключение к Яндекс Музыке.",
        ),
        MusicError::ResponseTooLarge | MusicError::InvalidResponse => (
            "Ответ сервиса не поддерживается",
            "Не удалось безопасно прочитать данные Яндекс Музыки.",
        ),
        MusicError::InvalidInput => ("Проверьте запрос", "Измените запрос и повторите действие."),
        MusicError::Http(_) => (
            "Запрос не выполнен",
            "Яндекс Музыка отклонила запрос. Повторите позже.",
        ),
    }
}

fn friendly_search_error(error: MusicError) -> &'static str {
    match error {
        MusicError::SearchResponse { kind, .. } => {
            use crate::music::SearchFailureKind;
            friendly_search_error(match kind {
                SearchFailureKind::Network => MusicError::Network,
                SearchFailureKind::Timeout => MusicError::Timeout,
                SearchFailureKind::Contract => MusicError::InvalidResponse,
                SearchFailureKind::ResponseTooLarge => MusicError::ResponseTooLarge,
            })
        }
        MusicError::Network => "Проверьте подключение к сети и попробуйте ещё раз.",
        MusicError::Timeout => "Сервис не ответил вовремя. Попробуйте ещё раз.",
        MusicError::RateLimited => "Слишком много запросов. Немного подождите и повторите поиск.",
        MusicError::Forbidden | MusicError::MissingScopes | MusicError::Restricted => {
            "Сервис ограничил этот поиск. Попробуйте повторить запрос позже."
        }
        MusicError::InvalidInput => "Уточните поисковый запрос и попробуйте ещё раз.",
        _ => "Не удалось прочитать результаты. Повторите поиск позже.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    pub(super) fn pump_until(mut condition: impl FnMut() -> bool) {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if condition() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native fixture operation timed out"
            );
            if context.pending() {
                context.iteration(false);
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
    }

    // Optional review evidence from this test-only fixture window. No synthetic
    // catalog, account or token is compiled into the normal application.
    pub(super) fn capture_native(name: &str) {
        let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") else {
            return;
        };
        let directory = std::path::PathBuf::from(directory);
        // Static synthetic fixture evidence uses reduced motion so an opening
        // Adw.Dialog fade cannot masquerade as settled layout. Normal smoke,
        // interactive WebKit and the full-window capture script keep their
        // ordinary motion settings in separate processes.
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_enable_animations(false);
        }
        std::fs::create_dir_all(&directory).unwrap();
        for (theme, scheme) in [
            ("light", adw::ColorScheme::ForceLight),
            ("dark", adw::ColorScheme::ForceDark),
        ] {
            adw::StyleManager::default().set_color_scheme(scheme);
            let painted = Instant::now() + Duration::from_millis(180);
            pump_until(|| Instant::now() >= painted);
            let result = std::process::Command::new("xdotool")
                .args([
                    "search",
                    "--onlyvisible",
                    "--name",
                    "^Karin synthetic HTTP fixture$",
                ])
                .output()
                .unwrap();
            assert!(result.status.success());
            let id = String::from_utf8(result.stdout).unwrap();
            let id = id.lines().next().unwrap();
            assert!(id.bytes().all(|byte| byte.is_ascii_digit()));
            assert!(
                std::process::Command::new("import")
                    .args(["-silent", "-screen", "-window", id])
                    .arg(directory.join(format!("{name}-{theme}.png")))
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }

    pub(super) fn contains_text(widget: &gtk::Widget, wanted: &str) -> bool {
        if widget
            .downcast_ref::<gtk::Label>()
            .is_some_and(|label| label.text().contains(wanted))
        {
            return true;
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            if contains_text(&widget, wanted) {
                return true;
            }
            child = widget.next_sibling();
        }
        false
    }

    fn button_with_label(widget: &gtk::Widget, label: &str) -> Option<gtk::Button> {
        if let Some(button) = widget.downcast_ref::<gtk::Button>()
            && button.label().as_deref() == Some(label)
        {
            return Some(button.clone());
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            if let Some(button) = button_with_label(&widget, label) {
                return Some(button);
            }
            child = widget.next_sibling();
        }
        None
    }

    fn rows(list: &gtk::ListBox) -> usize {
        let mut count = 0;
        let mut child = list.first_child();
        while let Some(widget) = child {
            count += 1;
            child = widget.next_sibling();
        }
        count
    }

    struct FixtureProvider {
        blocked: Arc<AtomicBool>,
    }

    impl AuthProvider for FixtureProvider {
        fn begin(&self) -> Result<Arc<AuthAttempt>, AuthError> {
            if self.blocked.load(Ordering::Acquire) {
                return Err(AuthError::MusicPermissionUnavailable);
            }
            AuthAttempt::for_test("http://127.0.0.1:1/authorize", BrowserMode::System)
        }

        fn exchange(
            &self,
            attempt: &AuthAttempt,
            code: crate::auth::AuthorizationCode,
        ) -> Result<AccessToken, AuthError> {
            if attempt.is_cancelled() {
                return Err(AuthError::Cancelled);
            }
            assert_eq!(code.expose(), "synthetic-ui-code");
            Ok(AccessToken::for_test(
                "synthetic-ui-token",
                Duration::from_secs(60),
            ))
        }
    }

    // Replaces only the system-browser launch in an explicit test window. The
    // real callback listener validates state and consumes the opaque code.
    fn launch_fixture_callback(authorization: &str) -> Result<(), AuthError> {
        let authorization = reqwest::Url::parse(authorization).unwrap();
        let value = |key| {
            authorization
                .query_pairs()
                .find(|(name, _)| name == key)
                .unwrap()
                .1
                .into_owned()
        };
        let mut callback = reqwest::Url::parse(&value("redirect_uri")).unwrap();
        callback
            .query_pairs_mut()
            .append_pair("state", &value("state"))
            .append_pair("code", "synthetic-ui-code");
        assert_eq!(callback.host_str(), Some("127.0.0.1"));
        thread::spawn(move || {
            let response = reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(4))
                .build()
                .unwrap()
                .get(callback)
                .send()
                .unwrap();
            assert!(response.status().is_success());
        });
        Ok(())
    }

    fn assert_no_credential_fields(widget: &gtk::Widget) {
        assert!(!widget.is::<gtk::Entry>());
        assert!(!widget.is::<gtk::PasswordEntry>());
        let mut child = widget.first_child();
        while let Some(current) = child {
            assert_no_credential_fields(&current);
            child = current.next_sibling();
        }
    }

    fn validation_failure_cancel_and_retry(app: &adw::Application) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let requested = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let server_requested = requested.clone();
        let finished = Arc::new(AtomicBool::new(false));
        let server_finished = finished.clone();
        let (release_validation, wait_for_release) = std::sync::mpsc::channel();
        let account =
            json!({"result":{"account":{"uid":1234,"displayName":"Synthetic retry account"}}})
                .to_string();
        let playlists = json!({"result":[{"kind":3,"title":"Synthetic independent playlist","trackCount":0,"owner":{"uid":1234}}]}).to_string();
        let responses = vec![
            ("GET /account/status", 200, account.clone(), true),
            (
                "GET /account/status",
                403,
                "synthetic-private-provider-message".to_owned(),
                false,
            ),
            ("GET /account/status", 200, account.clone(), false),
            (
                "GET /users/1234/likes/tracks",
                403,
                "synthetic-private-provider-message".to_owned(),
                false,
            ),
            ("GET /users/1234/playlists/list", 200, playlists, false),
            ("GET /account/status", 200, account, true),
        ];
        let server = thread::spawn(move || {
            for (index, (prefix, status, body, delayed)) in responses.into_iter().enumerate() {
                let deadline = Instant::now() + Duration::from_secs(8);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("validation fixture request missing: {error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    assert_eq!(stream.read(&mut byte).unwrap(), 1);
                    request.push(byte[0]);
                    assert!(request.len() < 32 * 1024);
                }
                assert!(String::from_utf8(request).unwrap().starts_with(prefix));
                server_requested.store(index + 1, Ordering::Release);
                if delayed {
                    wait_for_release
                        .recv_timeout(Duration::from_secs(8))
                        .unwrap();
                }
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
            server_finished.store(true, Ordering::Release);
        });
        let blocked = Arc::new(AtomicBool::new(false));
        let ui = super::super::build_with_music_pane(
            app,
            crate::application::ConnectionService::new(crate::api::YandexApi::new().unwrap()),
            |app, window| {
                MusicPane::with_provider(
                    app,
                    window,
                    Some(MusicService::for_test(&format!("http://{address}/"))),
                    Arc::new(FixtureProvider {
                        blocked: blocked.clone(),
                    }),
                    launch_fixture_callback,
                )
            },
        );
        let pane = &ui.music;
        let window = ui.window.clone();
        window.set_title(Some("Karin synthetic HTTP fixture"));
        window.set_default_size(900, 650);
        window.present();
        let this = &pane.controller;
        this.widgets.login.emit_clicked();
        pump_until(|| requested.load(Ordering::Acquire) == 1);
        assert!(!this.model.borrow().account_valid);
        assert!(
            this.login_dialog.borrow().is_some(),
            "Music validation keeps its dialog open"
        );
        let cancel = this.login_dialog.borrow().as_ref().unwrap().cancel.clone();
        cancel.emit_clicked();
        assert!(this.model.borrow().token.is_none());
        release_validation.send(()).unwrap();
        this.widgets.login.emit_clicked();
        pump_until(|| requested.load(Ordering::Acquire) == 2 && !this.model.borrow().busy);
        assert!(!this.model.borrow().account_valid);
        assert!(this.model.borrow().token.is_none());
        let dialog = this.login_dialog.borrow().as_ref().unwrap().clone();
        assert!(contains_text(
            dialog.dialog.upcast_ref(),
            "Music API отклонил запрос"
        ));
        assert!(!contains_text(pane.widget.upcast_ref(), "HTTP 403"));
        assert!(!contains_text(
            window.upcast_ref(),
            "synthetic-private-provider-message"
        ));
        assert!(
            this.widgets
                .status
                .subtitle()
                .is_none_or(|value| value.is_empty())
        );
        capture_native("login-music-permission-denied");
        dialog.retry.emit_clicked();
        pump_until(|| this.model.borrow().account_valid && !this.model.borrow().busy);
        assert!(this.login_dialog.borrow().is_none());
        assert!(contains_text(
            pane.widget.upcast_ref(),
            "Music API отклонил запрос"
        ));
        this.widgets.library_mode.set_selected(1);
        assert!(contains_text(
            this.playlist_grid.widget.upcast_ref(),
            "Synthetic independent playlist"
        ));
        this.logout();
        assert!(
            this.widgets
                .status
                .subtitle()
                .is_none_or(|value| value.is_empty())
        );
        assert!(!contains_text(
            pane.widget.upcast_ref(),
            "Music API отклонил запрос"
        ));
        blocked.store(true, Ordering::Release);
        this.widgets.login.emit_clicked();
        pump_until(|| !this.model.borrow().busy);
        assert!(
            this.widgets
                .status
                .subtitle()
                .is_none_or(|value| value.is_empty())
        );
        this.cancel_login();
        blocked.store(false, Ordering::Release);
        this.widgets.login.emit_clicked();
        pump_until(|| requested.load(Ordering::Acquire) == 6);
        let weak = Rc::downgrade(&pane.controller);
        pane.close();
        assert!(this.model.borrow().token.is_none());
        assert!(this.login_dialog.borrow().is_none());
        window.close();
        drop(ui);
        drop(window);
        release_validation.send(()).unwrap();
        pump_until(|| finished.load(Ordering::Acquire));
        server.join().unwrap();
        pump_until(|| weak.upgrade().is_none());
    }

    #[test]
    #[ignore = "requires an isolated GTK display; run explicitly under Xvfb"]
    fn native_music_session_catalog_queue_and_logout() {
        gtk::init().unwrap();
        adw::init().unwrap();
        let unpresented = LoginDialog::new();
        let unpresented_weak = unpresented.dialog.downgrade();
        unpresented.request_close();
        drop(unpresented);
        assert!(
            unpresented_weak.upgrade().is_none(),
            "Readiness ticks must not retain an unmapped dialog"
        );
        let track = json!({"id":"42","title":"Synthetic UI track","durationMs":123000,"available":true,
            "artists":[{"id":11,"name":"Synthetic artist"}],"albums":[{"id":7,"title":"Synthetic album"}]});
        let account =
            json!({"result":{"account":{"uid":1234,"displayName":"Synthetic UI account"}}});
        let liked = json!({"result":{"library":{"uid":1234,"revision":1,"tracks":vec![json!({"id":42,"albumId":7});51]}}});
        let tracks = json!({"result":[track.clone()]});
        let playlists = json!({"result":[{"kind":3,"title":"Synthetic playlist","trackCount":1,"owner":{"uid":1234}}]});
        let playlist = json!({"result":{"kind":3,"title":"Synthetic playlist","trackCount":1,"tracks":[{"id":42,"albumId":7}]}});
        let search = json!({"result":{"text":"Synthetic","tracks":{"total":1,"perPage":20,"page":0,"results":[track.clone()]}}});
        let album = json!({"result":{"id":7,"title":"Synthetic album","artists":[{"id":11,"name":"Synthetic artist"}],"volumes":[vec![track;63]]}});
        let membership_body = liked.to_string();
        let responses = vec![
            ("GET /account/status", account, false),
            ("GET /users/1234/likes/tracks", liked.clone(), false),
            ("POST /tracks", tracks.clone(), false),
            ("GET /users/1234/playlists/list", playlists, false),
            ("GET /users/1234/likes/tracks", liked.clone(), false),
            ("POST /tracks", tracks.clone(), false),
            ("GET /users/1234/likes/tracks", liked, false),
            ("POST /tracks", tracks.clone(), false),
            ("GET /users/1234/playlists/3", playlist, false),
            ("POST /tracks", tracks.clone(), false),
            ("GET /search", search.clone(), false),
            ("POST /tracks", tracks.clone(), false),
            ("POST /tracks", tracks, false),
            ("GET /albums/7/with-tracks", album, false),
            ("GET /search", search, true),
        ];
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let served = Arc::new(AtomicBool::new(false));
        let server_done = served.clone();
        let late_started = Arc::new(AtomicBool::new(false));
        let server_late_started = late_started.clone();
        let (release_search, wait_for_release) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            let mut responses = responses.into_iter().peekable();
            while let Some((prefix, response, delayed)) = responses.peek() {
                let deadline = Instant::now() + Duration::from_secs(8);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("native fixture expected a loopback request: {error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    assert_eq!(stream.read(&mut byte).unwrap(), 1);
                    bytes.push(byte[0]);
                    assert!(bytes.len() < 32 * 1024);
                }
                let request = String::from_utf8(bytes).unwrap();
                let independent_membership = !request.starts_with(prefix)
                    && request.starts_with("GET /users/1234/likes/tracks");
                assert!(request.starts_with(prefix) || independent_membership);
                assert!(
                    !request
                        .to_ascii_lowercase()
                        .contains("x-yandex-music-client:")
                );
                assert!(request.contains("authorization: OAuth synthetic-ui-token"));
                let length = request
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|n| n.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                assert!(length < 32 * 1024);
                stream.read_exact(&mut vec![0; length]).unwrap();
                if *delayed && !independent_membership {
                    server_late_started.store(true, Ordering::Release);
                    wait_for_release
                        .recv_timeout(Duration::from_secs(8))
                        .unwrap();
                }
                let body = if independent_membership {
                    membership_body.clone()
                } else {
                    response.to_string()
                };
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                if !independent_membership {
                    responses.next();
                }
            }
            server_done.store(true, Ordering::Release);
        });
        let app = adw::Application::builder()
            .application_id("io.github.tracerxbrhd.Karin.NativeFixture")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(gio::Cancellable::NONE).unwrap();
        let blocked = Arc::new(AtomicBool::new(true));
        let ui = super::super::build_with_music_pane(
            &app,
            crate::application::ConnectionService::new(crate::api::YandexApi::new().unwrap()),
            |app, window| {
                MusicPane::with_provider(
                    app,
                    window,
                    Some(MusicService::for_test(&format!("http://{address}/"))),
                    Arc::new(FixtureProvider {
                        blocked: blocked.clone(),
                    }),
                    launch_fixture_callback,
                )
            },
        );
        let pane = &ui.music;
        let window = ui.window.clone();
        window.set_title(Some("Karin synthetic HTTP fixture"));
        window.set_default_size(900, 650);
        window.present();
        let this = &pane.controller;
        // Cancel before native readiness and immediately replace the logical
        // attempt. The old implementation presents two sheets synchronously,
        // exposing Libadwaita 1.5's shadowed-dialog weak-focus lifetime bug.
        this.open_login();
        let retiring = this.login_dialog.borrow().as_ref().unwrap().clone();
        retiring.cancel.emit_clicked();
        this.open_login();
        let pending = this.login_dialog.borrow().as_ref().unwrap().clone();
        assert!(retiring.dialog.parent().is_some());
        assert!(
            pending.dialog.parent().is_none(),
            "replacement login must wait for native unparenting"
        );
        assert_eq!(window.dialogs().n_items(), 1);
        assert!(this.model.borrow().attempt.is_none());
        assert!(this.model.borrow().auth_busy);
        let pending_generation = this.model.borrow().generation;
        retiring.cancel.emit_clicked();
        retiring.retry.emit_clicked();
        assert_eq!(this.model.borrow().generation, pending_generation);
        assert_eq!(
            this.login_dialog.borrow().as_ref().unwrap().dialog,
            pending.dialog,
            "retiring controls must not cancel or restart a pending login"
        );
        pump_until(|| {
            assert!(window.dialogs().n_items() <= 1);
            pending.dialog.parent().is_some()
                && pending.presentation_ready()
                && !this.model.borrow().busy
        });
        assert!(retiring.dialog.parent().is_none());
        let presented_generation = this.model.borrow().generation;
        retiring.cancel.emit_clicked();
        retiring.retry.emit_clicked();
        assert_eq!(this.model.borrow().generation, presented_generation);
        assert_eq!(
            this.login_dialog.borrow().as_ref().unwrap().dialog,
            pending.dialog,
            "late native close must not cancel the replacement attempt"
        );
        pending.cancel.emit_clicked();
        pump_until(|| window.dialogs().n_items() == 0 && pending.dialog.parent().is_none());
        drop(pending);
        drop(retiring);
        assert_eq!(this.widgets.login.label().as_deref(), Some("Войти"));
        this.widgets.login.emit_clicked();
        pump_until(|| !this.model.borrow().busy);
        let dialog = this.login_dialog.borrow().as_ref().unwrap().clone();
        assert_no_credential_fields(dialog.dialog.upcast_ref());
        assert!(contains_text(dialog.dialog.upcast_ref(), "не подтверждён"));
        assert!(!this.widgets.search.is_sensitive());
        assert!(this.model.borrow().token.is_none());
        capture_native("login-blocked");
        dialog.retry.emit_clicked();
        pump_until(|| !this.model.borrow().busy);
        dialog.cancel.emit_clicked();
        assert!(this.login_dialog.borrow().is_none());
        assert!(
            this.widgets
                .status
                .subtitle()
                .is_none_or(|value| value.is_empty())
        );
        blocked.store(false, Ordering::Release);
        this.widgets.login.emit_clicked();
        let accepted_dialog = this.login_dialog.borrow().as_ref().unwrap().dialog.clone();
        pump_until(|| this.model.borrow().account_valid && !this.model.borrow().busy);
        assert!(this.login_dialog.borrow().is_none());
        // Logical completion starts the native closing animation. Wait for
        // actual unparenting before asserting/capturing the exposed library.
        pump_until(|| {
            window.visible_dialog().is_none()
                && window.dialogs().n_items() == 0
                && accepted_dialog.parent().is_none()
                && dialog.dialog.parent().is_none()
        });
        drop(accepted_dialog);
        drop(dialog);
        assert_eq!(this.widgets.account.text(), "Synthetic UI account");
        assert_eq!(
            this.widgets.stack.visible_child_name().as_deref(),
            Some("home")
        );
        assert!(this.widgets.home_signedin.is_visible());
        assert!(!this.widgets.home_signedout.is_visible());
        assert_eq!(rows(&this.widgets.home_preview), 5);
        assert!(contains_text(
            this.widgets.home_preview.upcast_ref(),
            "Synthetic UI track"
        ));
        capture_native("home");
        gtk::prelude::WidgetExt::activate_action(&window, "win.diagnostics", None).unwrap();
        gtk::prelude::WidgetExt::activate_action(&window, "win.music-search", None).unwrap();
        pump_until(|| {
            gtk::prelude::GtkWindowExt::focus(&window).is_some_and(|focus| {
                focus.is::<gtk::SearchEntry>()
                    || focus.ancestor(gtk::SearchEntry::static_type()).is_some()
            })
        });
        assert_eq!(ui.stack.visible_child_name().as_deref(), Some("home"));
        this.widgets.home_search.emit_clicked();
        assert_eq!(
            this.widgets.stack.visible_child_name().as_deref(),
            Some("search")
        );
        this.widgets.home_liked.emit_clicked();
        assert_eq!(
            this.widgets.stack.visible_child_name().as_deref(),
            Some("library")
        );
        this.widgets.stack.set_visible_child_name("library");
        this.show_library();
        assert_eq!(rows(&this.widgets.library_rows), 50);
        assert!(this.widgets.library_next.is_visible());
        capture_native("library");
        this.widgets.library_next.emit_clicked();
        pump_until(|| !this.model.borrow().busy);
        assert_eq!(rows(&this.widgets.library_rows), 1);
        assert!(!this.widgets.library_next.is_visible());
        assert!(this.widgets.library_previous.is_visible());
        this.widgets.library_previous.emit_clicked();
        pump_until(|| !this.model.borrow().busy);
        assert_eq!(rows(&this.widgets.library_rows), 50);
        assert!(!this.widgets.library_previous.is_visible());
        this.widgets.library_mode.set_selected(1);
        pump_until(|| !this.model.borrow().busy);
        assert!(contains_text(
            this.playlist_grid.widget.upcast_ref(),
            "Synthetic playlist"
        ));
        let playlist_tile = this
            .playlist_grid
            .tiles
            .child_at_index(1)
            .and_then(|child| child.child())
            .and_downcast::<gtk::Button>()
            .expect("the owned grid has a Create tile followed by its playlist");
        assert_eq!(playlist_tile.widget_name(), "music-playlist-tile");
        playlist_tile.emit_clicked();
        pump_until(|| !this.model.borrow().busy);
        assert!(contains_text(
            this.widgets.detail.upcast_ref(),
            "Synthetic UI track"
        ));
        this.widgets.stack.set_visible_child_name("search");
        this.widgets.search.set_text("Synthetic");
        this.widgets.search.emit_by_name::<()>("activate", &[]);
        pump_until(|| !this.model.borrow().busy);
        assert!(contains_text(
            this.widgets.search_rows.upcast_ref(),
            "Synthetic UI track"
        ));
        capture_native("search");
        let row = this
            .widgets
            .search_rows
            .first_child()
            .unwrap()
            .downcast::<gtk::ListBoxRow>()
            .unwrap();
        let menu = row
            .child()
            .unwrap()
            .last_child()
            .unwrap()
            .downcast::<gtk::MenuButton>()
            .unwrap();
        menu.popup();
        pump_until(|| menu.popover().unwrap().is_visible());
        button_with_label(menu.popover().unwrap().upcast_ref(), "Добавить в очередь")
            .unwrap()
            .emit_clicked();
        assert_eq!(this.model.borrow().queue.items().len(), 1);
        assert_eq!(this.widgets.current.text(), "Synthetic UI track");
        this.widgets.stack.set_visible_child_name("queue");
        capture_native("queue");
        this.widgets.stack.set_visible_child_name("search");
        menu.popup();
        pump_until(|| menu.popover().unwrap().is_visible());
        button_with_label(
            menu.popover().unwrap().upcast_ref(),
            "Открыть сведения о треке",
        )
        .unwrap()
        .emit_clicked();
        pump_until(|| !this.model.borrow().busy);
        this.widgets
            .detail
            .last_child()
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        pump_until(|| !this.model.borrow().busy);
        let album_rows = this
            .widgets
            .detail
            .first_child()
            .unwrap()
            .next_sibling()
            .unwrap()
            .next_sibling()
            .unwrap()
            .next_sibling()
            .unwrap()
            .downcast::<gtk::ListBox>()
            .unwrap();
        assert_eq!(rows(&album_rows), 50);
        capture_native("album");
        this.widgets
            .detail
            .last_child()
            .unwrap()
            .last_child()
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        let album_rows = this
            .widgets
            .detail
            .first_child()
            .unwrap()
            .next_sibling()
            .unwrap()
            .next_sibling()
            .unwrap()
            .next_sibling()
            .unwrap()
            .downcast::<gtk::ListBox>()
            .unwrap();
        assert_eq!(rows(&album_rows), 13);
        this.widgets.stack.set_visible_child_name("search");
        this.widgets.search.set_text("Late result");
        this.widgets.search_button.emit_clicked();
        assert!(this.model.borrow().busy);
        assert!(this.widgets.search.is_sensitive());
        let previous_view = this.model.borrow().view_generation;
        this.widgets
            .search
            .set_text("Changed before the old response");
        assert!(this.model.borrow().view_generation > previous_view);
        // Exercise an actual in-flight response. A merely queued request is now
        // correctly suppressed by the shared token invalidation on logout.
        pump_until(|| late_started.load(Ordering::Acquire));
        this.logout();
        release_search.send(()).unwrap();
        pump_until(|| served.load(Ordering::Acquire));
        server.join().unwrap();
        let settle = Instant::now() + Duration::from_millis(100);
        pump_until(|| Instant::now() >= settle);
        assert!(this.model.borrow().token.is_none());
        assert!(!this.model.borrow().account_valid);
        assert!(this.model.borrow().queue.items().is_empty());
        assert!(!contains_text(
            pane.widget.upcast_ref(),
            "Synthetic UI track"
        ));
        assert!(!contains_text(
            pane.widget.upcast_ref(),
            "Synthetic UI account"
        ));
        assert!(this.widgets.home_signedout.is_visible());
        assert!(!this.widgets.home_signedin.is_visible());
        assert!(!this.widgets.status_revealer.reveals_child());
        let weak = Rc::downgrade(&pane.controller);
        pane.close();
        let closed_generation = this.model.borrow().generation;
        this.widgets
            .player_status
            .set_tooltip_text(Some("synthetic repeated-close marker"));
        pane.close();
        assert_eq!(this.model.borrow().generation, closed_generation);
        assert_eq!(
            this.widgets.player_status.tooltip_text().as_deref(),
            Some("synthetic repeated-close marker"),
            "a repeated close must not render disposed player widgets"
        );
        window.close();
        drop(ui);
        drop(window);
        pump_until(|| weak.upgrade().is_none());
        validation_failure_cancel_and_retry(&app);
    }
}
