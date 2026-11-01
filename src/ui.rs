// SPDX-License-Identifier: GPL-3.0-or-later
//! Native presentation layer. Network transport details never enter this module.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use gtk::{gio, glib};

mod artwork;
mod auth;
mod music;
mod playback;
mod preferences;
mod style;

use crate::{
    application::ConnectionService,
    audio::{self, AudioOutput},
    capabilities::{AuthenticationCapability, AuthenticationUnavailable},
    config,
    domain::{AccountAccess, ConnectionState, ServiceError},
    platform,
};

pub struct MainWindow {
    pub window: adw::ApplicationWindow,
    stack: adw::ViewStack,
    header: adw::HeaderBar,
    header_items: Vec<gtk::Widget>,
    split: adw::OverlaySplitView,
    sidebar_toggle: gtk::ToggleButton,
    sidebar_expand: gtk::ToggleButton,
    sidebar: gtk::ListBox,
    scrolls: Vec<gtk::ScrolledWindow>,
    lifetime_refs: Vec<glib::WeakRef<gtk::Widget>>,
    player: playback::DeveloperPlayback,
    music: music::MusicPane,
}

impl MainWindow {
    /// Enable the owner's explicitly requested metadata-only local experiment.
    /// Credentials stay in this window's ordinary ephemeral Music session.
    pub fn enable_delivery_diagnostic(&self) {
        self.music.enable_delivery_diagnostic();
    }

    /// Validate actual allocations as well as native responsive navigation.
    pub async fn smoke_check(&self) -> bool {
        self.stack.set_visible_child_name("diagnostics");
        if let Err(error) = self.wait_for_layout().await {
            eprintln!("GTK navigation allocation failure: {error}");
            return false;
        }
        self.window.is_visible() && self.window.is_realized()
    }

    pub fn show_page(&self, name: &str) {
        match name {
            "music-home" | "music-search" | "music-queue" | "music-library" => {
                self.stack.set_visible_child_name("home");
                let _ = gtk::prelude::WidgetExt::activate_action(
                    &self.window,
                    &format!("win.{name}"),
                    None,
                );
            }
            _ => self.stack.set_visible_child_name(name),
        }
    }

    pub fn visible_page(&self) -> Option<glib::GString> {
        if self.stack.visible_child_name().as_deref() == Some("home") {
            self.music
                .section_stack()
                .visible_child_name()
                .map(|name| format!("music-{name}").into())
        } else {
            self.stack.visible_child_name()
        }
    }

    pub fn focus_primary_action(&self) -> bool {
        self.show_page("home");
        let primary = self.music.primary_action();
        primary.grab_focus() && primary.has_focus()
    }

    pub async fn exercise_login_for_smoke(&self) -> Result<(), String> {
        self.show_page("music-library");
        self.music.exercise_login_for_smoke().await
    }

    pub async fn exercise_sidebar_for_smoke(&self) -> Result<(), String> {
        // Exercise the actual window action from an unmapped Music page. The
        // convenience show_page() pre-maps it and would hide an ordering bug.
        self.stack.set_visible_child_name("diagnostics");
        self.player.prepare.grab_focus();
        gtk::prelude::WidgetExt::activate_action(&self.window, "win.music-search", None)
            .map_err(|_| "Search window action is missing")?;
        let mut search_focused = false;
        for _ in 0..20 {
            let target = self.music.navigation_target("music-search");
            search_focused = if target.is_sensitive() {
                gtk::prelude::GtkWindowExt::focus(&self.window)
                    .is_some_and(|focus| focus == target || focus.is_ancestor(&target))
            } else {
                self.music.primary_action().has_focus()
            };
            if search_focused {
                break;
            }
            glib::timeout_future(std::time::Duration::from_millis(25)).await;
        }
        if !search_focused {
            return Err("Search action did not move keyboard focus from diagnostics".into());
        }
        self.show_page("music-library");
        if self.split.is_collapsed() {
            self.sidebar_toggle.set_active(true);
            glib::timeout_future(std::time::Duration::from_millis(250)).await;
            if !self.split.shows_sidebar() || !self.sidebar.is_mapped() {
                return Err("Compact navigation did not open".into());
            }
        }
        let search = self
            .sidebar
            .row_at_index(2)
            .ok_or("Missing search destination")?;
        search.emit_by_name::<()>("activate", &[]);
        glib::timeout_future(std::time::Duration::from_millis(250)).await;
        if self.music.section_stack().visible_child_name().as_deref() != Some("search")
            || self.stack.visible_child_name().as_deref() != Some("home")
            || (self.split.is_collapsed() && self.split.shows_sidebar())
        {
            return Err("Sidebar action did not navigate and return to content".into());
        }
        self.show_page("music-home");
        Ok(())
    }

    pub fn verify_layout(&self) -> Result<(), String> {
        let width = self.window.width();
        let header = self
            .header
            .compute_bounds(&self.window)
            .ok_or("HeaderBar has no allocation")?;
        if header.y() < 0.0 || header.height() < 48.0 {
            return Err(format!("HeaderBar clipped or too short: {header:?}"));
        }
        for item in self.header_items.iter().filter(|item| item.is_visible()) {
            let bounds = item
                .compute_bounds(&self.window)
                .ok_or("Header item missing allocation")?;
            if !item.is_mapped()
                || bounds.height() < 16.0
                || bounds.y() < header.y()
                || bounds.y() + bounds.height() > header.y() + header.height() + 1.0
            {
                return Err(format!(
                    "Header item clipped: {} {bounds:?}",
                    item.type_().name()
                ));
            }
        }
        let compact = width < 720;
        if self.split.is_collapsed() != compact || self.sidebar_toggle.is_visible() != compact {
            return Err(format!("Navigation does not match width {width}"));
        }
        if !compact && !self.split.shows_sidebar() {
            return Err("Desktop sidebar is hidden".into());
        }
        if !compact && !self.sidebar_expand.is_active() && self.split.min_sidebar_width() > 80.0 {
            return Err("Desktop icon rail was not restored".into());
        }
        for scroll in &self.scrolls {
            let adjustment = scroll.hadjustment();
            if adjustment.upper() - adjustment.page_size() > 1.0 {
                return Err("Horizontal scrolling would hide content".into());
            }
        }
        if self.stack.visible_child_name().as_deref() == Some("home") {
            self.music.verify_layout()?;
        }
        check_widget_geometry(self.window.upcast_ref(), self.window.upcast_ref())
    }

