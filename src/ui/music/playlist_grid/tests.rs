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

fn cover(button: &gtk::Button) -> gtk::Widget {
    button.child().unwrap().first_child().unwrap()
}

fn wait_responsive_covers(grid: &PlaylistGrid) {
    pump_until(|| {
        let size = cover_size_for_width(grid.widget.width());
        buttons(grid).iter().all(|button| {
            let cover = cover(button);
            cover.width() == size && cover.height() == size
        })
    });
}

fn assert_tile_geometry(grid: &PlaylistGrid, minimum: i32, maximum: i32) {
    for tile in buttons(grid) {
        let cover = cover(&tile);
        assert!(
            (minimum..=maximum).contains(&cover.width()),
            "cover={} section={} must fit {minimum}..={maximum}",
            cover.width(),
            grid.widget.width()
        );
        assert_eq!(cover.width(), cover.height());
        assert!(
            tile.width() <= cover.width() + TILE_HORIZONTAL_BUDGET,
            "a playlist keeps natural width: tile={} cover={} section={}",
            tile.width(),
            cover.width(),
            grid.widget.width(),
        );
        let bounds = cover.compute_bounds(&tile).unwrap();
        assert!(bounds.x() >= 0.0);
        assert!(bounds.x() + bounds.width() <= tile.width() as f32);
    }
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
    // Match the collection page's real content insets. A bare 420px viewport
    // masks the old title-natural-width bug, while two cover-sized columns
    // should fit the actual smaller viewport without shrinking typography.
    grid.widget.set_margin_start(24);
    grid.widget.set_margin_end(24);
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
    wait_responsive_covers(&grid);
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
    assert_tile_geometry(&grid, 145, 175);
    let original_cover = cover(&tiles[1]);
    for tile in &tiles {
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
    window.set_default_size(850, 750);
    settle();
    wait_responsive_covers(&grid);
    assert_tile_geometry(&grid, 145, 175);
    capture_native("workspace-playlist-grid-reference-size");
    window.set_default_size(420, 720);
    settle();
    wait_responsive_covers(&grid);
    assert!(
        window.width() <= 444,
        "compact grid must not force a wider window"
    );
    assert!(scroll.hadjustment().upper() - scroll.hadjustment().page_size() < 1.0);
    assert_tile_geometry(&grid, 96, 160);
    let create_bounds = tiles[0].compute_bounds(&grid.tiles).unwrap();
    let first_bounds = tiles[1].compute_bounds(&grid.tiles).unwrap();
    assert!(
        (create_bounds.y() - first_bounds.y()).abs() <= 1.0,
        "compact Create and first playlist must share one natural row: create={create_bounds:?}, first={first_bounds:?}, section={}, tiles={}",
        grid.widget.width(),
        grid.tiles.width(),
    );
    assert!(
        first_bounds.x() >= create_bounds.x() + create_bounds.width(),
        "compact columns must not overlap: create={create_bounds:?}, first={first_bounds:?}"
    );
    capture_native("workspace-playlist-grid-compact");
    window.set_default_size(1440, 900);
    settle();
    wait_responsive_covers(&grid);
    assert_tile_geometry(&grid, 180, 210);
    assert_eq!(
        cover(&tiles[1]),
        original_cover,
        "responsive sizing must retain the mounted cover and its request lifetime"
    );
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
    wait_responsive_covers(&grid);
    assert_tile_geometry(&grid, 180, 210);
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
    drop(original_cover);
    drop(remaining);
    drop(scroll);
    drop(viewport);
    drop(window);
    let widget = grid.widget.downgrade();
    drop(grid);
    pump_until(|| widget.upgrade().is_none());
}
