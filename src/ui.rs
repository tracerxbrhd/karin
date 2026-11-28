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
mod diagnostics;
mod fonts;
mod icons;
mod mini_player;
mod music;
mod playback;
mod preferences;
mod shortcuts;
mod style;
mod tray;
mod tray_window;
mod updates;
mod volume_readout;

pub use fonts::FontError;
pub use icons::APP_ICON;
pub use updates::install_update_action;

/// Register the bundled typefaces before creating GTK/Pango objects.
/// This is startup-only resource loading, independent of account and playback.
pub fn prepare_typography() -> Result<(), FontError> {
    fonts::install()
}

use crate::{application::ConnectionService, config};

pub struct MainWindow {
    pub window: adw::ApplicationWindow,
    stack: adw::ViewStack,
    header: adw::HeaderBar,
    header_items: Vec<gtk::Widget>,
    scrolls: Vec<gtk::ScrolledWindow>,
    lifetime_refs: Vec<glib::WeakRef<gtk::Widget>>,
    diagnostics: Option<diagnostics::DiagnosticsPage>,
    music: music::MusicPane,
    tray: Rc<tray_window::WindowTray>,
}

pub fn main_application_window(app: &adw::Application) -> Option<adw::ApplicationWindow> {
    app.windows()
        .into_iter()
        .filter_map(|window| window.downcast::<adw::ApplicationWindow>().ok())
        .find(|window| window.has_css_class("music-shell"))
}

pub fn present_main_window(window: &adw::ApplicationWindow) {
    if let Some(show) = window.lookup_action("show-main") {
        show.activate(None);
    } else {
        window.present();
    }
}

impl MainWindow {
    /// Connect the normal preference owner to the optional desktop integration.
    pub fn enable_tray_preferences(&self) {
        self.tray.attach_playback(self.music.mini_playback());
        self.music.attach_tray(&self.tray);
    }

    /// Enable the explicitly requested metadata-only delivery experiment.
    pub fn enable_delivery_diagnostic(&self) {
        self.music.enable_delivery_diagnostic();
    }

    pub fn open_diagnostics(&self) {
        let _ = gtk::prelude::WidgetExt::activate_action(&self.window, "win.diagnostics", None);
    }

    /// Validate actual allocations as well as native responsive navigation.
    pub async fn smoke_check(&self) -> bool {
        if fonts::verify(&self.window.pango_context()).is_err() || !icons::verify() {
            eprintln!("GTK bundled presentation resources are unavailable");
            return false;
        }
        self.stack.set_visible_child_name("diagnostics");
        if let Err(error) = self.wait_for_layout().await {
            eprintln!("GTK navigation allocation failure: {error}");
            return false;
        }
        if !icons::verify_rendering(&self.window) {
            eprintln!("GTK bundled symbolic icon contours are invalid");
            return false;
        }
        self.window.is_visible() && self.window.is_realized()
    }

