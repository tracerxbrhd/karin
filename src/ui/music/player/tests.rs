// SPDX-License-Identifier: GPL-3.0-or-later
//! Original synthetic HTTP media; this is not a real Music/Fedora sound test.

use super::super::tests::{capture_native, contains_text, pump_until};
use super::*;
use gst::prelude::*;
use gstreamer as gst;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::PathBuf,
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
struct Counts {
    offers: [usize; 4],
    descriptors: [usize; 4],
    media: [usize; 4],
    range_requests: usize,
    slow_started: bool,
}

struct Fixture {
    origin: String,
    counts: Arc<Mutex<Counts>>,
    reject_media: Arc<AtomicBool>,
    release: Arc<(Mutex<bool>, Condvar)>,
    stop: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<()>>,
}

impl Fixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let counts = Arc::new(Mutex::new(Counts::default()));
        let reject_media = Arc::new(AtomicBool::new(false));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let media = Arc::new(generated_mp3());
        let server = {
            let origin = origin.clone();
            let counts = counts.clone();
            let reject_media = reject_media.clone();
            let release = release.clone();
            let stop = stop.clone();
            thread::spawn(move || {
                let mut children = Vec::new();
                while !stop.load(Ordering::Acquire) {
                    let (stream, _) = match listener.accept() {
                        Ok(value) => value,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => panic!("synthetic player listener failed"),
                    };
                    assert!(children.len() < 128, "unbounded synthetic media requests");
                    let origin = origin.clone();
                    let counts = counts.clone();
                    let reject_media = reject_media.clone();
                    let release = release.clone();
                    let media = media.clone();
                    children.push(thread::spawn(move || {
                        serve(stream, &origin, &counts, &release, &media, &reject_media)
                    }));
                }
                for child in children {
                    child.join().unwrap();
                }
            })
        };
        Self {
            origin,
            counts,
            reject_media,
            release,
            stop,
            server: Some(server),
        }
    }

    fn release_descriptor(&self) {
        *self.release.0.lock().unwrap() = true;
        self.release.1.notify_all();
    }

    fn hold_descriptor(&self) {
        self.counts.lock().unwrap().slow_started = false;
        *self.release.0.lock().unwrap() = false;
    }

    fn offers(&self, track: usize) -> usize {
        self.counts.lock().unwrap().offers[track]
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.release_descriptor();
        self.stop.store(true, Ordering::Release);
        if let Some(server) = self.server.take() {
            server.join().unwrap();
        }
    }
}

