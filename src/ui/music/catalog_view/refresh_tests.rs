// SPDX-License-Identifier: GPL-3.0-or-later
//! Header content reads use only original loopback fixtures and one GTK owner.

use super::super::tests::{contains_text, pump_until};
use super::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[derive(Default)]
struct State {
    paths: Vec<String>,
    hold: Option<String>,
    entered: bool,
    released: bool,
    fail: Option<String>,
}
struct Server {
    origin: String,
    state: Arc<(Mutex<State>, Condvar)>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let state = state.clone();
            let stop = stop.clone();
            thread::spawn(move || {
                let mut workers = Vec::new();
                while !stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            assert!(workers.len() < 64, "bounded refresh fixture requests");
                            let state = state.clone();
                            workers.push(thread::spawn(move || serve(stream, &state)));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => panic!("loopback refresh listener failed"),
                    }
                }
                for worker in workers {
                    worker.join().unwrap();
                }
            })
        };
        Self {
            origin,
            state,
            stop,
            worker: Some(worker),
        }
    }
    fn count(&self, path: &str) -> usize {
        self.state
            .0
            .lock()
            .unwrap()
            .paths
            .iter()
            .filter(|p| *p == path)
            .count()
    }
    fn hold(&self, path: &str) {
        let mut state = self.state.0.lock().unwrap();
        state.hold = Some(path.into());
        state.entered = false;
        state.released = false;
    }
    fn release(&self) {
        self.state.0.lock().unwrap().released = true;
        self.state.1.notify_all();
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.release();
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
fn serve(mut stream: TcpStream, state: &(Mutex<State>, Condvar)) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut headers = Vec::new();
    let mut byte = [0];
    while !headers.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            return;
        }
        headers.push(byte[0]);
        assert!(headers.len() < 16 * 1024);
    }
    let request = String::from_utf8(headers).unwrap();
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: oauth synthetic-header-refresh-token")
    );
    assert!(!request.to_ascii_lowercase().contains("cookie:"));
    assert!(
        !request
            .to_ascii_lowercase()
            .contains("x-yandex-music-client:")
    );
    assert!(
        request.starts_with("GET "),
        "refresh performs no service mutation"
    );
    let target = request
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap();
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).unwrap();
    let path = url.path();
    let failed = {
        let mut current = state.0.lock().unwrap();
        current.paths.push(path.into());
        if current.hold.as_deref() == Some(path) {
            current.entered = true;
            state.1.notify_all();
            let (next, timeout) = state
                .1
                .wait_timeout_while(current, Duration::from_secs(8), |s| !s.released)
                .unwrap();
            current = next;
            assert!(
                !timeout.timed_out(),
                "refresh gate released within its budget"
            );
        }
        current.fail.as_deref() == Some(path)
    };
    let (status, value) = if failed {
        (
            503,
            json!({"error":{"message":"synthetic-private-provider-error"}}),
        )
    } else {
        (
            200,
            match path {
                "/account/status" => {
                    json!({"result":{"account":{"uid":1234,"displayName":"Synthetic refresh account"}}})
                }
                "/users/1234/likes/tracks" | "/users/1234/dislikes/tracks" => {
                    json!({"result":{"library":{"uid":1234,"revision":1,"tracks":[]}}})
                }
                "/users/1234/playlists/list" | "/users/1234/likes/playlists" => {
                    json!({"result":[]})
                }
                "/users/1234/playlists/7" => {
                    json!({"result":{"kind":7,"trackCount":0,"tracks":[]}})
                }
                "/albums/7/with-tracks" => {
                    json!({"result":{"id":7,"title":"Synthetic refreshed album","artists":[],"volumes":[[]]}})
                }
                "/search" => {
                    assert!(
                        url.query_pairs()
                            .any(|(key, value)| key == "text" && value == "Synthetic active query")
                    );
                    assert!(
                        url.query_pairs()
                            .any(|(key, value)| key == "type" && value == "track")
                    );
                    json!({"result":{"tracks":{"page":0,"perPage":20,"total":0,"results":[]}}})
                }
                path if crate::ui::music::tests::empty_similar_response(path).is_some() => {
                    crate::ui::music::tests::empty_similar_response(path).unwrap()
                }
                _ => panic!("unexpected refresh fixture endpoint"),
            },
        )
    };
    let body = value.to_string();
    let _ = write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}
