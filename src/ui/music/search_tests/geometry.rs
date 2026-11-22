// SPDX-License-Identifier: GPL-3.0-or-later
//! Compare the two presentations of the navigation slot on GTK's frame clock.

use super::header::{entry_focused, window_id};
use super::*;
use gtk::graphene::Rect;
use std::process::Command;

fn painted_header(
    window: &adw::ApplicationWindow,
    header: &gtk::Box,
) -> Option<gtk::gsk::RenderNode> {
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    // Observe exactly the shared presentation slot. Toggle icon/focus changes
    // outside it cannot masquerade as a transitioning search field.
    let group = header.first_child()?;
    let group_bounds = group.compute_bounds(window)?;
    let toggle = group.last_child()?;
    let toggle = toggle.compute_bounds(window)?;
    let x = if group.direction() == gtk::TextDirection::Rtl {
        toggle.x() + toggle.width() + 6.0
    } else {
        group_bounds.x()
    };
    let bounds = Rect::new(
        x,
        group_bounds.y(),
        group_bounds.width() - toggle.width() - 6.0,
        group_bounds.height(),
    );
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
    let slot = widgets.search.parent().and_downcast::<gtk::Stack>();
    let header = widgets.primary_navigation.clone();
    let group = widgets.search_header.clone();
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
                && let Some(node) = painted_header(&window, &header)
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
                    && slot
                        .as_ref()
                        .is_none_or(|slot| !slot.is_transition_running())
            } else {
                !widgets.search.is_mapped()
                    && widgets.nav_wave.is_mapped()
                    && slot
                        .as_ref()
                        .is_none_or(|slot| !slot.is_transition_running())
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

fn measure_state(ui: &crate::ui::MainWindow, id: &str, tag: &str, motion: bool) {
    let window = &ui.window;
    let this = &ui.music.controller;
    let widgets = &this.widgets;
    let header = &widgets.primary_navigation;
    let tabs = widgets.nav_wave.parent().unwrap();
    let natural = tabs.measure(gtk::Orientation::Horizontal, -1).1;
    super::super::test_input::wait_for_layout(window, &tabs);
    let tabs_bounds = tabs.compute_bounds(window).unwrap();
    let group = widgets.search_header.compute_bounds(window).unwrap();
    let button = widgets.nav_search.compute_bounds(window).unwrap();
    let header_height = ui.header.height();
    let row_height = header.height();
    assert!(!widgets.search.is_mapped());
    assert_eq!(tabs_bounds.width(), natural as f32);
    assert!(
        (2.0 * group.x() + group.width() - window.width() as f32).abs() <= 2.0,
        "centered navigation in {tag}: {group:?}"
    );
    assert_eq!(group.x(), tabs_bounds.x());
    assert_eq!(group.x() + group.width(), button.x() + button.width());
    let before = painted_header(window, header).unwrap();
    let opening = transition(ui, id, true);
    let field = widgets.search.compute_bounds(window).unwrap();
    let open_button = widgets.nav_search.compute_bounds(window).unwrap();
    let open_group = widgets.search_header.compute_bounds(window).unwrap();
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
    let after = painted_header(window, header).unwrap();
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
    let settings = gtk::Settings::default().unwrap();
    let style = adw::StyleManager::default();
    for (width, height) in [
        (420, 720),
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
                window
                    .surface()
                    .is_some_and(|surface| surface.width() == width && surface.height() == height)
            });
            format!("{width}x{height}")
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
    let original = liked.label().unwrap();
    let before = liked.parent().unwrap().width();
    input(&["key", "--clearmodifiers", "ctrl+f"]);
    pump_until(|| entry_focused(window, this) && !liked.is_mapped());
    liked.set_label("Любимые композиции в коллекции");
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
    // direction. The toggle keeps its complete hit area on the leading edge.
    window.unmaximize();
    pump_until(|| !window.is_maximized());
    window.set_default_size(420, 720);
    pump_until(|| {
        window
            .surface()
            .is_some_and(|s| s.width() == 420 && s.height() == 720)
    });
    this.widgets
        .search_header
        .set_direction(gtk::TextDirection::Rtl);
    super::super::test_input::wait_for_layout(window, &this.widgets.nav_search);
    let toggle = this.widgets.nav_search.compute_bounds(window).unwrap();
    let compact = liked.parent().unwrap().compute_bounds(window).unwrap();
    assert!(compact.width() < enlarged && compact.x() >= toggle.x() + toggle.width());
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
    let entry = this.widgets.search.compute_bounds(window).unwrap();
    assert_eq!((entry.x(), entry.width()), (compact.x(), compact.width()));
    assert_eq!(
        rect(this.widgets.nav_search.compute_bounds(window).unwrap()),
        rect(toggle)
    );
    liked.set_label(&original);
    ui.music.close();
    window.close();
}