fn serve(
    mut stream: TcpStream,
    origin: &str,
    counts: &Mutex<Counts>,
    release: &(Mutex<bool>, Condvar),
    media: &[u8],
    reject_media: &AtomicBool,
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            return;
        }
        bytes.push(byte[0]);
        assert!(bytes.len() < 16 * 1024);
    }
    let request = String::from_utf8(bytes).unwrap();
    let first = request
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    assert!(matches!(first[0], "GET" | "HEAD"));
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{}", first[1])).unwrap();
    let header = |name: &str| {
        request.lines().skip(1).find_map(|line| {
            let (field, value) = line.split_once(':')?;
            field.eq_ignore_ascii_case(name).then_some(value.trim())
        })
    };
    let api = url.path() == "/account/status" || url.path().starts_with("/tracks/");
    if api {
        assert_eq!(
            header("authorization"),
            Some("OAuth synthetic-native-music-token")
        );
    } else {
        for forbidden in ["authorization", "cookie", "origin", "referer"] {
            assert!(
                header(forbidden).is_none(),
                "credential reached synthetic media transport"
            );
        }
    }
    assert!(header("x-yandex-music-client").is_none());
    if url.path() == "/account/status" {
        json_response(
            &mut stream,
            json!({"result":{"account":{"uid":1234,"displayName":"Synthetic player account"}}}),
        );
    } else if url.path().starts_with("/tracks/") {
        let track: usize = url.path().split('/').nth(2).unwrap().parse().unwrap();
        assert!((42..=45).contains(&track));
        counts.lock().unwrap().offers[track - 42] += 1;
        json_response(
            &mut stream,
            json!({"result":[{"codec":"mp3","bitrateInKbps":192,"preview":false,"direct":false,
            "downloadInfoUrl":format!("{origin}download-info/{track}")}]}),
        );
    } else if let Some(track) = url.path().strip_prefix("/download-info/") {
        let track: usize = track.parse().unwrap();
        assert!((42..=45).contains(&track));
        {
            let mut state = counts.lock().unwrap();
            state.descriptors[track - 42] += 1;
            if track == 45 {
                state.slow_started = true;
            }
        }
        if track == 45 {
            let released = release.0.lock().unwrap();
            let (released, _) = release
                .1
                .wait_timeout_while(released, Duration::from_secs(8), |ready| !*ready)
                .unwrap();
            assert!(*released, "synthetic cancellation gate was not released");
        }
        let xml = format!(
            "<download-info><host>s268myt.storage.yandex.net</host><path>/fixture-{track}.mp3</path><ts>1700000000</ts><s>synthetic-server-input</s></download-info>"
        );
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{xml}",
            xml.len()
        );
    } else if url.path().starts_with("/get-mp3/") {
        let track = (42..=45)
            .find(|track| url.path().ends_with(&format!("/fixture-{track}.mp3")))
            .unwrap();
        let data = media;
        let range = header("range").map(|value| {
            let value = value.strip_prefix("bytes=").unwrap();
            let (start, end) = value.split_once('-').unwrap();
            let start = start.parse::<usize>().unwrap();
            let end = if end.is_empty() {
                data.len() - 1
            } else {
                end.parse::<usize>().unwrap().min(data.len() - 1)
            };
            assert!(start <= end && end < data.len());
            (start, end)
        });
        {
            let mut state = counts.lock().unwrap();
            state.media[track - 42] += 1;
            if range.is_some() {
                state.range_requests += 1;
            }
        }
        if reject_media.load(Ordering::Acquire) {
            let body = "synthetic-http-private-body synthetic-response-secret";
            let _ = write!(
                stream,
                "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain\r\nX-Synthetic-Private-Header: synthetic-http-private-header\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if first[0] != "HEAD" {
                let _ = stream.write_all(body.as_bytes());
            }
            return;
        }
        let (start, end) = range.unwrap_or((0, data.len() - 1));
        let status = if range.is_some() {
            "206 Partial Content"
        } else {
            "200 OK"
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: audio/mpeg\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\nConnection: close\r\n",
            end - start + 1
        );
        if range.is_some() {
            let _ = write!(
                stream,
                "Content-Range: bytes {start}-{end}/{}\r\n",
                data.len()
            );
        }
        let _ = stream.write_all(b"\r\n");
        if first[0] != "HEAD" {
            let _ = stream.write_all(&data[start..=end]);
        }
    } else {
        panic!("unexpected synthetic player request");
    }
}