fn refresh_button(widget: &gtk::Widget) -> Option<gtk::Button> {
    if let Some(button) = widget.downcast_ref::<gtk::Button>()
        && button.action_name().as_deref() == Some("win.music-refresh")
    {
        return Some(button.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(button) = refresh_button(&current) {
            return Some(button);
        }
        child = current.next_sibling();
    }
    None
}
fn click(window: &adw::ApplicationWindow, button: &gtk::Button) {
    let (x, y) =
        super::super::test_input::pointer(window, button, super::super::test_input::Target::Center);
    let found = std::process::Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic header refresh$",
        ])
        .output()
        .unwrap();
    assert!(found.status.success());
    let ids = String::from_utf8(found.stdout).unwrap();
    let id = ids.lines().next().unwrap();
    // Repeated refresh clicks intentionally target the same pixel. xdotool's
    // mousemove --sync waits for a motion event even when already there; send
    // ordered move/click requests in one client instead of waiting for none.
    assert!(
        std::process::Command::new("xdotool")
            .args([
                "windowfocus",
                "--sync",
                id,
                "mousemove",
                "--window",
                id,
                &x.to_string(),
                &y.to_string(),
                "click",
                "1"
            ])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
#[ignore = "requires isolated D-Bus, GTK display and native pointer input; run under Xvfb"]
fn native_content_refresh_routes_and_coalescing() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let server = Server::new();
    let service = MusicService::for_test(&server.origin);
    let token = AccessToken::for_test("synthetic-header-refresh-token", Duration::from_secs(120));
    service.account(&token).unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.HeaderRefreshFixture")
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
    ui.window.set_title(Some("Karin synthetic header refresh"));
    ui.window.set_default_size(900, 650);
    ui.window.present();
    let this = &ui.music.controller;
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token.clone());
        model.account_valid = true;
        model.library_touched = true;
        model.liked = Some(Err(MusicError::Network));
        model.playlists = Some(Err(MusicError::Network));
        model
            .queue
            .append(Track {
                id: "42".into(),
                title: "Synthetic retained queue track".into(),
                artists: Vec::new(),
                artist_links: Vec::new(),
                album_id: None,
                album_title: None,
                duration_ms: None,
                cover: None,
                explicit: false,
                available: true,
            })
            .unwrap();
    }
    this.render_controls();
    this.widgets.stack.set_visible_child_name("home");
    let button =
        refresh_button(ui.window.upcast_ref()).expect("one HeaderBar refresh action button");
    let action = ui
        .window
        .lookup_action("music-refresh")
        .and_downcast::<gio::SimpleAction>()
        .unwrap();
    pump_until(|| button.is_mapped() && button.is_sensitive());
    server.hold("/users/1234/likes/tracks");
    click(&ui.window, &button);
    pump_until(|| server.state.0.lock().unwrap().entered);
    assert!(!action.is_enabled() && !button.is_sensitive());
    assert_eq!(
        action.state().and_then(|value| value.get::<bool>()),
        Some(true)
    );
    let indicator = button.child().and_downcast::<gtk::Stack>().unwrap();
    pump_until(|| {
        indicator
            .visible_child()
            .and_downcast::<gtk::Spinner>()
            .is_some_and(|spinner| spinner.is_mapped() && spinner.is_spinning())
    });
    for _ in 0..8 {
        button.emit_clicked();
        this.refresh_catalog();
    }
    assert_eq!(server.count("/users/1234/likes/tracks"), 1);
    server.release();
    pump_until(|| !this.model.borrow().busy);
    assert_eq!(server.count("/users/1234/likes/tracks"), 1);
    assert_eq!(server.count("/users/1234/playlists/list"), 1);
    assert!(this.model.borrow().liked.as_ref().unwrap().is_ok());
    assert!(this.model.borrow().playlists.as_ref().unwrap().is_ok());
    assert_eq!(
        action.state().and_then(|value| value.get::<bool>()),
        Some(false)
    );
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("home")
    );

    for (page, path) in [
        ("library", "/users/1234/likes/tracks"),
        ("playlists", "/users/1234/playlists/list"),
    ] {
        this.widgets.stack.set_visible_child_name(page);
        pump_until(|| !this.model.borrow().busy);
        let before = server.count(path);
        click(&ui.window, &button);
        pump_until(|| !this.model.borrow().busy && server.count(path) == before + 1);
        assert_eq!(
            this.widgets.stack.visible_child_name().as_deref(),
            Some(page)
        );
    }
    assert_eq!(server.count("/users/1234/likes/playlists"), 1);
    this.open_album("7".into());
    pump_until(|| !this.model.borrow().busy);
    let pending_source = crate::stream::DeliveryCancellation::new();
    this.model.borrow_mut().delivery_cancellation = Some(pending_source.clone());
    let origin = this.model.borrow().detail_origin.clone();
    let before = server.count("/albums/7/with-tracks");
    click(&ui.window, &button);
    pump_until(|| !this.model.borrow().busy && server.count("/albums/7/with-tracks") == before + 1);
    assert!(!pending_source.cancelled(&token));
    assert_eq!(this.model.borrow().detail_origin, origin);
    assert!(contains_text(
        this.widgets.detail.upcast_ref(),
        "Synthetic refreshed album"
    ));
    this.open_playlist(
        Playlist {
            id: crate::music::PlaylistId {
                owner: "1234".into(),
                kind: "7".into(),
            },
            title: "Synthetic current playlist".into(),
            track_count: 0,
            cover: None,
        },
        0,
    );
    pump_until(|| !this.model.borrow().busy);
    let before = server.count("/users/1234/playlists/7");
    click(&ui.window, &button);
    pump_until(|| {
        !this.model.borrow().busy && server.count("/users/1234/playlists/7") == before + 1
    });
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("detail")
    );

    this.open_global_search();
    this.widgets.search.set_text("Synthetic active query");
    this.model.borrow_mut().search_scope = Some(SearchKind::Tracks);
    this.search(0);
    pump_until(|| !this.model.borrow().busy);
    assert!(this.widgets.search.grab_focus());
    let before = server.count("/search");
    click(&ui.window, &button);
    pump_until(|| !this.model.borrow().busy && server.count("/search") == before + 1);
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("search")
    );
    assert_eq!(this.widgets.search.text(), "Synthetic active query");
    assert_eq!(this.model.borrow().search_scope, Some(SearchKind::Tracks));
    assert!(
        gtk::prelude::GtkWindowExt::focus(&ui.window).is_some_and(|focus| focus
            == *this.widgets.search.upcast_ref::<gtk::Widget>()
            || focus.is_ancestor(&this.widgets.search))
    );
    server.state.0.lock().unwrap().fail = Some("/search".into());
    click(&ui.window, &button);
    pump_until(|| {
        !this.model.borrow().busy
            && contains_text(
                this.widgets.search_feedback.upcast_ref(),
                "Не удалось загрузить следующие треки.",
            )
    });
    assert!(action.is_enabled());
    assert!(!contains_text(
        ui.window.upcast_ref(),
        "synthetic-private-provider-error"
    ));
    assert!(this.model.borrow().account_valid && !token.is_expired());
    assert_eq!(
        server.count("/account/status"),
        1,
        "refresh keeps the admitted session"
    );
    assert_eq!(this.model.borrow().queue.items().len(), 1);
    assert_eq!(this.model.borrow().queue.items()[0].id, "42");
    this.close();
    ui.window.close();
}