    /// Breakpoints can expose a widget before its next frame allocates it.
    /// Keep every structural assertion, but wait a bounded time for GTK layout.
    pub async fn wait_for_layout(&self) -> Result<(), String> {
        let mut last = Err("GTK layout did not settle".into());
        for _ in 0..20 {
            glib::timeout_future(std::time::Duration::from_millis(25)).await;
            last = self.verify_layout();
            if last.is_ok() {
                return last;
            }
        }
        last
    }

    /// Consume every strong reference before observing deferred widget cleanup.
    pub fn close_for_smoke(self) -> impl Fn(bool) -> bool {
        let mut refs = self.lifetime_refs.clone();
        let mut ancestor = self.stack.parent();
        while let Some(widget) = ancestor {
            refs.push(widget.downgrade());
            ancestor = widget.parent();
        }
        let player = self.player.cleanup_observer();
        self.music.close();
        self.window.close();
        drop(self);
        move |report| {
            let remaining: Vec<_> = refs
                .iter()
                .filter_map(|weak| weak.upgrade())
                .map(|widget| {
                    format!(
                        "{}:{} refs={} parent={:?}",
                        widget.type_().name(),
                        widget.widget_name(),
                        widget.ref_count(),
                        widget.parent().map(|p| p.type_().name().to_owned())
                    )
                })
                .collect();
            let worker_running = player.as_ref().is_some_and(|p| p.snapshot().worker_running);
            if report && (!remaining.is_empty() || worker_running) {
                eprintln!(
                    "GTK cleanup detail: retained={remaining:?}, worker_running={worker_running}"
                );
            }
            remaining.is_empty() && !worker_running
        }
    }

    /// Exercise generated local media through the actual GTK diagnostics entry.
    pub async fn exercise_playback_for_smoke(&self) -> bool {
        use crate::playback::PlaybackState;
        self.player.prepare_for_smoke();
        for _ in 0..100 {
            glib::timeout_future(std::time::Duration::from_millis(50)).await;
            if self
                .player
                .snapshot()
                .is_some_and(|s| s.state == PlaybackState::Ready)
            {
                if self.player.play_for_smoke().is_err() {
                    return false;
                }
                for _ in 0..100 {
                    glib::timeout_future(std::time::Duration::from_millis(50)).await;
                    if self
                        .player
                        .snapshot()
                        .is_some_and(|s| s.state == PlaybackState::Playing)
                    {
                        return true;
                    }
                }
                return false;
            }
        }
        false
    }
}

pub fn build(app: &adw::Application, service: ConnectionService) -> MainWindow {
    build_with_options(app, service, false)
}

pub fn build_with_options(
    app: &adw::Application,
    service: ConnectionService,
    experimental: bool,
) -> MainWindow {
    build_with_auth_mode(app, service, experimental, false)
}

/// Explicitly offline presentation for automated captures and GTK lifecycle
/// checks. Ordinary application launches always use the real provider.
pub fn build_offline_ui_fixture(
    app: &adw::Application,
    service: ConnectionService,
    experimental: bool,
) -> MainWindow {
    build_with_auth_mode(app, service, experimental, true)
}

fn build_with_auth_mode(
    app: &adw::Application,
    service: ConnectionService,
    experimental: bool,
    offline_auth: bool,
) -> MainWindow {
    build_with_music_pane(app, service, |app, window| {
        if offline_auth {
            music::MusicPane::new_offline(app, window, experimental)
        } else {
            music::MusicPane::new(app, window, experimental)
        }
    })
}

