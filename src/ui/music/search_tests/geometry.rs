// SPDX-License-Identifier: GPL-3.0-or-later
//! Compare the two presentations of the navigation slot on GTK's frame clock.

use super::header::{entry_focused, window_id};
use super::*;
use gtk::graphene::Rect;
use std::process::Command;

fn painted_region(
    window: &adw::ApplicationWindow,
    widget: &impl IsA<gtk::Widget>,
) -> Option<gtk::gsk::RenderNode> {
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    let bounds = widget.compute_bounds(window)?;
    let (dx, dy) = window.surface_transform();
    snapshot.push_clip(&Rect::new(
        bounds.x() + dx as f32,
        bounds.y() + dy as f32,
        bounds.width(),
        bounds.height(),
    ));
    paintable.snapshot(
        &snapshot,
        f64::from(paintable.intrinsic_width()),
        f64::from(paintable.intrinsic_height()),
    );
    snapshot.pop();
    snapshot.to_node()
}

fn rect(bounds: Rect) -> [f32; 4] {
    [bounds.x(), bounds.y(), bounds.width(), bounds.height()]
}

struct Frame {
    width: i32,
    toggle_x: f32,
    shared_slot: bool,
    group: Rect,
    header_height: i32,
    node: gtk::gsk::RenderNode,
}

fn transition(ui: &crate::ui::MainWindow, id: &str, open: bool) -> Vec<Frame> {
    let window = &ui.window;
    let this = &ui.music.controller;
    let widgets = &this.widgets;
    let field = widgets.search.clone();
    let tabs = widgets.nav_wave.parent().unwrap();
    let slot = widgets
        .search
        .parent()
        .unwrap()
        .downcast::<gtk::Stack>()
        .unwrap();
    let group = widgets.primary_navigation.clone();
    let title_bar = ui.header.clone();
    let toggle = widgets.nav_search.clone();
    let (x, y) = super::super::test_input::pointer(
        window,
        &toggle,
        super::super::test_input::Target::Center,
    );
    let clock = window.frame_clock().unwrap();
    let frames = Rc::new(RefCell::new(Vec::<Frame>::new()));
    let handle = clock.connect_after_paint({
        let window = window.clone();
        let frames = frames.clone();
        let slot = slot.clone();
        move |_| {
            let bounds = field.compute_bounds(&window);
            let width = if field.is_mapped() {
                bounds.map_or(0, |rect| rect.width().round() as i32)
            } else {
                0
            };
            let mut frames = frames.borrow_mut();
            if let Some(toggle) = toggle.compute_bounds(&window)
                && let Some(group) = group.compute_bounds(&window)
                // Toggle icon/focus changes outside the shared slot must not
                // masquerade as a transitioning search field.
                && let Some(node) = painted_region(&window, &slot)
            {
                frames.push(Frame {
                    width,
                    group,
                    header_height: title_bar.height(),
                    toggle_x: toggle.x(),
                    shared_slot: !field.is_mapped()
                        || !tabs.is_mapped()
                        || bounds
                            .zip(tabs.compute_bounds(&window))
                            .is_some_and(|(a, b)| a == b),
                    node,
                });
            }
        }
    });
    let mut mouse = Command::new("xdotool")
        .args([
            "mousemove",
            "--window",
            id,
            &x.to_string(),
            &y.to_string(),
            "click",
            "1",
        ])
        .spawn()
        .unwrap();
    pump_until(|| {
        mouse.try_wait().unwrap().is_some_and(|status| {
            assert!(status.success());
            true
        })
    });
    pump_until(|| {
        widgets.search_header.is_open() == open
            && if open {
                entry_focused(window, this)
                    && !widgets.nav_wave.is_mapped()
                    && !slot.is_transition_running()
            } else {
                !widgets.search.is_mapped()
                    && widgets.nav_wave.is_mapped()
                    && !slot.is_transition_running()
            }
    });
    super::super::test_input::wait_for_layout(window, &widgets.nav_search);
    clock.disconnect(handle);
    frames.take()
}

