// SPDX-License-Identifier: GPL-3.0-or-later
//! A playlist refresh survives independent Liked reads and confirmed writes.

use super::*;
use crate::ui::music::tests::{contains_text, pump_until};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
};

#[derive(Default)]
struct Counts {
    all: AtomicUsize,
    accounts: AtomicUsize,
    membership: AtomicUsize,
    owned: AtomicUsize,
    liked_playlists: AtomicUsize,
    posts: AtomicUsize,
}

struct Server {
    stop: Arc<AtomicBool>,
    release: Arc<(Mutex<bool>, Condvar)>,
    thread: Option<thread::JoinHandle<()>>,
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
        self.thread.take().unwrap().join().unwrap();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.release();
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn playlist(kind: u64, liked: bool) -> Playlist {
    Playlist {
        id: crate::music::PlaylistId {
            owner: if liked { "4321" } else { "1234" }.into(),
            kind: kind.to_string(),
        },
        title: format!("Synthetic retirement playlist {kind}"),
        track_count: 0,
        cover: None,
    }
}

fn wire_playlists(liked: bool) -> serde_json::Value {
    let first = if liked { 101 } else { 1 };
    let values = (first..first + 51)
        .map(|kind| {
            let value = playlist(kind, liked);
            let wire = json!({
                "kind":kind,
                "title":value.title,
                "trackCount":0,
                "owner":{"uid":value.id.owner},
            });
            if liked {
                json!({"playlist":wire})
            } else {
                wire
            }
        })
        .collect::<Vec<_>>();
    json!({"result":values})
}

fn serve(
    mut stream: TcpStream,
    counts: Arc<Counts>,
    liked: Arc<AtomicBool>,
    release: Arc<(Mutex<bool>, Condvar)>,
) {
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
    assert!(headers.contains("authorization: oauth synthetic-refresh-like-token"));
    assert!(!headers.contains("cookie:"));
    assert!(!headers.contains("x-yandex-music-client:"));
    let length = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .map_or(0, |value| value.trim().parse::<usize>().unwrap());
    assert!(length < 1024);
    let mut form = vec![0; length];
    stream.read_exact(&mut form).unwrap();
    let mut start = request.lines().next().unwrap().split_whitespace();
    let method = start.next().unwrap();
    let path = start.next().unwrap().split('?').next().unwrap();
    counts.all.fetch_add(1, Ordering::AcqRel);
    let response = match (method, path) {
        ("GET", "/account/status") => {
            counts.accounts.fetch_add(1, Ordering::AcqRel);
            json!({"result":{"account":{"uid":1234,"displayName":"Synthetic refresh account"}}})
        }
        ("GET", "/users/1234/likes/tracks") => {
            counts.membership.fetch_add(1, Ordering::AcqRel);
            let liked = liked.load(Ordering::Acquire);
            json!({"result":{"library":{
                "uid":1234,
                "revision":if liked {2}else{1},
                "tracks":if liked {vec![json!({"id":42})]}else{Vec::new()},
            }}})
        }
        ("POST", "/users/1234/likes/tracks/add-multiple") => {
            assert_eq!(form, b"track-ids=42");
            counts.posts.fetch_add(1, Ordering::AcqRel);
            liked.store(true, Ordering::Release);
            json!({"result":{"revision":2}})
        }
        ("GET", "/users/1234/playlists/list") => {
            counts.owned.fetch_add(1, Ordering::AcqRel);
            let (lock, changed) = &*release;
            let ready = lock.lock().unwrap();
            let (ready, timeout) = changed
                .wait_timeout_while(ready, Duration::from_secs(20), |ready| !*ready)
                .unwrap();
            assert!(
                *ready && !timeout.timed_out(),
                "refresh gate was not released"
            );
            drop(ready);
            wire_playlists(false)
        }
        ("GET", "/users/1234/likes/playlists") => {
            counts.liked_playlists.fetch_add(1, Ordering::AcqRel);
            wire_playlists(true)
        }
        _ => panic!("unexpected synthetic refresh/Like route"),
    };
    let body = response.to_string();
    write!(
        stream,
        "HTTP/1.1 200 Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn button_named(widget: &gtk::Widget, name: &str) -> Option<gtk::Button> {
    if widget.widget_name() == name {
        return widget.clone().downcast().ok();
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Some(button) = button_named(&widget, name) {
            return Some(button);
        }
    }
    None
}

fn active_spinners(widget: &gtk::Widget) -> usize {
    let own = widget
        .downcast_ref::<gtk::Spinner>()
        .is_some_and(|spinner| spinner.is_spinning());
    let mut count = usize::from(own);
    let mut child = widget.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        count += active_spinners(&widget);
    }
    count
}

/// Run from the existing session/catalog selector after its other isolated UI
/// fixtures. Every input and response here is synthetic and loopback-only.
pub(super) fn exercise_refresh_like_independence() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let counts = Arc::new(Counts::default());
    let liked = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let server_thread = {
        let counts = counts.clone();
        let liked = liked.clone();
        let stop = stop.clone();
        let release = release.clone();
        thread::spawn(move || {
            let mut workers = Vec::new();
            while !stop.load(Ordering::Acquire) {
                let (stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("synthetic refresh listener failed: {error}"),
                };
                let counts = counts.clone();
                let liked = liked.clone();
                let release = release.clone();
                workers.push(thread::spawn(move || serve(stream, counts, liked, release)));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        })
    };
    let server = Server {
        stop,
        release,
        thread: Some(server_thread),
    };
    let service = MusicService::for_test(&format!("http://{address}/"));
    let token = AccessToken::for_test("synthetic-refresh-like-token", Duration::from_secs(120));
    service.account(&token).unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.RefreshLikeFixture")
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
    let this = &ui.music.controller;
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token);
        model.account_valid = true;
        model.liked = Some(Ok(Page {
            items: Vec::new(),
            has_more: false,
        }));
        model.playlists = Some(Ok(Page {
            items: (9001..9052).map(|kind| playlist(kind, false)).collect(),
            has_more: false,
        }));
        model.liked_playlists = Some(Ok(Page {
            items: (9101..9152).map(|kind| playlist(kind, true)).collect(),
            has_more: false,
        }));
    }
    this.playlist_pages.set([1, 1]);
    this.render_controls();
    this.set_info_open(false);
    this.widgets.library_mode.set_selected(1);
    let controls = this.like_controls("42".into());
    ui.music.widget.prepend(&controls);
    let like = button_named(controls.upcast_ref(), "music-like-toggle").unwrap();
    ui.window.set_default_size(1440, 900);
    ui.window.present();
    pump_until(|| counts.membership.load(Ordering::Acquire) == 1 && like.is_sensitive());
    assert!(!contains_text(controls.upcast_ref(), "В любимом"));
    this.load_playlist_sections(true);
    pump_until(|| counts.owned.load(Ordering::Acquire) == 1);
    assert_eq!(this.playlist_refreshing.get(), [true, true]);
    assert_eq!(counts.liked_playlists.load(Ordering::Acquire), 0);

    // The old shared Library lane would queue this unrelated Liked read behind
    // the blocked playlist response, then cancel that refresh's UI callback.
    this.widgets.library_mode.set_selected(0);
    this.load_library_refresh(0);
    pump_until(|| {
        counts.membership.load(Ordering::Acquire) == 2
            && this.track_feeds.borrow().iter().any(|feed| {
                feed.rows == this.widgets.library_rows
                    && !feed.data.borrow().loading
                    && feed.data.borrow().next_page == 1
            })
    });
    assert_eq!(counts.owned.load(Ordering::Acquire), 1);
    assert_eq!(this.playlist_refreshing.get(), [true, true]);

    // This is the same confirmed production Mutation path used by the direct
    // footer action. No audio or fake playback intent is needed for the race.
    like.emit_clicked();
    pump_until(|| {
        counts.posts.load(Ordering::Acquire) == 1
            && !this.mutation_busy.get()
            && like.is_sensitive()
    });
    assert!(contains_text(controls.upcast_ref(), "В любимом"));
    assert_eq!(counts.membership.load(Ordering::Acquire), 3);
    assert_eq!(this.playlist_refreshing.get(), [true, true]);
    assert!(this.model.borrow().account_valid);
    {
        let model = this.model.borrow();
        let original = model.playlists.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(original.items.len(), 51);
        assert_eq!(original.items[0].id.kind, "9001");
    }
    server.release();
    pump_until(|| !this.model.borrow().busy);
    assert_eq!(this.playlist_refreshing.get(), [false, false]);
    assert_eq!(this.playlist_pages.get(), [0, 0]);
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
            assert_eq!(page.items.len(), 50);
            assert!(
                page.has_more,
                "51 provider records are not a false end at 50"
            );
            let actual = page
                .items
                .iter()
                .map(|item| (&item.id.owner, item.id.kind.parse::<u64>().unwrap()))
                .collect::<Vec<_>>();
            let expected = (first..first + 50)
                .map(|kind| (owner, kind))
                .collect::<Vec<_>>();
            assert_eq!(
                actual
                    .iter()
                    .map(|(owner, kind)| (owner.as_str(), *kind))
                    .collect::<Vec<_>>(),
                expected
            );
        }
        assert!(model.account_valid);
        assert!(model.queue.items().is_empty());
    }
    this.widgets.library_mode.set_selected(1);
    assert_eq!(active_spinners(this.playlist_grid.widget.upcast_ref()), 0);
    assert_eq!(
        active_spinners(this.liked_playlist_grid.widget.upcast_ref()),
        0
    );
    assert_eq!(counts.accounts.load(Ordering::Acquire), 1);
    assert_eq!(counts.owned.load(Ordering::Acquire), 1);
    assert_eq!(counts.liked_playlists.load(Ordering::Acquire), 1);
    assert_eq!(counts.posts.load(Ordering::Acquire), 1);
    assert_eq!(counts.all.load(Ordering::Acquire), 7);
    ui.music.close();
    ui.window.close();
    server.finish();
}
