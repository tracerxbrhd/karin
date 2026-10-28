// SPDX-License-Identifier: GPL-3.0-or-later
//! Native account/catalog presentation. Only typed application results reach GTK.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::{gio, glib};

use crate::{
    music::{Album, Artist, MusicError, MusicService, Playlist, SearchKind, Track},
    queue::MusicQueue,
    session::AccessToken,
};

#[cfg(feature = "experimental-device-auth")]
use crate::session::{DeviceAuthClient, DeviceChallenge, DevicePoll};

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
    #[cfg(feature = "experimental-device-auth")]
    challenge: Option<DeviceChallenge>,
}

#[derive(Clone)]
struct Widgets {
    account: gtk::Label,
    notice: gtk::Label,
    login: gtk::Button,
    logout: gtk::Button,
    spinner: gtk::Spinner,
    status: gtk::Label,
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
    #[cfg(feature = "experimental-device-auth")]
    auth: AuthWidgets,
}

#[cfg(feature = "experimental-device-auth")]
#[derive(Clone)]
struct AuthWidgets {
    revealer: gtk::Revealer,
    configuration: gtk::Box,
    client_id: gtk::Entry,
    client_secret: gtk::PasswordEntry,
    begin: gtk::Button,
    progress: gtk::Box,
    code: gtk::Label,
    remaining: gtk::Label,
    open: gtk::Button,
    copy: gtk::Button,
    cancel: gtk::Button,
}

struct Controller {
    app: glib::WeakRef<adw::Application>,
    service: Option<MusicService>,
    experimental: bool,
    model: RefCell<Model>,
    widgets: Widgets,
}

impl MusicPane {
    pub fn new(
        app: &adw::Application,
        window: &adw::ApplicationWindow,
        experimental: bool,
    ) -> Self {
        Self::with_service(app, window, experimental, MusicService::new().ok())
    }

