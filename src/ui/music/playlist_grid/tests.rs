// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual tile mouse/keyboard input, replacement and responsive allocations.

use super::super::{
    test_input,
    tests::{capture_native, pump_until},
};
use super::*;
use crate::music::PlaylistId;
use gtk::gio;
use std::{
    cell::RefCell,
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn playlist_count_uses_actual_count_and_russian_plural_forms() {
    for (count, expected) in [
        (0, "0 треков"),
        (1, "1 трек"),
        (2, "2 трека"),
        (11, "11 треков"),
        (21, "21 трек"),
        (112, "112 треков"),
    ] {
        assert_eq!(track_count(count), expected);
    }
}

fn playlist(kind: &str, title: &str) -> Playlist {
    Playlist {
        id: PlaylistId {
            owner: "1234".into(),
            kind: kind.into(),
        },
        title: title.into(),
        track_count: 17,
        cover: None,
    }
}

fn input(args: &[&str]) {
    assert!(
        Command::new("timeout")
            .args(["5", "xdotool"])
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}

fn settle() {
    let deadline = Instant::now() + Duration::from_millis(120);
    pump_until(|| Instant::now() >= deadline);
}

fn buttons(grid: &PlaylistGrid) -> Vec<gtk::Button> {
    let mut buttons = Vec::new();
    let mut child = grid.tiles.first_child();
    while let Some(current) = child {
        let next = current.next_sibling();
        buttons.push(
            current
                .downcast::<gtk::FlowBoxChild>()
                .unwrap()
                .child()
                .unwrap()
                .downcast()
                .unwrap(),
        );
        child = next;
    }
    buttons
}

#[test]
#[ignore = "requires isolated Xvfb, xdotool and GTK; synthetic playlists only"]
fn native_workspace_playlist_grid_input_states_and_resize() {
    gtk::init().unwrap();
    adw::init().unwrap();
    crate::ui::style::install();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.PlaylistGridFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let events = Rc::new(RefCell::new(Vec::<String>::new()));
    let grid = PlaylistGrid::new(
        None,
        Rc::new({
            let events = Rc::clone(&events);
            move |action| {
                events.borrow_mut().push(match action {
                    PlaylistAction::Open(playlist) => playlist.id.kind,
                    PlaylistAction::Create => "create".into(),
                    PlaylistAction::Retry => "retry".into(),
                })
            }
        }),
    );
    let playlists = vec![
        playlist("3", "Synthetic playlist"),
        playlist(
            "4",
            "Synthetic long playlist title that must wrap and retain its full accessible name",
        ),
    ];
    grid.set_playlists(&playlists, true);
    let viewport = gtk::Viewport::builder()
        .child(&grid.widget)
        .scroll_to_focus(true)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&viewport)
        .build();
    let window = adw::ApplicationWindow::builder()
        .application(&app)
        .title("Karin synthetic HTTP fixture")
        .default_width(900)
        .default_height(650)
        .content(&scroll)
        .build();
    window.present();
    pump_until(|| grid.tiles.is_mapped() && grid.tiles.width() > 0);
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
    let id = String::from_utf8(output.stdout).unwrap();
    let id = id.lines().next().unwrap();
    input(&["windowfocus", id]);
    let tiles = buttons(&grid);
    assert_eq!(tiles.len(), 3);
    for tile in &tiles {
        assert!(
            tile.width() <= 224,
            "a tile must not become a stretched card"
        );
        let content = tile.child().unwrap().downcast::<gtk::Box>().unwrap();
        let cover = content.first_child().unwrap();
        assert_eq!((cover.width(), cover.height()), (160, 160));
        assert!(tile.has_css_class("karin-playlist-tile"));
        assert!(!tile.has_css_class("card"));
    }
    let (x, y) = test_input::pointer(&window, &tiles[1], test_input::Target::Center);
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "1",
    ]);
    pump_until(|| events.borrow().len() == 1);
    assert_eq!(events.borrow().as_slice(), &["3"]);
    assert!(tiles[1].state_flags().contains(gtk::StateFlags::PRELIGHT));
    tiles[2].grab_focus();
    input(&["key", "Return"]);
    pump_until(|| events.borrow().len() == 2);
    tiles[0].grab_focus();
    input(&["key", "space"]);
    pump_until(|| events.borrow().len() == 3);
    assert!(tiles[0].has_focus());
    assert_eq!(events.borrow().as_slice(), &["3", "4", "create"]);
    capture_native("workspace-playlist-grid-medium");
    window.set_default_size(420, 720);
    settle();
    assert!(
        window.width() <= 444,
        "compact grid must not force a wider window"
    );
    assert!(scroll.hadjustment().upper() - scroll.hadjustment().page_size() < 1.0);
    capture_native("workspace-playlist-grid-compact");
    window.set_default_size(1440, 900);
    settle();
    capture_native("workspace-playlist-grid-wide");
    grid.hide_create();
    let remaining = buttons(&grid);
    assert_eq!(remaining.len(), 2);
    tiles[0].emit_clicked();
    assert_eq!(
        events.borrow().len(),
        3,
        "removed refused Create must stay inactive"
    );
    let (x, y) = test_input::pointer(&window, &remaining[0], test_input::Target::Center);
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "1",
    ]);
    pump_until(|| events.borrow().len() == 4);
    assert_eq!(
        events.borrow().last().map(String::as_str),
        Some("3"),
        "hiding Create must preserve other tile actions"
    );
    grid.set_loading();
    assert!(grid.spinner.is_spinning());
    assert!(!grid.tiles.is_visible());
    tiles[1].emit_clicked();
    assert_eq!(
        events.borrow().len(),
        4,
        "a removed old tile cannot open an obsolete page"
    );
    grid.set_error();
    assert!(!grid.spinner.is_spinning());
    assert!(grid.retry.is_visible());
    grid.retry.grab_focus();
    input(&["key", "Return"]);
    pump_until(|| events.borrow().len() == 5);
    assert_eq!(events.borrow().last().map(String::as_str), Some("retry"));
    capture_native("workspace-playlist-grid-error");
    grid.set_playlists(&[], false);
    assert!(
        buttons(&grid).is_empty(),
        "unsupported create must not leave a dummy tile"
    );
    assert!(grid.feedback.is_visible());
    grid.set_playlists(&playlists, false);
    assert_eq!(buttons(&grid).len(), 2);
    grid.set_playlists(&playlists[..1], false);
    let sparse = buttons(&grid);
    assert_eq!(sparse.len(), 1);
    pump_until(|| sparse[0].is_mapped() && sparse[0].width() > 0);
    settle();
    assert!(
        sparse[0].width() <= 224,
        "a single playlist keeps natural tile width: tile={} container={} window={}",
        sparse[0].width(),
        grid.tiles.width(),
        window.width(),
    );
    let content = sparse[0].child().unwrap().downcast::<gtk::Box>().unwrap();
    let cover = content.first_child().unwrap();
    assert_eq!((cover.width(), cover.height()), (160, 160));
    let cover_bounds = cover.compute_bounds(&sparse[0]).unwrap();
    assert!(cover_bounds.x() >= 0.0);
    assert!(cover_bounds.x() + cover_bounds.width() <= sparse[0].width() as f32);
    capture_native("native-music-playlist-grid-sparse");
    grid.set_playlists(&[playlist("5", " ")], false);
    assert_eq!(
        buttons(&grid)[0].tooltip_text().as_deref(),
        Some("Плейлист")
    );
    grid.clear();
    assert!(buttons(&grid).is_empty());
    assert!(!grid.feedback.is_visible());
    window.set_content(gtk::Widget::NONE);
    window.close();
    drop(tiles);
    drop(remaining);
    drop(scroll);
    drop(viewport);
    drop(window);
    let widget = grid.widget.downgrade();
    drop(grid);
    pump_until(|| widget.upgrade().is_none());
}
