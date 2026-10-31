// SPDX-License-Identifier: GPL-3.0-or-later
//! Native component/input checks, with synthetic metadata and no service audio.

use super::super::{
    test_input,
    tests::{capture_native, pump_until},
};
use super::*;
use gtk::gio;
use std::{
    process::Command,
    time::{Duration, Instant},
};

fn track(id: &str, title: &str) -> Track {
    Track {
        id: id.into(),
        title: title.into(),
        artists: vec!["Synthetic artist".into()],
        artist_links: vec![Artist {
            id: "11".into(),
            name: "Synthetic artist".into(),
            cover: None,
        }],
        album_id: Some("7".into()),
        album_title: Some("Synthetic album".into()),
        duration_ms: Some(123_000),
        cover: None,
        explicit: false,
        available: true,
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

fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    let (x, y) = test_input::pointer(window, widget, test_input::Target::Center);
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

#[test]
#[ignore = "requires isolated Xvfb, xdotool and GTK; synthetic metadata only"]
fn native_workspace_track_pane_actions_state_and_cleanup() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.TrackPaneFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let events = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let reenter = Rc::new(Cell::new(false));
    let weak = Rc::new(RefCell::new(std::rc::Weak::<TrackPane>::new()));
    let pane = Rc::new(TrackPane::new(Rc::new({
        let events = Rc::clone(&events);
        let reenter = Rc::clone(&reenter);
        let weak = Rc::clone(&weak);
        move |action| {
            events.borrow_mut().push(match action {
                PaneAction::Close => "close",
                PaneAction::Play(track) => {
                    assert_eq!(track.id, "42");
                    "play"
                }
                PaneAction::Enqueue(track) => {
                    assert_eq!(track.id, "42");
                    "enqueue"
                }
                PaneAction::Details(track) => {
                    assert_eq!(track.id, "42");
                    "details"
                }
                PaneAction::Album(id) => {
                    assert_eq!(id, "7");
                    "album"
                }
                PaneAction::Artist(artist) => {
                    assert_eq!(artist.id, "11");
                    "artist"
                }
            });
            if reenter.get()
                && let Some(pane) = weak.borrow().upgrade()
            {
                // A synchronous controller render must not reborrow a
                // selected Track that the signal dispatcher still holds.
                pane.set_track(None, None);
            }
        }
    })));
    *weak.borrow_mut() = Rc::downgrade(&pane);
    let selected = track(
        "42",
        "Synthetic selected track with a long but readable title",
    );
    pane.set_track(Some(&selected), None);
    pane.set_playback("Приостановлено", true, true);
    pane.set_context(Some("Понравившиеся треки · загруженная часть коллекции"));
    let window = adw::ApplicationWindow::builder()
        .application(&app)
        .title("Karin synthetic HTTP fixture")
        .default_width(420)
        .default_height(980)
        .content(&pane.widget)
        .build();
    window.present();
    pump_until(|| pane.play.is_mapped() && pane.play.width() > 0);
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
    click(&window, id, &pane.play);
    assert_eq!(events.borrow().as_slice(), &["play"]);
    pane.enqueue.grab_focus();
    input(&["key", "space"]);
    pump_until(|| events.borrow().len() == 2);
    pane.details.grab_focus();
    input(&["key", "Return"]);
    pump_until(|| events.borrow().len() == 3);
    pane.album.grab_focus();
    settle();
    click(&window, id, &pane.album);
    let artist = pane
        .artists
        .first_child()
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    artist.grab_focus();
    settle();
    click(&window, id, &artist);
    assert_eq!(
        events.borrow().as_slice(),
        &["play", "enqueue", "details", "album", "artist"]
    );

    pane.set_track(Some(&selected), None);
    pane.set_playback("Воспроизведение", true, true);
    assert_eq!(pane.status.text(), "Воспроизведение");
    assert_eq!(pane.selected().as_ref(), Some(&selected));
    assert_eq!(pane.cover.first_child().unwrap().width_request(), 224);
    capture_native("workspace-track-info");
    window.set_default_size(420, 720);
    pane.close.grab_focus();
    settle();
    let scale = window.scale_factor();
    input(&[
        "mousemove",
        "--window",
        id,
        &(200 * scale).to_string(),
        &(250 * scale).to_string(),
        "click",
        "--repeat",
        "24",
        "--delay",
        "30",
        "4",
    ]);
    settle();
    assert!(
        window.width() <= 444,
        "compact pane must not force a wider window"
    );
    capture_native("workspace-track-info-compact");

    let mut unavailable = track("43", "Synthetic unavailable track");
    unavailable.available = false;
    unavailable.artist_links.clear();
    unavailable.album_id = None;
    unavailable.album_title = None;
    pane.set_track(Some(&unavailable), None);
    pane.set_playback("Недоступен", true, false);
    assert!(!pane.play.is_sensitive());
    assert!(!pane.enqueue.is_sensitive());
    assert!(!pane.album.is_visible());
    assert!(!pane.artists.is_visible());
    artist.emit_clicked();
    assert_eq!(
        events.borrow().len(),
        5,
        "late old link must not navigate a new selection"
    );

    pane.set_track(Some(&selected), None);
    pane.set_playback("", true, true);
    reenter.set(true);
    pane.play.grab_focus();
    input(&["key", "Return"]);
    pump_until(|| pane.selected().is_none());
    assert_eq!(events.borrow().last(), Some(&"play"));
    assert!(!pane.content.is_visible());
    assert!(pane.empty.is_visible());
    assert!(!pane.play.is_sensitive());
    reenter.set(false);
    click(&window, id, &pane.close);
    assert_eq!(events.borrow().last(), Some(&"close"));
    window.set_content(gtk::Widget::NONE);
    window.close();
    drop(artist);
    drop(window);
    let widget = pane.widget.downgrade();
    drop(pane);
    pump_until(|| widget.upgrade().is_none());
}