// Production and loopback fixtures construct the same native shell. Only the
// caller supplies the existing service/provider composition; no fixture catalog
// or credential entry is shipped in the ordinary application.
fn build_with_music_pane(
    app: &adw::Application,
    service: ConnectionService,
    make_music: impl FnOnce(&adw::Application, &adw::ApplicationWindow) -> music::MusicPane,
) -> MainWindow {
    style::install();
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(config::APP_NAME)
        .default_width(900)
        .default_height(650)
        .width_request(360)
        .height_request(480)
        .build();
    window.add_css_class("music-shell");
    let toolbar = adw::ToolbarView::new();
    {
        let saved_focus: Rc<RefCell<Option<glib::WeakRef<gtk::Widget>>>> = Rc::default();
        let window_unmap = window.downgrade();
        let focus_unmap = saved_focus.clone();
        // GTK 4.14 can overwrite a pending focus-move reference when adaptive
        // layout hides focused content repeatedly before the next paint.
        // Clear focus to prevent that overwrite; restore a valid control on map.
        toolbar.connect_unmap(move |toolbar| {
            if let Some(window) = window_unmap.upgrade() {
                let focus = gtk::prelude::GtkWindowExt::focus(&window).filter(|focus| {
                    focus == toolbar.upcast_ref::<gtk::Widget>() || focus.is_ancestor(toolbar)
                });
                let had_focus = focus.is_some();
                *focus_unmap.borrow_mut() = focus.map(|widget| widget.downgrade());
                if had_focus {
                    gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
                }
            }
        });
        let window = window.downgrade();
        toolbar.connect_map(move |_| {
            let focus = saved_focus.borrow_mut().take();
            if let (Some(window), Some(focus)) = (window.upgrade(), focus)
                && gtk::prelude::GtkWindowExt::focus(&window).is_none()
                && let Some(focus) = focus.upgrade()
                && focus.is_mapped()
                && focus.is_sensitive()
            {
                focus.grab_focus();
            }
        });
    }
    let header = adw::HeaderBar::builder().height_request(48).build();
    header.add_css_class("music-header");
    header.set_show_start_title_buttons(false);
    let stack = adw::ViewStack::new();
    stack.set_hhomogeneous(false);
    stack.set_vhomogeneous(false);
    let title = adw::WindowTitle::new("Karin", "");
    header.set_title_widget(Some(&title));
    let split = adw::OverlaySplitView::builder()
        .min_sidebar_width(80.0)
        .max_sidebar_width(80.0)
        .sidebar_width_fraction(0.1)
        .build();
    let focus_epoch = Rc::new(Cell::new(0_u64));
    split.connect_show_sidebar_notify({
        let epoch = Rc::clone(&focus_epoch);
        move |split| {
            if split.is_collapsed() && split.shows_sidebar() {
                epoch.set(epoch.get().wrapping_add(1));
            }
        }
    });
    let sidebar_toggle = gtk::ToggleButton::builder()
        .active(true)
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Разделы музыки")
        .visible(false)
        .build();
    sidebar_toggle.set_widget_name("navigation-sidebar-toggle");
    sidebar_toggle
        .bind_property("active", &split, "show-sidebar")
        .bidirectional()
        .sync_create()
        .build();
    header.pack_start(&sidebar_toggle);
    let menu = gio::Menu::new();
    menu.append(Some("Настройки"), Some("win.preferences"));
    menu.append(Some("Сведения о треке"), Some("win.music-info"));
    menu.append(Some("Обновить данные"), Some("win.music-refresh"));
    menu.append(Some("Диагностика"), Some("win.diagnostics"));
    menu.append(Some("О Karin"), Some("app.about"));
    menu.append(Some("Закрыть Karin"), Some("app.quit"));
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Меню приложения")
        .menu_model(&menu)
        .build();
    header.pack_end(&menu_button);
    toolbar.add_top_bar(&header);
    split.set_content(Some(&toolbar));
    window.set_content(Some(&split));

    let sidebar_toolbar = adw::ToolbarView::new();
    sidebar_toolbar.add_css_class("music-rail");
    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.set_show_start_title_buttons(false);
    sidebar_header.set_show_end_title_buttons(false);
    let brand = gtk::Label::new(Some("K"));
    brand.add_css_class("title-3");
    sidebar_header.set_title_widget(Some(&brand));
    sidebar_toolbar.add_top_bar(&sidebar_header);
    let sidebar_content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let destinations = gtk::ListBox::new();
    destinations.add_css_class("navigation-sidebar");
    destinations.set_selection_mode(gtk::SelectionMode::Single);
    destinations.set_widget_name("music-sidebar");
    let mut navigation_labels = Vec::new();
    for (name, label, icon) in [
        ("music-home", "Главная", "go-home-symbolic"),
        ("music-library", "Библиотека", "folder-music-symbolic"),
        ("music-search", "Поиск", "system-search-symbolic"),
    ] {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("music-nav-item");
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        margins(&content, 10);
        let glyph = gtk::Image::from_icon_name(icon);
        glyph.set_pixel_size(20);
        content.append(&glyph);
        let text = gtk::Label::new(Some(label));
        text.set_xalign(0.0);
        text.set_visible(false);
        navigation_labels.push(text.downgrade());
        content.append(&text);
        row.set_tooltip_text(Some(label));
        row.update_property(&[gtk::accessible::Property::Label(label)]);
        row.set_child(Some(&content));
        row.set_action_name(Some(&format!("win.{name}")));
        row.set_widget_name(name);
        destinations.append(&row);
    }
    let sidebar_expand = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Развернуть навигацию")
        .halign(gtk::Align::Center)
        .build();
    sidebar_expand.set_widget_name("navigation-expand");
    sidebar_expand.update_property(&[gtk::accessible::Property::Label(
        "Развернуть или свернуть навигацию",
    )]);
    sidebar_content.append(&destinations);
    let navigation_space = gtk::Box::new(gtk::Orientation::Vertical, 0);
    navigation_space.set_vexpand(true);
    sidebar_content.append(&navigation_space);
    sidebar_content.append(&sidebar_expand);
    let update_navigation: Rc<dyn Fn()> = {
        let split = split.downgrade();
        let button = sidebar_expand.downgrade();
        let brand = brand.downgrade();
        Rc::new(move || {
            let (Some(split), Some(button)) = (split.upgrade(), button.upgrade()) else {
                return;
            };
            let expanded = split.is_collapsed() || button.is_active();
            for label in &navigation_labels {
                if let Some(label) = label.upgrade() {
                    label.set_visible(expanded);
                }
            }
            if let Some(brand) = brand.upgrade() {
                brand.set_text(if expanded { "Karin" } else { "K" });
            }
            if expanded {
                split.set_max_sidebar_width(240.0);
                split.set_min_sidebar_width(200.0);
            } else {
                split.set_min_sidebar_width(80.0);
                split.set_max_sidebar_width(80.0);
            }
            button.set_visible(!split.is_collapsed());
            button.set_tooltip_text(Some(if expanded {
                "Свернуть навигацию"
            } else {
                "Развернуть навигацию"
            }));
        })
    };
    sidebar_expand.connect_toggled({
        let update = update_navigation.clone();
        move |_| update()
    });
    split.connect_collapsed_notify({
        let update = update_navigation.clone();
        move |_| update()
    });
    update_navigation();
    sidebar_toolbar.set_content(Some(&sidebar_content));
    split.set_sidebar(Some(&sidebar_toolbar));

    let (diagnostics, diagnostics_heading) = page(
        "Диагностика",
        "Проверки работают независимо от авторизации.",
    );
    let connection_group = adw::PreferencesGroup::builder()
        .title("Подключение к сервису")
        .build();
    let connection_row = row("Яндекс Музыка", "Доступность ещё не проверена");
    let check_diagnostics = gtk::Button::builder()
        .label("Проверить")
        .valign(gtk::Align::Center)
        .build();
    connection_row.add_suffix(&check_diagnostics);
    connection_group.add(&connection_row);
    let AuthenticationCapability::AuthenticationUnavailable(
        AuthenticationUnavailable::MusicGrantNotEstablished,
    ) = service.capabilities().authentication;
    let (login_title, login_description) = if cfg!(feature = "cassette-auth") {
        (
            "Прототип входа через Cassette",
            "На странице «Главная» нажмите «Войти». Доступ к аккаунту, библиотеке и поиску проверяется после входа. Воспроизведение проверяется отдельно для выбранного трека.",
        )
    } else {
        (
            "Минимальная сборка без входа",
            "В этой сборке нет WebKit и провайдера авторизации. Для проверки входа нужна обычная сборка Karin. Музыкальное аудио пока недоступно.",
        )
    };
    connection_group.add(&row(login_title, login_description));
    diagnostics.append(&connection_group);
    let player = playback::DeveloperPlayback::new(app, &window);
    diagnostics.append(&player.group);

    let diagnostic_columns = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    diagnostic_columns.set_homogeneous(true);
    diagnostic_columns.set_valign(gtk::Align::Start);
    let audio_group = adw::PreferencesGroup::builder()
        .title("Аудиодиагностика")
        .description("Синтетический сигнал, не музыка из сервиса.")
        .hexpand(true)
        .build();
    let audio_row = row("Результат", "Проверка ещё не выполнялась");
    audio_group.add(&audio_row);
    let silent = gtk::Button::builder()
        .label("Проверить")
        .valign(gtk::Align::Center)
        .build();
    let silent_row = row("Обработка аудио", "Без устройства вывода");
    silent_row.add_suffix(&silent);
    audio_group.add(&silent_row);
    let audible = gtk::Button::builder()
        .label("Прослушать")
        .valign(gtk::Align::Center)
        .build();
    let audible_row = row("Тестовый сигнал", "Негромкий синтетический звук");
    audible_row.add_suffix(&audible);
    audio_group.add(&audible_row);
    diagnostic_columns.append(&audio_group);

    let desktop_group = adw::PreferencesGroup::builder()
        .title("Рабочий стол")
        .description("Проверка без чтения сохранённых секретов.")
        .hexpand(true)
        .build();
    let desktop_row = row("D-Bus и Secret Service", "Проверка ещё не выполнялась");
    let inspect = gtk::Button::builder()
        .label("Проверить")
        .valign(gtk::Align::Center)
        .build();
    desktop_row.add_suffix(&inspect);
    desktop_group.add(&desktop_row);
    desktop_group.add(&row(
        "Интеграция",
        "Состояние защищённого хранения и запоминание входа доступны в настройках",
    ));
    diagnostic_columns.append(&desktop_group);
    diagnostics.append(&diagnostic_columns);
    let runtime_group = adw::PreferencesGroup::builder().title("Окружение").build();
    runtime_group.add(&row(
        "Нативные библиотеки",
        &format!(
            "GTK {}.{}.{} · Libadwaita {}.{}.{} · {}",
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version(),
            adw::major_version(),
            adw::minor_version(),
            adw::micro_version(),
            gstreamer::version_string()
        ),
    ));
    diagnostics.append(&runtime_group);

    let home = gtk::Box::new(gtk::Orientation::Vertical, 0);
    home.add_css_class("music-workspace");
    margins(&home, 24);
    home.set_margin_top(16);
    home.set_margin_bottom(16);
    let music = make_music(app, &window);
    // Keep one information widget mounted. Unlike a collapsed split sidebar,
    // this native revealer has no modal backdrop or outside-dismiss policy.
    let track_info = gtk::Overlay::new();
    track_info.set_child(Some(&stack));
    track_info.set_widget_name("music-track-info-host");
    track_info.add_css_class("music-information-host");
    let information_content = music.track_info_widget();
    information_content.add_css_class("music-information-view");
    let information_view = music::sliding_panel::SlidingPanel::new(&information_content);
    information_view.set_width_request(320);
    information_view.set_halign(gtk::Align::End);
    information_view.set_valign(gtk::Align::Fill);
    information_view.set_widget_name("music-track-info-revealer");
    track_info.add_overlay(&information_view);
    track_info.set_measure_overlay(&information_view, false);
    toolbar.set_content(Some(&track_info));
    music.attach_track_info(&track_info, &information_view, stack.upcast_ref());
    let reflow_panel = music.reflow_panel_callback();
    sidebar_expand.connect_toggled(move |_| reflow_panel());
    let music_visibility = music.visibility_callback();
    stack.connect_visible_child_name_notify(move |stack| {
        music_visibility(stack.visible_child_name().as_deref() == Some("home"));
    });
    let account_menu = music.account_menu();
    header.pack_end(&account_menu);
    home.append(&music.widget);
    toolbar.add_bottom_bar(&music.player_bar);

    // Keep the anonymous availability request in the technical diagnostics.
    let status = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let status_title = label("", "heading");
    let status_description = label("", "");
    let check = gtk::Button::with_label("Проверить доступность");
    check.set_halign(gtk::Align::Start);
    let spinner = gtk::Spinner::builder().visible(false).build();
    status.append(&status_title);
    status.append(&status_description);
    status.append(&check);
    status.append(&spinner);
    diagnostics.append(&status);
    let home_scroll = scroll_music_page(&home);
    let diagnostics_scroll = scroll_page(&diagnostics);
    stack
        .add_titled(&home_scroll, Some("home"), "Музыка")
        .set_icon_name(Some("go-home-symbolic"));
    stack
        .add_titled(&diagnostics_scroll, Some("diagnostics"), "Диагностика")
        .set_icon_name(Some("applications-engineering-symbolic"));

    let compact_information = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 1279sp").expect("constant breakpoint"),
    );
    compact_information.add_setter(
        &information_view,
        "width-request",
        Some(&300_i32.to_value()),
    );
    window.add_breakpoint(compact_information);
    let compact_navigation = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 719sp").expect("constant breakpoint"),
    );
    compact_navigation.add_setter(&split, "collapsed", Some(&true.to_value()));
    compact_navigation.add_setter(
        &information_view,
        "width-request",
        Some(&(-1_i32).to_value()),
    );
    compact_navigation.add_setter(&split, "show-sidebar", Some(&false.to_value()));
    compact_navigation.add_setter(&sidebar_toggle, "visible", Some(&true.to_value()));
    compact_navigation.add_setter(
        &diagnostic_columns,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    compact_navigation.add_setter(&diagnostic_columns, "homogeneous", Some(&false.to_value()));
    window.add_breakpoint(compact_navigation);
    for (name, page) in [("overview", "home"), ("diagnostics", "diagnostics")] {
        let action = gio::SimpleAction::new(name, None);
        let stack = stack.downgrade();
        let target = if page == "home" {
            music.primary_action()
        } else {
            player.prepare.clone()
        }
        .downgrade();
        action.connect_activate(move |_, _| {
            if let (Some(stack), Some(target)) = (stack.upgrade(), target.upgrade()) {
                stack.set_visible_child_name(page);
                target.grab_focus();
            }
        });
        window.add_action(&action);
    }
    let preferences = gio::SimpleAction::new("preferences", None);
    let show_preferences = music.preferences_callback();
    preferences.connect_activate(move |_, _| {
        show_preferences();
    });
    window.add_action(&preferences);
    app.set_accels_for_action("win.preferences", &["<Primary>comma"]);
    let sidebar_action = gio::SimpleAction::new("sidebar", None);
    let weak_split = split.downgrade();
    let weak_destinations = destinations.downgrade();
    let weak_expand = sidebar_expand.downgrade();
    sidebar_action.connect_activate(move |_, _| {
        if let Some(split) = weak_split.upgrade() {
            if !split.is_collapsed() {
                if let Some(expand) = weak_expand.upgrade() {
                    expand.set_active(!expand.is_active());
                }
                return;
            }
            split.set_show_sidebar(!split.shows_sidebar());
            if split.shows_sidebar()
                && let Some(list) = weak_destinations.upgrade()
                && let Some(row) = list.selected_row().or_else(|| list.row_at_index(0))
            {
                row.grab_focus();
            }
        }
    });
    window.add_action(&sidebar_action);
    app.set_accels_for_action("win.sidebar", &["<Primary>b"]);
    let sections = music.section_stack();
    for name in [
        "music-home",
        "music-library",
        "music-search",
        "music-queue",
        "diagnostics",
        "overview",
    ] {
        if let Some(action) = window
            .lookup_action(name)
            .and_downcast::<gio::SimpleAction>()
        {
            let weak_stack = stack.downgrade();
            let weak_split = split.downgrade();
            let weak_focus = music.navigation_target(name).downgrade();
            let weak_login = music.primary_action().downgrade();
            let weak_sections = sections.downgrade();
            let weak_window = window.downgrade();
            let epoch = Rc::clone(&focus_epoch);
            action.connect_activate(move |_, _| {
                epoch.set(epoch.get().wrapping_add(1));
                let serial = epoch.get();
                if let Some(stack) = weak_stack.upgrade() {
                    stack.set_visible_child_name(if name == "diagnostics" {
                        "diagnostics"
                    } else {
                        "home"
                    });
                }
                if let Some(split) = weak_split.upgrade()
                    && split.is_collapsed()
                {
                    split.set_show_sidebar(false);
                }
                if name != "diagnostics" {
                    let primary = weak_focus.clone();
                    let login = weak_login.clone();
                    let stack = weak_stack.clone();
                    let sections = weak_sections.clone();
                    let window = weak_window.clone();
                    let split = weak_split.clone();
                    let epoch = Rc::clone(&epoch);
                    let expected_section = match name {
                        "music-home" => Some("home"),
                        "music-library" => Some("library"),
                        "music-search" => Some("search"),
                        "music-queue" => None,
                        _ => None,
                    };
                    glib::spawn_future_local(async move {
                        // Mapping follows native layout, not necessarily the first
                        // idle. Obsolete requests must never steal later focus.
                        for _ in 0..20 {
                            glib::timeout_future(std::time::Duration::from_millis(25)).await;
                            if epoch.get() != serial
                                || !stack.upgrade().is_some_and(|stack| {
                                    stack.visible_child_name().as_deref() == Some("home")
                                })
                                || expected_section.is_some_and(|expected| {
                                    !sections.upgrade().is_some_and(|sections| {
                                        sections.visible_child_name().as_deref() == Some(expected)
                                    })
                                })
                                || !window
                                    .upgrade()
                                    .is_some_and(|window| window.visible_dialog().is_none())
                                || split.upgrade().is_some_and(|split| {
                                    split.is_collapsed() && split.shows_sidebar()
                                })
                            {
                                return;
                            }
                            let moved = if let Some(target) = primary.upgrade() {
                                if target.is_sensitive() {
                                    target.is_mapped() && target.grab_focus()
                                } else {
                                    login.upgrade().is_some_and(|login| {
                                        login.is_mapped()
                                            && login.is_sensitive()
                                            && login.grab_focus()
                                    })
                                }
                            } else {
                                return;
                            };
                            if moved {
                                return;
                            }
                        }
                    });
                }
            });
        }
    }
    let sync_navigation = {
        let stack = stack.downgrade();
        let sections = sections.downgrade();
        let destinations = destinations.downgrade();
        let title = title.downgrade();
        let panel_title = music.panel_title_callback();
        move || {
            if let (Some(stack), Some(sections), Some(destinations), Some(title)) = (
                stack.upgrade(),
                sections.upgrade(),
                destinations.upgrade(),
                title.upgrade(),
            ) {
                let (index, page_title) =
                    if stack.visible_child_name().as_deref() == Some("diagnostics") {
                        (None, "Диагностика")
                    } else {
                        match sections.visible_child_name().as_deref() {
                            Some("home") => (Some(0), "Главная"),
                            Some("library") => (Some(1), "Библиотека"),
                            Some("search") => (Some(2), "Поиск"),
                            Some("detail") => (None, "Сведения"),
                            _ => (None, "Karin"),
                        }
                    };
                if let Some(index) = index
                    && let Some(row) = destinations.row_at_index(index)
                {
                    destinations.select_row(Some(&row));
                } else {
                    destinations.unselect_all();
                }
                title.set_title(panel_title().unwrap_or(page_title));
            }
        }
    };
    let sync_navigation = Rc::new(sync_navigation);
    let sync = sync_navigation.clone();
    stack.connect_visible_child_name_notify(move |_| sync());
    let sync = sync_navigation.clone();
    sections.connect_visible_child_name_notify(move |_| sync());
    let sync = sync_navigation.clone();
    music.panel_heading().connect_label_notify(move |_| sync());
    let sync = sync_navigation.clone();
    information_view.connect_notify_local(Some("progress"), move |_, _| sync());
    let sync = sync_navigation.clone();
    information_view.connect_width_request_notify(move |_| sync());
    let sync = sync_navigation.clone();
    information_view.connect_notify_local(Some("compact-presentation"), move |_, _| sync());
    sync_navigation();
    // Both real controls trigger the same state machine and bounded request.
    check
        .bind_property("sensitive", &check_diagnostics, "sensitive")
        .sync_create()
        .build();
    let weak_check = check.downgrade();
    check_diagnostics.connect_clicked(move |_| {
        if let Some(check) = weak_check.upgrade() {
            check.emit_clicked();
        }
    });
    let state = Rc::new(RefCell::new(ConnectionState::NotChecked));
    render_connection(
        &state.borrow(),
        &status_title,
        &status_description,
        &connection_row,
        &spinner,
        &check,
    );
    {
        // Permanent signal handlers must not retain their own widget tree.
        // Workers may temporarily keep widgets alive until bounded cleanup.
        let app = app.downgrade();
        let status_title = status_title.downgrade();
        let status_description = status_description.downgrade();
        let connection_row = connection_row.downgrade();
        let spinner = spinner.downgrade();
        check.connect_clicked(move |button| {
            let (
                Some(app),
                Some(status_title),
                Some(status_description),
                Some(connection_row),
                Some(spinner),
            ) = (
                app.upgrade(),
                status_title.upgrade(),
                status_description.upgrade(),
                connection_row.upgrade(),
                spinner.upgrade(),
            )
            else {
                return;
            };
            if !state.borrow_mut().begin_check() {
                return;
            }
            render_connection(
                &state.borrow(),
                &status_title,
                &status_description,
                &connection_row,
                &spinner,
                button,
            );
            let service = service.clone();
            let state = state.clone();
            let status_title = status_title.clone();
            let status_description = status_description.clone();
            let connection_row = connection_row.clone();
            let spinner = spinner.clone();
            let button = button.clone();
            let hold = app.hold();
            glib::spawn_future_local(async move {
                let _hold_until_cleanup = hold;
                let result = gio::spawn_blocking(move || service.check_connection())
                    .await
                    .unwrap_or(Err(ServiceError::Network));
                state.borrow_mut().complete(result);
                render_connection(
                    &state.borrow(),
                    &status_title,
                    &status_description,
                    &connection_row,
                    &spinner,
                    &button,
                );
            });
        });
    }
    connect_audio(app, &silent, &audible, &audio_row, AudioOutput::Fakesink);
    connect_audio(app, &audible, &silent, &audio_row, AudioOutput::System);
    {
        let app = app.downgrade();
        let desktop_row = desktop_row.downgrade();
        inspect.connect_clicked(move |button| {
            let (Some(app), Some(desktop_row)) = (app.upgrade(), desktop_row.upgrade()) else {
                return;
            };
            button.set_sensitive(false);
            desktop_row.set_subtitle("Проверяем окружение…");
            let button = button.clone();
            let desktop_row = desktop_row.clone();
            let hold = app.hold();
            glib::spawn_future_local(async move {
                let _hold_until_cleanup = hold;
                match gio::spawn_blocking(platform::inspect_session).await {
                    Ok(report) => desktop_row.set_subtitle(&format!(
                        "Сеансовая шина: {} · Secret Service: {}. Секреты не читались.",
                        if report.session_bus {
                            "доступна"
                        } else {
                            "недоступна"
                        },
                        if report.secret_service_running {
                            "запущен"
                        } else {
                            "не обнаружен"
                        }
                    )),
                    Err(_) => desktop_row.set_subtitle("Не удалось проверить окружение"),
                }
                button.set_sensitive(true);
            });
        });
    }

    let lifetime_refs = [
        window.upcast_ref::<gtk::Widget>(),
        stack.upcast_ref::<gtk::Widget>(),
        status.upcast_ref::<gtk::Widget>(),
        check.upcast_ref::<gtk::Widget>(),
        silent.upcast_ref::<gtk::Widget>(),
        audible.upcast_ref::<gtk::Widget>(),
        inspect.upcast_ref::<gtk::Widget>(),
        header.upcast_ref::<gtk::Widget>(),
        player.group.upcast_ref::<gtk::Widget>(),
        player.prepare.upcast_ref::<gtk::Widget>(),
    ]
    .into_iter()
    .map(|widget| widget.downgrade())
    .collect();
    // Structural smoke validation also walks these critical heading labels.
    diagnostics_heading.set_widget_name("diagnostics-heading");
    MainWindow {
        window,
        stack,
        header,
        header_items: vec![
            title.upcast(),
            sidebar_toggle.clone().upcast(),
            menu_button.upcast(),
            account_menu.upcast(),
        ],
        split,
        sidebar_toggle,
        sidebar_expand,
        sidebar: destinations,
        scrolls: vec![home_scroll, diagnostics_scroll],
        lifetime_refs,
        player,
        music,
    }
}