fn rendered_png(window: &adw::ApplicationWindow, node: &gtk::gsk::RenderNode) -> glib::Bytes {
    let scale = window.scale_factor() as f32;
    let snapshot = gtk::Snapshot::new();
    snapshot.scale(scale, scale);
    snapshot.append_node(node);
    window
        .renderer()
        .unwrap()
        .render_texture(snapshot.to_node().unwrap(), None)
        .save_to_png_bytes()
}

fn header_actions(ui: &crate::ui::MainWindow) -> [Rect; 3] {
    let children = descendants(ui.header.upcast_ref());
    let marks: Vec<_> = children
        .iter()
        .filter_map(|w| w.downcast_ref::<gtk::Picture>())
        .filter(|w| w.is_mapped() && w.widget_name() == "karin-header-icon")
        .collect();
    assert_eq!(
        marks.len(),
        1,
        "the HeaderBar must paint one transparent Karin mark"
    );
    let mark = marks[0];
    let paintable = mark
        .paintable()
        .unwrap()
        .downcast::<gtk::IconPaintable>()
        .unwrap();
    assert_eq!(
        paintable.icon_name().as_deref(),
        Some(std::path::Path::new("karin-brand-mark"))
    );
    assert_eq!(paintable.intrinsic_width(), 30);
    if ui.window.width() >= 720 {
        assert!(
            mark.width() >= 28,
            "the larger brand should get its natural width"
        );
    }
    let home = marks[0]
        .ancestor(gtk::Button::static_type())
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    assert!(home.is_focusable() && (home.has_css_class("flat") || !home.has_frame()));
    assert_eq!(home.tooltip_text().as_deref(), Some("На главную"));
    assert_eq!(home.action_name().as_deref(), Some("win.music-home"));
    let refresh = children
        .iter()
        .find(|w| w.widget_name() == "music-header-refresh")
        .unwrap();
    assert!(refresh.is_mapped() && refresh.is_focusable());
    assert_eq!(refresh.tooltip_text().as_deref(), Some("Обновить"));
    let back = children
        .iter()
        .find(|w| w.widget_name() == "music-header-back")
        .unwrap();
    let back_label = descendants(back)
        .into_iter()
        .find(|w| w.widget_name() == "music-back-label")
        .unwrap()
        .downcast::<gtk::Label>()
        .unwrap();
    assert_eq!(back_label.text(), "Назад");
    assert_eq!(back_label.is_mapped(), ui.window.width() >= 720);
    if back_label.is_mapped() {
        assert!(!back_label.layout().is_ellipsized());
        crate::ui::fonts::assert_label_font(
            &back_label,
            "Montserrat",
            gtk::pango::Weight::Semibold,
        );
    }
    let bounds = |w: &gtk::Widget| w.compute_bounds(&ui.window).unwrap();
    let home = bounds(home.upcast_ref());
    let back = bounds(back);
    let refresh = bounds(refresh);
    assert!(back.height() >= 40.0);
    assert!(home.x() + home.width() <= back.x());
    assert!(back.x() + back.width() <= refresh.x());
    assert!((home.center().y() - refresh.center().y()).abs() <= 1.0);
    assert_eq!(
        ui.header.decoration_layout().as_deref(),
        Some(":minimize,maximize,close")
    );
    [home, back, refresh]
}

