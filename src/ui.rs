// SPDX-License-Identifier: GPL-3.0-or-later
//! Native presentation layer. Network transport details never enter this module.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::{gio, glib};

mod auth;
mod music;
mod playback;

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
    switcher: adw::ViewSwitcher,
    navigation: adw::ViewSwitcherBar,
    scrolls: Vec<gtk::ScrolledWindow>,
    lifetime_refs: Vec<glib::WeakRef<gtk::Widget>>,
    player: playback::DeveloperPlayback,
    music: music::MusicPane,
}

impl MainWindow {
    /// Validate actual allocations as well as native responsive navigation.
    pub fn smoke_check(&self) -> bool {
        self.stack.set_visible_child_name("diagnostics");
        self.window.is_visible() && self.window.is_realized() && self.verify_layout().is_ok()
    }

    pub fn show_page(&self, name: &str) {
        match name {
            "music-search" | "music-queue" | "music-library" => {
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
        self.stack.visible_child_name()
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
        let compact = width <= 600;
        if self.switcher.is_visible() == compact || self.navigation.reveals() != compact {
            return Err(format!("Navigation does not match width {width}"));
        }
        for scroll in &self.scrolls {
            let adjustment = scroll.hadjustment();
            if adjustment.upper() - adjustment.page_size() > 1.0 {
                return Err("Horizontal scrolling would hide content".into());
            }
        }
        if self.visible_page().as_deref() == Some("home") {
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
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(config::APP_NAME)
        .default_width(900)
        .default_height(650)
        .width_request(360)
        .height_request(480)
        .build();
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
    let stack = adw::ViewStack::new();
    let brand = gtk::Label::new(Some("Karin"));
    brand.add_css_class("heading");
    header.pack_start(&brand);
    let compact_title = gtk::Label::builder().label("Karin").visible(false).build();
    compact_title.add_css_class("heading");
    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    let header_title = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    header_title.append(&compact_title);
    header_title.append(&switcher);
    header.set_title_widget(Some(&header_title));
    let menu = gio::Menu::new();
    menu.append(Some("О Karin"), Some("app.about"));
    menu.append(Some("Выйти"), Some("app.quit"));
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Меню приложения")
        .menu_model(&menu)
        .build();
    header.pack_end(&menu_button);
    toolbar.add_top_bar(&header);
    let navigation = adw::ViewSwitcherBar::builder()
        .stack(&stack)
        .reveal(false)
        .build();
    toolbar.set_content(Some(&stack));
    window.set_content(Some(&toolbar));

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
    connection_group.add(&row(
        "Вход в Музыку пока недоступен",
        "Для Karin ещё не подтверждён разрешённый способ авторизации в Яндекс Музыке. Источник музыкального аудио проверяется отдельно.",
    ));
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
        "Хранение токенов и MPRIS ещё не подключены",
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

    let (home, home_heading) = page("Музыка", "Библиотека, поиск и плейлисты");
    let music = if offline_auth {
        music::MusicPane::new_offline(app, &window, experimental)
    } else {
        music::MusicPane::new(app, &window, experimental)
    };
    home.append(&music.widget);
    toolbar.add_bottom_bar(&music.player_bar);
    toolbar.add_bottom_bar(&navigation);

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
    let home_scroll = scroll_page(&home);
    let diagnostics_scroll = scroll_page(&diagnostics);
    stack
        .add_titled(&home_scroll, Some("home"), "Музыка")
        .set_icon_name(Some("go-home-symbolic"));
    stack
        .add_titled(&diagnostics_scroll, Some("diagnostics"), "Диагностика")
        .set_icon_name(Some("applications-engineering-symbolic"));

    let compact_navigation = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 600sp").expect("constant breakpoint"),
    );
    compact_navigation.add_setter(&switcher, "visible", Some(&false.to_value()));
    compact_navigation.add_setter(&compact_title, "visible", Some(&true.to_value()));
    compact_navigation.add_setter(&brand, "visible", Some(&false.to_value()));
    compact_navigation.add_setter(&navigation, "reveal", Some(&true.to_value()));
    let compact_content = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 850sp").expect("constant breakpoint"),
    );
    compact_content.add_setter(
        &diagnostic_columns,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    compact_content.add_setter(&diagnostic_columns, "homogeneous", Some(&false.to_value()));
    window.add_breakpoint(compact_content);
    // Only the last matching breakpoint is active. Include content setters in
    // the smallest range as well, then register it after the medium range.
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
    home_heading.set_widget_name("overview-heading");
    diagnostics_heading.set_widget_name("diagnostics-heading");
    MainWindow {
        window,
        stack,
        header,
        header_items: vec![
            brand.upcast(),
            compact_title.upcast(),
            switcher.clone().upcast(),
            menu_button.upcast(),
        ],
        switcher,
        navigation,
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

fn scroll_page(content: &gtk::Box) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::builder()
        .maximum_size(1120)
        .tightening_threshold(760)
        .child(content)
        .build();
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
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
            // Track titles in a fixed-height player bar use native ellipsis;
            // the full metadata must remain available as a tooltip/accessibility
            // label. Headings, status text and actions still must fit in full.
            let complete_metadata = label.widget_name() == "music-current-title"
                && label.tooltip_text().as_deref() == Some(label.text().as_str());
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
                    "Получен ответ HTTP {}. Авторизация и воспроизведение музыки пока не подключены.{}",
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
        assert!(text.contains("пока не подключены"));
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