fn label(text: &str, css_class: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .selectable(false)
        .build();
    if !css_class.is_empty() {
        label.add_css_class(css_class);
    }
    label
}

fn margins(widget: &impl IsA<gtk::Widget>, margin: i32) {
    widget.set_margin_top(margin);
    widget.set_margin_bottom(margin);
    widget.set_margin_start(margin);
    widget.set_margin_end(margin);
}

fn page(title: &str, subtitle: &str) -> (gtk::Box, gtk::Label) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 24);
    content.set_valign(gtk::Align::Start);
    margins(&content, 24);
    let heading = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let title = label(title, "title-1");
    heading.append(&title);
    heading.append(&label(subtitle, "dim-label"));
    content.append(&heading);
    (content, title)
}

fn scroll_music_page(content: &gtk::Box) -> gtk::ScrolledWindow {
    let viewport = gtk::Viewport::builder()
        .scroll_to_focus(true)
        .child(content)
        .build();
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&viewport)
        .build()
}

fn scroll_page(content: &gtk::Box) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::builder()
        .maximum_size(1120)
        .tightening_threshold(760)
        .child(content)
        .build();
    let viewport = gtk::Viewport::builder()
        .scroll_to_focus(true)
        .child(&clamp)
        .build();
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&viewport)
        .build()
}

