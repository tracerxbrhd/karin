// SPDX-License-Identifier: GPL-3.0-or-later
//! Physical search input on the navigation, text, clear icon and content background.

use super::*;
use std::process::Command;

pub(super) fn entry_focused(window: &adw::ApplicationWindow, this: &Controller) -> bool {
    gtk::prelude::GtkWindowExt::focus(window).is_some_and(|focus| {
        focus == this.widgets.search.clone().upcast::<gtk::Widget>()
            || focus.is_ancestor(&this.widgets.search)
    })
}

pub(super) fn window_id() -> String {
    let output = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic HTTP fixture$",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let ids = String::from_utf8(output.stdout).unwrap();
    let id = ids.lines().next().unwrap();
    assert!(id.bytes().all(|b| b.is_ascii_digit()));
    id.to_owned()
}

pub(super) fn open_by_mouse(window: &adw::ApplicationWindow, id: &str, this: &Controller) {
    let widgets = &this.widgets;
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(true);
    click(window, id, &widgets.nav_search);
    pump_until(|| entry_focused(window, this) && !widgets.nav_liked.is_mapped());
    // Pick inside the blank text area, well beyond the placeholder and away
    // from either icon. The XTest event must be delivered to this entry.
    let entry = widgets.search.compute_bounds(window).unwrap();
    let x = entry.x() + entry.width() * 0.78;
    let y = entry.y() + entry.height() / 2.0;
    let target = window
        .pick(x.into(), y.into(), gtk::PickFlags::DEFAULT)
        .unwrap();
    assert!(
        target == *widgets.search.upcast_ref::<gtk::Widget>()
            || target.is_ancestor(&widgets.search)
    );
    let (x, y) = surface_point(window, x, y);
    click_at(id, x, y);
    assert!(widgets.search_header.is_open() && entry_focused(window, this));
    assert_eq!(widgets.stack.visible_child_name().as_deref(), Some("home"));
    let (blank_x, blank_y) = blank_content_point(window, this);
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "mousedown",
        "1",
    ]);
    settle();
    input(&[
        "mousemove",
        "--window",
        id,
        &blank_x.to_string(),
        &blank_y.to_string(),
        "mouseup",
        "1",
    ]);
    settle();
    assert!(
        widgets.search_header.is_open(),
        "a drag starting in the entry must not dismiss search"
    );
    click_at(id, blank_x, blank_y);
    pump_until(|| !widgets.search.is_mapped() && widgets.nav_search.has_focus());
    click(window, id, &widgets.home_search);
    pump_until(|| entry_focused(window, this));
    // This ordinary navigation action is visible while the three header tabs
    // are replaced. Its destination wins over the saved Home origin.
    click(window, id, &widgets.home_liked);
    pump_until(|| {
        widgets.stack.visible_child_name().as_deref() == Some("library")
            && !widgets.search.is_mapped()
            && !this.model.borrow().busy
    });
    click(window, id, &widgets.nav_wave);
    pump_until(|| widgets.stack.visible_child_name().as_deref() == Some("home"));
    gap_and_revealing_tab(window, id, this);
    input(&["key", "--clearmodifiers", "ctrl+f"]);
    pump_until(|| entry_focused(window, this));
    input(&["type", "--clearmodifiers", "--delay", "0", "short"]);
    pump_until(|| widgets.search.text() == "short");
    input(&["key", "--clearmodifiers", "ctrl+a", "BackSpace"]);
    pump_until(|| widgets.search.text().is_empty());
    // Reverse several in-flight transitions before the next allocation.
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
    super::super::test_input::wait_for_layout(window, &widgets.search);
    window.set_default_size(900, 650);
    pump_until(|| {
        window
            .surface()
            .is_some_and(|s| s.width() == 900 && s.height() == 650)
    });
    super::super::test_input::wait_for_layout(window, &widgets.search);
    assert!(entry_focused(window, this));
}

