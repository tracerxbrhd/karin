// SPDX-License-Identifier: GPL-3.0-or-later
//! Real input and bounded loopback continuation; no account or service audio.
use super::*;
use crate::ui::music::{
    continuous_loading::Feed,
    test_input,
    tests::{capture_native, contains_text, pump_until},
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Counts {
    all: AtomicUsize,
    batches: AtomicUsize,
    second: AtomicUsize,
    late: AtomicBool,
    offers: AtomicUsize,
}
struct Server {
    stop: Arc<AtomicBool>,
    release: Arc<(Mutex<bool>, Condvar)>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn release(&self) {
        let (lock, changed) = &*self.release;
        *lock.lock().unwrap() = true;
        changed.notify_all();
    }
    fn finish(mut self) {
        self.release();
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.release();
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn wire(id: &str) -> Value {
    let title = if ["42", "1002", "1010", "1055", "1090"].contains(&id) {
        "Ёлка"
    } else {
        "Сосна"
    };
    json!({"id":id,"title":title,"available":true,"artists":[{"id":11,"name":"Тёплая группа"}],"albums":[{"id":7,"title":"Летний альбом"}],"durationMs":180000})
}
fn serve(mut stream: TcpStream, counts: &Counts, release: &(Mutex<bool>, Condvar)) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap() == 0 {
            return;
        }
        bytes.push(byte[0]);
        assert!(bytes.len() < 16 * 1024);
    }
    let request = String::from_utf8(bytes).unwrap();
    let headers = request.to_ascii_lowercase();
    assert!(headers.contains("authorization: oauth synthetic-local-collection-filter-token"));
    assert!(!headers.contains("cookie:") && !headers.contains("x-yandex-music-client:"));
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
    let path = request
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .split('?')
        .next()
        .unwrap();
    counts.all.fetch_add(1, Ordering::AcqRel);
    let (status, value) = match path {
        "/account/status" => (
            200,
            json!({"result":{"account":{"uid":1234,"displayName":"Synthetic filter account"}}}),
        ),
        "/users/1234/likes/tracks" => (
            200,
            json!({"result":{"library":{"uid":1234,"revision":1,"tracks":(1000..1120).map(|id|json!({"id":id})).collect::<Vec<_>>()}}}),
        ),
        "/tracks" => {
            counts.batches.fetch_add(1, Ordering::AcqRel);
            let form = reqwest::Url::parse(&format!(
                "http://127.0.0.1/?{}",
                String::from_utf8(body).unwrap()
            ))
            .unwrap();
            let ids = form
                .query_pairs()
                .filter(|(key, _)| key == "track-ids")
                .map(|(_, id)| id.into_owned())
                .collect::<Vec<_>>();
            assert!(!ids.is_empty() && ids.len() <= 50);
            assert_eq!(
                ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
                ids.len(),
                "the existing service batches distinct identities and restores occurrences"
            );
            if ids.first().map(String::as_str) == Some("1050")
                && counts.second.fetch_add(1, Ordering::AcqRel) == 0
            {
                (
                    503,
                    json!({"error":{"message":"synthetic-private-provider-diagnostic"}}),
                )
            } else {
                if ids.first().map(String::as_str) == Some("1100") {
                    counts.late.store(true, Ordering::Release);
                    let (lock, changed) = release;
                    let ready = lock.lock().unwrap();
                    let (ready, timed) = changed
                        .wait_timeout_while(ready, Duration::from_secs(15), |ready| !*ready)
                        .unwrap();
                    assert!(
                        *ready && !timed.timed_out(),
                        "late fixture read not released"
                    );
                }
                (
                    200,
                    json!({"result":ids.iter().rev().map(|id|wire(id)).collect::<Vec<_>>()}),
                )
            }
        }
        "/users/1234/playlists/7" | "/users/4321/playlists/8" | "/users/4321/playlists/9" => (
            200,
            json!({"result":{"kind":path.rsplit('/').next().unwrap(),"title":"Synthetic repeated playlist","trackCount":4,"tracks":[{"id":42},{"id":43},{"id":42},{"id":44}]}}),
        ),
        "/albums/7/with-tracks" => (
            200,
            json!({"result":{"id":7,"title":"Летний альбом","artists":[{"name":"Тёплая группа"}],"volumes":[[wire("42"),wire("43"),wire("42")]]}}),
        ),
        "/artists/11/tracks" => (
            200,
            json!({"result":{"tracks":[wire("42"),wire("43"),wire("42")],"pager":{"page":0,"perPage":50,"total":3}}}),
        ),
        path if path.starts_with("/tracks/") && path.ends_with("/download-info") => {
            counts.offers.fetch_add(1, Ordering::AcqRel);
            (200, json!({"result":[]}))
        }
        _ => panic!("unexpected synthetic collection-filter route"),
    };
    let body = value.to_string();
    let _ = write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}
fn input(args: &[&str]) {
    assert!(
        std::process::Command::new("xdotool")
            // Xdo decodes multibyte text with the caller's locale. The isolated
            // cloud process may intentionally start in C; keep real Unicode
            // XTest input rather than bypassing GtkEditable with set_text.
            .env("LC_ALL", "C.UTF-8")
            .env("LANG", "C.UTF-8")
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}
fn settle(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    pump_until(|| Instant::now() >= end);
}
fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    test_input::wait_for_layout(window, widget);
    let (x, y) = test_input::pointer(window, widget, test_input::Target::Leading);
    input(&[
        "mousemove",
        "--sync",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "1",
    ]);
    settle(100);
}
fn drain(this: &Rc<Controller>) {
    pump_until(|| !this.music_requested() && !this.model.borrow().busy);
}
fn find_button(widget: &gtk::Widget, name: &str) -> Option<gtk::Button> {
    if widget.widget_name() == name {
        return widget.clone().downcast().ok();
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Some(found) = find_button(&widget, name) {
            return Some(found);
        }
    }
    None
}
fn query(window: &adw::ApplicationWindow, id: &str, feed: &Feed, value: &str) {
    let entry = &feed.filter_ui.as_ref().unwrap().search;
    click(window, id, entry);
    input(&["key", "--clearmodifiers", "ctrl+a"]);
    input(&["key", "--clearmodifiers", "BackSpace"]);
    if !value.is_empty() {
        if value.is_ascii() {
            input(&["type", "--clearmodifiers", value]);
        } else {
            // Exercise GTK's genuine clipboard transfer through a real paste
            // key event. A headless Latin XKB map is not a Russian keyboard;
            // do not replace this native input with GtkEditable::set_text.
            entry.clipboard().set_text(value);
            input(&["key", "--clearmodifiers", "ctrl+v"]);
        }
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    pump_until(|| {
        let actual = entry.text();
        if actual.as_str() == value {
            return true;
        }
        assert!(
            Instant::now() < deadline,
            "synthetic collection input mismatch: expected-codepoints={:?} actual-codepoints={:?}",
            value.chars().map(u32::from).collect::<Vec<_>>(),
            actual.chars().map(u32::from).collect::<Vec<_>>()
        );
        false
    });
    settle(160);
}
fn visible(feed: &Feed) -> Vec<usize> {
    (0..feed.data.borrow().items.len())
        .filter(|index| feed.rows.row_at_index(*index as i32).unwrap().is_visible())
        .collect()
}
fn detail_feed(this: &Rc<Controller>) -> Feed {
    this.track_feeds
        .borrow()
        .iter()
        .find(|feed| feed.rows.is_mapped() && feed.rows != this.widgets.library_rows)
        .cloned()
        .expect("current real detail source")
}
fn liked_marker(feed: &Feed) -> gtk::Widget {
    let body = feed.rows.row_at_index(0).unwrap().child().unwrap();
    let mut child = body.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.widget_name() == "music-loaded-liked-marker" {
            return widget;
        }
    }
    panic!("passive loaded-membership marker must be present")
}
fn playlist(owner: &str, kind: &str) -> Playlist {
    Playlist {
        id: crate::music::PlaylistId {
            owner: owner.into(),
            kind: kind.into(),
        },
        title: "Synthetic repeated playlist".into(),
        track_count: 4,
        cover: None,
    }
}

#[test]
#[ignore = "requires isolated GTK/X11, xdotool and loopback HTTP; no real account"]
fn native_collection_filter_continuation_and_exact_occurrences() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let counts = Arc::new(Counts::default());
    let worker = {
        let stop = stop.clone();
        let release = release.clone();
        let counts = counts.clone();
        thread::spawn(move || {
            let mut children = Vec::new();
            while !stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let counts = counts.clone();
                        let release = release.clone();
                        children.push(thread::spawn(move || serve(stream, &counts, &release)));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => panic!("fixture listener failed"),
                }
            }
            for child in children {
                child.join().unwrap();
            }
        })
    };
    let server = Server {
        stop,
        release,
        worker: Some(worker),
    };
    let origin = format!("http://{address}/");
    let service = MusicService::for_test(&origin);
    let token = AccessToken::for_test(
        "synthetic-local-collection-filter-token",
        Duration::from_secs(300),
    );
    service.account(&token).unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.CollectionFilterFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let ui = crate::ui::build_with_music_pane(
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
    let window = &ui.window;
    window.set_title(Some("Karin synthetic HTTP fixture"));
    window.set_default_size(900, 750);
    window.present();
    let this = &ui.music.controller;
    this.set_fixture_resolver_for_test(crate::stream::StreamResolver::for_test(
        &origin, &origin, &origin,
    ));
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token);
        model.account_valid = true;
    }
    this.render_controls();
    gtk::prelude::WidgetExt::activate_action(window, "win.music-library", None).unwrap();
    pump_until(|| {
        this.track_feeds.borrow().iter().any(|feed| {
            feed.rows == this.widgets.library_rows
                && feed.data.borrow().items.len() == 50
                && !feed.data.borrow().loading
        })
    });
    let liked = this
        .track_feeds
        .borrow()
        .iter()
        .find(|feed| feed.rows == this.widgets.library_rows)
        .unwrap()
        .clone();
    pump_until(|| liked.filter_ui.as_ref().unwrap().search.is_mapped());
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
    let ids = String::from_utf8(found.stdout).unwrap();
    let id = ids.lines().next().unwrap();
    input(&["windowfocus", "--sync", id]);
    assert_eq!(counts.batches.load(Ordering::Acquire), 1);
    assert!(liked_marker(&liked).is_visible());
    drain(this);
    let before = counts.all.load(Ordering::Acquire);
    query(window, id, &liked, "no-match");
    assert!(visible(&liked).is_empty());
    settle(200);
    assert_eq!(
        counts.all.load(Ordering::Acquire),
        before,
        "typing and viewport collapse cannot drain the collection"
    );
    assert!(contains_text(
        liked.filter_ui.as_ref().unwrap().status.upcast_ref(),
        "В загруженной части (50 треков)"
    ));
    capture_native("collection-filter-partial");
    click(window, id, &liked.filter_ui.as_ref().unwrap().more);
    pump_until(|| {
        liked.data.borrow().error == Some(MusicError::Http(503)) && !liked.data.borrow().loading
    });
    assert_eq!(liked.data.borrow().items.len(), 50);
    assert_eq!(counts.batches.load(Ordering::Acquire), 2);
    assert!(!contains_text(
        window.upcast_ref(),
        "synthetic-private-provider-diagnostic"
    ));
    capture_native("collection-filter-retry");
    let retry = find_button(liked.status.upcast_ref(), "music-feed-retry").unwrap();
    click(window, id, &retry);
    pump_until(|| liked.data.borrow().items.len() == 100 && !liked.data.borrow().loading);
    assert_eq!(counts.batches.load(Ordering::Acquire), 3);
    assert!(visible(&liked).is_empty());
    drain(this);
    let before = counts.all.load(Ordering::Acquire);
    query(window, id, &liked, "ёЛКА");
    assert_eq!(visible(&liked), [2, 10, 55, 90]);
    assert_eq!(counts.all.load(Ordering::Acquire), before);
    capture_native("collection-filter-append");
    let row = liked.rows.row_at_index(55).unwrap();
    let late_play = find_button(row.upcast_ref(), "music-track-play").unwrap();
    click(window, id, &row);
    pump_until(|| counts.offers.load(Ordering::Acquire) > 0 && !this.music_requested());
    {
        let model = this.model.borrow();
        let context = model.queue.context().unwrap();
        assert_eq!(context.items().len(), 4);
        assert_eq!(context.current_index(), 2);
        assert_eq!(context.source_position(2), Some(55));
        assert!(context.is_partial());
    }
    drain(this);
    let active = this.model.borrow().queue.context().unwrap().clone();
    let original_serial = this.music_serial();
    let original_offers = counts.offers.load(Ordering::Acquire);
    query(window, id, &liked, "no-match");
    late_play.emit_clicked();
    settle(150);
    assert_eq!(
        this.model.borrow().queue.context(),
        Some(&active),
        "a filtered-out old action must not restart playback"
    );
    assert_eq!(
        this.music_serial(),
        original_serial,
        "a hidden old action cannot restart delivery"
    );
    assert_eq!(counts.offers.load(Ordering::Acquire), original_offers);
    query(window, id, &liked, "");
    assert_eq!(visible(&liked).len(), 100);
    assert_eq!(
        this.model.borrow().queue.context(),
        Some(&active),
        "clearing the filter never changes the active queue"
    );
    assert_eq!(
        this.music_serial(),
        original_serial,
        "clearing a local filter cannot restart delivery"
    );
    assert_eq!(counts.offers.load(Ordering::Acquire), original_offers);
    query(window, id, &liked, "ёЛКА");
    click(window, id, &liked.filter_ui.as_ref().unwrap().more);
    pump_until(|| counts.late.load(Ordering::Acquire));
    this.load_library_refresh(0);
    let fresh_liked = this
        .track_feeds
        .borrow()
        .iter()
        .find(|feed| feed.rows == this.widgets.library_rows)
        .unwrap()
        .clone();
    assert!(!Rc::ptr_eq(&fresh_liked, &liked));
    assert!(
        fresh_liked
            .filter_ui
            .as_ref()
            .unwrap()
            .search
            .text()
            .is_empty()
    );
    this.open_playlist(playlist("1234", "7"), 0);
    server.release();
    pump_until(|| {
        this.track_feeds.borrow().iter().any(|feed| {
            matches!(feed.source, continuous_loading::FeedSource::Playlist(_))
                && feed.data.borrow().items.len() == 4
        })
    });
    drain(this);
    let own = detail_feed(this);
    assert!(own.filter_ui.as_ref().unwrap().search.text().is_empty());
    settle(160);
    assert_eq!(
        liked.data.borrow().items.len(),
        100,
        "an explicitly refreshed source rejects its retired late page"
    );
    query(window, id, &own, "ёЛКА");
    assert_eq!(visible(&own), [0, 2]);
    let row = own.rows.row_at_index(2).unwrap();
    row.grab_focus();
    input(&["key", "--clearmodifiers", "space"]);
    pump_until(|| {
        this.model.borrow().queue.context().is_some_and(|context| {
            context.source_position(context.current_index()) == Some(2)
                && context.items().len() == 2
        })
    });
    drain(this);
    assert!(row.has_css_class("music-current"));
    assert!(
        !own.rows
            .row_at_index(0)
            .unwrap()
            .has_css_class("music-current")
    );
    let first = own.rows.row_at_index(0).unwrap();
    first.grab_focus();
    input(&["key", "--clearmodifiers", "Return"]);
    pump_until(|| {
        this.model.borrow().queue.context().is_some_and(|context| {
            context.source_position(context.current_index()) == Some(0)
                && context.items().len() == 2
        })
    });
    drain(this);
    click(window, id, &row);
    pump_until(|| row.has_css_class("music-current"));
    drain(this);
    let active = this.model.borrow().queue.context().unwrap().clone();
    for (owner, kind) in [("4321", "8"), ("4321", "9")] {
        if kind == "9" {
            this.open_search_playlist(
                crate::music::SearchPlaylist {
                    id: crate::music::PlaylistId {
                        owner: owner.into(),
                        kind: kind.into(),
                    },
                    title: "Synthetic found playlist".into(),
                    track_count: Some(4),
                    cover: None,
                },
                0,
            );
        } else {
            this.open_playlist(playlist(owner, kind), 0);
        }
        pump_until(|| {
            this.track_feeds.borrow().iter().any(|feed| {
                feed.rows.is_mapped()
                    && feed.data.borrow().items.len() == 4
                    && feed.data.borrow().epoch != own.data.borrow().epoch
            })
        });
        let feed = detail_feed(this);
        assert!(feed.filter_ui.as_ref().unwrap().search.text().is_empty());
        drain(this);
        let before = counts.all.load(Ordering::Acquire);
        query(window, id, &feed, "гРУППА аЛЬБОМ");
        assert_eq!(visible(&feed), [0, 1, 2, 3]);
        assert_eq!(counts.all.load(Ordering::Acquire), before);
        assert_eq!(this.model.borrow().queue.context(), Some(&active));
    }
    this.open_album("7".into());
    pump_until(|| {
        this.track_feeds.borrow().iter().any(|feed| {
            matches!(feed.source, continuous_loading::FeedSource::Album(_))
                && feed.data.borrow().items.len() == 3
        })
    });
    let album = detail_feed(this);
    query(window, id, &album, "ёЛКА");
    assert_eq!(visible(&album), [0, 2]);
    assert_eq!(this.model.borrow().queue.context(), Some(&active));
    this.open_artist(
        Artist {
            id: "11".into(),
            name: "Тёплая группа".into(),
            cover: None,
        },
        0,
    );
    pump_until(|| {
        this.track_feeds.borrow().iter().any(|feed| {
            matches!(feed.source, continuous_loading::FeedSource::Artist(_))
                && feed.data.borrow().items.len() == 3
        })
    });
    let artist = detail_feed(this);
    query(window, id, &artist, "ЁлКа");
    assert_eq!(visible(&artist), [0, 2]);
    pump_until(|| fresh_liked.data.borrow().items.len() == 50);
    assert!(liked_marker(&fresh_liked).is_visible());
    this.invalidate_liked_feeds();
    assert!(
        !liked_marker(&fresh_liked).is_visible(),
        "a known membership mutation hides the old snapshot indication"
    );
    assert_eq!(this.model.borrow().queue.context(), Some(&active));
    let retired_filter = fresh_liked.filter_ui.as_ref().unwrap();
    retired_filter.search.set_text("no-match");
    assert!(
        retired_filter
            .status
            .text()
            .contains("В загруженной части (50 треков)")
    );
    assert_ne!(
        retired_filter.status.text().as_str(),
        "Совпадений нет",
        "an invalidated snapshot must not claim complete search coverage"
    );
    drain(this);
    this.logout();
    assert!(this.track_feeds.borrow().is_empty());
    assert!(this.widgets.library_hero.first_child().is_none());
    drain(this);
    let before = counts.all.load(Ordering::Acquire);
    artist.filter_ui.as_ref().unwrap().search.set_text("late");
    late_play.emit_clicked();
    settle(150);
    assert_eq!(counts.all.load(Ordering::Acquire), before);
    assert!(this.model.borrow().queue.items().is_empty());
    ui.music.close();
    window.close();
    server.finish();
}