fn row(title: &str, subtitle: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .title_lines(0)
        .subtitle(subtitle)
        .subtitle_lines(0)
        .build()
}

/// Layout checks complement screenshots; intentional vertical scrolling is allowed.
fn check_widget_geometry(widget: &gtk::Widget, window: &gtk::Widget) -> Result<(), String> {
    if let Some(split) = widget.downcast_ref::<adw::OverlaySplitView>() {
        // Libadwaita 1.5 keeps a closed sidebar mapped outside its viewport,
        // including a dismissible wide information pane. Inspect its public
        // visibility state rather than treating that native transform as
        // clipped user-visible content. Open sidebars retain every check.
        let content = split.content().ok_or("Split view has no content")?;
        check_widget_geometry(&content, window)?;
        if split.shows_sidebar() {
            let sidebar = split.sidebar().ok_or("Split view has no sidebar")?;
            check_widget_geometry(&sidebar, window)?;
        }
        return Ok(());
    }
    if widget
        .downcast_ref::<music::sliding_panel::SlidingPanel>()
        .is_some_and(|panel| !panel.is_child_revealed())
    {
        // A translated, clipped surface is deliberately offscreen during its
        // native transition. Settled frames retain the full geometry checks.
        return Ok(());
    }
    if widget.is_mapped() {
        // Inspect content and controls, not GtkWindow shadows or decorative
        // CSS nodes such as an overlay scrollbar's overhanging trough.
        let content = widget.is::<gtk::Button>()
            || widget.is::<gtk::Label>()
            || widget.is::<adw::PreferencesGroup>()
            || widget.is::<adw::ToolbarView>();
        if content
            && let Some(bounds) = widget.compute_bounds(window)
            && (bounds.x() < -2.0 || bounds.x() + bounds.width() > window.width() as f32 + 2.0)
        {
            return Err(format!(
                "Widget exceeds window width: {} {bounds:?}, window={}",
                widget.type_().name(),
                window.width()
            ));
        }
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            // Bounded music metadata deliberately uses native ellipsis. Its
            // original text remains available through the label's accessible
            // name and matching full tooltip. Ordinary headings/status/actions
            // are not opted in and still must fit in full.
            let metadata = label.has_css_class("music-ellipsized-metadata")
                || matches!(
                    label.widget_name().as_str(),
                    "music-current-title" | "music-current-artist"
                );
            let complete_metadata =
                metadata && label.tooltip_text().as_deref() == Some(label.text().as_str());
            if label.layout().is_ellipsized() && !complete_metadata {
                return Err(format!("Text was ellipsized: {}", label.text()));
            }
            // Text layout height must fit its own allocation. Viewport clipping
            // below the fold is expected and does not count as text truncation.
            if label.layout().pixel_size().1 > label.height() + 2 {
                return Err(format!("Text allocation too short: {}", label.text()));
            }
        }
        if widget.widget_name() == "overview-heading"
            || widget.widget_name() == "diagnostics-heading"
        {
            let bounds = widget
                .compute_bounds(window)
                .ok_or("Heading allocation missing")?;
            let at_top = widget
                .ancestor(gtk::ScrolledWindow::static_type())
                .and_then(|parent| parent.downcast::<gtk::ScrolledWindow>().ok())
                .is_none_or(|scroll| scroll.vadjustment().value() < 1.0);
            if at_top && bounds.y() < 48.0 {
                return Err("Page heading overlaps HeaderBar".into());
            }
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        check_widget_geometry(&current, window)?;
        child = current.next_sibling();
    }
    Ok(())
}

