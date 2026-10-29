// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual GTK/service loopback interaction tests; no account or media access.

use super::super::tests::{capture_native, contains_text, pump_until};
use super::super::{MusicPane, OfflineProvider, launch_system_browser};
use super::*;
use crate::{music::MusicService, session::AccessToken};
use gtk::gio;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[test]
fn interaction_feedback_contains_only_fixed_categories() {
    for error in [
        InteractionError::Conflict,
        InteractionError::WriteOutcomeUnknown,
        InteractionError::ConfirmationUnavailable,
        InteractionError::ConfirmationMismatch,
        InteractionError::Music(MusicError::Forbidden),
        InteractionError::Music(MusicError::Http(503)),
    ] {
        let message = interaction_message(error);
        assert!(!message.is_empty());
        assert!(!message.contains("http://"));
        assert!(!message.contains("https://"));
        assert!(!message.contains("synthetic-private"));
    }
}

fn button_named(widget: &gtk::Widget, name: &str) -> Option<gtk::Button> {
    if widget.widget_name() == name {
        return widget.clone().downcast().ok();
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(found) = button_named(&widget, name) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

#[test]
#[ignore = "requires an isolated GTK display; run explicitly under Xvfb"]
fn native_library_interactions_confirmed_fixture() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let liked = Arc::new(AtomicBool::new(false));
    let posts = Arc::new(AtomicUsize::new(0));
    let reads = Arc::new(AtomicUsize::new(0));
    let playlist_reads = Arc::new(AtomicUsize::new(0));
    // A single explicit future write can lose its response after being applied.
    let lose_write_response = Arc::new(AtomicBool::new(false));
    let conflict_write = Arc::new(AtomicBool::new(false));
    let hold_read = Arc::new(AtomicBool::new(false));
    let read_held = Arc::new(AtomicBool::new(false));
    let (release, waiting) = std::sync::mpsc::channel();
    let server = {
        let stop = stop.clone();
        let liked = liked.clone();
        let posts = posts.clone();
        let reads = reads.clone();
        let playlist_reads = playlist_reads.clone();
        let lose_write_response = lose_write_response.clone();
        let conflict_write = conflict_write.clone();
        let hold_read = hold_read.clone();
        let read_held = read_held.clone();
        thread::spawn(move || {
            let mut revision = 1_u64;
            while !stop.load(Ordering::Acquire) {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(_) => panic!("synthetic interaction listener failed"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    assert_eq!(stream.read(&mut byte).unwrap(), 1);
                    bytes.push(byte[0]);
                    assert!(bytes.len() < 16 * 1024);
                }
                let request = String::from_utf8(bytes).unwrap();
                let mut parts = request.lines().next().unwrap().split_whitespace();
                let method = parts.next().unwrap();
                let target = parts.next().unwrap();
                let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).unwrap();
                let length = request
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                assert!(length < 16 * 1024);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                let (status, response) = match (method, url.path()) {
                    ("GET", "/account/status") => (200, json!({"result":{"account":{"uid":1234,"displayName":"Synthetic interaction account"}}}).to_string()),
                    ("GET", "/users/1234/likes/tracks") => {
                        reads.fetch_add(1, Ordering::AcqRel);
                        if hold_read.swap(false, Ordering::AcqRel) {
                            read_held.store(true, Ordering::Release);
                            waiting.recv_timeout(Duration::from_secs(1)).unwrap();
                        }
                        (200, json!({"result":{"library":{"uid":1234,"revision":revision,"tracks":if liked.load(Ordering::Acquire) {vec![json!({"id":42})]} else {vec![]}}}}).to_string())
                    }
                    ("POST", "/users/1234/likes/tracks/add-multiple" | "/users/1234/likes/tracks/remove") => {
                        posts.fetch_add(1, Ordering::AcqRel);
                        assert_eq!(std::str::from_utf8(&body).unwrap(), "track-ids=42");
                        if conflict_write.swap(false, Ordering::AcqRel) {
                            (409, json!({"error":{"message":"synthetic-private-provider-message"}}).to_string())
                        } else {
                            liked.store(url.path().ends_with("add-multiple"), Ordering::Release);
                            revision += 1;
                            if lose_write_response.swap(false, Ordering::AcqRel) {
                                (200, "synthetic-private-invalid-response".into())
                            } else {
                                (200, json!({"result":{"revision":revision}}).to_string())
                            }
                        }
                    }
                    ("GET", "/users/1234/likes/playlists") => {
                        playlist_reads.fetch_add(1, Ordering::AcqRel);
                        (200, json!({"result":[{"playlist":{"owner":{"uid":5678},"kind":9,"title":"Synthetic liked playlist","trackCount":0}}]}).to_string())
                    }
                    ("GET", "/users/5678/playlists/9") => (200, json!({"result":{"kind":9,"trackCount":0}}).to_string()),
                    _ => panic!("unexpected synthetic interaction operation"),
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                    response.len()
                );
            }
        })
    };
    let service = MusicService::for_test(&format!("http://{address}/"));
    let token = AccessToken::for_test("synthetic-ui-interactions-token", Duration::from_secs(90));
    service.account(&token).unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.ConfirmedLibraryFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let ui = super::super::super::build_with_music_pane(
        &app,
        crate::application::ConnectionService::new(crate::api::YandexApi::new().unwrap()),
        |app, window| {
            MusicPane::with_provider(
                app,
                window,
                Some(service),
                Arc::new(OfflineProvider),
                launch_system_browser,
            )
        },
    );
    let window = ui.window.clone();
    window.set_title(Some("Karin synthetic HTTP fixture"));
    window.set_default_size(900, 650);
    let this = &ui.music.controller;
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token);
        model.account_valid = true;
    }
    this.render_controls();
    window.present();
    // These are the same production controls appended to actual track details;
    // only the test metadata and HTTP service are synthetic.
    this.detail_header("Synthetic interaction track", "Synthetic artist");
    let controls = this.like_controls("42".into());
    this.widgets.detail.append(&controls);
    let heart = button_named(controls.upcast_ref(), "music-like-toggle").unwrap();
    assert!(
        !heart.is_sensitive(),
        "track metadata never guesses membership"
    );
    pump_until(|| reads.load(Ordering::Acquire) == 1 && heart.is_sensitive());
    assert!(!contains_text(controls.upcast_ref(), "В любимом"));
    heart.emit_clicked();
    // Repeated synthetic activation cannot queue a second user write even
    // though emit_clicked bypasses GTK's disabled-button input protection.
    heart.emit_clicked();
    pump_until(|| !this.mutation_busy.get() && heart.is_sensitive());
    assert_eq!(posts.load(Ordering::Acquire), 1);
    assert_eq!(reads.load(Ordering::Acquire), 2);
    assert!(liked.load(Ordering::Acquire));
    assert!(contains_text(controls.upcast_ref(), "В любимом"));
    capture_native("library-like-confirmed");
    heart.emit_clicked();
    pump_until(|| !this.mutation_busy.get() && heart.is_sensitive());
    assert_eq!(posts.load(Ordering::Acquire), 2);
    assert_eq!(reads.load(Ordering::Acquire), 3);
    assert!(!liked.load(Ordering::Acquire));
    assert!(!contains_text(controls.upcast_ref(), "В любимом"));
    lose_write_response.store(true, Ordering::Release);
    heart.emit_clicked();
    pump_until(|| !this.mutation_busy.get());
    assert_eq!(posts.load(Ordering::Acquire), 3);
    assert_eq!(
        reads.load(Ordering::Acquire),
        3,
        "uncertain POST is never automatically confirmed or repeated"
    );
    assert!(
        liked.load(Ordering::Acquire),
        "fixture applied the write before losing its response"
    );
    assert!(!heart.is_sensitive());
    assert!(!contains_text(controls.upcast_ref(), "В любимом"));
    assert!(contains_text(
        controls.upcast_ref(),
        "Изменение могло сохраниться"
    ));
    assert!(!contains_text(
        window.upcast_ref(),
        "synthetic-private-invalid-response"
    ));
    let refresh = button_named(controls.upcast_ref(), "music-like-refresh").unwrap();
    refresh.emit_clicked();
    pump_until(|| reads.load(Ordering::Acquire) == 4 && heart.is_sensitive());
    assert_eq!(posts.load(Ordering::Acquire), 3, "Refresh is GET-only");
    assert!(contains_text(controls.upcast_ref(), "В любимом"));
    conflict_write.store(true, Ordering::Release);
    heart.emit_clicked();
    pump_until(|| !this.mutation_busy.get());
    assert_eq!(posts.load(Ordering::Acquire), 4);
    assert_eq!(reads.load(Ordering::Acquire), 4);
    assert!(!heart.is_sensitive());
    assert!(contains_text(
        controls.upcast_ref(),
        "Библиотека изменилась"
    ));
    assert!(!contains_text(
        window.upcast_ref(),
        "synthetic-private-provider-message"
    ));
    assert!(refresh.grab_focus());
    capture_native("library-like-conflict");
    refresh.emit_clicked();
    pump_until(|| reads.load(Ordering::Acquire) == 5 && heart.is_sensitive());
    assert_eq!(posts.load(Ordering::Acquire), 4);
    this.widgets.stack.set_visible_child_name("library");
    this.widgets.library_mode.set_selected(2);
    pump_until(|| playlist_reads.load(Ordering::Acquire) == 1 && !this.model.borrow().busy);
    assert!(contains_text(
        this.widgets.library_rows.upcast_ref(),
        "Synthetic liked playlist"
    ));
    capture_native("library-liked-playlists");
    this.widgets
        .library_rows
        .first_child()
        .unwrap()
        .downcast::<gtk::ListBoxRow>()
        .unwrap()
        .emit_by_name::<()>("activate", &[]);
    pump_until(|| !this.model.borrow().busy);
    assert!(contains_text(
        this.widgets.detail.upcast_ref(),
        "Треков пока нет"
    ));
    hold_read.store(true, Ordering::Release);
    this.detail_header("Synthetic interaction track", "Synthetic artist");
    this.widgets.detail.append(&this.like_controls("42".into()));
    pump_until(|| read_held.load(Ordering::Acquire));
    this.logout();
    release.send(()).unwrap();
    let settle = Instant::now() + Duration::from_millis(100);
    pump_until(|| Instant::now() >= settle);
    assert!(this.model.borrow().token.is_none());
    assert!(!this.mutation_busy.get());
    assert!(!contains_text(
        window.upcast_ref(),
        "Synthetic interaction track"
    ));
    assert!(!contains_text(window.upcast_ref(), "В любимом"));
    assert_eq!(posts.load(Ordering::Acquire), 4);
    stop.store(true, Ordering::Release);
    server.join().unwrap();
    window.close();
    drop(ui);
    drop(window);
}
