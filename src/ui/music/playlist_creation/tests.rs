// SPDX-License-Identifier: GPL-3.0-or-later
//! Native input and bounded loopback creation fixtures.

use super::super::tests::{capture_native, pump_until};
use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[test]
fn creation_form_validation_and_messages_do_not_enable_unsafe_replay() {
    assert!(valid_title("  синтетический плейлист  "));
    for title in [
        "",
        "   ",
        "synthetic\nvalue",
        "synthetic\0",
        &"x".repeat(201),
    ] {
        assert!(!valid_title(title));
    }
    assert!(creation_message(InteractionError::WriteOutcomeUnknown).contains("Обновите"));
    assert!(
        creation_message(InteractionError::Music(MusicError::Forbidden))
            .contains("Остальная библиотека")
    );
    for error in [
        InteractionError::WriteOutcomeUnknown,
        InteractionError::ConfirmationUnavailable,
        InteractionError::ConfirmationMismatch,
    ] {
        let message = creation_message(error);
        assert!(!message.contains("HTTP"));
        assert!(!message.contains("POST"));
    }
}

struct Fixture {
    origin: String,
    mode: Arc<AtomicUsize>,
    posts: Arc<AtomicUsize>,
    list_reads: Arc<AtomicUsize>,
    hold_confirmation: Arc<AtomicBool>,
    confirming: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        let mode = Arc::new(AtomicUsize::new(0));
        let posts = Arc::new(AtomicUsize::new(0));
        let list_reads = Arc::new(AtomicUsize::new(0));
        let hold_confirmation = Arc::new(AtomicBool::new(false));
        let confirming = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let server = {
            let mode = mode.clone();
            let posts = posts.clone();
            let list_reads = list_reads.clone();
            let hold = hold_confirmation.clone();
            let confirming = confirming.clone();
            let stop = stop.clone();
            thread::spawn(move || {
                let records = Mutex::new(Vec::<serde_json::Value>::new());
                while !stop.load(Ordering::Acquire) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(v) => v,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(3));
                            continue;
                        }
                        Err(_) => panic!("synthetic creation listener failed"),
                    };
                    let request = read_request(&mut stream);
                    let first = request.lines().next().unwrap();
                    let target = first.split_whitespace().nth(1).unwrap();
                    let path = target.split('?').next().unwrap();
                    assert!(
                        request
                            .to_ascii_lowercase()
                            .contains("authorization: oauth synthetic-native-playlist-token\r\n")
                    );
                    assert!(
                        !request
                            .to_ascii_lowercase()
                            .contains("x-yandex-music-client")
                    );
                    let (status, body) = if path == "/account/status" {
                        (200,serde_json::json!({"result":{"account":{"uid":1234,"displayName":"Synthetic creation account"}}}).to_string())
                    } else if path == "/users/1234/playlists/create" && first.starts_with("POST ") {
                        let index = posts.fetch_add(1, Ordering::AcqRel);
                        let wire_mode = mode.load(Ordering::Acquire);
                        if wire_mode == 2 {
                            (403, r#"{"error":{"name":"FORBIDDEN"}}"#.into())
                        } else {
                            let data = request.split("\r\n\r\n").nth(1).unwrap();
                            let form =
                                reqwest::Url::parse(&format!("http://localhost/?{data}")).unwrap();
                            let value = |name: &str| {
                                form.query_pairs()
                                    .find(|(key, _)| key == name)
                                    .unwrap()
                                    .1
                                    .into_owned()
                            };
                            let playlist = serde_json::json!({"owner":{"uid":1234},"kind":17+index,"title":value("title"),"trackCount":0,"visibility":value("visibility")});
                            records.lock().unwrap().push(playlist.clone());
                            if wire_mode == 1 {
                                (
                                    500,
                                    r#"{"error":{"message":"synthetic lost reply"}}"#.into(),
                                )
                            } else {
                                (200, serde_json::json!({"result":playlist}).to_string())
                            }
                        }
                    } else if path == "/users/1234/playlists/list" {
                        list_reads.fetch_add(1, Ordering::AcqRel);
                        (
                            200,
                            serde_json::json!({"result":*records.lock().unwrap()}).to_string(),
                        )
                    } else if let Some(kind) = path
                        .strip_prefix("/users/1234/playlists/")
                        .and_then(|tail| tail.parse::<usize>().ok())
                    {
                        confirming.store(true, Ordering::Release);
                        let deadline = Instant::now() + Duration::from_secs(5);
                        while hold.load(Ordering::Acquire)
                            && !stop.load(Ordering::Acquire)
                            && Instant::now() < deadline
                        {
                            thread::sleep(Duration::from_millis(2));
                        }
                        let data = records
                            .lock()
                            .unwrap()
                            .iter()
                            .find(|item| item["kind"].as_u64() == Some(kind as u64))
                            .cloned();
                        (200, serde_json::json!({"result":data}).to_string())
                    } else if path == "/users/1234/likes/tracks" {
                        (200,serde_json::json!({"result":{"library":{"uid":1234,"revision":1,"tracks":[]}}}).to_string())
                    } else if path == "/users/1234/likes/playlists" {
                        (200, r#"{"result":[]}"#.into())
                    } else {
                        (404, r#"{"error":{"name":"NOT_FOUND"}}"#.into())
                    };
                    write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                }
            })
        };
        Self {
            origin,
            mode,
            posts,
            list_reads,
            hold_confirmation,
            confirming,
            stop,
            server: Some(server),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.hold_confirmation.store(false, Ordering::Release);
        self.stop.store(true, Ordering::Release);
        if let Some(server) = self.server.take() {
            server.join().unwrap();
        }
    }
}
fn read_request(stream: &mut TcpStream) -> String {
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
    let header = String::from_utf8(bytes.clone()).unwrap();
    let size = header
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|s| s.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(size <= 4096);
    let mut body = vec![0; size];
    stream.read_exact(&mut body).unwrap();
    bytes.extend_from_slice(&body);
    String::from_utf8(bytes).unwrap()
}
fn input(args: &[&str]) {
    assert!(
        std::process::Command::new("timeout")
            .args(["5", "xdotool"])
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}
fn wait_phase(phase: &str, condition: impl FnMut() -> bool) {
    // Test-only fixed phases and synthetic geometry. Never log the form value,
    // HTTP data, token, account metadata or any URL even in this local fixture.
    eprintln!("Karin synthetic creation wait: {phase}");
    pump_until(condition);
    eprintln!("Karin synthetic creation ready: {phase}");
}
fn descendants(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut widgets = vec![root.clone()];
    let mut child = root.first_child();
    while let Some(node) = child {
        widgets.extend(descendants(&node));
        child = node.next_sibling();
    }
    widgets
}
fn named(dialog: &adw::Dialog, name: &str) -> gtk::Widget {
    descendants(dialog.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == name)
        .expect("native creation control")
}
fn dialog(this: &Rc<Controller>) -> adw::Dialog {
    wait_phase("dialog mapped", || {
        this.playlist_dialog
            .borrow()
            .as_ref()
            .is_some_and(|dialog| dialog.is_mapped())
    });
    let dialog = this.playlist_dialog.borrow().as_ref().unwrap().clone();
    let window = this.window.upgrade().unwrap();
    let cancel = named(&dialog, "music-create-playlist-cancel");
    // Mapping precedes the native layout/focus frame. Input must not depend
    // on capture_native() happening to settle a theme/opening transition.
    super::super::test_input::wait_for_layout(&window, &cancel);
    eprintln!("Karin synthetic creation ready: dialog native controls");
    dialog
}
fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    super::super::test_input::wait_for_layout(window, widget);
    let (x, y) =
        super::super::test_input::pointer(window, widget, super::super::test_input::Target::Center);
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "1",
    ]);
}
fn title(window: &adw::ApplicationWindow, id: &str, dialog: &adw::Dialog, value: &str) {
    let entry = named(dialog, "music-create-playlist-title")
        .downcast::<adw::EntryRow>()
        .unwrap();
    click(window, id, &entry);
    input(&["key", "--clearmodifiers", "ctrl+a"]);
    input(&["type", "--clearmodifiers", "--delay", "1", value]);
    wait_phase("synthetic title entered", || entry.text() == value);
}