fn connect_audio(
    app: &adw::Application,
    button: &gtk::Button,
    other: &gtk::Button,
    row: &adw::ActionRow,
    output: AudioOutput,
) {
    let app = app.downgrade();
    let other = other.downgrade();
    let row = row.downgrade();
    button.connect_clicked(move |button| {
        let (Some(app), Some(other), Some(row)) = (app.upgrade(), other.upgrade(), row.upgrade())
        else {
            return;
        };
        button.set_sensitive(false);
        other.set_sensitive(false);
        row.set_subtitle("Проверяем синтетический сигнал…");
        let button = button.clone();
        let other = other.clone();
        let row = row.clone();
        let hold = app.hold();
        glib::spawn_future_local(async move {
            let _hold_until_cleanup = hold;
            let message = match gio::spawn_blocking(move || audio::run_test_tone(output)).await {
                Ok(Ok(_)) => match output {
                    AudioOutput::Fakesink => {
                        "Проверка без звука завершена. Поток обработан до конца.".to_owned()
                    }
                    AudioOutput::System => {
                        "Сигнал завершён. Подтвердите слышимость самостоятельно.".to_owned()
                    }
                },
                Ok(Err(error)) => format!("Ошибка аудио: {error}"),
                Err(_) => "Не удалось завершить проверку аудио".to_owned(),
            };
            row.set_subtitle(&message);
            button.set_sensitive(true);
            other.set_sensitive(true);
        });
    });
}