    pub fn show_page(&self, name: &str) {
        match name {
            "music-home" | "music-search" | "music-queue" | "music-collection"
            | "music-library" | "music-playlists" => {
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
        self.show_page("home");
        self.music.exercise_login_for_smoke().await
    }

    pub async fn exercise_welcome_navigation_for_smoke(&self) -> Result<(), String> {
        let diagnostics = self.diagnostics.as_ref().ok_or("Smoke needs diagnostics")?;
        self.open_diagnostics();
        self.wait_for_layout().await?;
        if !diagnostics.back.is_mapped() || !diagnostics.player.prepare.has_focus() {
            return Err("Diagnostics did not expose Back and focus its local controls".into());
        }
        for name in [
            "music-collection",
            "music-library",
            "music-playlists",
            "music-search",
            "music-queue",
        ] {
            if !self
                .window
                .lookup_action(name)
                .is_some_and(|action| !action.is_enabled())
            {
                return Err(format!("Signed-out welcome exposes {name}"));
            }
        }
        diagnostics.back.emit_clicked();
        self.wait_for_layout().await?;
        for _ in 0..20 {
            if self.music.primary_action().has_focus() {
                break;
            }
            glib::timeout_future(std::time::Duration::from_millis(25)).await;
        }
        if self.stack.visible_child_name().as_deref() != Some("home")
            || !self.music.primary_action().is_mapped()
            || !self.music.primary_action().has_focus()
            || diagnostics.back.is_mapped()
        {
            return Err("Back did not restore the signed-out page and keyboard focus".into());
        }
        Ok(())
    }

    pub fn verify_layout(&self) -> Result<(), String> {
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
                    "Header item clipped: {} {bounds:?}, header={header:?}, mapped={}, child-visible={}, window={}x{}, header-width={}, header-measure={:?}, window-measure={:?}",
                    item.type_().name(),
                    item.is_mapped(),
                    item.is_child_visible(),
                    self.window.width(),
                    self.window.height(),
                    self.header.width(),
                    self.header.measure(gtk::Orientation::Horizontal, -1),
                    self.window.measure(gtk::Orientation::Horizontal, -1),
                ));
            }
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
        let player = self
            .diagnostics
            .as_ref()
            .and_then(|diagnostics| diagnostics.player.cleanup_observer());
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
        let Some(player) = self
            .diagnostics
            .as_ref()
            .map(|diagnostics| &diagnostics.player)
        else {
            return false;
        };
        player.prepare_for_smoke();
        for _ in 0..100 {
            glib::timeout_future(std::time::Duration::from_millis(50)).await;
            if player
                .snapshot()
                .is_some_and(|s| s.state == PlaybackState::Ready)
            {
                if player.play_for_smoke().is_err() {
                    return false;
                }
                for _ in 0..100 {
                    glib::timeout_future(std::time::Duration::from_millis(50)).await;
                    if player
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
    build_with_auth_mode(app, service, experimental, false, false)
}

/// Explicitly offline presentation for automated captures and GTK lifecycle
/// checks. Ordinary application launches always use the real provider.
pub fn build_offline_ui_fixture(
    app: &adw::Application,
    service: ConnectionService,
    experimental: bool,
) -> MainWindow {
    build_with_auth_mode(app, service, experimental, true, false)
}

/// The developer route uses the normal account and playback controllers.
pub fn build_diagnostics(
    app: &adw::Application,
    service: ConnectionService,
    experimental: bool,
) -> MainWindow {
    build_with_auth_mode(app, service, experimental, false, true)
}

/// Exercise the original local diagnostic controls and offline sign-in.
pub fn build_offline_diagnostics_fixture(
    app: &adw::Application,
    service: ConnectionService,
    experimental: bool,
) -> MainWindow {
    build_with_auth_mode(app, service, experimental, true, true)
}

/// The same executable's explicit isolated loopback acceptance composition.
/// No actual login or desktop credential store participates in this mode.
pub fn build_application_fixture(
    app: &adw::Application,
    fixture: &crate::application_fixture::ApplicationFixture,
) -> MainWindow {
    build_shell(app, None, |app, window| {
        music::MusicPane::new_application_fixture(app, window, fixture)
    })
}

fn build_with_auth_mode(
    app: &adw::Application,
    service: ConnectionService,
    experimental: bool,
    offline_auth: bool,
    diagnostics: bool,
) -> MainWindow {
    build_shell(app, diagnostics.then_some(service), |app, window| {
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
#[cfg(test)]
fn build_with_music_pane(
    app: &adw::Application,
    service: ConnectionService,
    make_music: impl FnOnce(&adw::Application, &adw::ApplicationWindow) -> music::MusicPane,
) -> MainWindow {
    build_shell(app, Some(service), make_music)
}

fn build_shell(
    app: &adw::Application,
    diagnostics_service: Option<ConnectionService>,
    make_music: impl FnOnce(&adw::Application, &adw::ApplicationWindow) -> music::MusicPane,
) -> MainWindow {
    style::install();
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(config::APP_NAME)
        .icon_name(APP_ICON)
        .default_width(900)
        .default_height(650)
        .width_request(360)
        .height_request(480)
        .build();
    let tray = tray_window::WindowTray::attach(&window);
    window.add_css_class("music-shell");
    style::attach(&window);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_css_class("music-toolbar-surface");
    // The native CenterBox centers in the full window when space allows,
    // then yields to the side controls without increasing the window minimum.
    let header = adw::HeaderBar::builder().height_request(52).build();
    header.set_widget_name("music-header");
    header.add_css_class("music-header");
    // GTK can add its own app icon through the desktop's CSD layout. The
    // Home button supplies the brand here, so keep the other native controls
    // in their configured order and omit only the duplicate app icon.
    bind_window_decoration_layout(&header);
    let stack = adw::ViewStack::new();
    stack.set_hhomogeneous(false);
    stack.set_vhomogeneous(false);
    let focus_epoch = Rc::new(Cell::new(0_u64));
    let menu = gio::Menu::new();
    let music_menu = gio::Menu::new();
    menu.append(Some("Проверить обновления"), Some("app.check-updates"));
    menu.append(Some("Настройки"), Some("win.preferences"));
    menu.append(Some("Сочетания клавиш"), Some("win.shortcuts"));
    menu.append_section(None, &music_menu);
    if diagnostics_service.is_some() {
        menu.append(Some("Диагностика"), Some("win.diagnostics"));
    }
    menu.append(Some("О Karin"), Some("app.about"));
    menu.append(Some("Выйти из Karin"), Some("app.quit"));
    let menu_button = gtk::MenuButton::builder()
        .icon_name("karin-menu-2-symbolic")
        .tooltip_text("Меню приложения")
        .has_frame(false)
        .menu_model(&menu)
        .build();
    menu_button.update_property(&[gtk::accessible::Property::Label("Меню приложения")]);
    let end_group = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    end_group.append(&menu_button);
    header.pack_end(&end_group);
    toolbar.add_top_bar(&header);

    let diagnostics =
        diagnostics_service.map(|service| diagnostics::DiagnosticsPage::new(app, &window, service));
    if let Some(diagnostics) = &diagnostics {
        header.pack_start(&diagnostics.back);
    }
    let home = gtk::Box::new(gtk::Orientation::Vertical, 0);
    home.add_css_class("music-workspace");
    margins(&home, 24);
    home.set_margin_top(16);
    home.set_margin_bottom(16);
    let music = make_music(app, &window);
    let brand = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Contain)
        .can_shrink(true)
        .build();
    brand.set_widget_name("karin-header-icon");
    render_header_brand(&brand);
    brand.connect_scale_factor_notify(render_header_brand);
    let brand_home = gtk::Button::builder()
        .child(&brand)
        .action_name("win.music-home")
        .tooltip_text("На главную")
        .has_frame(false)
        .valign(gtk::Align::Center)
        .build();
    brand_home.set_widget_name("karin-header-home");
    brand_home.add_css_class("music-header-control");
    brand_home.update_property(&[gtk::accessible::Property::Label("На главную")]);
    let refresh = header_refresh_button(&window);
    let (back_content, back_label) = music::back_button_content();
    let music_back = gtk::Button::builder()
        .child(&back_content)
        .action_name("win.music-back")
        .tooltip_text("Назад")
        .focus_on_click(false)
        .has_frame(false)
        .valign(gtk::Align::Center)
        .build();
    music_back.set_widget_name("music-header-back");
    music_back.add_css_class("music-header-control");
    music_back.add_css_class("music-back-button");
    music_back.update_property(&[gtk::accessible::Property::Label("Назад")]);
    let music_navigation = music.header_navigation();
    let start_group = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    start_group.add_css_class("karin-font-primary");
    start_group.append(&brand_home);
    start_group.append(&music_back);
    start_group.append(&refresh);
    header.pack_start(&start_group);
    let header_center = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    header_center.append(music_navigation);
    let diagnostics_title = adw::WindowTitle::new("Диагностика", "");
    diagnostics_title.set_visible(false);
    header_center.append(&diagnostics_title);
    header.set_title_widget(Some(&header_center));
    // The side panel hosts the shared information widget without a modal
    // backdrop or outside-dismiss policy.
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
    // BreakpointBin briefly unmaps its child while applying a breakpoint.
    // Only the body changes presentation. The search entry and active player
    // controls stay in their original mapped parents throughout a resize.
    let workspace_bin = adw::BreakpointBin::builder()
        .width_request(320)
        .height_request(200)
        .build();
    workspace_bin.set_child(Some(&track_info));
    toolbar.set_content(Some(&workspace_bin));
    let toasts = adw::ToastOverlay::new();
    let shell = gtk::Stack::new();
    shell.set_hhomogeneous(false);
    shell.set_vhomogeneous(false);
    shell.add_named(&music.toolbar_with_player(&toolbar), Some("workspace"));
    toasts.set_child(Some(&shell));
    window.set_content(Some(&toasts));
    {
        let saved_focus: Rc<RefCell<Option<glib::WeakRef<gtk::Widget>>>> = Rc::default();
        let window_unmap = window.downgrade();
        let focus_unmap = saved_focus.clone();
        // GTK 4.14 can overwrite a pending focus-move reference when adaptive
        // layout hides focused content repeatedly before the next paint.
        // Clear focus to prevent that overwrite; restore a valid control on map.
        track_info.connect_unmap(move |content| {
            if let Some(window) = window_unmap.upgrade() {
                let focus = gtk::prelude::GtkWindowExt::focus(&window).filter(|focus| {
                    focus == content.upcast_ref::<gtk::Widget>() || focus.is_ancestor(content)
                });
                let had_focus = focus.is_some();
                *focus_unmap.borrow_mut() = focus.map(|widget| widget.downgrade());
                if had_focus {
                    gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
                }
            }
        });
        let window = window.downgrade();
        track_info.connect_map(move |_| {
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
    music.attach_track_info(&track_info, &information_view, stack.upcast_ref());
    music.attach_now_playing(&shell);
    let music_visibility = music.visibility_callback();
    let workspace_surface = music.workspace_surface();
    let sync_shell = {
        let stack = stack.downgrade();
        let workspace_surface = workspace_surface.downgrade();
        let navigation = music_navigation.downgrade();
        let refresh = refresh.downgrade();
        let music_back = music_back.downgrade();
        let brand_home = brand_home.downgrade();
        let diagnostics_title = diagnostics_title.downgrade();
        let back = diagnostics
            .as_ref()
            .map(|diagnostics| diagnostics.back.downgrade());
        move || {
            let (Some(stack), Some(workspace)) = (stack.upgrade(), workspace_surface.upgrade())
            else {
                return;
            };
            let home_visible = stack.visible_child_name().as_deref() == Some("home");
            if let Some(brand_home) = brand_home.upgrade() {
                // Diagnostics supplies its own Back button.
                brand_home.set_visible(home_visible);
            }
            // The workspace's own visible property expresses account admission.
            // A hidden window or diagnostics page can mask ancestor visibility.
            let music_visible = home_visible && workspace.get_visible();
            if let Some(navigation) = navigation.upgrade() {
                navigation.set_visible(music_visible);
            }
            if let Some(refresh) = refresh.upgrade() {
                refresh.set_visible(music_visible);
            }
            if let Some(back) = music_back.upgrade() {
                back.set_visible(music_visible);
            }
            if let Some(title) = diagnostics_title.upgrade() {
                title.set_visible(!home_visible);
            }
            if let Some(back) = back.as_ref().and_then(glib::WeakRef::upgrade) {
                back.set_visible(!home_visible);
            }
            if music_visible && music_menu.n_items() == 0 {
                music_menu.append(Some("Сведения о треке"), Some("win.music-info"));
                music_menu.append(Some("Очередь"), Some("win.music-queue"));
                music_menu.append(Some("Обновить данные"), Some("win.music-refresh"));
            } else if !music_visible && music_menu.n_items() != 0 {
                music_menu.remove_all();
            }
        }
    };
    let sync_shell = Rc::new(sync_shell);
    let sync = sync_shell.clone();
    stack.connect_visible_child_name_notify(move |stack| {
        music_visibility(stack.visible_child_name().as_deref() == Some("home"));
        sync();
    });
    let sync = sync_shell.clone();
    let weak_window = window.downgrade();
    let weak_stack = stack.downgrade();
    workspace_surface.connect_visible_notify(move |surface| {
        sync();
        if !surface.get_visible()
            && weak_stack
                .upgrade()
                .is_some_and(|stack| stack.visible_child_name().as_deref() == Some("diagnostics"))
        {
            let surface = surface.downgrade();
            let window = weak_window.clone();
            let stack = weak_stack.clone();
            // Logout can change visibility while rendering the account model.
            // Leave the developer page after that rendering has completed.
            glib::idle_add_local_once(move || {
                if surface
                    .upgrade()
                    .is_some_and(|surface| !surface.get_visible())
                    && stack.upgrade().is_some_and(|stack| {
                        stack.visible_child_name().as_deref() == Some("diagnostics")
                    })
                    && let Some(window) = window.upgrade()
                {
                    let _ = gtk::prelude::WidgetExt::activate_action(&window, "win.overview", None);
                }
            });
        }
    });
    let account_menu = music.account_menu();
    end_group.prepend(&account_menu);
    home.append(&music.widget);

    let home_scroll = scroll_music_page(&home);
    music.attach_content_scroll(&home_scroll);
    stack
        .add_titled(&home_scroll, Some("home"), "Музыка")
        .set_icon_name(Some("karin-home-symbolic"));
    if let Some(diagnostics) = &diagnostics {
        stack.add_titled(&diagnostics.scroll, Some("diagnostics"), "Диагностика");
        let stop_preview = diagnostics.player.stop_callback();
        stack.connect_visible_child_name_notify(move |stack| {
            if stack.visible_child_name().as_deref() != Some("diagnostics") {
                stop_preview();
            }
        });
    }
    sync_shell();

    let compact_information = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 1279sp").expect("constant breakpoint"),
    );
    compact_information.add_setter(
        &information_view,
        "width-request",
        Some(&300_i32.to_value()),
    );
    workspace_bin.add_breakpoint(compact_information);
    let compact_navigation = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 719sp").expect("constant breakpoint"),
    );
    music.add_compact_navigation_setters(&compact_navigation);
    compact_navigation.add_setter(&back_label, "visible", Some(&false.to_value()));
    compact_navigation.add_setter(&start_group, "spacing", Some(&0.to_value()));
    compact_navigation.add_setter(&refresh, "width-request", Some(&20_i32.to_value()));
    compact_navigation.add_setter(&refresh, "margin-start", Some(&2_i32.to_value()));
    compact_navigation.add_setter(
        &information_view,
        "width-request",
        Some(&(-1_i32).to_value()),
    );
    if let Some(diagnostics) = &diagnostics {
        compact_navigation.add_setter(
            &diagnostics.columns,
            "orientation",
            Some(&gtk::Orientation::Vertical.to_value()),
        );
        compact_navigation.add_setter(&diagnostics.columns, "homogeneous", Some(&false.to_value()));
    }
    if let Some(diagnostics) = &diagnostics {
        let action = gio::SimpleAction::new("diagnostics", None);
        let stack = stack.downgrade();
        let prepare = diagnostics.player.prepare.downgrade();
        let epoch = Rc::clone(&focus_epoch);
        action.connect_activate(move |_, _| {
            epoch.set(epoch.get().wrapping_add(1));
            if let (Some(stack), Some(prepare)) = (stack.upgrade(), prepare.upgrade()) {
                stack.set_visible_child_name("diagnostics");
                prepare.grab_focus();
            }
        });
        window.add_action(&action);
        app.set_accels_for_action("win.diagnostics", &["<Alt>2"]);
    }
    workspace_bin.add_breakpoint(compact_navigation);
    window.add_action(&gio::SimpleAction::new("overview", None));
    let preferences = gio::SimpleAction::new("preferences", None);
    let show_preferences = music.preferences_callback();
    preferences.connect_activate(move |_, _| {
        show_preferences();
    });
    window.add_action(&preferences);
    app.set_accels_for_action("win.preferences", &["<Primary>comma"]);
    shortcuts::install_help(&window, app);
    // Retain the existing shortcut as navigation focus, not a hidden rail.
    let navigation_action = gio::SimpleAction::new("sidebar", None);
    let window_for_sidebar = window.downgrade();
    navigation_action.connect_activate(move |_, _| {
        if let Some(window) = window_for_sidebar.upgrade() {
            let _ = gtk::prelude::WidgetExt::activate_action(&window, "win.overview", None);
        }
    });
    window.add_action(&navigation_action);
    app.set_accels_for_action("win.sidebar", &["<Primary>b"]);
    let sections = music.section_stack();
    for name in [
        "music-home",
        "music-collection",
        "music-library",
        "music-playlists",
        "music-search",
        "music-queue",
        "overview",
    ] {
        if let Some(action) = window
            .lookup_action(name)
            .and_downcast::<gio::SimpleAction>()
        {
            let weak_stack = stack.downgrade();
            let focus_action = if name == "overview" {
                "music-home"
            } else {
                name
            };
            let weak_focus = music.navigation_target(focus_action).downgrade();
            let weak_login = music.primary_action().downgrade();
            let weak_workspace = workspace_surface.downgrade();
            let weak_sections = sections.downgrade();
            let weak_window = window.downgrade();
            let epoch = Rc::clone(&focus_epoch);
            action.connect_activate(move |_, _| {
                epoch.set(epoch.get().wrapping_add(1));
                let serial = epoch.get();
                #[cfg(test)]
                eprintln!("Karin synthetic navigation focus: action={name} scheduled={serial}");
                if let Some(stack) = weak_stack.upgrade() {
                    stack.set_visible_child_name("home");
                }
                // A normal Ctrl+F has already focused the mapped header.
                // A second delayed grab can select text typed since the action.
                if name == "music-search"
                    && let Some(target) = weak_focus.upgrade()
                    && target.is_mapped()
                    && weak_window.upgrade().and_then(|w| gtk::prelude::GtkWindowExt::focus(&w))
                        .is_some_and(|focus| focus == target || focus.is_ancestor(&target))
                {
                    return;
                }
                {
                    let primary = weak_focus.clone();
                    let login = weak_login.clone();
                    let workspace = weak_workspace.clone();
                    let stack = weak_stack.clone();
                    let sections = weak_sections.clone();
                    let window = weak_window.clone();
                    let epoch = Rc::clone(&epoch);
                    let expected_section = match name {
                        "music-home" => Some("home"),
                        "music-collection" => Some("collection"),
                        "music-library" => Some("library"),
                        "music-playlists" => Some("playlists"),
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
                            {
                                return;
                            }
                            let moved = if let Some(target) = primary.upgrade() {
                                if target.is_sensitive()
                                    && workspace.upgrade().is_some_and(|surface| surface.get_visible())
                                {
                                    // A later Escape may have retired the header field
                                    // while this action waited for the outer Music page.
                                    if name == "music-search" && !target.can_focus() {
                                        return;
                                    }
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
                                #[cfg(test)]
                                if let Some(window) = window.upgrade() {
                                    let focus = gtk::prelude::GtkWindowExt::focus(&window);
                                    eprintln!(
                                        "Karin synthetic navigation focus: action={name} completed={serial} kind={} name={}",
                                        focus.as_ref().map_or("none", |widget| widget.type_().name()),
                                        focus.as_ref().map(|widget| widget.widget_name()).as_deref().unwrap_or("none"),
                                    );
                                }
                                return;
                            }
                        }
                    });
                }
            });
        }
    }
    let mut lifetime_refs: Vec<_> = [
        window.upcast_ref::<gtk::Widget>(),
        stack.upcast_ref::<gtk::Widget>(),
        header.upcast_ref::<gtk::Widget>(),
    ]
    .into_iter()
    .map(|widget| widget.downgrade())
    .collect();
    let mut header_items = vec![
        brand_home.upcast(),
        music_back.upcast(),
        refresh.upcast(),
        diagnostics_title.upcast(),
        music_navigation.clone().upcast(),
        menu_button.upcast(),
        account_menu.upcast(),
    ];
    let mut scrolls = vec![home_scroll];
    if let Some(diagnostics) = &diagnostics {
        lifetime_refs.extend(diagnostics.lifetime_refs.iter().cloned());
        header_items.push(diagnostics.back.clone().upcast());
        scrolls.push(diagnostics.scroll.clone());
    }
    MainWindow {
        window,
        stack,
        header,
        header_items,
        scrolls,
        lifetime_refs,
        diagnostics,
        music,
        tray,
    }
}

fn render_header_brand(picture: &gtk::Picture) {
    let theme = gtk::IconTheme::for_display(&picture.display());
    let paintable = theme.lookup_icon(
        icons::BRAND_MARK,
        &[],
        30,
        picture.scale_factor(),
        picture.direction(),
        gtk::IconLookupFlags::empty(),
    );
    picture.set_paintable(Some(&paintable));
}

fn bind_window_decoration_layout(titlebar: &impl IsA<gtk::Widget>) {
    titlebar
        .settings()
        .bind_property("gtk-decoration-layout", titlebar, "decoration-layout")
        .transform_to(|_, layout: Option<String>| {
            Some(layout.map(|layout| {
                layout
                    .split(':')
                    .map(|side| {
                        side.split(',')
                            .filter(|token| *token != "icon")
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .collect::<Vec<_>>()
                    .join(":")
            }))
        })
        .sync_create()
        .build();
}

fn header_refresh_button(window: &adw::ApplicationWindow) -> gtk::Button {
    let icon = gtk::Image::from_icon_name("karin-refresh-symbolic");
    icon.set_pixel_size(16);
    let spinner = gtk::Spinner::new();
    for widget in [icon.upcast_ref::<gtk::Widget>(), spinner.upcast_ref()] {
        widget.set_size_request(16, 16);
        widget.set_halign(gtk::Align::Center);
        widget.set_valign(gtk::Align::Center);
    }
    let indicator = gtk::Stack::new();
    indicator.add_child(&icon);
    indicator.add_child(&spinner);
    let refresh = gtk::Button::builder()
        .child(&indicator)
        .action_name("win.music-refresh")
        .tooltip_text("Обновить")
        .focus_on_click(false)
        .has_frame(false)
        .margin_start(4)
        .width_request(36)
        .height_request(40)
        .valign(gtk::Align::Center)
        .build();
    refresh.set_widget_name("music-header-refresh");
    refresh.add_css_class("music-header-control");
    refresh.update_property(&[gtk::accessible::Property::Label("Обновить")]);
    if let Some(action) = window.lookup_action("music-refresh") {
        let indicator = indicator.downgrade();
        action.connect_state_notify(move |action| {
            if let Some(indicator) = indicator.upgrade() {
                let busy = action.state().and_then(|value| value.get::<bool>()) == Some(true);
                spinner.set_spinning(busy);
                let child: &gtk::Widget = if busy {
                    spinner.upcast_ref()
                } else {
                    icon.upcast_ref()
                };
                indicator.set_visible_child(child);
            }
        });
    }
    refresh
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