fn json_response(stream: &mut TcpStream, body: serde_json::Value) {
    let body = body.to_string();
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

struct GeneratedDirectory(PathBuf);

impl Drop for GeneratedDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn generated_mp3() -> Vec<u8> {
    // Original four-second signal, encoded by the installed normal MP3 plugin.
    // A private temporary encoding file is removed before HTTP playback; these
    // bytes never contain service music or establish real Music audibility.
    gst::init().unwrap();
    let directory = std::env::temp_dir().join(format!(
        "karin-original-ui-mp3-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let directory = GeneratedDirectory(directory);
    let path = directory.0.join("original-synthetic-signal.mp3");
    std::fs::write(&path, []).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let pipeline = gst::Pipeline::new();
    let source = gst::ElementFactory::make("audiotestsrc")
        .property("num-buffers", 400_i32)
        .property("samplesperbuffer", 441_i32)
        .property("volume", 0.03_f64)
        .build()
        .unwrap();
    let convert = gst::ElementFactory::make("audioconvert").build().unwrap();
    let filter = gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("audio/x-raw")
                .field("rate", 44_100_i32)
                .field("channels", 1_i32)
                .build(),
        )
        .build()
        .unwrap();
    let encoder = gst::ElementFactory::make("lamemp3enc")
        .property("bitrate", 128_i32)
        .property("cbr", true)
        .build()
        .expect("the normal MP3 encoder must be installed for the audio fixture");
    encoder.set_property_from_str("target", "bitrate");
    let sink = gst::ElementFactory::make("filesink")
        .property("location", path.to_str().unwrap())
        .property("sync", false)
        .build()
        .unwrap();
    pipeline
        .add_many([&source, &convert, &filter, &encoder, &sink])
        .unwrap();
    gst::Element::link_many([&source, &convert, &filter, &encoder, &sink]).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let completed = pipeline
        .bus()
        .unwrap()
        .timed_pop_filtered(
            gst::ClockTime::from_seconds(5),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        )
        .is_some_and(|message| matches!(message.view(), gst::MessageView::Eos(_)));
    let _ = pipeline.set_state(gst::State::Null);
    assert!(completed, "original synthetic MP3 encoding completed");
    let data = std::fs::read(path).unwrap();
    assert!(data.len() > 1_000 && data.len() < 1024 * 1024);
    data
}

fn track(id: &str) -> Track {
    Track {
        id: id.into(),
        title: "Synthetic remote player track".into(),
        artists: vec!["Synthetic artist".into()],
        album_id: None,
        album_title: None,
        duration_ms: Some(4_000),
        cover: None,
        explicit: false,
        available: true,
    }
}

fn wait_playing(this: &Controller, index: usize) {
    pump_until(|| {
        this.model.borrow().queue.current_index() == Some(index)
            && this
                .music_snapshot()
                .is_some_and(|snapshot| snapshot.state == PlaybackState::Playing)
            && this.widgets.player_status.text() == "Воспроизведение"
    });
}

fn change_range(range: &gtk::Scale, value: f64) {
    let _: bool = range.emit_by_name("change-value", &[&gtk::ScrollType::Jump, &value]);
}

fn resize_player_window(ui: &crate::ui::MainWindow, width: i32, height: i32) {
    ui.window.set_default_size(width, height);
    // The GDK surface is the application rectangle captured by X11 tools.
    // GtkWindow's widget allocation excludes its client-side shadow margins.
    let surface = ui.window.surface().expect("native player window is mapped");
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(8);
    while surface.width() != width || surface.height() != height {
        if Instant::now() >= deadline {
            let widgets = &ui.music.controller.widgets;
            let top = widgets.play.parent().unwrap();
            let bar = top.parent().unwrap();
            eprintln!(
                "synthetic resize target={width}x{height} surface={}x{} widget={}x{} default={}x{}",
                surface.width(),
                surface.height(),
                ui.window.width(),
                ui.window.height(),
                ui.window.default_width(),
                ui.window.default_height()
            );
            for (name, widget) in [
                ("window", ui.window.upcast_ref::<gtk::Widget>()),
                ("error-revealer", widgets.status_revealer.upcast_ref()),
                ("error-row", widgets.status.upcast_ref()),
                ("player-bar", &bar),
                ("player-top", &top),
                ("player-title", widgets.current.upcast_ref()),
                ("player-caption", widgets.player_status.upcast_ref()),
                ("player-progress", widgets.progress.upcast_ref()),
                ("player-time", widgets.position.upcast_ref()),
                ("player-volume", widgets.volume.upcast_ref()),
            ] {
                let (min_width, natural_width, _, _) =
                    widget.measure(gtk::Orientation::Horizontal, -1);
                let (min_height, natural_height, _, _) =
                    widget.measure(gtk::Orientation::Vertical, width);
                eprintln!(
                    "synthetic resize component={name} allocated={}x{} minimum={min_width}x{min_height} natural={natural_width}x{natural_height}",
                    widget.width(),
                    widget.height()
                );
            }
            panic!("synthetic player did not reach the required window size");
        }
        if context.pending() {
            context.iteration(false);
        } else {
            thread::sleep(Duration::from_millis(5));
        }
    }
}

fn player_window(fixture: &Fixture) -> (adw::Application, crate::ui::MainWindow) {
    static NEXT_WINDOW: AtomicU64 = AtomicU64::new(0);
    // Each registered application exports its own D-Bus object path, even
    // with NON_UNIQUE. Keep independent windows distinct on one test bus.
    let application_id = format!(
        "io.github.tracerxbrhd.Karin.NativeMusicPlaybackFixture.Window{}",
        NEXT_WINDOW.fetch_add(1, Ordering::Relaxed)
    );
    let service = MusicService::for_test(&fixture.origin);
    let token = AccessToken::for_test("synthetic-native-music-token", Duration::from_secs(300));
    service.account(&token).unwrap();
    let app = adw::Application::builder()
        .application_id(application_id)
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
    let pane = &ui.music;
    let this = &pane.controller;
    ui.window.set_title(Some("Karin synthetic HTTP fixture"));
    ui.window.set_default_size(900, 650);
    ui.window.present();
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token);
        model.account_valid = true;
    }
    {
        let mut player = this.player.borrow_mut();
        player.output = PlaybackOutput::Fakesink;
        player.fixture_resolver = Some(StreamResolver::for_test(
            &fixture.origin,
            &fixture.origin,
            &fixture.origin,
        ));
    }
    this.render_controls();
    (app, ui)
}