fn presentation(state: &ConnectionState) -> (&'static str, String) {
    match state {
        ConnectionState::NotChecked => (
            "Подключение не проверено",
            "Karin ещё не авторизован. Можно проверить доступность сервиса без входа в аккаунт."
                .into(),
        ),
        ConnectionState::Checking => (
            "Проверяем доступность…",
            "Окно остаётся доступным во время запроса.".into(),
        ),
        ConnectionState::Available(report) => {
            let title = match report.access {
                AccountAccess::AuthorizationRequired => "Требуется авторизация",
                AccountAccess::Forbidden => "Доступ запрещён",
                AccountAccess::Anonymous if report.service_available == Some(false) => {
                    "Сервис недоступен в этой среде"
                }
                AccountAccess::Anonymous => "API отвечает · вход не выполнен",
            };
            (
                title,
                format!(
                    "Получен ответ HTTP {}. Этот запрос выполнен без входа и не проверяет аккаунт или доступ к музыке.{}",
                    report.http_status,
                    if report.service_available == Some(false) {
                        " Сервис сообщил о недоступности для текущего подключения."
                    } else {
                        ""
                    }
                ),
            )
        }
        ConnectionState::Failed(error) => {
            let message = match error {
                ServiceError::Network => "Сервис недоступен. Проверьте сетевое подключение.".into(),
                ServiceError::Timeout => "Сервис не ответил вовремя.".into(),
                ServiceError::Http(status) => format!("Сервис вернул HTTP {status}."),
                ServiceError::InvalidResponse => "Сервис вернул неподдерживаемый ответ.".into(),
                ServiceError::ResponseTooLarge => {
                    "Ответ сервиса превышает допустимый размер.".into()
                }
                ServiceError::Configuration => {
                    "Не удалось подготовить подключение к сервису.".into()
                }
            };
            (
                "Не удалось проверить подключение",
                format!("{message} Можно повторить попытку."),
            )
        }
    }
}

