// SPDX-License-Identifier: GPL-3.0-or-later
//! Native account/catalog presentation. Only typed application results reach GTK.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use adw::prelude::*;
use gtk::{gio, glib};

use super::auth::LoginDialog;
use crate::auth::AuthorizationCode;
#[cfg(feature = "auth-webkit")]
use crate::auth::SessionCookies;
use crate::{
    auth::{AuthAttempt, AuthError, AuthProvider, BrowserMode},
    music::{Album, Artist, MusicError, MusicService, Page, Playlist, SearchKind, Track},
    queue::MusicQueue,
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
    closed: bool,
    generation: u64,
    view_generation: u64,
    library_page: u32,
    search_page: u32,
    queue: MusicQueue,
    attempt: Option<Arc<AuthAttempt>>,
    liked: Option<Result<Page<Track>, MusicError>>,
    playlists: Option<Result<Page<Playlist>, MusicError>>,
}

#[derive(Clone)]
struct Widgets {
    account: gtk::Label,
    notice: gtk::Label,
    login: gtk::Button,
    logout: gtk::Button,
    spinner: gtk::Spinner,
    status: adw::ActionRow,
    stack: gtk::Stack,
    library_mode: gtk::DropDown,
    library_refresh: gtk::Button,
    library_rows: gtk::ListBox,
    library_previous: gtk::Button,
    library_next: gtk::Button,
    search: gtk::SearchEntry,
    search_kind: gtk::DropDown,
    search_button: gtk::Button,
    search_rows: gtk::ListBox,
    search_previous: gtk::Button,
    search_next: gtk::Button,
    detail: gtk::Box,
    queue_rows: gtk::ListBox,
    queue_count: gtk::Label,
    current: gtk::Label,
    previous: gtk::Button,
    next: gtk::Button,
    clear_queue: gtk::Button,
}