fn measure_state(ui: &crate::ui::MainWindow, id: &str, tag: &str, motion: bool) {
    let window = &ui.window;
    let this = &ui.music.controller;
    let widgets = &this.widgets;
    let header = &widgets.primary_navigation;
    let tabs = widgets.nav_wave.parent().unwrap();
    let orientation = gtk::Orientation::Horizontal;
    let natural = tabs
        .measure(orientation, -1)
        .1
        .max(widgets.search.measure(orientation, -1).1);
    super::super::test_input::wait_for_layout(window, &tabs);
    let tabs_bounds = tabs.compute_bounds(window).unwrap();
    let group = header.compute_bounds(window).unwrap();
    let button = widgets.nav_search.compute_bounds(window).unwrap();
    let header_height = ui.header.height();
    let row_height = header.height();
    let bar = ui.header.compute_bounds(window).unwrap();
    assert!(header.is_ancestor(&ui.header));
    assert!(widgets.nav_search.is_ancestor(header));
    assert!(!widgets.search.is_mapped());
    if window.width() >= 720 {
        assert!(
            (2.0 * group.x() + group.width() - window.width() as f32).abs() <= 2.0,
            "centered navigation in {tag}: {group:?}"
        );
    }
    assert!(group.y() >= bar.y() && group.y() + group.height() <= bar.y() + bar.height());
    assert!(button.y() >= bar.y() && button.y() + button.height() <= bar.y() + bar.height());
    let gap = button.x() - (tabs_bounds.x() + tabs_bounds.width());
    let available = super::super::test_input::available_title_width(window, header);
    let field_space = available - button.width() - gap;
    assert_eq!(
        tabs_bounds.width(),
        (natural as f32).min(field_space),
        "{tag}: the title must fill the available width up to its natural size"
    );
    assert!(
        (1.0..=12.0).contains(&gap),
        "{tag}: the search action must follow the tabs ({gap})"
    );
    assert_eq!(group.x(), tabs_bounds.x());
    assert_eq!(group.x() + group.width(), button.x() + button.width());
    let [home, back, refresh] = header_actions(ui);
    for control in [home, back, refresh] {
        assert!(
            control.x() + control.width() <= group.x(),
            "{tag}: navigation overlaps a leading action"
        );
    }
    let account = widgets.account_menu.compute_bounds(window).unwrap();
    assert!(group.x() + group.width() <= account.x());
    for label in &widgets.nav_labels {
        assert_eq!(label.is_mapped(), window.width() >= 720, "{tag}");
    }
    let slot = widgets.search.parent().unwrap();
    let before = painted_region(window, &slot).unwrap();
    let opening = transition(ui, id, true);
    let field = widgets.search.compute_bounds(window).unwrap();
    let open_button = widgets.nav_search.compute_bounds(window).unwrap();
    let open_group = header.compute_bounds(window).unwrap();
    assert_eq!(
        (field.x(), field.width()),
        (tabs_bounds.x(), tabs_bounds.width()),
        "{tag}"
    );
    assert_eq!(rect(open_button), rect(button));
    assert_eq!(rect(open_group), rect(group));
    assert_eq!(header.height(), row_height);
    assert_eq!(ui.header.height(), header_height);
    for tab in [
        &widgets.nav_wave,
        &widgets.nav_liked,
        &widgets.nav_playlists,
    ] {
        assert!(!tab.is_mapped());
    }
    let after = painted_region(window, &slot).unwrap();
    if motion && let Some(directory) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let whole_header = painted_region(window, &ui.header).unwrap();
        std::fs::write(
            directory.join(format!("{tag}-header-open.png")),
            rendered_png(window, &whole_header),
        )
        .unwrap();
    }
    let closing = transition(ui, id, false);
    assert_eq!(
        rect(tabs.compute_bounds(window).unwrap()),
        rect(tabs_bounds)
    );
    assert_eq!(
        rect(widgets.nav_search.compute_bounds(window).unwrap()),
        rect(button)
    );
    let width = field.width() as i32;
    for (name, frames) in [("opening", &opening), ("closing", &closing)] {
        let widths: Vec<_> = frames.iter().map(|f| f.width).collect();
        assert_eq!(
            widths.last().copied(),
            Some(if name == "opening" { width } else { 0 }),
            "frame clock observed the settled endpoint"
        );
        if let Some(offending) = frames.iter().find(|frame| !frame.shared_slot)
            && let Some(directory) = std::env::var_os("KARIN_TEST_CAPTURE_DIR")
        {
            std::fs::write(
                std::path::Path::new(&directory).join("regression-neighboring-presentations.png"),
                rendered_png(window, &offending.node),
            )
            .unwrap();
        }
        assert!(
            frames.iter().all(|f| f.toggle_x == button.x()
                && f.group == group
                && f.header_height == header_height
                && f.shared_slot),
            "{tag}/{name} pages must share the SAME painted slot and toggle anchor"
        );
    }
    let closed = rendered_png(window, &before);
    let opened = rendered_png(window, &after);
    assert!(closed != opened, "the rendered header changes on open");
    let mut pngs = vec![("closed", closed), ("open", opened)];
    let mut distinct_frames = Vec::new();
    for (phase, frames) in [("revealing", &opening), ("closing", &closing)] {
        let mut distinct = Vec::new();
        for frame in frames {
            let png = rendered_png(window, &frame.node);
            if png != pngs[0].1 && png != pngs[1].1 && !distinct.contains(&png) {
                distinct.push(png);
            }
        }
        if motion {
            assert!(
                distinct.len() >= 2,
                "{tag}/{phase} must PAINT at least two distinct crossfade stages, got {}",
                distinct.len()
            );
            pngs.push((phase, distinct[distinct.len() / 2].clone()));
        } else {
            assert!(
                distinct.is_empty(),
                "{tag}/{phase} reduced motion paints only endpoints"
            );
        }
        distinct_frames.push(distinct.len());
    }
    if let Some(directory) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
        let directory = std::path::Path::new(&directory);
        std::fs::create_dir_all(directory).unwrap();
        let surface = window.surface().unwrap();
        let data = json!({
            "scale": window.scale_factor(),
            "window": [window.width(), window.height()],
            "surface": [surface.width(), surface.height()],
            "maximized": window.is_maximized(),
            "theme_dark": adw::StyleManager::default().is_dark(),
            "animations": motion,
            "tabs_natural": natural,
            "tabs": rect(tabs_bounds),
            "entry": rect(field),
            "button_closed": rect(button),
            "button_open": rect(open_button),
            "group_closed": rect(group),
            "group_open": rect(open_group),
            "header_bar_height": header_height,
            "home": rect(home),
            "back": rect(back),
            "refresh": rect(refresh),
            "navigation_row_height": row_height,
            "distinct_intermediate_painted_frames": distinct_frames,
            "transition": "native-stack-crossfade-same-slot",
            "opening_widths": opening.iter().map(|f| f.width).collect::<Vec<_>>(),
            "closing_widths": closing.iter().map(|f| f.width).collect::<Vec<_>>(),
        });
        std::fs::write(
            directory.join(format!("{tag}.json")),
            serde_json::to_vec_pretty(&data).unwrap(),
        )
        .unwrap();
        for (phase, png) in pngs {
            std::fs::write(directory.join(format!("{tag}-{phase}.png")), png).unwrap();
        }
        let native_header = painted_region(window, &ui.header).unwrap();
        std::fs::write(
            directory.join(format!("{tag}-header.png")),
            rendered_png(window, &native_header),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "requires X11 at >= 2560 logical pixels, Openbox and xdotool"]
