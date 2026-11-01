// SPDX-License-Identifier: GPL-3.0-or-later
//! GTK regression: concurrent catalog requests, coalescing, detail and logout.

use super::tests::{capture_native, contains_text, pump_until};
use super::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

fn mapped_loading_spinner(widget: &gtk::Widget) -> bool {
    if let Some(spinner) = widget.downcast_ref::<gtk::Spinner>()
        && spinner.is_mapped()
        && spinner.is_spinning()
    {
        return true;
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if mapped_loading_spinner(&widget) {
            return true;
        }
    }
    false
}

fn playlist_feed(this: &Rc<Controller>, kind: &str) -> continuous_loading::Feed {
    this.track_feeds.borrow().iter().find(|feed|matches!(&feed.source,continuous_loading::FeedSource::Playlist(playlist) if playlist.id.kind==kind)).cloned().expect("current typed playlist feed")
}

#[test]
#[ignore = "requires an isolated GTK display; run explicitly under Xvfb"]
fn native_catalog_navigation_latest_actions() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let slow_started = Arc::new(AtomicBool::new(false));
    let library_started = Arc::new(AtomicBool::new(false));
    let failed_requests = Arc::new(AtomicUsize::new(0));
    let server_failed = failed_requests.clone();
    let search_queries = Arc::new(Mutex::new(Vec::<String>::new()));
    let server_queries = search_queries.clone();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let playlist_gate = Arc::new((Mutex::new(false), Condvar::new()));
    let server_playlist_gate = playlist_gate.clone();
    let server_stop = stop.clone();
    let server_slow = slow_started.clone();
    let server_library = library_started.clone();
    let server_gate = gate.clone();
    let server = thread::spawn(move || {
        let mut children = Vec::new();
        while !server_stop.load(Ordering::Acquire) {
            let stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(_) => panic!("synthetic fixture listener failed"),
            };
            let gate = server_gate.clone();
            let playlist_gate = server_playlist_gate.clone();
            let slow = server_slow.clone();
            let library = server_library.clone();
            let failures = server_failed.clone();
            let search_queries = server_queries.clone();
            children.push(thread::spawn(move || {
                let mut stream = stream;
                stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut bytes = Vec::new();
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap() == 0 { return; }
                    bytes.push(byte[0]);
                    assert!(bytes.len() < 32 * 1024);
                }
                let request = String::from_utf8(bytes).unwrap();
                let headers=request.to_ascii_lowercase();
                assert!(headers.contains("authorization: oauth synthetic-ui-concurrency-token"));
                assert!(!headers.contains("cookie:") && !headers.contains("x-yandex-music-client:"));
                let length = request.lines().find_map(|line| {
                    line.to_ascii_lowercase().strip_prefix("content-length:").map(|value| value.trim().parse::<usize>().unwrap())
                }).unwrap_or(0);
                assert!(length < 32 * 1024);
                let mut form = vec![0; length];
                stream.read_exact(&mut form).unwrap();
                let target = request.lines().next().unwrap().split_whitespace().nth(1).unwrap();
                let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).unwrap();
                let track = json!({"id":42,"title":"Synthetic selected track","available":true,"artists":[{"name":"Synthetic artist"}]});
                let mut delayed = false;
                let mut delayed_playlist = false;
                let (status, body) = match url.path() {
                    "/account/status" => (200, json!({"result":{"account":{"uid":1234,"displayName":"Synthetic account"}}})),
                    "/users/1234/likes/tracks" => {
                        library.store(true, Ordering::Release); delayed = true;
                        (200, json!({"result":{"library":{"tracks":[]}}}))
                    }
                    "/search" => {
                        assert!(url.query_pairs().any(|(name,value)|name=="type"&&value=="track"),"this coalescing fixture explicitly requests the track scope");
                        let query = url.query_pairs().find(|(name, _)| name == "text").unwrap().1.into_owned();
                        search_queries.lock().unwrap().push(query.clone());
                        if query == "slow" { slow.store(true, Ordering::Release); delayed = true; }
                        let id = if query == "slow" { 44 } else { 43 };
                        (200, json!({"result":{"tracks":{"page":0,"perPage":20,"total":1,"results":[{"id":id,"title":{"ignored":"search-display"},"available":null,"artists":false}]}}}))
                    }
                    "/users/1234/playlists/3" => { delayed_playlist = true; (200, json!({"result":{"kind":3,"trackCount":0}})) },
                    "/users/1234/playlists/4" => {
                        delayed_playlist=failures.fetch_add(1, Ordering::AcqRel)>0;
                        (404, json!({"error":{"message":"synthetic-private-provider-text"}}))
                    },
                    "/tracks" => {
                        let form = String::from_utf8(form).unwrap();
                        let mut normalized = track;
                        if form.contains("43") {
                            normalized["id"] = json!(43);
                            normalized["title"] = json!("Fresh synthetic search");
                        } else if form.contains("44") {
                            normalized["id"] = json!(44);
                            normalized["title"] = json!("Stale synthetic search");
                        }
                        (200, json!({"result":[normalized]}))
                    },
                    _ => (404, json!({"error":"synthetic-unknown-route"})),
                };
                if delayed_playlist {
                    let (lock, changed) = &*playlist_gate;
                    let released = lock.lock().unwrap();
                    let (released, timeout) = changed.wait_timeout_while(released, Duration::from_secs(8), |value| !*value).unwrap();
                    assert!(*released && !timeout.timed_out());
                }
                if delayed {
                    let (lock, changed) = &*gate;
                    let released = lock.lock().unwrap();
                    let (released, timeout) = changed.wait_timeout_while(released, Duration::from_secs(8), |value| !*value).unwrap();
                    assert!(*released && !timeout.timed_out(), "synthetic delayed response not released");
                }
                let body = body.to_string();
                let _ = write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
            }));
        }
        for child in children {
            child.join().unwrap();
        }
    });
    let service = MusicService::for_test(&format!("http://{address}/"));
    let token = AccessToken::for_test("synthetic-ui-concurrency-token", Duration::from_secs(60));
    service.account(&token).unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.ConcurrentFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let ui = super::super::build_with_music_pane(
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
    let pane = &ui.music;
    let window = ui.window.clone();
    window.set_title(Some("Karin synthetic HTTP fixture"));
    window.set_default_size(420, 720);
    window.present();
    let this = &pane.controller;
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token);
        model.account_valid = true;
    }
    this.render_controls();
    // This fixture exercises the visible catalog, rather than a compact
    // companion page restored from an independent information-pane preference.
    this.set_info_open(false);
    this.widgets.stack.set_visible_child_name("library");
    this.load_library(0);
    pump_until(|| library_started.load(Ordering::Acquire));
    assert!(this.model.borrow().busy);
    let library_feed = this
        .track_feeds
        .borrow()
        .iter()
        .find(|feed| feed.rows == this.widgets.library_rows)
        .cloned()
        .unwrap();
    pump_until(|| {
        library_feed.data.borrow().loading
            && mapped_loading_spinner(library_feed.status.upcast_ref())
    });
    assert!(
        this.widgets.search_button.is_sensitive(),
        "library loading must not disable search"
    );
    this.widgets.stack.set_visible_child_name("search");
    // Unified four-section search has its own native fixture. Keep this
    // request-lane/latest-query regression explicitly in the track scope;
    // changing query text normally resets that scope, so reselect it below.
    this.widgets.search.set_text("slow");
    this.model.borrow_mut().search_scope = Some(SearchKind::Tracks);
    this.widgets.search.emit_by_name::<()>("activate", &[]);
    pump_until(|| slow_started.load(Ordering::Acquire));
    // Both requests are in flight; the playlist opens immediately anyway.
    let playlist = Playlist {
        id: crate::music::PlaylistId {
            owner: "1234".into(),
            kind: "3".into(),
        },
        title: "Synthetic empty playlist".into(),
        track_count: 0,
        cover: None,
    };
    this.open_playlist(playlist.clone(), 0);
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("detail")
    );
    assert!(contains_text(
        this.widgets.detail.upcast_ref(),
        "Synthetic empty playlist"
    ));
    let loading_playlist = playlist_feed(this, "3");
    pump_until(|| {
        loading_playlist.data.borrow().loading
            && mapped_loading_spinner(loading_playlist.status.upcast_ref())
    });
    assert!(
        this.model.borrow().busy,
        "immediately opened detail still owns the blocked request"
    );
    capture_native("catalog-compact-playlist-loading");
    {
        let (lock, changed) = &*playlist_gate;
        *lock.lock().unwrap() = true;
        changed.notify_all();
    }
    pump_until(|| {
        !loading_playlist.data.borrow().loading
            && loading_playlist.data.borrow().next_page == 1
            && contains_text(this.widgets.detail.upcast_ref(), "Треков пока нет")
    });
    assert!(!mapped_loading_spinner(
        loading_playlist.status.upcast_ref()
    ));
    capture_native("catalog-compact-playlist-empty");
    // Back returns to the initiating search rather than always to library.
    this.widgets
        .detail
        .first_child()
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("search")
    );
    this.widgets.search.set_text("Новый запрос");
    this.model.borrow_mut().search_scope = Some(SearchKind::Tracks);
    this.widgets.search_button.emit_clicked();
    this.widgets.search.set_text("Final Latin query");
    this.model.borrow_mut().search_scope = Some(SearchKind::Tracks);
    this.widgets.search_button.emit_clicked();
    {
        let (lock, changed) = &*gate;
        *lock.lock().unwrap() = true;
        changed.notify_all();
    }
    pump_until(|| !this.model.borrow().busy);
    assert!(contains_text(
        this.widgets.search_rows.upcast_ref(),
        "Fresh synthetic search"
    ));
    assert!(!contains_text(
        this.widgets.search_rows.upcast_ref(),
        "Stale synthetic search"
    ));
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("search")
    );
    assert_eq!(
        *search_queries.lock().unwrap(),
        vec!["slow".to_owned(), "Final Latin query".to_owned()],
        "only the latest pending query reaches the service"
    );
    assert_eq!(
        library_feed.data.borrow().next_page,
        1,
        "the independently blocked library response still completes"
    );
    assert!(!library_feed.data.borrow().loading);
    window.set_default_size(900, 650);
    capture_native("catalog-medium-search");
    let failed = Playlist {
        id: crate::music::PlaylistId {
            owner: "1234".into(),
            kind: "4".into(),
        },
        ..playlist
    };
    this.open_playlist(failed, 0);
    pump_until(|| !this.model.borrow().busy);
    assert!(contains_text(this.widgets.detail.upcast_ref(), "Повторить"));
    assert!(contains_text(
        this.widgets.detail.upcast_ref(),
        "Synthetic empty playlist"
    ));
    assert!(!contains_text(
        pane.widget.upcast_ref(),
        "synthetic-private-provider-text"
    ));
    window.set_default_size(420, 720);
    capture_native("catalog-compact-playlist-error");
    fn retry_button(widget: &gtk::Widget) -> Option<gtk::Button> {
        if let Some(button) = widget.downcast_ref::<gtk::Button>()
            && button.label().as_deref() == Some("Повторить")
        {
            return Some(button.clone());
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            if let Some(button) = retry_button(&widget) {
                return Some(button);
            }
            child = widget.next_sibling();
        }
        None
    }
    let failed_feed = playlist_feed(this, "4");
    assert!(!failed_feed.data.borrow().loading);
    assert_eq!(failed_feed.data.borrow().error, Some(MusicError::Http(404)));
    let retry = retry_button(this.widgets.detail.upcast_ref()).expect("native retry action");
    assert!(retry.grab_focus());
    let keyboard_window = if std::env::var_os("KARIN_TEST_CAPTURE_DIR").is_some() {
        let found = std::process::Command::new("xdotool")
            .args([
                "search",
                "--onlyvisible",
                "--name",
                "^Karin synthetic HTTP fixture$",
            ])
            .output()
            .unwrap();
        assert!(found.status.success());
        let result = String::from_utf8(found.stdout).unwrap();
        let id = result.lines().next().unwrap().to_owned();
        assert!(id.bytes().all(|byte| byte.is_ascii_digit()));
        assert!(
            std::process::Command::new("xdotool")
                .args(["windowfocus", "--sync", &id])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            std::process::Command::new("xdotool")
                .args(["key", "--clearmodifiers", "Tab", "shift+Tab"])
                .status()
                .unwrap()
                .success()
        );
        pump_until(|| retry.has_focus() && window.gets_focus_visible());
        Some(id)
    } else {
        None
    };
    capture_native("catalog-compact-playlist-error-focused");
    let before_retry = failed_requests.load(Ordering::Acquire);
    {
        let (lock, _) = &*playlist_gate;
        *lock.lock().unwrap() = false;
    }
    if keyboard_window.is_some() {
        assert!(
            std::process::Command::new("xdotool")
                .args(["key", "--clearmodifiers", "Return"])
                .status()
                .unwrap()
                .success()
        );
    } else {
        retry.emit_clicked();
    }
    pump_until(|| {
        failed_requests.load(Ordering::Acquire) == before_retry + 1
            && failed_feed.data.borrow().loading
            && mapped_loading_spinner(failed_feed.status.upcast_ref())
    });
    assert_eq!(
        failed_feed.data.borrow().next_page,
        0,
        "a failed first page retries the same cursor"
    );
    capture_native("catalog-compact-playlist-retry-loading");
    {
        let (lock, changed) = &*playlist_gate;
        *lock.lock().unwrap() = true;
        changed.notify_all();
    }
    pump_until(|| {
        failed_requests.load(Ordering::Acquire) == before_retry + 1 && !this.model.borrow().busy
    });
    assert!(!failed_feed.data.borrow().loading);
    assert_eq!(failed_feed.data.borrow().error, Some(MusicError::Http(404)));
    assert!(!mapped_loading_spinner(failed_feed.status.upcast_ref()));
    assert!(contains_text(this.widgets.detail.upcast_ref(), "Повторить"));
    this.logout();
    assert!(this.track_feeds.borrow().is_empty());
    assert!(this.widgets.library_hero.first_child().is_none());
    assert!(!contains_text(
        pane.widget.upcast_ref(),
        "Fresh synthetic search"
    ));
    assert!(!contains_text(
        pane.widget.upcast_ref(),
        "Synthetic empty playlist"
    ));
    stop.store(true, Ordering::Release);
    server.join().unwrap();
    pane.close();
    window.close();
    drop(ui);
    drop(window);
}