struct Controller {
    app: glib::WeakRef<adw::Application>,
    service: Option<MusicService>,
    provider: Arc<dyn AuthProvider>,
    launch_browser: fn(&str) -> Result<(), AuthError>,
    window: glib::WeakRef<adw::ApplicationWindow>,
    login_dialog: RefCell<Option<LoginDialog>>,
    model: RefCell<Model>,
    widgets: Widgets,
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
        Self::with_provider(
            app,
            window,
            service,
            crate::auth::public_provider(),
            launch_system_browser,
        )
    }

    fn with_provider(
        app: &adw::Application,
        window: &adw::ApplicationWindow,
        service: Option<MusicService>,
        provider: Arc<dyn AuthProvider>,
        launch_browser: fn(&str) -> Result<(), AuthError>,
    ) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 18);
        widget.set_widget_name("music-pane");
        let account_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
        account_card.add_css_class("card");
        let account_content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        super::margins(&account_content, 18);
        let account = text("Музыка вашего аккаунта", "title-2");
        account.set_widget_name("music-account");
        let notice = text(public_notice(), "dim-label");
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let login = gtk::Button::with_label("Войти");
        login.set_widget_name("music-login");
        login.add_css_class("suggested-action");
        let logout = gtk::Button::with_label("Выйти");
        logout.set_tooltip_text(Some("Закрыть локальный сеанс и удалить токен из памяти"));
        logout.set_sensitive(false);
        actions.append(&login);
        actions.append(&logout);
        account_content.append(&account);
        account_content.append(&notice);
        account_content.append(&actions);
        account_card.append(&account_content);
        widget.append(&account_card);

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
        widget.append(&progress);

        let stack = gtk::Stack::new();
        // Measure the visible page. GTK 4.14 otherwise measures hidden wrapped
        // pages at their minimum height, forcing their natural width on the
        // entire window even when the visible page fits a compact allocation.
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);
        stack.set_widget_name("music-sections");
        // Native linked toggles avoid StackSwitcher's equal-width tabs imposing
        // the longest label's minimum width on every compact navigation item.
        let switcher = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        switcher.add_css_class("linked");
        let mut first = None::<gtk::ToggleButton>;
        for (name, title) in [
            ("library", "Библиотека"),
            ("search", "Поиск"),
            ("queue", "Очередь"),
        ] {
            let button = gtk::ToggleButton::with_label(title);
            if let Some(first) = &first {
                button.set_group(Some(first));
            } else {
                first = Some(button.clone());
                button.set_active(true);
            }
            let weak = stack.downgrade();
            button.connect_toggled(move |button| {
                if button.is_active()
                    && let Some(stack) = weak.upgrade()
                {
                    stack.set_visible_child_name(name);
                }
            });
            let weak = button.downgrade();
            stack.connect_visible_child_name_notify(move |stack| {
                if let Some(button) = weak.upgrade() {
                    button.set_active(stack.visible_child_name().as_deref() == Some(name));
                }
            });
            switcher.append(&button);
        }
        switcher.set_halign(gtk::Align::Center);
        widget.append(&switcher);
        widget.append(&stack);

        let library = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let library_mode = gtk::DropDown::from_strings(&["Понравившиеся треки", "Мои плейлисты"]);
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

        let search_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Треки, альбомы и исполнители"));
        search.set_hexpand(true);
        search.set_widget_name("music-search");
        search.update_property(&[gtk::accessible::Property::Label("Поиск в Яндекс Музыке")]);
        search_page.append(&search);
        let search_controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let search_kind = gtk::DropDown::from_strings(&["Треки", "Альбомы", "Исполнители"]);
        search_kind.set_hexpand(true);
        search_kind.update_property(&[gtk::accessible::Property::Label("Тип результатов")]);
        let search_button = gtk::Button::with_label("Найти");
        search_button.set_widget_name("music-search-button");
        search_button.add_css_class("suggested-action");
        search_controls.append(&search_kind);
        search_controls.append(&search_button);
        search_page.append(&search_controls);
        let search_rows = list();
        set_empty(&search_rows, "Введите запрос и нажмите Enter или «Найти».");
        search_page.append(&search_rows);
        let search_pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let search_previous = gtk::Button::with_label("Назад");
        search_previous.set_tooltip_text(Some("Предыдущая страница поиска"));
        search_previous.set_visible(false);
        let search_next = gtk::Button::with_label("Далее");
        search_next.set_tooltip_text(Some("Следующая страница поиска"));
        search_next.set_visible(false);
        search_next.set_halign(gtk::Align::Start);
        search_pages.append(&search_previous);
        search_pages.append(&search_next);
        search_page.append(&search_pages);
        stack.add_titled(&search_page, Some("search"), "Поиск");

        let queue_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let queue_count = text("Очередь пуста", "heading");
        queue_page.append(&queue_count);
        queue_page.append(&text("Очередь хранит выбранные треки. Аудиопоток для Karin пока не подтверждён: выбор не запускает звук.", "dim-label"));
        let clear_queue = gtk::Button::with_label("Очистить очередь");
        clear_queue.set_halign(gtk::Align::Start);
        queue_page.append(&clear_queue);
        let queue_rows = list();
        queue_page.append(&queue_rows);
        stack.add_titled(&queue_page, Some("queue"), "Очередь");

        let detail = gtk::Box::new(gtk::Orientation::Vertical, 12);
        stack.add_named(&detail, Some("detail"));
        let player_bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        player_bar.set_widget_name("music-player-bar");
        super::margins(&player_bar, 12);
        let playing = gtk::Box::new(gtk::Orientation::Vertical, 2);
        playing.set_hexpand(true);
        let current = text("Трек не выбран", "heading");
        current.set_widget_name("music-current-title");
        current.set_ellipsize(gtk::pango::EllipsizeMode::End);
        current.set_lines(1);
        current.set_wrap(false);
        current.set_width_chars(1);
        playing.append(&current);
        playing.append(&text("Воспроизведение недоступно", "dim-label"));
        player_bar.append(&playing);
        let previous = icon_button(
            "media-skip-backward-symbolic",
            "Выбрать предыдущий трек в очереди, без воспроизведения",
        );
        let play = icon_button(
            "media-playback-start-symbolic",
            "Разрешённый аудиопоток для Karin не подтверждён",
        );
        play.set_sensitive(false);
        let next = icon_button(
            "media-skip-forward-symbolic",
            "Выбрать следующий трек в очереди, без воспроизведения",
        );
        player_bar.append(&previous);
        player_bar.append(&play);
        player_bar.append(&next);

        let widgets = Widgets {
            account,
            notice,
            login,
            logout,
            spinner,
            status,
            stack,
            library_mode,
            library_refresh,
            library_rows,
            library_previous,
            library_next,
            search,
            search_kind,
            search_button,
            search_rows,
            search_previous,
            search_next,
            detail,
            queue_rows,
            queue_count,
            current,
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
            model: RefCell::new(Model {
                token: None,
                account_valid: false,
                busy: false,
                closed: false,
                generation: 0,
                view_generation: 0,
                library_page: 0,
                search_page: 0,
                queue: MusicQueue::new(),
                attempt: None,
                liked: None,
                playlists: None,
            }),
            widgets,
        });
        controller.connect(window);
        controller.render_controls();
        controller.render_queue();
        Self {
            widget,
            player_bar,
            controller,
        }
    }

    pub fn primary_action(&self) -> gtk::Button {
        self.controller.widgets.login.clone()
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
        cancelled.cancel.emit_clicked();
        self.controller.open_login();
        let native_cancelled = self
            .controller
            .login_dialog
            .borrow()
            .as_ref()
            .cloned()
            .ok_or("Native-close login dialog was not created")?;
        // Escape, the header close button and the host's window-close handler
        // all call Dialog.close(). Before presentation it routes close-attempt
        // to immediate session cancellation and a deferred native close.
        if native_cancelled.dialog.close() {
            return Err("Native dialog close bypassed presentation readiness".into());
        }
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
                || native_cancelled.dialog.parent().is_some()
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
                && native_cancelled.dialog.parent().is_none()
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
            if let Some(window) = self.window.upgrade() {
                dialog.dialog.present(Some(&window));
            }
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
        dialog.cancel.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.cancel_login();
            }
        });
        let weak = Rc::downgrade(self);
        dialog.retry.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.start_login();
            }
        });
        *self.login_dialog.borrow_mut() = Some(dialog.clone());
        dialog.dialog.present(Some(&window));
        dialog.cancel.grab_focus();
        self.start_login();
    }

    fn dismiss_login(&self) {
        let dialog = self.login_dialog.borrow_mut().take();
        if let Some(dialog) = dialog {
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
        {
            let mut model = self.model.borrow_mut();
            model.generation += 1;
            model.view_generation += 1;
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
        if self.login_dialog.borrow().is_none() {
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
        let weak = Rc::downgrade(self);
        self.widgets.logout.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.logout();
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
                this.load_library(page);
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
                let mut model = this.model.borrow_mut();
                if model.account_valid {
                    model.view_generation += 1;
                }
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.search_kind.connect_selected_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.model.borrow_mut().view_generation += 1;
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
                }
            });
        let weak = Rc::downgrade(self);
        self.widgets.clear_queue.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.model.borrow_mut().queue.clear();
                this.render_queue();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.previous.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.model.borrow_mut().queue.previous_track();
                this.render_queue();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.next.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.model.borrow_mut().queue.next_track();
                this.render_queue();
            }
        });
        for (name, page, accelerator) in [
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
    }

    fn render_controls(&self) {
        let model = self.model.borrow();
        let ready = model.account_valid && !model.busy && !model.closed;
        self.widgets.library_mode.set_sensitive(ready);
        self.widgets.library_refresh.set_sensitive(ready);
        // The owner can edit a new query while the bounded old request finishes.
        // Its result is discarded via view_generation; submission stays bounded.
        self.widgets
            .search
            .set_sensitive(model.account_valid && !model.closed);
        self.widgets
            .search_kind
            .set_sensitive(model.account_valid && !model.closed);
        self.widgets.search_button.set_sensitive(ready);
        self.widgets.library_next.set_sensitive(ready);
        self.widgets.library_previous.set_sensitive(ready);
        self.widgets.search_next.set_sensitive(ready);
        self.widgets.search_previous.set_sensitive(ready);
        self.widgets
            .login
            .set_sensitive(!model.account_valid && !model.busy && !model.closed);
        self.widgets
            .login
            .set_tooltip_text(Some("Войти в Яндекс Музыку"));
        self.widgets.login.set_visible(!model.account_valid);
        self.widgets
            .logout
            .set_sensitive(model.token.is_some() && !model.closed);
        self.widgets.spinner.set_spinning(model.busy);
        self.widgets.spinner.set_visible(model.busy);
    }

    fn close(&self) {
        self.clear_text_focus();
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
        if let Some(service) = &self.service {
            service.clear_session();
        }
    }

    fn logout(self: &Rc<Self>) {
        self.clear_text_focus();
        {
            let mut model = self.model.borrow_mut();
            model.generation += 1;
            model.view_generation += 1;
            if let Some(token) = model.token.take() {
                token.invalidate();
            }
            model.account_valid = false;
            model.busy = false;
            model.queue.clear();
            model.liked = None;
            model.playlists = None;
            model.library_page = 0;
            model.search_page = 0;
            if let Some(attempt) = model.attempt.take() {
                attempt.cancel();
            }
        }
        self.dismiss_login();
        if let Some(service) = &self.service {
            service.clear_session();
        }
        self.widgets.account.set_text("Музыка вашего аккаунта");
        self.widgets.notice.set_text(
            "Сеанс закрыт, локальный токен удалён. Это не отзывает разрешение на сервере Яндекса.",
        );
        self.reset_status();
        self.widgets.search.set_text("");
        set_empty(
            &self.widgets.library_rows,
            "Ваша библиотека появится после входа.",
        );
        set_empty(&self.widgets.search_rows, "Введите запрос после входа.");
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

    fn run<R: Send + 'static>(
        self: &Rc<Self>,
        work: impl FnOnce(MusicService, AccessToken) -> Result<R, MusicError> + Send + 'static,
        done: impl FnOnce(&Rc<Self>, Result<R, MusicError>) + 'static,
    ) {
        self.run_with_view(true, work, done);
    }

    fn run_with_view<R: Send + 'static>(
        self: &Rc<Self>,
        observe_view: bool,
        work: impl FnOnce(MusicService, AccessToken) -> Result<R, MusicError> + Send + 'static,
        done: impl FnOnce(&Rc<Self>, Result<R, MusicError>) + 'static,
    ) {
        let (Some(app), Some(service)) = (self.app.upgrade(), self.service.clone()) else {
            self.widgets.status.set_title("Сетевой клиент недоступен.");
            return;
        };
        let (token, generation, view_generation, view_matters) = {
            let mut model = self.model.borrow_mut();
            if model.closed || model.busy {
                return;
            }
            let Some(token) = model.token.clone() else {
                self.widgets.status.set_title("Сначала откройте сеанс.");
                return;
            };
            if token.is_expired() {
                drop(model);
                self.logout();
                self.widgets
                    .status
                    .set_title("Срок действия сеанса истёк. Войдите снова.");
                return;
            }
            model.busy = true;
            (
                token,
                model.generation,
                model.view_generation,
                model.account_valid && observe_view,
            )
        };
        self.widgets.status.set_title("Загрузка…");
        self.widgets.status.set_subtitle("");
        self.render_controls();
        let hold = app.hold();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let _hold = hold;
            let result = gio::spawn_blocking(move || work(service, token)).await;
            let Some(this) = weak.upgrade() else {
                return;
            };
            let current = {
                let mut model = this.model.borrow_mut();
                if model.generation == generation {
                    model.busy = false;
                }
                !model.closed
                    && model.generation == generation
                    && (!view_matters || model.view_generation == view_generation)
            };
            this.render_controls();
            if !current {
                let model = this.model.borrow();
                if !model.closed && model.generation == generation {
                    if matches!(
                        &result,
                        Ok(Err(MusicError::AccessDenied | MusicError::Unauthenticated))
                    ) {
                        drop(model);
                        this.show_error(MusicError::AccessDenied);
                        return;
                    }
                    this.widgets.status.set_title(
                        "Запрос завершён; раздел изменён. При необходимости обновите данные.",
                    );
                }
                return;
            }
            match result {
                Ok(result) => done(&this, result),
                Err(_) => {
                    if this.login_dialog.borrow().is_some() && !this.model.borrow().account_valid {
                        this.auth_failed(AuthError::Configuration);
                    } else {
                        this.widgets
                            .status
                            .set_title("Не удалось завершить операцию.");
                    }
                }
            }
        });
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
        match result {
            Ok(result) => {
                self.model.borrow_mut().library_page = page;
                clear_list(&self.widgets.library_rows);
                for playlist in result.items {
                    self.playlist_row(&self.widgets.library_rows, playlist);
                }
                self.widgets.library_next.set_visible(result.has_more);
                self.widgets.library_previous.set_visible(page > 0);
                if self.widgets.library_rows.first_child().is_none() {
                    set_empty(&self.widgets.library_rows, "Плейлистов пока нет.");
                }
                self.widgets
                    .status
                    .set_title(&format!("Мои плейлисты · страница {}", page + 1));
                self.widgets.status.set_subtitle("");
            }
            Err(error) => {
                set_empty(
                    &self.widgets.library_rows,
                    "Не удалось загрузить плейлисты. Повторите обновление.",
                );
                self.show_error(error);
            }
        }
    }

    fn render_liked(self: &Rc<Self>, result: Result<Page<Track>, MusicError>, page: u32) {
        match result {
            Ok(result) => {
                self.model.borrow_mut().library_page = page;
                self.tracks(&self.widgets.library_rows, result.items);
                self.widgets.library_next.set_visible(result.has_more);
                self.widgets.library_previous.set_visible(page > 0);
                self.widgets
                    .status
                    .set_title(&format!("Понравившиеся треки · страница {}", page + 1));
                self.widgets.status.set_subtitle("");
            }
            Err(error) => {
                set_empty(
                    &self.widgets.library_rows,
                    "Не удалось загрузить треки. Повторите обновление.",
                );
                self.show_error(error);
            }
        }
    }

    fn load_initial_library(self: &Rc<Self>) {
        self.run_with_view(
            false,
            |service, token| {
                // A failure in one section must not suppress the other request.
                let liked = service.liked_tracks(&token, 0);
                let playlists = service.playlists(&token, 0);
                Ok((liked, playlists))
            },
            |this, result| match result {
                Ok((liked, playlists)) => {
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
                    model.liked = Some(liked);
                    model.playlists = Some(playlists);
                    drop(model);
                    this.show_library();
                }
                Err(error) => this.show_error(error),
            },
        );
    }

    fn load_library(self: &Rc<Self>, page: u32) {
        {
            let model = self.model.borrow();
            if model.busy || !model.account_valid || model.closed {
                return;
            }
        }
        set_empty(&self.widgets.library_rows, "Загрузка библиотеки…");
        self.widgets.library_next.set_visible(false);
        self.widgets.library_previous.set_visible(false);
        let playlists = self.widgets.library_mode.selected() == 1;
        if playlists {
            self.run(
                move |service, token| service.playlists(&token, page),
                move |this, result| {
                    if page == 0 {
                        this.model.borrow_mut().playlists = Some(result.clone());
                    }
                    this.render_playlists(result, page);
                },
            );
        } else {
            self.run(
                move |service, token| service.liked_tracks(&token, page),
                move |this, result| {
                    if page == 0 {
                        this.model.borrow_mut().liked = Some(result.clone());
                    }
                    this.render_liked(result, page);
                },
            );
        }
    }

    fn search(self: &Rc<Self>, page: u32) {
        {
            let model = self.model.borrow();
            if model.busy || !model.account_valid || model.closed {
                return;
            }
        }
        let query = self.widgets.search.text().trim().to_owned();
        if query.is_empty() {
            self.widgets.status.set_title("Введите поисковый запрос.");
            return;
        }
        set_empty(&self.widgets.search_rows, "Поиск…");
        self.widgets.search_next.set_visible(false);
        self.widgets.search_previous.set_visible(false);
        let kind = match self.widgets.search_kind.selected() {
            1 => SearchKind::Albums,
            2 => SearchKind::Artists,
            _ => SearchKind::Tracks,
        };
        self.run(
            move |service, token| service.search(&token, &query, kind, page),
            move |this, result| match result {
                Ok(result) => {
                    this.model.borrow_mut().search_page = page;
                    clear_list(&this.widgets.search_rows);
                    for track in result.tracks {
                        this.track_row(&this.widgets.search_rows, track);
                    }
                    for album in result.albums {
                        this.album_row(&this.widgets.search_rows, album);
                    }
                    for artist in result.artists {
                        this.artist_row(&this.widgets.search_rows, artist);
                    }
                    if this.widgets.search_rows.first_child().is_none() {
                        set_empty(
                            &this.widgets.search_rows,
                            "Ничего не найдено. Попробуйте другой запрос.",
                        );
                    }
                    this.widgets.search_next.set_visible(result.has_more);
                    this.widgets.search_previous.set_visible(page > 0);
                    this.widgets
                        .status
                        .set_title(&format!("Результаты поиска · страница {}", page + 1));
                }
                Err(error) => {
                    set_empty(
                        &this.widgets.search_rows,
                        "Поиск не выполнен. Повторите запрос.",
                    );
                    this.show_error(error);
                }
            },
        );
    }

    fn tracks(self: &Rc<Self>, list: &gtk::ListBox, tracks: Vec<Track>) {
        clear_list(list);
        for track in tracks {
            self.track_row(list, track);
        }
        if list.first_child().is_none() {
            set_empty(list, "Треков пока нет.");
        }
    }

    fn track_row(self: &Rc<Self>, list: &gtk::ListBox, track: Track) {
        let (row, body) = item(&track.title, &track.artists.join(", "));
        let add = icon_button("list-add-symbolic", "Добавить трек в очередь");
        let weak = Rc::downgrade(self);
        let queued = track.clone();
        add.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.enqueue(queued.clone());
            }
        });
        body.append(&add);
        let weak = Rc::downgrade(self);
        row.set_activatable(true);
        row.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.open_track(track.id.clone());
            }
        });
        list.append(&row);
    }

    fn playlist_row(self: &Rc<Self>, list: &gtk::ListBox, playlist: Playlist) {
        let (row, _) = item(&playlist.title, &format!("{} треков", playlist.track_count));
        row.set_activatable(true);
        let weak = Rc::downgrade(self);
        row.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.open_playlist(playlist.clone(), 0);
            }
        });
        list.append(&row);
    }

    fn album_row(self: &Rc<Self>, list: &gtk::ListBox, album: Album) {
        let (row, _) = item(&album.title, &album.artists.join(", "));
        row.set_activatable(true);
        let weak = Rc::downgrade(self);
        row.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.open_album(album.id.clone());
            }
        });
        list.append(&row);
    }

    fn artist_row(self: &Rc<Self>, list: &gtk::ListBox, artist: Artist) {
        let (row, _) = item(&artist.name, "Найти треки исполнителя");
        row.set_activatable(true);
        let weak = Rc::downgrade(self);
        row.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.widgets.search.set_text(&artist.name);
                this.widgets.search_kind.set_selected(0);
                this.search(0);
            }
        });
        list.append(&row);
    }

    fn detail_header(self: &Rc<Self>, title: &str, subtitle: &str) {
        clear_box(&self.widgets.detail);
        let back = gtk::Button::with_label("Назад в библиотеку");
        back.set_halign(gtk::Align::Start);
        let weak = Rc::downgrade(self);
        back.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.widgets.stack.set_visible_child_name("library");
            }
        });
        self.widgets.detail.append(&back);
        self.widgets.detail.append(&text(title, "title-2"));
        self.widgets.detail.append(&text(subtitle, "dim-label"));
        self.widgets.stack.set_visible_child_name("detail");
    }

    fn open_track(self: &Rc<Self>, id: String) {
        self.run(
            move |service, token| service.track(&token, &id),
            move |this, result| match result {
                Ok(track) => {
                    this.detail_header(&track.title, &track.artists.join(", "));
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
                    this.widgets
                        .status
                        .set_title("Сведения о треке получены. Доступ к аудио не подтверждён.");
                }
                Err(error) => this.show_error(error),
            },
        );
    }

    fn open_album(self: &Rc<Self>, id: String) {
        self.run(
            move |service, token| service.album(&token, &id),
            move |this, result| match result {
                Ok(album) => {
                    this.render_album(Rc::new(album), 0);
                    this.widgets.status.set_title("Альбом получен.");
                }
                Err(error) => this.show_error(error),
            },
        );
    }

    fn render_album(self: &Rc<Self>, album: Rc<Album>, page: usize) {
        const ROWS: usize = 50;
        self.detail_header(&album.title, &album.artists.join(", "));
        let rows = list();
        let offset = page.saturating_mul(ROWS);
        self.tracks(
            &rows,
            album
                .tracks
                .iter()
                .skip(offset)
                .take(ROWS)
                .cloned()
                .collect(),
        );
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
        let id = playlist.id.clone();
        self.run(
            move |service, token| service.playlist_tracks(&token, &id, page),
            move |this, result| match result {
                Ok(result) => {
                    this.detail_header(
                        &playlist.title,
                        &format!("Плейлист · страница {}", page + 1),
                    );
                    let rows = list();
                    this.tracks(&rows, result.items);
                    this.widgets.detail.append(&rows);
                    if result.has_more {
                        let next = gtk::Button::with_label("Следующая страница");
                        next.set_halign(gtk::Align::Start);
                        let weak = Rc::downgrade(this);
                        next.connect_clicked(move |_| {
                            if let Some(this) = weak.upgrade() {
                                this.open_playlist(playlist.clone(), page.saturating_add(1));
                            }
                        });
                        this.widgets.detail.append(&next);
                    }
                    this.widgets.status.set_title("Плейлист получен.");
                }
                Err(error) => this.show_error(error),
            },
        );
    }

    fn enqueue(self: &Rc<Self>, track: Track) {
        let result = self.model.borrow_mut().queue.append(track);
        match result {
            Ok(_) => self.widgets.status.set_title(
                "Трек добавлен в очередь. Звук не запускается: источник не подтверждён.",
            ),
            Err(_) => self
                .widgets
                .status
                .set_title("Очередь заполнена. Удалите ненужные треки."),
        }
        self.render_queue();
    }

    fn render_queue(self: &Rc<Self>) {
        clear_list(&self.widgets.queue_rows);
        let model = self.model.borrow();
        self.widgets
            .queue_count
            .set_text(&format!("Треков в очереди: {}", model.queue.items().len()));
        let current = model
            .queue
            .current()
            .map(|track| track.title.as_str())
            .unwrap_or("Трек не выбран");
        self.widgets.current.set_text(current);
        self.widgets.current.set_tooltip_text(Some(current));
        self.widgets
            .current
            .update_property(&[gtk::accessible::Property::Label(current)]);
        self.widgets
            .previous
            .set_sensitive(model.queue.current_index().is_some_and(|i| i > 0));
        self.widgets.next.set_sensitive(
            model
                .queue
                .current_index()
                .is_some_and(|i| i + 1 < model.queue.items().len()),
        );
        self.widgets
            .clear_queue
            .set_sensitive(!model.queue.items().is_empty());
        for (index, track) in model.queue.items().iter().enumerate() {
            let (row, body) = item(&track.title, &track.artists.join(", "));
            let remove = icon_button("list-remove-symbolic", "Удалить трек из очереди");
            let weak = Rc::downgrade(self);
            remove.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.model.borrow_mut().queue.remove(index);
                    this.render_queue();
                }
            });
            body.append(&remove);
            row.set_activatable(true);
            let weak = Rc::downgrade(self);
            row.connect_activate(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.model.borrow_mut().queue.select(index);
                    this.render_queue();
                }
            });
            self.widgets.queue_rows.append(&row);
        }
        if model.queue.items().is_empty() {
            set_empty(
                &self.widgets.queue_rows,
                "Добавляйте треки из библиотеки, поиска или альбома.",
            );
        }
    }
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
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    super::margins(&body, 12);
    let labels = gtk::Box::new(gtk::Orientation::Vertical, 4);
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
            model.busy = false;
        }
        if self.service.is_none() {
            self.auth_failed(AuthError::Configuration);
            return;
        }
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            dialog.loading("Проверка доступа к Яндекс Музыке…");
        }
        self.run(|service, token| service.account(&token), |this, result| match result {
            Ok(account) => {
                {
                    let mut model = this.model.borrow_mut();
                    model.generation += 1;
                    model.account_valid = true;
                    if let Some(attempt) = model.attempt.take() { attempt.cancel(); }
                }
                this.widgets.account.set_text(&account.display_name);
                this.widgets.notice.set_text("Аккаунт Яндекс Музыки проверен. Доступ к библиотеке и поиску проверяется отдельно; токен хранится только в памяти.");
                this.dismiss_login();
                this.render_controls();
                this.load_initial_library();
            }
            Err(error) => {
                let (title, message) = music_error_message(error);
                {
                    let mut model = this.model.borrow_mut();
                    model.generation += 1;
                    if let Some(token) = model.token.take() { token.invalidate(); }
                    if let Some(attempt) = model.attempt.take() { attempt.cancel(); }
                    model.account_valid = false;
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

fn music_error_message(error: MusicError) -> (&'static str, &'static str) {
    match error {
        MusicError::Forbidden | MusicError::MissingScopes => (
            "Доступ Karin к Музыке не разрешён",
            "Яндекс Музыка не приняла разрешения приложения. Это не означает, что вы ввели неверный пароль Яндекса.",
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

    fn pump_until(mut condition: impl FnMut() -> bool) {
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
    fn capture_native(name: &str) {
        let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") else {
            return;
        };
        let directory = std::path::PathBuf::from(directory);
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

    fn contains_text(widget: &gtk::Widget, wanted: &str) -> bool {
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

    fn rows(list: &gtk::ListBox) -> usize {
        let mut count = 0;
        let mut child = list.first_child();
        while let Some(widget) = child {
            count += 1;
            child = widget.next_sibling();
        }
        count
    }

    fn activate_first(list: &gtk::ListBox) {
        list.first_child()
            .unwrap()
            .downcast::<gtk::ListBoxRow>()
            .unwrap()
            .emit_by_name::<()>("activate", &[]);
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
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Karin synthetic HTTP fixture")
            .default_width(900)
            .default_height(650)
            .build();
        let blocked = Arc::new(AtomicBool::new(false));
        let pane = MusicPane::with_provider(
            app,
            &window,
            Some(MusicService::for_test(&format!("http://{address}/"))),
            Arc::new(FixtureProvider {
                blocked: blocked.clone(),
            }),
            launch_fixture_callback,
        );
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&gtk::Label::new(Some(
            "Синтетические HTTP-фикстуры · проверка отказа",
        ))));
        toolbar.add_top_bar(&header);
        toolbar.add_bottom_bar(&pane.player_bar);
        toolbar.set_content(Some(&super::super::scroll_page(&pane.widget)));
        window.set_content(Some(&toolbar));
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
        assert!(contains_text(dialog.dialog.upcast_ref(), "не разрешён"));
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
        assert!(contains_text(pane.widget.upcast_ref(), "не разрешён"));
        this.widgets.library_mode.set_selected(1);
        assert!(contains_text(
            this.widgets.library_rows.upcast_ref(),
            "Synthetic independent playlist"
        ));
        this.logout();
        assert!(
            this.widgets
                .status
                .subtitle()
                .is_none_or(|value| value.is_empty())
        );
        assert!(!contains_text(pane.widget.upcast_ref(), "не разрешён"));
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
        drop(pane);
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
        let liked = json!({"result":{"library":{"tracks":vec![json!({"id":42,"albumId":7});51]}}});
        let tracks = json!({"result":[track.clone()]});
        let playlists = json!({"result":[{"kind":3,"title":"Synthetic playlist","trackCount":1,"owner":{"uid":1234}}]});
        let playlist = json!({"result":{"kind":3,"title":"Synthetic playlist","trackCount":1,"tracks":[{"id":42,"albumId":7}]}});
        let search = json!({"result":{"text":"Synthetic","tracks":{"total":1,"perPage":20,"page":0,"results":[track.clone()]}}});
        let album = json!({"result":{"id":7,"title":"Synthetic album","artists":[{"id":11,"name":"Synthetic artist"}],"volumes":[vec![track;63]]}});
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
            for (prefix, response, delayed) in responses {
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
                assert!(request.starts_with(prefix));
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
                if delayed {
                    server_late_started.store(true, Ordering::Release);
                    wait_for_release
                        .recv_timeout(Duration::from_secs(8))
                        .unwrap();
                }
                let body = response.to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            server_done.store(true, Ordering::Release);
        });
        let app = adw::Application::builder()
            .application_id("io.github.tracerxbrhd.Karin.NativeFixture")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(gio::Cancellable::NONE).unwrap();
        let window = adw::ApplicationWindow::builder()
            .application(&app)
            .title("Karin synthetic HTTP fixture")
            .default_width(900)
            .default_height(650)
            .build();
        let blocked = Arc::new(AtomicBool::new(true));
        let pane = MusicPane::with_provider(
            &app,
            &window,
            Some(MusicService::for_test(&format!("http://{address}/"))),
            Arc::new(FixtureProvider {
                blocked: blocked.clone(),
            }),
            launch_fixture_callback,
        );
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&gtk::Label::new(Some(
            "Синтетические HTTP-фикстуры · проверка UI",
        ))));
        toolbar.add_top_bar(&header);
        toolbar.add_bottom_bar(&pane.player_bar);
        toolbar.set_content(Some(&super::super::scroll_page(&pane.widget)));
        window.set_content(Some(&toolbar));
        window.present();
        let this = &pane.controller;
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
            this.widgets.library_rows.upcast_ref(),
            "Synthetic playlist"
        ));
        activate_first(&this.widgets.library_rows);
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
        row.child()
            .unwrap()
            .last_child()
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        assert_eq!(this.model.borrow().queue.items().len(), 1);
        assert_eq!(this.widgets.current.text(), "Synthetic UI track");
        this.widgets.stack.set_visible_child_name("queue");
        capture_native("queue");
        this.widgets.stack.set_visible_child_name("search");
        activate_first(&this.widgets.search_rows);
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
        let weak = Rc::downgrade(&pane.controller);
        pane.close();
        window.close();
        drop(pane);
        drop(window);
        pump_until(|| weak.upgrade().is_none());
        validation_failure_cancel_and_retry(&app);
    }
}