    fn with_service(
        app: &adw::Application,
        window: &adw::ApplicationWindow,
        experimental: bool,
        service: Option<MusicService>,
    ) -> Self {
        let experimental = experimental && cfg!(feature = "experimental-device-auth");
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
        let login = gtk::Button::with_label(if experimental {
            "Войти"
        } else {
            "О входе"
        });
        login.set_widget_name("music-login");
        if experimental {
            login.add_css_class("suggested-action");
            notice.set_text("Локальный эксперимент. Токен остаётся в памяти до выхода; использование OAuth-приложения и доступ к Music проверяются отдельно.");
        }
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

        #[cfg(feature = "experimental-device-auth")]
        let auth = {
            let auth = auth_widgets();
            widget.append(&auth.revealer);
            auth
        };

        let progress = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let spinner = gtk::Spinner::new();
        let status = text(
            "Откройте сеанс для доступа к библиотеке и поиску.",
            "dim-label",
        );
        status.set_widget_name("music-status");
        progress.append(&spinner);
        progress.append(&status);
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
            #[cfg(feature = "experimental-device-auth")]
            auth,
        };
        let controller = Rc::new(Controller {
            app: app.downgrade(),
            service,
            experimental,
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
                #[cfg(feature = "experimental-device-auth")]
                challenge: None,
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
                #[cfg(feature = "experimental-device-auth")]
                if this.experimental {
                    this.widgets.auth.revealer.set_reveal_child(true);
                    this.widgets.auth.configuration.set_visible(true);
                    this.widgets.auth.progress.set_visible(false);
                    this.widgets.auth.client_id.grab_focus();
                    return;
                }
                this.widgets.notice.set_text(public_notice());
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
                this.load_library(0);
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
        #[cfg(feature = "experimental-device-auth")]
        self.connect_auth();
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
        #[cfg(feature = "experimental-device-auth")]
        let auth_pending = model.challenge.is_some();
        #[cfg(not(feature = "experimental-device-auth"))]
        let auth_pending = false;
        self.widgets
            .login
            .set_sensitive(!model.account_valid && !model.busy && !model.closed && !auth_pending);
        self.widgets
            .login
            .set_tooltip_text(Some(if self.experimental {
                "Экспериментальный локальный вход"
            } else {
                "Почему публичный вход пока недоступен"
            }));
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
        #[cfg(feature = "experimental-device-auth")]
        if let Some(challenge) = model.challenge.take() {
            challenge.cancel();
        }
        if let Some(service) = &self.service {
            service.clear_session();
        }
        #[cfg(feature = "experimental-device-auth")]
        {
            self.widgets.auth.client_id.set_text("");
            self.widgets.auth.client_secret.set_text("");
            self.widgets.auth.code.set_text("");
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
            #[cfg(feature = "experimental-device-auth")]
            if let Some(challenge) = model.challenge.take() {
                challenge.cancel();
            }
        }
        if let Some(service) = &self.service {
            service.clear_session();
        }
        self.widgets.account.set_text("Музыка вашего аккаунта");
        self.widgets.notice.set_text(
            "Сеанс закрыт, локальный токен удалён. Это не отзывает разрешение на сервере Яндекса.",
        );
        self.widgets
            .status
            .set_text("Откройте сеанс для доступа к библиотеке и поиску.");
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
        #[cfg(feature = "experimental-device-auth")]
        {
            self.widgets.auth.revealer.set_reveal_child(false);
            self.widgets.auth.client_id.set_text("");
            self.widgets.auth.client_secret.set_text("");
            self.widgets.auth.code.set_text("");
        }
        self.render_controls();
        self.render_queue();
    }

    fn run<R: Send + 'static>(
        self: &Rc<Self>,
        work: impl FnOnce(MusicService, AccessToken) -> Result<R, MusicError> + Send + 'static,
        done: impl FnOnce(&Rc<Self>, Result<R, MusicError>) + 'static,
    ) {
        let (Some(app), Some(service)) = (self.app.upgrade(), self.service.clone()) else {
            self.widgets.status.set_text("Сетевой клиент недоступен.");
            return;
        };
        let (token, generation, view_generation, view_matters) = {
            let mut model = self.model.borrow_mut();
            if model.closed || model.busy {
                return;
            }
            let Some(token) = model.token.clone() else {
                self.widgets.status.set_text("Сначала откройте сеанс.");
                return;
            };
            if token.is_expired() {
                drop(model);
                self.logout();
                self.widgets
                    .status
                    .set_text("Срок действия сеанса истёк. Войдите снова.");
                return;
            }
            model.busy = true;
            (
                token,
                model.generation,
                model.view_generation,
                model.account_valid,
            )
        };
        self.widgets.status.set_text("Загрузка…");
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
                    this.widgets.status.set_text(
                        "Запрос завершён; раздел изменён. При необходимости обновите данные.",
                    );
                }
                return;
            }
            match result {
                Ok(result) => done(&this, result),
                Err(_) => this
                    .widgets
                    .status
                    .set_text("Не удалось завершить операцию."),
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
        let status = error
            .http_status()
            .map(|s| format!(" · HTTP {s}"))
            .unwrap_or_default();
        self.widgets.status.set_text(&format!("{error}{status}"));
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
                move |this, result| match result {
                    Ok(result) => {
                        this.model.borrow_mut().library_page = page;
                        clear_list(&this.widgets.library_rows);
                        for playlist in result.items {
                            this.playlist_row(&this.widgets.library_rows, playlist);
                        }
                        this.widgets.library_next.set_visible(result.has_more);
                        this.widgets.library_previous.set_visible(page > 0);
                        if this.widgets.library_rows.first_child().is_none() {
                            set_empty(&this.widgets.library_rows, "Плейлистов пока нет.");
                        }
                        this.widgets
                            .status
                            .set_text(&format!("Мои плейлисты · страница {}", page + 1));
                    }
                    Err(error) => {
                        set_empty(
                            &this.widgets.library_rows,
                            "Не удалось загрузить плейлисты. Повторите обновление.",
                        );
                        this.show_error(error);
                    }
                },
            );
        } else {
            self.run(
                move |service, token| service.liked_tracks(&token, page),
                move |this, result| match result {
                    Ok(result) => {
                        this.model.borrow_mut().library_page = page;
                        this.tracks(&this.widgets.library_rows, result.items);
                        this.widgets.library_next.set_visible(result.has_more);
                        this.widgets.library_previous.set_visible(page > 0);
                        this.widgets
                            .status
                            .set_text(&format!("Понравившиеся треки · страница {}", page + 1));
                    }
                    Err(error) => {
                        set_empty(
                            &this.widgets.library_rows,
                            "Не удалось загрузить треки. Повторите обновление.",
                        );
                        this.show_error(error);
                    }
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
            self.widgets.status.set_text("Введите поисковый запрос.");
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
                        .set_text(&format!("Результаты поиска · страница {}", page + 1));
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
                        .set_text("Сведения о треке получены. Доступ к аудио не подтверждён.");
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
                    this.widgets.status.set_text("Альбом получен.");
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
                    this.widgets.status.set_text("Плейлист получен.");
                }
                Err(error) => this.show_error(error),
            },
        );
    }

    fn enqueue(self: &Rc<Self>, track: Track) {
        let result = self.model.borrow_mut().queue.append(track);
        match result {
            Ok(_) => self
                .widgets
                .status
                .set_text("Трек добавлен в очередь. Звук не запускается: источник не подтверждён."),
            Err(_) => self
                .widgets
                .status
                .set_text("Очередь заполнена. Удалите ненужные треки."),
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
    "Вход в этой версии пока недоступен: разрешённый доступ к Яндекс Музыке ещё проверяется. Экспериментальный сеанс доступен в отдельной локальной сборке для разработки."
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

#[cfg(feature = "experimental-device-auth")]
fn auth_widgets() -> AuthWidgets {
    let revealer = gtk::Revealer::new();
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("card");
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    super::margins(&content, 18);
    content.append(&text("Экспериментальный Device Flow", "heading"));
    content.append(&text("Только локальная разработка. Укажите данные OAuth-приложения, которое вы вправе использовать. Это не пароль аккаунта и не OAuth-токен. Karin не содержит предустановленной чужой идентичности; разрешение на публичное распространение этого способа не подтверждено.", "dim-label"));
    let configuration = gtk::Box::new(gtk::Orientation::Vertical, 8);
    configuration.append(&text("Client ID приложения", ""));
    let client_id = gtk::Entry::new();
    client_id.set_max_length(128);
    client_id.update_property(&[gtk::accessible::Property::Label(
        "Client ID OAuth-приложения для локального теста",
    )]);
    configuration.append(&client_id);
    configuration.append(&text("Client secret приложения", ""));
    let client_secret = gtk::PasswordEntry::new();
    client_secret.set_show_peek_icon(false);
    client_secret.update_property(&[gtk::accessible::Property::Label(
        "Client secret OAuth-приложения, не пароль аккаунта",
    )]);
    configuration.append(&client_secret);
    let begin = gtk::Button::with_label("Начать авторизацию");
    begin.add_css_class("suggested-action");
    begin.set_halign(gtk::Align::Start);
    configuration.append(&begin);
    content.append(&configuration);
    let progress = gtk::Box::new(gtk::Orientation::Vertical, 12);
    progress.set_visible(false);
    progress.append(&text(
        "Откройте страницу Яндекса в системном браузере и введите этот одноразовый код:",
        "",
    ));
    let code = text("", "title-1");
    code.set_halign(gtk::Align::Center);
    progress.append(&code);
    let remaining = text("", "dim-label");
    progress.append(&remaining);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let open = gtk::Button::with_label("Браузер");
    let copy = gtk::Button::with_label("Копировать код");
    actions.append(&open);
    actions.append(&copy);
    progress.append(&actions);
    content.append(&progress);
    let cancel = gtk::Button::with_label("Отменить вход");
    cancel.set_halign(gtk::Align::Start);
    content.append(&cancel);
    card.append(&content);
    revealer.set_child(Some(&card));
    AuthWidgets {
        revealer,
        configuration,
        client_id,
        client_secret,
        begin,
        progress,
        code,
        remaining,
        open,
        copy,
        cancel,
    }
}

#[cfg(feature = "experimental-device-auth")]
impl Controller {
    fn connect_auth(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.widgets.auth.begin.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.begin_auth();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.auth.cancel.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.cancel_auth();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.auth.open.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let model = this.model.borrow();
                if let Some(challenge) = &model.challenge
                    && gio::AppInfo::launch_default_for_uri(
                        challenge.verification_url(),
                        None::<&gio::AppLaunchContext>,
                    )
                    .is_err()
                {
                    this.widgets.status.set_text(
                        "Не удалось открыть браузер. Проверьте системный обработчик HTTPS.",
                    );
                }
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.auth.copy.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade()
                && let Some(challenge) = &this.model.borrow().challenge
            {
                this.widgets
                    .auth
                    .code
                    .clipboard()
                    .set_text(challenge.user_code());
                this.widgets
                    .status
                    .set_text("Одноразовый код скопирован в буфер обмена. Не публикуйте его.");
            }
        });
    }

    fn cancel_auth(&self) {
        self.clear_text_focus();
        let mut model = self.model.borrow_mut();
        model.generation += 1;
        model.busy = false;
        if let Some(challenge) = model.challenge.take() {
            challenge.cancel();
        }
        drop(model);
        self.widgets.auth.client_id.set_text("");
        self.widgets.auth.client_secret.set_text("");
        self.widgets.auth.code.set_text("");
        self.widgets.auth.revealer.set_reveal_child(false);
        self.widgets
            .status
            .set_text("Вход отменён. Локальные данные попытки удалены.");
        self.render_controls();
    }

    fn begin_auth(self: &Rc<Self>) {
        if !self.experimental {
            return;
        }
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let mut model = self.model.borrow_mut();
        if model.closed || model.busy || model.challenge.is_some() {
            return;
        }
        model.generation += 1;
        let generation = model.generation;
        model.busy = true;
        drop(model);
        let client_id = zeroize::Zeroizing::new(self.widgets.auth.client_id.text().to_string());
        let client_secret =
            zeroize::Zeroizing::new(self.widgets.auth.client_secret.text().to_string());
        // Remove the native-entry copies before any request. Owned worker buffers
        // zeroize on drop; GTK/HTTP libraries may retain transient copies.
        self.widgets.auth.client_id.set_text("");
        self.widgets.auth.client_secret.set_text("");
        self.widgets.auth.begin.set_sensitive(false);
        self.widgets.status.set_text("Подготовка авторизации…");
        self.render_controls();
        let hold = app.hold();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let _hold = hold;
            let result = gio::spawn_blocking(move || {
                let client = DeviceAuthClient::new(client_id, client_secret)?;
                let challenge = client.begin()?;
                Ok::<_, crate::session::SessionError>((client, challenge))
            })
            .await;
            let Some(this) = weak.upgrade() else {
                return;
            };
            let current = {
                let mut model = this.model.borrow_mut();
                if model.generation == generation {
                    model.busy = false;
                }
                !model.closed && model.generation == generation
            };
            this.widgets.auth.begin.set_sensitive(true);
            this.render_controls();
            if !current {
                if let Ok(Ok((_, challenge))) = result {
                    challenge.cancel();
                }
                return;
            }
            match result {
                Ok(Ok((client, challenge))) => {
                    this.clear_text_focus();
                    this.widgets.auth.code.set_text(challenge.user_code());
                    this.widgets.auth.configuration.set_visible(false);
                    this.widgets.auth.progress.set_visible(true);
                    this.widgets
                        .status
                        .set_text("Ожидание подтверждения в браузере…");
                    this.model.borrow_mut().challenge = Some(challenge.clone());
                    this.render_controls();
                    this.widgets.auth.open.emit_clicked();
                    this.poll_auth(client, challenge, generation);
                }
                Ok(Err(error)) => this
                    .widgets
                    .status
                    .set_text(&format!("Не удалось начать вход: {error}")),
                Err(_) => this
                    .widgets
                    .status
                    .set_text("Не удалось подготовить авторизацию."),
            }
        });
    }

    fn poll_auth(
        self: &Rc<Self>,
        client: DeviceAuthClient,
        challenge: DeviceChallenge,
        generation: u64,
    ) {
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let expires_at = std::time::Instant::now() + challenge.expires_in();
            let mut next_poll = std::time::Instant::now() + challenge.interval();
            loop {
                {
                    let Some(this) = weak.upgrade() else {
                        challenge.cancel();
                        return;
                    };
                    let model = this.model.borrow();
                    if model.closed || model.generation != generation {
                        challenge.cancel();
                        return;
                    }
                    let remaining = expires_at
                        .saturating_duration_since(std::time::Instant::now())
                        .as_secs();
                    this.widgets.auth.remaining.set_text(&format!(
                        "Осталось {} мин {:02} с",
                        remaining / 60,
                        remaining % 60
                    ));
                    if std::time::Instant::now() >= expires_at {
                        drop(model);
                        challenge.cancel();
                        this.finish_auth("Срок действия кода истёк. Начните новую попытку.");
                        return;
                    }
                }
                // Wake for cancellation/expiry/countdown without polling the
                // provider early or retaining a canceled attempt for minutes.
                if std::time::Instant::now() < next_poll {
                    glib::timeout_future(
                        next_poll
                            .saturating_duration_since(std::time::Instant::now())
                            .min(expires_at.saturating_duration_since(std::time::Instant::now()))
                            .min(std::time::Duration::from_secs(1)),
                    )
                    .await;
                    continue;
                }
                let hold = {
                    let Some(this) = weak.upgrade() else {
                        challenge.cancel();
                        return;
                    };
                    let model = this.model.borrow();
                    if model.closed || model.generation != generation {
                        challenge.cancel();
                        return;
                    }
                    this.app.upgrade().map(|app| app.hold())
                };
                let poll_client = client.clone();
                let poll_challenge = challenge.clone();
                let result = gio::spawn_blocking(move || poll_client.poll(&poll_challenge)).await;
                next_poll = std::time::Instant::now() + challenge.interval();
                drop(hold);
                let Some(this) = weak.upgrade() else {
                    challenge.cancel();
                    return;
                };
                {
                    let model = this.model.borrow();
                    if model.closed || model.generation != generation {
                        challenge.cancel();
                        return;
                    }
                }
                match result {
                    Ok(Ok(DevicePoll::Pending)) => this
                        .widgets
                        .status
                        .set_text("Ожидание подтверждения в браузере…"),
                    Ok(Ok(DevicePoll::SlowDown)) => this
                        .widgets
                        .status
                        .set_text("Яндекс просит подождать. Интервал проверки увеличен."),
                    Ok(Ok(DevicePoll::Authorized(token))) => {
                        this.clear_text_focus();
                        this.model.borrow_mut().challenge = None;
                        this.widgets.auth.code.set_text("");
                        this.widgets.auth.revealer.set_reveal_child(false);
                        this.authenticate(token);
                        return;
                    }
                    Ok(Ok(DevicePoll::Denied)) => {
                        this.finish_auth("Авторизация отклонена в Яндексе.");
                        return;
                    }
                    Ok(Ok(DevicePoll::Expired)) => {
                        this.finish_auth("Срок действия кода истёк. Начните новую попытку.");
                        return;
                    }
                    Ok(Err(error)) => {
                        this.finish_auth(&format!("Ошибка авторизации: {error}"));
                        return;
                    }
                    Err(_) => {
                        this.finish_auth("Не удалось проверить подтверждение.");
                        return;
                    }
                }
            }
        });
    }

    fn finish_auth(&self, message: &str) {
        self.clear_text_focus();
        if let Some(challenge) = self.model.borrow_mut().challenge.take() {
            challenge.cancel();
        }
        self.widgets.auth.code.set_text("");
        self.widgets.auth.revealer.set_reveal_child(false);
        self.widgets.status.set_text(message);
        self.render_controls();
    }
}

