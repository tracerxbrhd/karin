// SPDX-License-Identifier: GPL-3.0-or-later
//! Both visible playlist continuation actions must keep their own admission.

use super::*;
use crate::ui::music::{test_input, tests::pump_until};
use serde_json::json;
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

struct FixtureServer {
    stop: Arc<AtomicBool>,
    released: Arc<(Mutex<bool>, Condvar)>,
    thread: Option<thread::JoinHandle<()>>,
}

impl FixtureServer {
    fn release(&self) {
        let (lock, changed) = &*self.released;
        *lock.lock().unwrap() = true;
        changed.notify_all();
    }

    fn finish(mut self) {
        self.release();
        self.stop.store(true, Ordering::Release);
        self.thread.take().unwrap().join().unwrap();
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        // Release blocked loopback requests even when the parallelism assertion
        // fails. Do not leave a worker behind or replace the original panic.
        self.release();
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn owned_playlist(kind: u64, liked: bool) -> Playlist {
    Playlist {
        id: crate::music::PlaylistId {
            owner: if liked { "4321" } else { "1234" }.into(),
            kind: kind.to_string(),
        },
        title: format!(
            "Synthetic {} playlist {kind}",
            if liked { "liked" } else { "owned" }
        ),
        track_count: 0,
        cover: None,
    }
}

fn wire_playlist(kind: u64, liked: bool) -> serde_json::Value {
    let playlist = owned_playlist(kind, liked);
    json!({
        "kind":kind,
        "title":playlist.title,
        "trackCount":0,
        "owner":{"uid":playlist.id.owner},
    })
}

fn respond(mut stream: TcpStream, value: serde_json::Value) {
    let body = value.to_string();
    write!(
        stream,
        "HTTP/1.1 200 Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut found = vec![widget.clone()];
    let mut child = widget.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        found.extend(descendants(&widget));
    }
    found
}

fn more_button(widget: &gtk::Widget) -> gtk::Button {
    descendants(widget)
        .into_iter()
        .find(|widget| widget.widget_name() == "music-playlist-more")
        .and_then(|widget| widget.downcast().ok())
        .expect("a real rendered playlist continuation button")
}

fn input(arguments: &[&str]) {
    assert!(
        std::process::Command::new("xdotool")
            .args(arguments)
            .status()
            .expect("native playlist continuation fixture requires xdotool")
            .success()
    );
}

fn click_more(
    window: &adw::ApplicationWindow,
    scroll: &gtk::ScrolledWindow,
    window_id: &str,
    button: &gtk::Button,
) {
    // Keyboard focus can belong to a still-offscreen child. A mouse user
    // reaches either end of the long collection with actual wheel gestures;
    // never teleport the adjustment or count an allocation as visible input.
    test_input::wait_for_layout(window, button);
    let mut exposed = false;
    for _ in 0..80 {
        let bounds = button.compute_bounds(window).expect("mapped continuation");
        let viewport = scroll
            .compute_bounds(window)
            .expect("mapped scroll viewport");
        if bounds.y() >= viewport.y() + 4.0
            && bounds.y() + bounds.height() <= viewport.y() + viewport.height() - 4.0
        {
            exposed = true;
            break;
        }
        let direction = if bounds.y() < viewport.y() + 4.0 {
            "4"
        } else {
            "5"
        };
        let (x, y) = test_input::pointer(window, scroll, test_input::Target::Center);
        input(&[
            "mousemove",
            "--window",
            window_id,
            &x.to_string(),
            &y.to_string(),
            "click",
            "--repeat",
            "3",
            "--delay",
            "15",
            direction,
        ]);
        let settled = Instant::now() + Duration::from_millis(70);
        pump_until(|| Instant::now() >= settled);
    }
    assert!(
        exposed,
        "actual wheel input must expose the continuation target"
    );
    let (x, y) = test_input::pointer(window, button, test_input::Target::Center);
    let scale = f64::from(window.scale_factor());
    let (dx, dy) = window.surface_transform();
    let picked = window.pick(
        f64::from(x) / scale - dx,
        f64::from(y) / scale - dy,
        gtk::PickFlags::DEFAULT,
    );
    assert!(
        picked.as_ref().is_some_and(
            |picked| picked == button.upcast_ref::<gtk::Widget>() || picked.is_ancestor(button)
        ),
        "the native continuation target is not covered by another surface"
    );
    input(&[
        "mousemove",
        "--window",
        window_id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "1",
    ]);
    pump_until(|| !button.is_sensitive());
}

/// Called by the existing session/catalog native selector, keeping this
/// regression in the complete executable registration/CI partition.
pub(super) fn exercise_parallel_more() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let released = Arc::new((Mutex::new(false), Condvar::new()));
    let requests = Arc::new(AtomicUsize::new(0));
    let owned_requests = Arc::new(AtomicUsize::new(0));
    let liked_requests = Arc::new(AtomicUsize::new(0));
    let server_thread = {
        let stop = stop.clone();
        let released = released.clone();
        let requests = requests.clone();
        let owned_requests = owned_requests.clone();
        let liked_requests = liked_requests.clone();
        thread::spawn(move || {
            let mut workers = Vec::new();
            while !stop.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("synthetic playlist listener failed: {error}"),
                };
                let released = released.clone();
                let requests = requests.clone();
                let owned_requests = owned_requests.clone();
                let liked_requests = liked_requests.clone();
                workers.push(thread::spawn(move || {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    let mut byte = [0];
                    while !bytes.ends_with(b"\r\n\r\n") {
                        assert_eq!(stream.read(&mut byte).unwrap(), 1);
                        bytes.push(byte[0]);
                        assert!(bytes.len() < 16 * 1024);
                    }
                    let request = String::from_utf8(bytes).unwrap();
                    let headers = request.to_ascii_lowercase();
                    assert!(headers.contains(
                        "authorization: oauth synthetic-parallel-playlists-token"
                    ));
                    assert!(!headers.contains("cookie:"));
                    assert!(!headers.contains("x-yandex-music-client:"));
                    requests.fetch_add(1, Ordering::AcqRel);
                    if request.starts_with("GET /account/status ") {
                        respond(
                            stream,
                            json!({"result":{"account":{"uid":1234,"displayName":"Synthetic parallel playlist account"}}}),
                        );
                        return;
                    }
                    let liked = if request.starts_with("GET /users/1234/playlists/list ") {
                        owned_requests.fetch_add(1, Ordering::AcqRel);
                        false
                    } else if request.starts_with("GET /users/1234/likes/playlists ") {
                        liked_requests.fetch_add(1, Ordering::AcqRel);
                        true
                    } else {
                        panic!("unexpected synthetic parallel-playlist route");
                    };
                    let (lock, changed) = &*released;
                    let ready = lock.lock().unwrap();
                    let (ready, timeout) = changed
                        .wait_timeout_while(ready, Duration::from_secs(12), |ready| !*ready)
                        .unwrap();
                    assert!(*ready && !timeout.timed_out(), "both independent reads must be released");
                    drop(ready);
                    let first = if liked { 101 } else { 1 };
                    let playlists = (first..first + 51)
                        .map(|kind| {
                            let value = wire_playlist(kind, liked);
                            if liked {
                                json!({"playlist":value})
                            } else {
                                value
                            }
                        })
                        .collect::<Vec<_>>();
                    respond(stream, json!({"result":playlists}));
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        })
    };
    let server = FixtureServer {
        stop,
        released,
        thread: Some(server_thread),
    };
    // The first response is deliberately held while a real user scrolls past
    // both 50-tile collections. A two-second unit-fixture deadline would test
    // mouse traversal speed instead of independent request admission. Keep the
    // production-equivalent finite 15s client budget and the strict 12s server
    // gate; all one-read, simultaneous, ordering and stale-control guards stay.
    let service = MusicService::for_native_input_test(&format!("http://{address}/"));
    let token = AccessToken::for_test(
        "synthetic-parallel-playlists-token",
        Duration::from_secs(120),
    );
    service.account(&token).unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.ParallelPlaylistFixture")
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
    let this = &ui.music.controller;
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token);
        model.account_valid = true;
        model.playlists = Some(Ok(Page {
            items: (1..51).map(|kind| owned_playlist(kind, false)).collect(),
            has_more: true,
        }));
        model.liked_playlists = Some(Ok(Page {
            items: (101..151).map(|kind| owned_playlist(kind, true)).collect(),
            has_more: true,
        }));
    }
    this.render_controls();
    this.set_info_open(false);
    this.playlist_pages.set([0, 0]);
    this.widgets.library_mode.set_selected(1);
    this.widgets.stack.set_visible_child_name("playlists");
    this.render_playlist_section(false);
    this.render_playlist_section(true);
    window.set_title(Some("Karin synthetic parallel playlist continuations"));
    window.set_default_size(1440, 900);
    window.present();
    pump_until(|| this.playlist_grid.widget.is_mapped());
    let found = std::process::Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic parallel playlist continuations$",
        ])
        .output()
        .unwrap();
    assert!(found.status.success());
    let found = String::from_utf8(found.stdout).unwrap();
    let window_id = found.lines().next().unwrap();
    assert!(window_id.bytes().all(|byte| byte.is_ascii_digit()));
    input(&["windowfocus", "--sync", window_id]);
    let scroll = this.content_scroll.borrow().upgrade().unwrap();
    let owned_more = more_button(this.playlist_grid.widget.upcast_ref());
    let liked_more = more_button(this.liked_playlist_grid.widget.upcast_ref());
    click_more(window, &scroll, window_id, &owned_more);
    pump_until(|| owned_requests.load(Ordering::Acquire) == 1);
    click_more(window, &scroll, window_id, &liked_more);
    // On the old common Library lane, the second HTTP request cannot start
    // until the blocked first one finishes, and its callback would be lost.
    pump_until(|| {
        owned_requests.load(Ordering::Acquire) == 1 && liked_requests.load(Ordering::Acquire) == 1
    });
    assert!(this.model.borrow().busy);
    assert_eq!(requests.load(Ordering::Acquire), 3);
    server.release();
    pump_until(|| !this.model.borrow().busy);
    {
        let model = this.model.borrow();
        for (page, first, owner) in [
            (
                model.playlists.as_ref().unwrap().as_ref().unwrap(),
                1,
                "1234",
            ),
            (
                model.liked_playlists.as_ref().unwrap().as_ref().unwrap(),
                101,
                "4321",
            ),
        ] {
            assert_eq!(page.items.len(), 51);
            assert!(!page.has_more);
            let actual = page
                .items
                .iter()
                .map(|playlist| (playlist.id.owner.clone(), playlist.id.kind.clone()))
                .collect::<Vec<_>>();
            let expected = (first..first + 51)
                .map(|kind| (owner.to_owned(), kind.to_string()))
                .collect::<Vec<_>>();
            assert_eq!(
                actual, expected,
                "each source retains its ordered prefix exactly once"
            );
        }
    }
    assert_eq!(this.playlist_pages.get(), [1, 1]);
    for grid in [&this.playlist_grid, &this.liked_playlist_grid] {
        let widgets = descendants(grid.widget.upcast_ref());
        assert!(
            !widgets
                .iter()
                .any(|widget| widget.widget_name() == "music-playlist-more")
        );
        assert_eq!(
            widgets
                .iter()
                .filter(|widget| widget.widget_name() == "music-playlist-tile")
                .count(),
            51
        );
    }
    assert!(owned_more.parent().is_none() && liked_more.parent().is_none());
    // Pump a bounded quiet interval to catch accidental continuation replay.
    let quiet = Instant::now() + Duration::from_millis(120);
    pump_until(|| Instant::now() >= quiet);
    assert_eq!(requests.load(Ordering::Acquire), 3);
    assert!(this.model.borrow().queue.items().is_empty());
    // An explicit reset retires both old button epochs before admitting page
    // zero. Late callbacks from the removed native controls cannot race it or
    // append an old prefix while the replacement metadata is still loading.
    let previous_epochs = this.playlist_epochs.get();
    this.load_playlist_sections(true);
    assert_eq!(this.playlist_refreshing.get(), [true, true]);
    assert!(
        this.playlist_epochs
            .get()
            .iter()
            .zip(previous_epochs)
            .all(|(next, old)| *next != old)
    );
    owned_more.emit_clicked();
    liked_more.emit_clicked();
    for grid in [&this.playlist_grid, &this.liked_playlist_grid] {
        assert!(
            !descendants(grid.widget.upcast_ref())
                .iter()
                .any(|widget| widget.widget_name() == "music-playlist-more")
        );
    }
    pump_until(|| !this.model.borrow().busy);
    assert_eq!(requests.load(Ordering::Acquire), 5);
    assert_eq!(owned_requests.load(Ordering::Acquire), 2);
    assert_eq!(liked_requests.load(Ordering::Acquire), 2);
    assert_eq!(this.playlist_pages.get(), [0, 0]);
    assert_eq!(this.playlist_refreshing.get(), [false, false]);
    {
        let model = this.model.borrow();
        for result in [&model.playlists, &model.liked_playlists] {
            let page = result.as_ref().unwrap().as_ref().unwrap();
            assert_eq!(page.items.len(), 50);
            assert!(page.has_more);
        }
    }
    for grid in [&this.playlist_grid, &this.liked_playlist_grid] {
        assert!(more_button(grid.widget.upcast_ref()).is_sensitive());
    }
    ui.music.close();
    window.close();
    server.finish();
}