fn gap_and_revealing_tab(window: &adw::ApplicationWindow, id: &str, this: &Controller) {
    let widgets = &this.widgets;
    input(&["key", "--clearmodifiers", "ctrl+f"]);
    pump_until(|| entry_focused(window, this) && !widgets.nav_liked.is_mapped());
    let entry = widgets.search.compute_bounds(window).unwrap();
    let toggle = widgets.nav_search.compute_bounds(window).unwrap();
    let entry_end = entry.x() + entry.width();
    let x = (entry_end + toggle.x()) / 2.0;
    let y = entry.y() + entry.height() / 2.0;
    assert!(entry_end < x && x < toggle.x());
    assert_eq!(
        window.pick(x.into(), y.into(), gtk::PickFlags::DEFAULT),
        Some(widgets.search_header.clone().upcast())
    );
    let (x, y) = surface_point(window, x, y);
    click_at(id, x, y);
    pump_until(|| !widgets.search.is_mapped());

    // Choose the exposed Liked tab as the field starts to reveal. Observe
    // GTK's current pick and enqueue both button events before another frame.
    input(&["key", "--clearmodifiers", "ctrl+f"]);
    pump_until(|| widgets.search_header.is_open() && entry_focused(window, this));
    let tab = widgets.nav_liked.compute_bounds(window).unwrap();
    let x = tab.x() + tab.width() / 2.0;
    let y = tab.y() + tab.height() / 2.0;
    let picked = window
        .pick(x.into(), y.into(), gtk::PickFlags::DEFAULT)
        .unwrap();
    assert!(
        picked == *widgets.nav_liked.upcast_ref::<gtk::Widget>()
            || picked.is_ancestor(&widgets.nav_liked)
    );
    let (x, y) = surface_point(window, x, y);
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "mousedown",
        "1",
        "mouseup",
        "1",
    ]);
    pump_until(|| {
        widgets.stack.visible_child_name().as_deref() == Some("library")
            && !widgets.search.is_mapped()
            && !this.model.borrow().busy
    });
    click(window, id, &widgets.nav_wave);
    pump_until(|| widgets.stack.visible_child_name().as_deref() == Some("home"));
}

fn surface_point(window: &adw::ApplicationWindow, x: f32, y: f32) -> (i32, i32) {
    let scale = f64::from(window.scale_factor());
    let (dx, dy) = window.surface_transform();
    let x = ((f64::from(x) + dx) * scale).round() as i32;
    let y = ((f64::from(y) + dy) * scale).round() as i32;
    (x, y)
}

fn click_at(id: &str, x: i32, y: i32) {
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "1",
    ]);
    settle();
}

pub(super) fn click_blank_content(window: &adw::ApplicationWindow, id: &str, this: &Controller) {
    let (x, y) = blank_content_point(window, this);
    click_at(id, x, y);
}

fn blank_content_point(window: &adw::ApplicationWindow, this: &Controller) -> (i32, i32) {
    // Look exclusively for a point picked on the body background or padding.
    // Rows, images and labels are deliberately ineligible as substitutes.
    let stack = this.widgets.stack.compute_bounds(window).unwrap();
    for y in (stack.y().ceil() as i32..window.height() - 12).step_by(13) {
        for x in (4..window.width() / 2).step_by(13) {
            let Some(target) = window.pick(x.into(), y.into(), gtk::PickFlags::DEFAULT) else {
                continue;
            };
            if matches!(
                target.widget_name().as_str(),
                "music-pane" | "music-home" | "music-home-library"
            ) || target.has_css_class("music-workspace")
            {
                return surface_point(window, x as f32, y as f32);
            }
        }
    }
    panic!("expected exposed blank music content");
}

pub(super) fn click_clear(window: &adw::ApplicationWindow, id: &str, entry: &gtk::SearchEntry) {
    let clear = descendants(entry.upcast_ref())
        .into_iter()
        .find(|widget| {
            widget.is_mapped()
                && widget
                    .downcast_ref::<gtk::Image>()
                    .and_then(|icon| icon.icon_name())
                    .is_some_and(|name| name.starts_with("edit-clear"))
        })
        .expect("the query has a native clear icon");
    click(window, id, &clear);
}