fn native_global_search_measured_slot_and_motion() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let _manager = super::super::design_tests::WindowManager::start();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.SearchLayoutFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    // Exercise the desktop layout that previously produced both a native and
    // a custom Karin mark. A plain Openbox session omits the native icon.
    let settings = gtk::Settings::default().unwrap();
    settings.set_gtk_decoration_layout(Some("icon:minimize,maximize,close"));
    // The text cursor has its own blink clock even under reduced motion.
    // Keep it visible so pixel differences measure the native page transition.
    settings.set_gtk_cursor_blink(false);
    let ui = crate::ui::build_offline_ui_fixture(
        &app,
        crate::application::ConnectionService::new(crate::api::YandexApi::new().unwrap()),
        false,
    );
    let this = &ui.music.controller;
    this.model.borrow_mut().account_valid = true;
    this.render_controls();
    let window = &ui.window;
    assert_eq!(
        descendants(this.widgets.primary_navigation.upcast_ref())
            .iter()
            .filter(|w| w.is::<gtk::SearchEntry>())
            .count(),
        1
    );
    let results = this.widgets.stack.child_by_name("search").unwrap();
    assert!(
        descendants(&results)
            .iter()
            .all(|w| !w.is::<gtk::SearchEntry>())
    );
    window.set_title(Some("Karin synthetic HTTP fixture"));
    window.present();
    super::super::test_input::wait_for_layout(window, &this.widgets.nav_search);
    let id = window_id();
    input(&["windowfocus", "--sync", &id]);
    let style = adw::StyleManager::default();
    for (width, height) in [
        (420, 720),
        (360, 720),
        (900, 650),
        (1440, 900),
        (1920, 1080),
        (2560, 1080),
        (0, 0),
    ] {
        let size = if width == 0 {
            window.maximize();
            pump_until(|| window.is_maximized());
            "maximized".to_string()
        } else {
            window.set_default_size(width, height);
            pump_until(|| {
                window.surface().is_some_and(|surface| {
                    let actual_width = surface.width();
                    let fits = if width == 360 {
                        // GTK may clamp this additional stress case to the
                        // space required by the native CSD configuration.
                        (360..=400).contains(&actual_width)
                    } else {
                        actual_width == width
                    };
                    fits && surface.height() == height
                })
            });
            if width == 360 {
                super::super::test_input::wait_for_layout(window, &this.widgets.nav_search);
                assert!(
                    ui.header.measure(gtk::Orientation::Horizontal, -1).0 <= 380,
                    "compact HeaderBar must yield to native title buttons"
                );
            }
            format!("{}x{height}", window.surface().unwrap().width())
        };
        for (theme, scheme) in [
            ("light", adw::ColorScheme::ForceLight),
            ("dark", adw::ColorScheme::ForceDark),
        ] {
            style.set_color_scheme(scheme);
            settle();
            assert_eq!(style.is_dark(), theme == "dark");
            for (mode, motion) in [("normal", true), ("reduced", false)] {
                settings.set_gtk_enable_animations(motion);
                let tag = format!(
                    "global-search-{size}-{theme}-{mode}-scale{}",
                    window.scale_factor()
                );
                eprintln!("Measuring {tag}");
                measure_state(&ui, &id, &tag, motion);
            }
        }
    }
    // A wider translation applied while open must resize the field and stay
    // available to GTK measurement while the buttons are hidden.
    let liked = &this.widgets.nav_liked;
    let liked_label = &this.widgets.nav_labels[1];
    let original = liked_label.label();
    let before = liked.parent().unwrap().width();
    input(&["key", "--clearmodifiers", "ctrl+f"]);
    pump_until(|| entry_focused(window, this) && !liked.is_mapped());
    liked_label.set_label("Любимые композиции в коллекции");
    pump_until(|| this.widgets.search.width() > before);
    let enlarged = this.widgets.search.compute_bounds(window).unwrap().width();
    input(&["key", "--clearmodifiers", "Escape"]);
    pump_until(|| liked.is_mapped() && !this.widgets.search.is_mapped());
    super::super::test_input::wait_for_layout(window, liked);
    assert_eq!(
        liked
            .parent()
            .unwrap()
            .compute_bounds(window)
            .unwrap()
            .width(),
        enlarged
    );
    // Constrain that same translation, then mirror the header's reading
    // direction. The toggle keeps its complete hit area after the tabs in RTL.
    window.unmaximize();
    pump_until(|| !window.is_maximized());
    window.set_default_size(420, 720);
    pump_until(|| {
        window
            .surface()
            .is_some_and(|s| s.width() == 420 && s.height() == 720)
    });
    let direction = gtk::Widget::default_direction();
    gtk::Widget::set_default_direction(gtk::TextDirection::Rtl);
    super::super::test_input::wait_for_layout(window, &this.widgets.nav_search);
    let toggle = this.widgets.nav_search.compute_bounds(window).unwrap();
    let compact = liked.parent().unwrap().compute_bounds(window).unwrap();
    assert!(compact.width() < enlarged && toggle.x() + toggle.width() <= compact.x());
    assert!(!liked_label.is_mapped());
    assert!(toggle.x() >= 0.0 && compact.x() + compact.width() <= window.width() as f32);
    assert_eq!(
        toggle.width(),
        this.widgets
            .nav_search
            .measure(gtk::Orientation::Horizontal, -1)
            .1 as f32
    );
    click(window, &id, &this.widgets.nav_search);
    pump_until(|| entry_focused(window, this) && !liked.is_mapped());
    super::super::test_input::wait_for_layout(window, &this.widgets.search);
    let entry = this.widgets.search.compute_bounds(window).unwrap();
    assert_eq!((entry.x(), entry.width()), (compact.x(), compact.width()));
    assert_eq!(
        rect(this.widgets.nav_search.compute_bounds(window).unwrap()),
        rect(toggle)
    );
    liked_label.set_label(&original);
    gtk::Widget::set_default_direction(direction);
    ui.music.close();
    window.close();
}