fn remove_queue_row(this: &Controller, index: i32) {
    let row = this.widgets.queue_rows.row_at_index(index).unwrap();
    let mut descendants = vec![row.upcast::<gtk::Widget>()];
    while let Some(widget) = descendants.pop() {
        if let Some(button) = widget.downcast_ref::<gtk::Button>()
            && button.icon_name().as_deref() == Some("list-remove-symbolic")
        {
            button.emit_clicked();
            return;
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            descendants.push(widget);
        }
    }
    panic!("native queue remove button is missing");
}

#[test]
#[ignore = "requires an isolated GTK display and GStreamer HTTP/MP3 plugins; run under Xvfb"]
fn native_music_playback_resolver_queue_controls_and_logout() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let fixture = Fixture::new();
    let (_app, ui) = player_window(&fixture);
    let pane = &ui.music;
    let this = &pane.controller;
    for id in ["42", "43", "44"] {
        this.enqueue(track(id));
    }
    this.widgets.stack.set_visible_child_name("queue");
    assert!(this.widgets.play.is_sensitive());
    assert!(!this.widgets.progress.is_sensitive());
    assert!(!this.widgets.position.text().contains("0:00"));
    this.widgets.play.emit_clicked();
    wait_playing(this, 0);
    assert!(contains_text(
        this.widgets.player_status.upcast_ref(),
        "Воспроизведение"
    ));
    assert!(this.widgets.stop.is_sensitive());
    this.widgets.play.emit_clicked();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
    });
    pump_until(|| this.widgets.progress.is_sensitive());
    change_range(&this.widgets.volume, 0.23);
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| (snapshot.volume - 0.23).abs() < 0.001)
    });
    change_range(&this.widgets.progress, 1.0);
    pump_until(|| {
        this.music_snapshot()
            .and_then(|snapshot| snapshot.position)
            .is_some_and(|position| {
                position >= Duration::from_millis(900) && position < Duration::from_millis(1200)
            })
    });
    assert!(fixture.counts.lock().unwrap().range_requests > 0);
    capture_native("music-player-paused-controls");
    this.widgets.play.emit_clicked();
    wait_playing(this, 0);
    let old = this.player.borrow().controller.clone().unwrap();
    let offers = fixture.offers(0);
    this.widgets.stop.emit_clicked();
    assert!(this.player.borrow().controller.is_none());
    pump_until(|| !old.snapshot().worker_running);
    assert!(!this.widgets.progress.is_sensitive());
    this.widgets.play.emit_clicked();
    wait_playing(this, 0);
    assert_eq!(
        fixture.offers(0),
        offers + 1,
        "Stop/restart must resolve a fresh source"
    );

    this.widgets.next.emit_clicked();
    wait_playing(this, 1);
    this.widgets.previous.emit_clicked();
    wait_playing(this, 0);
    this.widgets.next.emit_clicked();
    wait_playing(this, 1);
    pump_until(|| this.widgets.progress.is_sensitive());
    change_range(&this.widgets.progress, 3.8);
    wait_playing(this, 2);
    let settled = Instant::now() + Duration::from_millis(180);
    pump_until(|| Instant::now() >= settled);
    assert_eq!(fixture.offers(2), 1, "EOS must advance exactly once");
    assert_eq!(this.model.borrow().queue.current_index(), Some(2));
    let old = this.player.borrow().controller.clone().unwrap();
    this.widgets.stop.emit_clicked();
    pump_until(|| !old.snapshot().worker_running);

    // A completed slow descriptor after Stop must not prepare or play media.
    this.enqueue(track("45"));
    this.select_music_queue(3);
    fixture.hold_descriptor();
    this.widgets.play.emit_clicked();
    pump_until(|| fixture.counts.lock().unwrap().slow_started);
    assert!(this.player.borrow().requested);
    assert!(
        !this
            .widgets
            .player_status
            .text()
            .contains("Воспроизведение")
    );
    this.widgets.stop.emit_clicked();
    fixture.release_descriptor();
    pump_until(|| !this.player.borrow().resolving);
    assert!(this.player.borrow().controller.is_none());
    assert_eq!(fixture.counts.lock().unwrap().media[3], 0);

    // The same late response must also be harmless after session teardown.
    fixture.hold_descriptor();
    this.widgets.play.emit_clicked();
    pump_until(|| fixture.counts.lock().unwrap().slow_started);
    this.widgets.logout.emit_clicked();
    fixture.release_descriptor();
    pump_until(|| !this.player.borrow().resolving);
    assert!(this.player.borrow().controller.is_none());
    assert!(this.model.borrow().token.is_none());
    assert!(this.model.borrow().queue.items().is_empty());
    assert!(!this.widgets.play.is_sensitive());
    assert_eq!(fixture.counts.lock().unwrap().media[3], 0);
    for value in [
        "synthetic-native-music-token",
        "127.0.0.1",
        "/get-mp3/",
        "synthetic-server-input",
    ] {
        assert!(!contains_text(ui.window.upcast_ref(), value));
    }
    pane.close();
    ui.window.close();
}