#[test]
#[ignore = "requires isolated Xvfb, xdotool and local HTTP; no real account or service"]
fn native_playlist_creation_confirmation_refresh_refusal_and_late_widgets() {
    gtk::init().unwrap();
    adw::init().unwrap();
    if let Some(settings) = gtk::Settings::default() {
        // The capture lane already requests reduced motion. Explicitly use
        // the same deterministic input setup in MSRV/no-capture lanes rather
        // than relying on an optional screenshot's side effects.
        settings.set_gtk_enable_animations(false);
    }
    let fixture = Fixture::new();
    let service = MusicService::for_test(&fixture.origin);
    let token = AccessToken::for_test("synthetic-native-playlist-token", Duration::from_secs(300));
    service.account(&token).unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.PlaylistCreationFixture")
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
    let window = &ui.window;
    window.set_title(Some("Karin synthetic HTTP fixture"));
    window.set_default_size(900, 650);
    let this = &ui.music.controller;
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token.clone());
        model.account_valid = true;
        model.playlists = Some(Ok(Page {
            items: Vec::new(),
            has_more: false,
        }));
        model
            .queue
            .append(Track {
                id: "42".into(),
                title: "Synthetic preserved track".into(),
                artists: vec!["Synthetic artist".into()],
                artist_links: Vec::new(),
                album_id: None,
                album_title: None,
                duration_ms: Some(123_000),
                cover: None,
                explicit: false,
                available: true,
            })
            .unwrap();
    }
    this.widgets.stack.set_visible_child_name("playlists");
    this.widgets.library_mode.set_selected(1);
    this.render_controls();
    this.render_queue();
    this.render_music_player();
    window.present();
    wait_phase("main window mapped", || window.is_mapped());
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
    let found = String::from_utf8(found.stdout).unwrap();
    let id = found.lines().next().unwrap();
    input(&["windowfocus", "--sync", id]);
    let generation = this.model.borrow().generation;
    for (shape, width, height) in [
        ("compact", 420, 720),
        ("medium", 900, 650),
        ("wide", 1440, 900),
    ] {
        window.set_default_size(width, height);
        wait_phase(&format!("{shape} surface allocation"), || {
            window
                .surface()
                .is_some_and(|s| s.width() == width && s.height() == height)
        });
        this.present_create_playlist();
        let current = dialog(this);
        let submit = named(&current, "music-create-playlist-submit")
            .downcast::<gtk::Button>()
            .unwrap();
        let cancel = named(&current, "music-create-playlist-cancel")
            .downcast::<gtk::Button>()
            .unwrap();
        let privacy = named(&current, "music-create-playlist-visibility")
            .downcast::<adw::ComboRow>()
            .unwrap();
        assert_eq!(privacy.selected(), 0);
        assert!(!submit.is_sensitive());
        assert_eq!(current.default_widget(), Some(cancel.clone().upcast()));
        capture_native(&format!("playlist-create-{shape}"));
        if shape == "compact" {
            input(&["key", "--clearmodifiers", "Escape"]);
        } else {
            click(window, id, &cancel);
        }
        wait_phase(&format!("{shape} cancelled dialog closed"), || {
            this.playlist_dialog.borrow().is_none()
        });
        submit.emit_clicked();
        assert_eq!(fixture.posts.load(Ordering::Acquire), 0);
        assert!(!token.is_expired());
        assert_eq!(this.model.borrow().queue.items().len(), 1);
    }
    window.set_default_size(900, 650);
    wait_phase("confirmation window allocation", || {
        window
            .surface()
            .is_some_and(|s| s.width() == 900 && s.height() == 650)
    });
    fixture.hold_confirmation.store(true, Ordering::Release);
    this.present_create_playlist();
    let current = dialog(this);
    title(window, id, &current, "Confirmed synthetic playlist");
    let submit = named(&current, "music-create-playlist-submit")
        .downcast::<gtk::Button>()
        .unwrap();
    wait_phase("valid title enables Create", || submit.is_sensitive());
    let play_sensitive = this.widgets.play.is_sensitive();
    assert!(submit.grab_focus());
    input(&["key", "--clearmodifiers", "space"]);
    wait_phase("server confirmation requested", || {
        fixture.confirming.load(Ordering::Acquire)
    });
    assert!(this.playlist_dialog.borrow().is_some());
    assert!(!submit.is_sensitive());
    assert_eq!(this.widgets.play.is_sensitive(), play_sensitive);
    assert_eq!(this.model.borrow().queue.items().len(), 1);
    assert_eq!(this.model.borrow().generation, generation);
    fixture.hold_confirmation.store(false, Ordering::Release);
    wait_phase("confirmed creation closes dialog", || {
        this.playlist_dialog.borrow().is_none() && !this.mutation_busy.get()
    });
    assert_eq!(fixture.posts.load(Ordering::Acquire), 1);
    assert!(!token.is_expired());
    fixture.mode.store(1, Ordering::Release);
    this.present_create_playlist();
    let uncertain = dialog(this);
    title(window, id, &uncertain, "Uncertain synthetic playlist");
    let submit = named(&uncertain, "music-create-playlist-submit")
        .downcast::<gtk::Button>()
        .unwrap();
    let refresh = named(&uncertain, "music-create-playlist-refresh")
        .downcast::<gtk::Button>()
        .unwrap();
    click(window, id, &submit);
    wait_phase("ambiguous write offers GET-only refresh", || {
        !this.mutation_busy.get() && refresh.is_visible()
    });
    assert!(!submit.is_sensitive());
    capture_native("playlist-create-uncertain");
    click(window, id, &submit);
    assert_eq!(fixture.posts.load(Ordering::Acquire), 2);
    let reads = fixture.list_reads.load(Ordering::Acquire);
    click(window, id, &refresh);
    wait_phase("GET-only refresh closes dialog and reads library", || {
        this.playlist_dialog.borrow().is_none()
            && fixture.list_reads.load(Ordering::Acquire) > reads
    });
    assert_eq!(fixture.posts.load(Ordering::Acquire), 2);
    wait_phase("refreshed library exposes Create tile", || {
        descendants(this.playlist_grid.tiles.upcast_ref())
            .iter()
            .any(|widget| widget.widget_name() == "music-playlist-create")
    });
    let old_create_tile = descendants(this.playlist_grid.tiles.upcast_ref())
        .into_iter()
        .find(|widget| widget.widget_name() == "music-playlist-create")
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    fixture.mode.store(2, Ordering::Release);
    this.present_create_playlist();
    let rejected = dialog(this);
    title(window, id, &rejected, "Rejected synthetic playlist");
    let submit = named(&rejected, "music-create-playlist-submit")
        .downcast::<gtk::Button>()
        .unwrap();
    click(window, id, &submit);
    wait_phase("permission refusal disables Create capability", || {
        !this.mutation_busy.get() && !this.can_create_playlist.get()
    });
    assert!(!submit.is_sensitive());
    assert!(!token.is_expired());
    assert!(this.model.borrow().account_valid);
    assert_eq!(this.model.borrow().queue.items().len(), 1);
    assert!(
        !descendants(this.playlist_grid.tiles.upcast_ref())
            .iter()
            .any(|widget| widget.widget_name() == "music-playlist-create")
    );
    capture_native("playlist-create-refused");
    click(
        window,
        id,
        &named(&rejected, "music-create-playlist-cancel"),
    );
    wait_phase("refused dialog cancelled", || {
        this.playlist_dialog.borrow().is_none()
    });
    this.present_create_playlist();
    assert!(this.playlist_dialog.borrow().is_none());
    old_create_tile.emit_clicked();
    assert!(this.playlist_dialog.borrow().is_none());
    submit.emit_clicked();
    assert_eq!(fixture.posts.load(Ordering::Acquire), 3);
    ui.music.close();
    window.close();
}