fn render_connection(
    state: &ConnectionState,
    title: &gtk::Label,
    description: &gtk::Label,
    row: &adw::ActionRow,
    spinner: &gtk::Spinner,
    button: &gtk::Button,
) {
    let (heading, text) = presentation(state);
    title.set_label(heading);
    description.set_label(&text);
    row.set_subtitle(&format!("{heading}. {text}"));
    let loading = matches!(state, ConnectionState::Checking);
    spinner.set_visible(loading);
    spinner.set_spinning(loading);
    button.set_sensitive(!loading);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ConnectionReport;

    #[test]
    fn anonymous_response_is_never_presented_as_authenticated() {
        let state = ConnectionState::Available(ConnectionReport {
            access: AccountAccess::Anonymous,
            http_status: 200,
            service_available: Some(false),
        });
        let (title, text) = presentation(&state);
        assert_eq!(title, "Сервис недоступен в этой среде");
        assert!(text.contains("запрос выполнен без входа"));
    }

    #[test]
    fn failure_offers_retry_and_loading_is_distinct() {
        let error_text = presentation(&ConnectionState::Failed(ServiceError::Timeout)).1;
        assert!(error_text.contains("Сервис не ответил вовремя."));
        assert!(!error_text.contains(".."));
        assert!(
            presentation(&ConnectionState::Failed(ServiceError::Timeout))
                .1
                .contains("повторить")
        );
        assert_ne!(
            presentation(&ConnectionState::NotChecked).0,
            presentation(&ConnectionState::Checking).0
        );
    }
}