#[cfg(any(test, feature = "experimental-device-auth"))]
impl Controller {
    fn authenticate(self: &Rc<Self>, token: AccessToken) {
        if let Some(previous) = self.model.borrow_mut().token.replace(token) {
            previous.invalidate();
        }
        self.run(|service, token| service.account(&token), |this, result| match result {
            Ok(account) => {
                this.model.borrow_mut().account_valid = true;
                this.widgets.account.set_text(&account.display_name);
                this.widgets.notice.set_text("Экспериментальный сеанс Music API. Данные получены из сервиса; доступ к аудио и разрешение на распространение не подтверждены. Токен только в памяти.");
                this.render_controls();
                this.load_library(0);
            }
            Err(error) => {
                this.logout();
                this.show_error(error);
            }
        });
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
            while context.pending() {
                context.iteration(false);
            }
            if condition() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native fixture operation timed out"
            );
            thread::sleep(Duration::from_millis(5));
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

    #[test]
    #[ignore = "requires an isolated GTK display; run explicitly under Xvfb"]
    fn native_music_session_catalog_queue_and_logout() {
        gtk::init().unwrap();
        adw::init().unwrap();
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
            ("GET /users/1234/likes/tracks", liked.clone(), false),
            ("POST /tracks", tracks.clone(), false),
            ("GET /users/1234/likes/tracks", liked, false),
            ("POST /tracks", tracks.clone(), false),
            ("GET /users/1234/playlists/list", playlists, false),
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
                    thread::sleep(Duration::from_millis(400));
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
        let pane = MusicPane::with_service(
            &app,
            &window,
            cfg!(feature = "experimental-device-auth"),
            Some(MusicService::for_test(&format!("http://{address}/"))),
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
        #[cfg(feature = "experimental-device-auth")]
        {
            this.widgets.login.emit_clicked();
            assert!(this.widgets.auth.revealer.reveals_child());
            this.widgets.auth.client_id.set_text("synthetic-ui-client");
            this.widgets
                .auth
                .client_secret
                .set_text("synthetic-ui-application-secret");
            this.widgets.auth.cancel.emit_clicked();
            assert!(this.widgets.auth.client_id.text().is_empty());
            assert!(this.widgets.auth.client_secret.text().is_empty());
            assert!(!this.widgets.auth.revealer.reveals_child());
            assert!(this.model.borrow().token.is_none());
        }
        this.authenticate(AccessToken::for_test(
            "synthetic-ui-token",
            Duration::from_secs(60),
        ));
        pump_until(|| this.model.borrow().account_valid && !this.model.borrow().busy);
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
    }
}