#[test]
#[ignore = "requires an isolated GTK display and GStreamer HTTP/MP3 plugins; run under Xvfb"]
fn native_music_playback_queue_races_remove_and_close() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let fixture = Fixture::new();
    let (_app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    for id in ["42", "43", "44", "45"] {
        this.enqueue(track(id));
    }
    this.widgets.stack.set_visible_child_name("queue");

    // The already-running request is held after its offer, before any media.
    // Two synchronous queue actions must retain only the latest pending intent.
    fixture.hold_descriptor();
    this.select_music_queue(3);
    this.widgets.play.emit_clicked();
    pump_until(|| fixture.counts.lock().unwrap().slow_started);
    let offers_42 = fixture.offers(0);
    let offers_43 = fixture.offers(1);
    this.select_music_queue(0);
    this.widgets.next.emit_clicked();
    assert_eq!(this.model.borrow().queue.current_index(), Some(1));
    assert_eq!(this.player.borrow().track.as_deref(), Some("43"));
    assert!(this.player.borrow().requested);
    assert!(this.player.borrow().controller.is_none());
    fixture.release_descriptor();
    wait_playing(this, 1);
    assert_eq!(
        fixture.offers(0),
        offers_42,
        "superseded selection must not resolve"
    );
    assert_eq!(
        fixture.offers(1),
        offers_43 + 1,
        "latest selection resolves once"
    );
    assert_eq!(fixture.counts.lock().unwrap().media[3], 0);
    assert_eq!(this.player.borrow().track.as_deref(), Some("43"));

    // Removing an earlier, non-current row changes its index but keeps the
    // exact live controller: a subsequent volume command reaches our old clone.
    let unchanged = this.player.borrow().controller.clone().unwrap();
    let generation = unchanged.snapshot().generation;
    let offers_43 = fixture.offers(1);
    remove_queue_row(this, 0);
    wait_playing(this, 0);
    change_range(&this.widgets.volume, 0.31);
    pump_until(|| (unchanged.snapshot().volume - 0.31).abs() < 0.001);
    assert!(unchanged.snapshot().worker_running);
    assert_eq!(unchanged.snapshot().generation, generation);
    assert_eq!(this.music_snapshot().unwrap().generation, generation);
    assert_eq!(fixture.offers(1), offers_43);
    assert_eq!(this.player.borrow().track.as_deref(), Some("43"));

    // Removing the playing row must retire its native worker and resolve the
    // replacement; merely relabeling the old stream would fail these checks.
    let removed = this.player.borrow().controller.clone().unwrap();
    let offers_44 = fixture.offers(2);
    remove_queue_row(this, 0);
    pump_until(|| !removed.snapshot().worker_running);
    wait_playing(this, 0);
    assert_eq!(this.model.borrow().queue.current().unwrap().id, "44");
    assert_eq!(this.player.borrow().track.as_deref(), Some("44"));
    assert_eq!(fixture.offers(2), offers_44 + 1);

    // Exercise the real window close-request hook while a pipeline is active.
    let closing = this.player.borrow().controller.clone().unwrap();
    ui.window.close();
    assert!(this.model.borrow().closed);
    assert!(this.model.borrow().token.is_none());
    assert!(this.model.borrow().queue.items().is_empty());
    assert!(this.player.borrow().controller.is_none());
    pump_until(|| !closing.snapshot().worker_running);
    assert!(!this.player.borrow().requested);

    // Closing permanently invalidates this session/window, so pending-close
    // uses a second, independently constructed native window and service.
    let pending_fixture = Fixture::new();
    let (_pending_app, pending_ui) = player_window(&pending_fixture);
    let pending = &pending_ui.music.controller;
    pending.enqueue(track("45"));
    pending_fixture.hold_descriptor();
    pending.widgets.play.emit_clicked();
    pump_until(|| pending_fixture.counts.lock().unwrap().slow_started);
    assert!(pending.player.borrow().requested);
    assert!(pending.player.borrow().controller.is_none());
    pending_ui.window.close();
    assert!(pending.model.borrow().closed);
    pending_fixture.release_descriptor();
    pump_until(|| !pending.player.borrow().resolving);
    assert!(pending.player.borrow().controller.is_none());
    assert!(!pending.player.borrow().requested);
    assert!(pending.player.borrow().pending.is_none());
    assert!(pending.player.borrow().track.is_none());
    assert!(pending.model.borrow().token.is_none());
    assert!(pending.model.borrow().queue.items().is_empty());
    assert_eq!(pending_fixture.counts.lock().unwrap().media[3], 0);
}

