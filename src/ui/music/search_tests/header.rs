// SPDX-License-Identifier: GPL-3.0-or-later
//! Observe the actual header allocation and rendered frames around a mouse click.

use super::*;

pub(super) fn entry_focused(window: &adw::ApplicationWindow, this: &Controller) -> bool {
    gtk::prelude::GtkWindowExt::focus(window).is_some_and(|focus| {
        focus == this.widgets.search.clone().upcast::<gtk::Widget>()
            || focus.is_ancestor(&this.widgets.search)
    })
}

fn painted_header(
    window: &adw::ApplicationWindow,
    header: &gtk::Box,
) -> Option<gtk::gsk::RenderNode> {
    // Include the window's actual theme background beneath the toolbar, and
    // retain the same viewport at each frame regardless of the field's width.
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    let bounds = header.compute_bounds(window)?;
    let (dx, dy) = window.surface_transform();
    snapshot.push_clip(&gtk::graphene::Rect::new(
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

pub(super) fn open_by_mouse(window: &adw::ApplicationWindow, id: &str, this: &Controller) {
    let header = &this.widgets.primary_navigation;
    let toggle = &this.widgets.nav_search;
    let entry = &this.widgets.search;
    let settings = gtk::Settings::default().unwrap();
    let original_motion = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);
    super::super::test_input::wait_for_layout(window, toggle);
    let height = header.height();
    let start = toggle.compute_bounds(window).unwrap().x().round() as i32;
    let before = painted_header(window, header).expect("closed navigation");
    let frames = Rc::new(RefCell::new(Vec::new()));
    let clock = window.frame_clock().unwrap();
    let handle = clock.connect_after_paint({
        let (window, header, toggle, frames) = (
            window.clone(),
            header.clone(),
            toggle.clone(),
            frames.clone(),
        );
        move |_| {
            let x = toggle.compute_bounds(&window).unwrap().x().round() as i32;
            let mut frames = frames.borrow_mut();
            if x > start
                && frames.last().is_none_or(|(last, _)| x != *last)
                && let Some(node) = painted_header(&window, &header)
            {
                frames.push((x, node));
            }
        }
    });
    // Let GTK consume the physical button events and paint continuously.
    // Waiting synchronously for xdotool's click would block the GTK thread
    // through part of the very transition we are observing.
    let (x, y) =
        super::super::test_input::pointer(window, toggle, super::super::test_input::Target::Center);
    let mut mouse = std::process::Command::new("xdotool")
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
    pump_until(|| entry_focused(window, this));
    let settle_end = Instant::now() + Duration::from_millis(360);
    pump_until(|| Instant::now() >= settle_end);
    clock.disconnect(handle);
    let end = toggle.compute_bounds(window).unwrap().x().round() as i32;
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("home")
    );
    assert!(entry.is_ancestor(header));
    assert_eq!(
        height,
        header.height(),
        "opening the field must preserve header height"
    );
    let results = this.widgets.stack.child_by_name("search").unwrap();
    assert!(
        !descendants(&results)
            .iter()
            .any(|w| w.is::<gtk::SearchEntry>())
    );
    let entry_bounds = entry.compute_bounds(window).unwrap();
    assert!(entry_bounds.x() + entry_bounds.width() <= end as f32 + 1.0);
    let frames = frames.take();
    let between: Vec<_> = frames
        .iter()
        .filter(|(x, _)| start < *x && *x < end)
        .collect();
    assert!(
        between.len() >= 2,
        "expected distinct painted magnifier positions between {start} and {end}, got {}",
        between.len()
    );
    eprintln!(
        "Karin search reveal: {start} to {end} logical pixels, {} intermediate positions",
        between.len()
    );
    let during = &between[between.len() / 2].1;
    let after = painted_header(window, header).expect("open navigation");
    let renderer = window.renderer().unwrap();
    let scale = window.scale_factor();
    let pngs = [&before, during, &after].map(|node| {
        let snapshot = gtk::Snapshot::new();
        snapshot.scale(scale as f32, scale as f32);
        snapshot.append_node(node);
        renderer
            .render_texture(snapshot.to_node().unwrap(), None)
            .save_to_png_bytes()
    });
    assert!(
        pngs[1] != pngs[0] && pngs[1] != pngs[2],
        "the rendered mid-reveal differs from both endpoints"
    );
    if let Some(directory) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
        let directory = std::path::Path::new(&directory);
        std::fs::create_dir_all(directory).unwrap();
        let motion = json!({
            "scale": scale,
            "window_width": window.width(),
            "closed_button_x": start,
            "open_button_x": end,
            "header_height": height,
            "intermediate_button_x": between.iter().map(|(x, _)| x).collect::<Vec<_>>(),
        });
        std::fs::write(
            directory.join(format!("global-search-reveal-scale{scale}.json")),
            serde_json::to_vec_pretty(&motion).unwrap(),
        )
        .unwrap();
        for (phase, png) in ["closed", "revealing", "open"].into_iter().zip(pngs) {
            std::fs::write(
                directory.join(format!("global-search-{phase}-scale{scale}.png")),
                png,
            )
            .unwrap();
        }
    }
    input(&["type", "--clearmodifiers", "--delay", "0", "short"]);
    pump_until(|| entry.text() == "short");
    input(&["key", "--clearmodifiers", "ctrl+a", "BackSpace"]);
    pump_until(|| entry.text().is_empty());
    // Reverse a reveal, reopen with the window shortcut and resize during the
    // transition. The same entry must receive focus and fit beside Close.
    input(&[
        "key",
        "--clearmodifiers",
        "Escape",
        "ctrl+f",
        "Escape",
        "ctrl+f",
    ]);
    pump_until(|| entry_focused(window, this));
    window.set_default_size(420, 720);
    pump_until(|| {
        window
            .surface()
            .is_some_and(|s| s.width() == 420 && s.height() == 720)
    });
    super::super::test_input::wait_for_layout(window, entry);
    let compact = entry.compute_bounds(window).unwrap();
    let close = toggle.compute_bounds(window).unwrap();
    assert!(compact.width() > 0.0 && compact.x() >= 0.0);
    assert!(compact.x() + compact.width() <= close.x() + 1.0);
    assert!(close.x() + close.width() <= window.width() as f32);
    assert_eq!(header.height(), height);
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("home")
    );
    window.set_default_size(900, 650);
    pump_until(|| {
        window
            .surface()
            .is_some_and(|s| s.width() == 900 && s.height() == 650)
    });
    super::super::test_input::wait_for_layout(window, entry);
    assert!(
        entry_focused(window, this),
        "the open entry retains focus across resize"
    );
    settings.set_gtk_enable_animations(original_motion);
}