fn wait_media_forbidden(this: &Controller) {
    pump_until(|| {
        this.music_snapshot().is_some_and(|snapshot| {
            snapshot.state == PlaybackState::Error
                && snapshot.last_error == Some(PlaybackError::RemoteHttp(403))
        }) && this.widgets.status.title() == "Сервер аудио отклонил запрос"
            && this
                .widgets
                .status
                .subtitle()
                .is_some_and(|subtitle| subtitle.starts_with("HTTP 403."))
            && this.widgets.status_revealer.reveals_child()
            && this.widgets.status_revealer.is_child_revealed()
            && this.widgets.status_revealer.is_mapped()
            && this.widgets.status.is_mapped()
    });
}

#[test]
#[ignore = "requires an isolated GTK display and GStreamer HTTP/MP3 plugins; run under Xvfb"]
fn native_music_playback_media_error_visibility_stop_retry_and_catalog_preservation() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let fixture = Fixture::new();
    let (_app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    this.enqueue(track("42"));
    this.widgets.stack.set_visible_child_name("queue");

    fixture.reject_media.store(true, Ordering::Release);
    this.widgets.play.emit_clicked();
    wait_media_forbidden(this);
    assert!(this.widgets.play.is_sensitive());
    assert!(this.widgets.stop.is_sensitive());
    assert!(!this.widgets.progress.is_sensitive());
    for sensitive in [
        "synthetic-http-private-body",
        "synthetic-http-private-header",
        "synthetic-response-secret",
        "synthetic-native-music-token",
        "synthetic-server-input",
        "127.0.0.1",
        "/get-mp3/",
    ] {
        assert!(!contains_text(ui.window.upcast_ref(), sensitive));
        assert!(
            !this
                .widgets
                .player_status
                .tooltip_text()
                .is_some_and(|text| text.contains(sensitive))
        );
    }
    crate::ui::check_widget_geometry(
        this.widgets.status_revealer.upcast_ref(),
        ui.window.upcast_ref(),
    )
    .unwrap();
    capture_native("music-player-media-http403");
    resize_player_window(&ui, 420, 720);
    wait_media_forbidden(this);
    let compact_layout = Instant::now() + Duration::from_millis(180);
    pump_until(|| Instant::now() >= compact_layout);
    crate::ui::check_widget_geometry(
        this.widgets.status_revealer.upcast_ref(),
        ui.window.upcast_ref(),
    )
    .unwrap();
    capture_native("music-player-media-http403-compact");
    resize_player_window(&ui, 900, 650);
    let offers = fixture.offers(0);
    let media_requests = fixture.counts.lock().unwrap().media[0];
    assert!(media_requests > 0);
    let settled = Instant::now() + Duration::from_millis(400);
    pump_until(|| Instant::now() >= settled);
    assert_eq!(
        fixture.offers(0),
        offers,
        "HTTP rejection must not trigger automatic resolution"
    );
    assert_eq!(
        fixture.counts.lock().unwrap().media[0],
        media_requests,
        "HTTP rejection must not trigger automatic media retries"
    );

    // Stop removes the player-owned visible error, including its stale title.
    let failed = this.player.borrow().controller.clone().unwrap();
    this.widgets.stop.emit_clicked();
    pump_until(|| !failed.snapshot().worker_running);
    pump_until(|| {
        !this.widgets.status_revealer.reveals_child()
            && !this.widgets.status_revealer.is_child_revealed()
    });
    assert!(
        this.widgets
            .status
            .subtitle()
            .is_none_or(|text| text.is_empty())
    );
    assert_eq!(this.widgets.status.title(), "Воспроизведение остановлено.");
    assert!(this.widgets.player_status.tooltip_text().is_none());
    fixture.reject_media.store(false, Ordering::Release);
    this.widgets.play.emit_clicked();
    wait_playing(this, 0);
    assert_eq!(fixture.offers(0), offers + 1);

    // The ordinary Play retry also clears the old error immediately and
    // obtains a new resource, without requiring a separate Stop first.
    this.widgets.stop.emit_clicked();
    fixture.reject_media.store(true, Ordering::Release);
    this.widgets.play.emit_clicked();
    wait_media_forbidden(this);
    let retry_offers = fixture.offers(0);
    fixture.reject_media.store(false, Ordering::Release);
    this.widgets.play.emit_clicked();
    assert!(
        this.widgets
            .status
            .subtitle()
            .is_none_or(|text| text.is_empty())
    );
    assert!(!this.widgets.status_revealer.reveals_child());
    wait_playing(this, 0);
    assert_eq!(fixture.offers(0), retry_offers + 1);
    assert!(this.widgets.player_status.tooltip_text().is_none());

    // The shared native row can subsequently belong to a catalog request.
    // Stopping the player must leave that newer, unrelated error visible.
    this.widgets.stop.emit_clicked();
    fixture.reject_media.store(true, Ordering::Release);
    this.widgets.play.emit_clicked();
    wait_media_forbidden(this);
    const CATALOG_TITLE: &str = "Не удалось выполнить поиск";
    const CATALOG_SUBTITLE: &str = "Ошибка каталога. Повторите запрос.";
    this.widgets.status.set_title(CATALOG_TITLE);
    this.widgets.status.set_subtitle(CATALOG_SUBTITLE);
    this.render_controls();
    let catalog_failed = this.player.borrow().controller.clone().unwrap();
    this.widgets.stop.emit_clicked();
    assert_eq!(this.widgets.status.title(), CATALOG_TITLE);
    assert_eq!(
        this.widgets.status.subtitle().as_deref(),
        Some(CATALOG_SUBTITLE)
    );
    pump_until(|| {
        this.widgets.status_revealer.reveals_child()
            && this.widgets.status_revealer.is_child_revealed()
    });
    pump_until(|| !catalog_failed.snapshot().worker_running);
    assert!(this.widgets.player_status.tooltip_text().is_none());
    ui.window.close();
}
