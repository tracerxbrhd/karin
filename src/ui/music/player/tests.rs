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
    searches: usize,
    lookups: usize,
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
    assert!(matches!(first[0], "GET" | "HEAD" | "POST"));
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{}", first[1])).unwrap();
    let header = |name: &str| {
        request.lines().skip(1).find_map(|line| {
            let (field, value) = line.split_once(':')?;
            field.eq_ignore_ascii_case(name).then_some(value.trim())
        })
    };
    let api = matches!(
        url.path(),
        "/account/status" | "/search" | "/tracks" | "/users/1234/likes/tracks"
    ) || url.path().starts_with("/tracks/");
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
    } else if url.path() == "/users/1234/likes/tracks" {
        json_response(
            &mut stream,
            json!({"result":{"library":{"uid":1234,"revision":1,"tracks":[]}}}),
        );
    } else if url.path() == "/search" {
        counts.lock().unwrap().searches += 1;
        json_response(
            &mut stream,
            json!({"result":{"tracks":{"page":0,"perPage":20,"total":1,"results":[{
                "id":42,"title":{"ignored":"display"},"artists":false,"albums":[null,17],"available":{"not":"canonical"},"explicit":[1,2]
            }]}}}),
        );
    } else if url.path() == "/tracks" {
        assert_eq!(first[0], "POST");
        let length: usize = header("content-length").unwrap().parse().unwrap();
        assert!(length < 4096);
        let mut form = vec![0; length];
        stream.read_exact(&mut form).unwrap();
        assert!(String::from_utf8(form).unwrap().contains("42"));
        counts.lock().unwrap().lookups += 1;
        json_response(
            &mut stream,
            json!({"result":[{"id":42,"title":"Synthetic remote player track","durationMs":4000,
            "available":true,"artists":[{"name":"Synthetic artist"}]}]}),
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
        artist_links: Vec::new(),
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

fn assert_pinned_track(this: &Controller, id: &str) {
    assert!(this.track_info_preference());
    assert!(this.info_is_open());
    assert_eq!(
        this.track_pane
            .selected()
            .as_ref()
            .map(|track| track.id.as_str()),
        Some(id)
    );
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
    let row = this.queue_row_at_physical(index as usize).unwrap();
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

fn native_click(window: &adw::ApplicationWindow, widget: &impl IsA<gtk::Widget>) {
    let (x, y) = super::super::test_input::pointer(
        window,
        widget,
        super::super::test_input::Target::Leading,
    );
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
    let id = String::from_utf8(found.stdout).unwrap();
    let id = id.lines().next().unwrap();
    let x = x.to_string();
    let y = y.to_string();
    for arguments in [
        vec!["windowfocus", "--sync", id],
        vec!["mousemove", "--sync", "--window", id, &x, &y],
        vec!["click", "1"],
    ] {
        assert!(
            std::process::Command::new("xdotool")
                .args(arguments)
                .status()
                .unwrap()
                .success()
        );
    }
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
    this.widgets.stack.set_visible_child_name("search");
    this.widgets.search.set_text("Synthetic playable search");
    native_click(&ui.window, &this.widgets.search_button);
    pump_until(|| {
        !this.model.borrow().busy
            && contains_text(
                this.widgets.search_rows.upcast_ref(),
                "Synthetic remote player track",
            )
    });
    assert_eq!(fixture.counts.lock().unwrap().searches, 1);
    assert_eq!(
        fixture.counts.lock().unwrap().lookups,
        1,
        "one batched normalization request for a search page"
    );
    native_click(&ui.window, &this.widgets.search_rows.first_child().unwrap());
    wait_playing(this, 0);
    assert_eq!(
        fixture.counts.lock().unwrap().lookups,
        1,
        "activating a normalized search hit plays without a second detail lookup"
    );
    assert_eq!(
        this.model.borrow().queue.context().unwrap().source_kind(),
        crate::queue::ContextKind::Search
    );
    for id in ["43", "44"] {
        this.enqueue(track(id));
    }
    this.toggle_queue_panel();
    assert!(this.widgets.play.is_sensitive());
    assert!(contains_text(
        this.widgets.player_status.upcast_ref(),
        "Воспроизведение"
    ));
    assert!(this.widgets.stop.is_sensitive());
    let playing = this.player.borrow().controller.clone().unwrap();
    let generation = this.model.borrow().generation;
    this.widgets.logout.emit_clicked();
    pump_until(|| {
        this.logout_confirmation
            .borrow()
            .as_ref()
            .is_some_and(|confirmation| {
                confirmation.dialog.is_mapped() && confirmation.dialog.can_close()
            })
    });
    this.respond_logout_for_test("cancel");
    pump_until(|| {
        this.logout_confirmation.borrow().is_none() && ui.window.dialogs().n_items() == 0
    });
    assert_eq!(this.model.borrow().generation, generation);
    assert!(this.model.borrow().token.is_some());
    assert_eq!(this.model.borrow().queue.items().len(), 3);
    assert!(playing.snapshot().worker_running);
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
    pump_until(|| {
        this.logout_confirmation
            .borrow()
            .as_ref()
            .is_some_and(|confirmation| {
                confirmation.dialog.is_mapped() && confirmation.dialog.can_close()
            })
    });
    assert!(
        this.model.borrow().token.is_some(),
        "requesting logout must not invalidate the live session"
    );
    this.respond_logout_for_test("logout");
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
fn native_music_playback_context_occurrences_and_shared_transport() {
    use crate::queue::{ContextKind, PlaybackContext};

    gtk::init().unwrap();
    adw::init().unwrap();
    let fixture = Fixture::new();
    let (_app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    let mut unavailable = track("43");
    unavailable.available = false;
    let mut clicked = track("42");
    clicked.title = "Synthetic selected duplicate occurrence".into();
    let context = PlaybackContext::new(
        ContextKind::Search,
        None,
        vec![track("42"), unavailable, track("44"), clicked],
        3,
        true,
    )
    .unwrap();
    eprintln!("Karin synthetic context phase=play-clicked-duplicate");
    this.play_music_context(context);
    assert_eq!(this.model.borrow().queue.current_index(), Some(3));
    assert_eq!(
        this.widgets.current.text(),
        "Synthetic selected duplicate occurrence"
    );
    assert!(this.player.borrow().requested);
    assert!(this.model.borrow().queue.context().unwrap().is_partial());
    let serial = this.player.borrow().serial;
    this.play_music();
    this.play_music();
    assert_eq!(
        this.player.borrow().serial,
        serial,
        "Play is idempotent while resolving"
    );
    wait_playing(this, 3);
    assert_eq!(fixture.offers(0), 1);
    assert_eq!(fixture.offers(1), 0);
    this.set_info_open(true);
    assert_pinned_track(this, "42");
    assert_eq!(
        this.track_pane.selected().unwrap().title,
        "Synthetic selected duplicate occurrence"
    );
    this.select_track_info(track("43"), None, false);

    eprintln!("Karin synthetic context phase=pause-current");
    this.pause_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|state| state.state == PlaybackState::Paused)
    });
    assert_pinned_track(this, "43");
    this.pause_music();
    eprintln!("Karin synthetic context phase=previous-while-paused");
    this.skip_music(false, false);
    pump_until(|| {
        this.model.borrow().queue.current_index() == Some(2)
            && this
                .music_snapshot()
                .is_some_and(|state| state.state == PlaybackState::Ready)
    });
    assert_eq!(fixture.offers(2), 1);
    assert_pinned_track(this, "44");
    assert!(!this.player.borrow().requested);
    eprintln!("Karin synthetic context phase=play-prepared-previous");
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Ready && snapshot.seekable)
    });
    let before_seek = this.music_snapshot().unwrap().seek_serial;
    // Deterministically model a Ready worker observed before the GUI poll has
    // consumed its pending preparation. Explicit Play must consume the flag
    // and retain its resume position before the worker becomes Playing.
    this.player.borrow_mut().preparing = Some((false, Some(Duration::from_secs(1))));
    this.play_music();
    assert!(this.player.borrow().preparing.is_none());
    wait_playing(this, 2);
    pump_until(|| {
        this.music_snapshot().is_some_and(|snapshot| {
            snapshot.seek_serial > before_seek
                && snapshot.seek_position == Some(Duration::from_secs(1))
        })
    });
    assert_eq!(
        fixture.offers(2),
        1,
        "Play resumes the prepared context item"
    );
    eprintln!("Karin synthetic context phase=skip-unavailable-previous");
    this.skip_music(false, false);
    wait_playing(this, 0);
    assert_pinned_track(this, "42");
    assert_eq!(
        fixture.offers(1),
        0,
        "unavailable context items are not resolved"
    );

    eprintln!("Karin synthetic context phase=next-while-stopped");
    this.stop_music();
    let offers = fixture.offers(2);
    this.skip_music(true, false);
    assert_eq!(this.model.borrow().queue.current_index(), Some(2));
    assert!(!this.music_requested());
    assert!(this.music_snapshot().is_none());
    assert_eq!(
        fixture.offers(2),
        offers,
        "Next preserves stopped transport"
    );
    eprintln!("Karin synthetic context phase=play-next-from-stopped");
    this.play_music();
    wait_playing(this, 2);

    eprintln!("Karin synthetic context phase=replace-playlist-context");
    this.play_music_context(
        PlaybackContext::new(
            ContextKind::Playlist,
            Some("77".into()),
            vec![track("43"), track("44")],
            0,
            true,
        )
        .unwrap(),
    );
    wait_playing(this, 0);
    this.enqueue(track("42"));
    {
        let model = this.model.borrow();
        let context = model.queue.context().unwrap();
        assert_eq!(context.source_kind(), ContextKind::Playlist);
        assert_eq!(context.loaded_items_len(), 2);
        assert_eq!(model.queue.items().len(), 3);
    }
    eprintln!("Karin synthetic context phase=playlist-eos-next-loaded");
    pump_until(|| this.widgets.progress.is_sensitive());
    this.seek_music(Duration::from_millis(3_800));
    wait_playing(this, 1);
    assert_pinned_track(this, "44");
    eprintln!("Karin synthetic context phase=playlist-eos-manual-addition");
    pump_until(|| this.widgets.progress.is_sensitive());
    this.seek_music(Duration::from_millis(3_800));
    wait_playing(this, 2);
    assert_eq!(this.model.borrow().queue.current().unwrap().id, "42");
    assert_pinned_track(this, "42");

    let mut second = track("42");
    second.title = "Synthetic exact queued duplicate".into();
    eprintln!("Karin synthetic context phase=replace-album-context");
    this.play_music_context(
        PlaybackContext::new(
            ContextKind::Album,
            Some("78".into()),
            vec![track("42"), second],
            0,
            false,
        )
        .unwrap(),
    );
    wait_playing(this, 0);
    this.select_track_info(track("43"), None, false);
    eprintln!("Karin synthetic context phase=play-exact-queued-duplicate");
    this.play_music_queue(1);
    wait_playing(this, 1);
    assert_eq!(
        this.widgets.current.text(),
        "Synthetic exact queued duplicate"
    );
    assert_eq!(this.model.borrow().queue.items().len(), 2);
    assert_pinned_track(this, "42");
    assert_eq!(
        this.track_pane.selected().unwrap().title,
        "Synthetic exact queued duplicate"
    );
    let selected = this.model.borrow().queue.current_index();
    let mut unavailable = track("44");
    unavailable.available = false;
    eprintln!("Karin synthetic context phase=refuse-unavailable-context");
    this.play_music_context(
        PlaybackContext::new(ContextKind::SingleTrack, None, vec![unavailable], 0, false).unwrap(),
    );
    assert_eq!(this.model.borrow().queue.current_index(), selected);
    assert_eq!(
        this.widgets.current.text(),
        "Synthetic exact queued duplicate"
    );

    eprintln!("Karin synthetic context phase=shared-volume");
    this.set_music_volume(0.41);
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|state| (state.volume - 0.41).abs() < 0.001)
    });
    this.set_music_volume(f64::NAN);
    assert_eq!(this.music_volume(), 0.41);
    let closing = this.player.borrow().controller.clone().unwrap();
    eprintln!("Karin synthetic context phase=close-context");
    ui.window.close();
    pump_until(|| !closing.snapshot().worker_running);
    assert!(this.model.borrow().queue.context().is_none());
    assert!(this.track_pane.selected().is_none());
    assert!(!this.info_is_open());
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
    this.toggle_queue_panel();

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
    let window = this.window.upgrade().unwrap();
    let layout_deadline = Instant::now() + Duration::from_secs(7);
    pump_until(|| {
        let ready = this.music_snapshot().is_some_and(|snapshot| {
            snapshot.state == PlaybackState::Error
                && snapshot.last_error == Some(PlaybackError::RemoteHttp(403))
        }) && this.widgets.status.title() == "Сервер аудио отклонил запрос"
            && this
                .widgets
                .status
                .subtitle()
                .is_some_and(|subtitle| subtitle.starts_with("HTTP 403."))
            // The compact information subview intentionally suspends the
            // central stack. Its model still holds the safe detailed error;
            // the persistent footer must show the failure and real Retry.
            && this.widgets.player_status.text() == "Ошибка воспроизведения"
            && this.widgets.player_status.is_visible()
            && this.widgets.player_status.is_mapped()
            && this.widgets.play.is_mapped()
            && this.widgets.play.is_sensitive()
            && this.widgets.play.tooltip_text().as_deref()
                == Some("Получить источник заново и повторить")
            && (!this.widgets.stack.is_mapped()
                || (this.widgets.status_revealer.reveals_child()
                    && this.widgets.status_revealer.is_child_revealed()
                    && this.widgets.status_revealer.is_mapped()
                    && this.widgets.status.is_mapped()))
            && this.widgets.player_status.compute_bounds(&window)
                .is_some_and(|bounds| bounds.width() > 0.0 && bounds.height() > 0.0);
        // Error text and visibility can change one GTK frame before the
        // height-for-width allocation. Wait for the actual readable geometry,
        // rather than accepting the previous loading caption's allocation.
        let readable = crate::ui::check_widget_geometry(
            this.widgets.player_status.upcast_ref(),
            window.upcast_ref(),
        )
        .is_ok();
        if ready && readable {
            return true;
        }
        if Instant::now() >= layout_deadline {
            eprintln!(
                "Karin synthetic player-error phase=unsettled-layout width={} height={} text-height={} main-mapped={} footer-mapped={}",
                this.widgets.player_status.width(),
                this.widgets.player_status.height(),
                this.widgets.player_status.layout().pixel_size().1,
                this.widgets.stack.is_mapped(),
                this.widgets.player_status.is_mapped()
            );
            capture_native("music-player-error-unsettled-layout");
            panic!(
                "the actual media error did not reach readable visible geometry within seven seconds"
            );
        }
        false
    });
    crate::ui::check_widget_geometry(this.widgets.player_status.upcast_ref(), window.upcast_ref())
        .expect("the actual visible footer error remains fully readable");
    let bounds = this.widgets.play.compute_bounds(&window).unwrap();
    assert!(
        bounds.x() >= 0.0
            && bounds.y() >= 0.0
            && bounds.x() + bounds.width() <= window.width() as f32
            && bounds.y() + bounds.height() <= window.height() as f32
    );
    let target = window
        .pick(
            f64::from(bounds.x() + bounds.width() / 2.0),
            f64::from(bounds.y() + bounds.height() / 2.0),
            gtk::PickFlags::DEFAULT,
        )
        .expect("the real retry control has a native pointer target");
    assert!(
        target == this.widgets.play || target.is_ancestor(&this.widgets.play),
        "the information presentation cannot obscure the persistent Retry action"
    );
}

/// Wait on actual native state, not an arbitrary delay: the panel must finish
/// sliding and the footer must finish its responsive first-frame reparenting.
/// The shared input helper then observes two stable completed frame-clock ticks
/// for each measured control before the before/after anchoring comparison.
fn wait_settled_player_error_layout(ui: &crate::ui::MainWindow) {
    let this = &ui.music.controller;
    let ready = || {
        let panel_ready = this
            .info_host
            .borrow()
            .as_ref()
            .and_then(|host| host.revealer())
            .is_some_and(|panel| panel.is_child_revealed());
        let compact = ui.music.player_bar.width() < 720;
        let mode_parent = this
            .widgets
            .shuffle
            .ancestor(gtk::MenuButton::static_type())
            .and_downcast::<gtk::MenuButton>();
        let footer_ready = ui.music.player_bar.width() > 0
            && ui.music.player_bar.has_css_class("music-player-compact") == compact
            && if compact {
                mode_parent.is_some_and(|menu| menu.is_mapped())
                    && !this.widgets.shuffle.is_mapped()
            } else {
                mode_parent.is_none()
                    && this.widgets.shuffle.is_mapped()
                    && this.widgets.repeat.is_mapped()
            };
        panel_ready && footer_ready && ui.verify_layout().is_ok()
    };
    pump_until(ready);
    super::super::test_input::wait_for_layout(&ui.window, &this.widgets.play);
    super::super::test_input::wait_for_layout(&ui.window, &this.widgets.progress);
    pump_until(ready);
    ui.verify_layout()
        .expect("the measured error-test layout must be structurally settled");
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
    this.toggle_queue_panel();
    wait_settled_player_error_layout(&ui);
    let play_before = this.widgets.play.compute_bounds(&ui.window).unwrap();
    let progress_before = this.widgets.progress.compute_bounds(&ui.window).unwrap();
    let window_before = (ui.window.width(), ui.window.height());
    let panel_before = this
        .info_host
        .borrow()
        .as_ref()
        .and_then(|host| host.revealer())
        .unwrap()
        .progress();

    fixture.reject_media.store(true, Ordering::Release);
    this.widgets.play.emit_clicked();
    wait_media_forbidden(this);
    // Safe feedback has its own full-width row above the transport. The
    // bottom-anchored controls must not jump when that row becomes visible.
    wait_settled_player_error_layout(&ui);
    let play_after = this.widgets.play.compute_bounds(&ui.window).unwrap();
    let progress_after = this.widgets.progress.compute_bounds(&ui.window).unwrap();
    let window_after = (ui.window.width(), ui.window.height());
    let panel_after = this
        .info_host
        .borrow()
        .as_ref()
        .and_then(|host| host.revealer())
        .unwrap()
        .progress();
    assert!(
        (play_before.y() - play_after.y()).abs() <= 1.0,
        "feedback moved Play: play_before={play_before:?} play_after={play_after:?} progress_before={progress_before:?} progress_after={progress_after:?} window_before={window_before:?} window_after={window_after:?} panel_before={panel_before} panel_after={panel_after}"
    );
    assert!(
        (progress_before.y() - progress_after.y()).abs() <= 1.0,
        "feedback moved progress: play_before={play_before:?} play_after={play_after:?} progress_before={progress_before:?} progress_after={progress_after:?} window_before={window_before:?} window_after={window_after:?} panel_before={panel_before} panel_after={panel_after}"
    );
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
    eprintln!("Karin synthetic player-error phase=compact-pinned-footer");
    assert!(this.info_is_open());
    assert!(!this.widgets.stack.is_mapped());
    wait_media_forbidden(this);
    let compact_layout = Instant::now() + Duration::from_millis(180);
    pump_until(|| Instant::now() >= compact_layout);
    crate::ui::check_widget_geometry(
        this.widgets.status_revealer.upcast_ref(),
        ui.window.upcast_ref(),
    )
    .unwrap();
    capture_native("music-player-media-http403-compact");
    let compact_retry_offers = fixture.offers(0);
    native_click(&ui.window, &this.widgets.play);
    pump_until(|| fixture.offers(0) == compact_retry_offers + 1);
    wait_media_forbidden(this);
    assert!(this.info_is_open());
    eprintln!("Karin synthetic player-error phase=compact-main-inline-error");
    this.set_info_open(false);
    pump_until(|| this.widgets.stack.is_mapped());
    wait_media_forbidden(this);
    assert!(this.widgets.status.is_mapped());
    capture_native("music-player-media-http403-compact-main");
    this.set_info_open(true);
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

/// External D-Bus clients must never wait on the thread serving Karin's bus
/// callbacks. Each bounded client operation runs elsewhere while GTK is pumped.
fn desktop_background<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        sender.send(result).unwrap();
    });
    let mut result = None;
    pump_until(|| {
        result = receiver.try_recv().ok();
        result.is_some()
    });
    worker.join().unwrap();
    match result.unwrap() {
        Ok(value) => value,
        Err(error) => std::panic::resume_unwind(error),
    }
}

fn desktop_cli(program: &str, arguments: &[&str]) -> String {
    static NEXT_COMMAND: AtomicU64 = AtomicU64::new(0);
    let program = program.to_owned();
    let arguments = arguments
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    desktop_background(move || {
        let operation = arguments
            .iter()
            .find_map(|argument| match argument.as_str() {
                "status" => Some("status"),
                "pause" => Some("pause"),
                "play" => Some("play"),
                "play-pause" => Some("play-pause"),
                "volume" => Some("volume"),
                "position" => Some("position"),
                "next" => Some("next"),
                "previous" => Some("previous"),
                "stop" => Some("stop"),
                "metadata" => Some("metadata"),
                "introspect" => Some("introspect"),
                "call" => Some("call"),
                _ => None,
            })
            .unwrap_or("unknown-operation");
        let step = NEXT_COMMAND.fetch_add(1, Ordering::Relaxed) + 1;
        eprintln!("Karin synthetic desktop command: step={step} operation={operation}");
        let output = std::process::Command::new("timeout")
            .arg("5")
            .arg(&program)
            .args(arguments)
            .env("LC_ALL", "C")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let category = if output.status.code() == Some(124) {
            "client-timeout"
        } else if stderr.contains("No players found") || stderr.contains("ServiceUnknown") {
            "player-not-found"
        } else if stderr.contains("does not support") || stderr.contains("NotSupported") {
            "unsupported-operation"
        } else if stderr.contains("GDBus.Error") || stderr.contains("org.freedesktop.DBus.Error") {
            "dbus-error"
        } else if stderr.is_empty() {
            "empty-stderr"
        } else {
            "unclassified-client-error"
        };
        assert!(
            output.status.success(),
            "synthetic desktop client failed: program={program} step={step} operation={operation} exit={:?} category={category}",
            output.status.code(),
        );
        String::from_utf8(output.stdout).unwrap()
    })
}

fn desktop_property(name: &str) -> glib::Variant {
    let name = name.to_owned();
    desktop_background(move || {
        let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
        bus.call_sync(
            Some(crate::mpris::BUS_NAME),
            "/org/mpris/MediaPlayer2",
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&("org.mpris.MediaPlayer2.Player", name).to_variant()),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            3_000,
            gio::Cancellable::NONE,
        )
        .unwrap()
        .get::<(glib::Variant,)>()
        .unwrap()
        .0
    })
}

fn desktop_method(method: &str, parameters: glib::Variant) {
    let method = method.to_owned();
    desktop_background(move || {
        let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
        bus.call_sync(
            Some(crate::mpris::BUS_NAME),
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2.Player",
            &method,
            Some(&parameters),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            3_000,
            gio::Cancellable::NONE,
        )
        .unwrap();
    });
}

#[test]
#[ignore = "requires isolated dbus-run-session, Xvfb, playerctl, gdbus and GStreamer HTTP/MP3 plugins"]
fn native_music_context_mpris_shared_player_and_cleanup() {
    use crate::queue::{ContextKind, PlaybackContext};
    use std::collections::BTreeMap;

    const PLAYER: &str = "io.github.tracerxbrhd.Karin";
    gtk::init().unwrap();
    adw::init().unwrap();
    let fixture = Fixture::new();
    let (_app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    pump_until(|| this.system_media_available());
    this.play_music_context(
        PlaybackContext::new(
            ContextKind::Playlist,
            Some("synthetic-playlist".into()),
            vec![track("42"), track("43"), track("44")],
            0,
            true,
        )
        .unwrap(),
    );
    wait_playing(this, 0);
    let original = this.player.borrow().controller.clone().unwrap();
    this.set_info_open(true);
    assert_pinned_track(this, "42");
    this.select_track_info(track("44"), None, false);
    assert_eq!(
        desktop_cli("playerctl", &["--player", PLAYER, "status"]).trim(),
        "Playing"
    );
    desktop_cli("playerctl", &["--player", PLAYER, "pause"]);
    pump_until(|| {
        original.snapshot().state == PlaybackState::Paused
            && this.widgets.player_status.text() == "Пауза"
    });
    assert_eq!(this.widgets.player_status.text(), "Пауза");
    assert_eq!(desktop_property("PlaybackStatus").str(), Some("Paused"));
    assert_pinned_track(this, "44");

    // Exercise the real mounted pane while a real HTTP/MP3 worker is paused.
    // Changing presentation cannot resolve again or replace/restart its source.
    let presentation_serial = this.player.borrow().serial;
    let source_generation = original.snapshot().generation;
    let presentation_offers = fixture.offers(0);
    this.set_info_open(false);
    native_click(&ui.window, &this.widgets.current_art_button);
    // Intent changes at the start of the slide. Wait for actual completed
    // native allocation before targeting the close button with the mouse.
    pump_until(|| {
        this.info_is_open()
            && this
                .info_host
                .borrow()
                .as_ref()
                .and_then(|host| host.revealer())
                .is_some_and(|view| view.is_child_revealed())
    });
    assert_pinned_track(this, "42");
    // Unlike Ctrl+I, the footer cover only opens/shows current information.
    // Finish actual GTK input frames before checking a repeated click: simply
    // waiting for an already-true intent would miss the old toggle regression.
    native_click(&ui.window, &this.widgets.current_art_button);
    super::super::test_input::wait_for_layout(&ui.window, &this.widgets.current_art_button);
    assert_pinned_track(this, "42");
    assert_eq!(this.player.borrow().serial, presentation_serial);
    assert_eq!(fixture.offers(0), presentation_offers);
    assert_eq!(original.snapshot().generation, source_generation);
    assert_eq!(original.snapshot().state, PlaybackState::Paused);
    this.select_track_info(track("44"), None, false);
    assert_pinned_track(this, "44");
    native_click(&ui.window, &this.widgets.current_art_button);
    super::super::test_input::wait_for_layout(&ui.window, &this.widgets.current_art_button);
    assert_pinned_track(this, "42");
    assert_eq!(this.model.borrow().queue.current().unwrap().id, "42");
    assert_eq!(this.player.borrow().serial, presentation_serial);
    assert_eq!(fixture.offers(0), presentation_offers);
    assert_eq!(original.snapshot().generation, source_generation);
    assert_eq!(original.snapshot().state, PlaybackState::Paused);
    native_click(&ui.window, &this.track_pane.close);
    pump_until(|| !this.info_is_open());
    native_click(&ui.window, &this.widgets.current_art_button);
    pump_until(|| {
        this.info_is_open()
            && this
                .info_host
                .borrow()
                .as_ref()
                .and_then(|host| host.revealer())
                .is_some_and(|view| view.is_child_revealed())
    });
    assert_eq!(this.player.borrow().serial, presentation_serial);
    assert_eq!(fixture.offers(0), presentation_offers);
    assert_eq!(original.snapshot().generation, source_generation);
    assert_eq!(original.snapshot().state, PlaybackState::Paused);
    assert!(original.snapshot().worker_running);
    this.select_track_info(track("44"), None, false);

    let metadata = desktop_property("Metadata")
        .get::<BTreeMap<String, glib::Variant>>()
        .unwrap();
    assert_eq!(
        metadata["xesam:title"].str(),
        Some("Synthetic remote player track")
    );
    assert_eq!(
        metadata["xesam:artist"].get::<Vec<String>>(),
        Some(vec!["Synthetic artist".into()])
    );
    assert_eq!(metadata["mpris:length"].get::<i64>(), Some(4_000_000));
    let original_path = metadata["mpris:trackid"]
        .get::<glib::variant::ObjectPath>()
        .unwrap();
    assert!(
        original_path
            .as_str()
            .starts_with("/org/mpris/MediaPlayer2/Track/t")
    );
    assert!(!metadata.contains_key("xesam:url"));
    let printable = metadata.to_variant().print(true);
    for forbidden in [
        "synthetic-native-music-token",
        "synthetic-server-input",
        "127.0.0.1",
        "get-mp3",
        "download-info",
        "storage.yandex.net",
        "OAuth",
    ] {
        assert!(!printable.contains(forbidden));
    }
    let introspection = desktop_cli(
        "gdbus",
        &[
            "introspect",
            "--session",
            "--dest",
            crate::mpris::BUS_NAME,
            "--object-path",
            "/org/mpris/MediaPlayer2",
        ],
    );
    assert!(introspection.contains("org.mpris.MediaPlayer2.Player"));
    assert!(introspection.contains("Seeked"));
    let bus = desktop_background(|| {
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap()
    });
    let volume_changes = Rc::new(RefCell::new(Vec::<f64>::new()));
    let observed_volume = volume_changes.clone();
    let volume_subscription = bus.subscribe_to_signal(
        Some(crate::mpris::BUS_NAME),
        Some("org.freedesktop.DBus.Properties"),
        Some("PropertiesChanged"),
        Some("/org/mpris/MediaPlayer2"),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let (interface, properties, _) = signal
                .parameters
                .get::<(String, BTreeMap<String, glib::Variant>, Vec<String>)>()
                .unwrap();
            if interface == "org.mpris.MediaPlayer2.Player"
                && let Some(value) = properties
                    .get("Volume")
                    .and_then(|value| value.get::<f64>())
            {
                observed_volume.borrow_mut().push(value);
            }
        },
    );
    desktop_cli("playerctl", &["--player", PLAYER, "volume", "0.23"]);
    pump_until(|| (original.snapshot().volume - 0.23).abs() < 0.001);
    pump_until(|| {
        volume_changes
            .borrow()
            .iter()
            .any(|volume| (*volume - 0.23).abs() < 0.001)
    });
    assert!((this.widgets.volume.value() - 0.23).abs() < 0.001);
    assert_eq!(desktop_property("Volume").get::<f64>(), Some(0.23));
    assert_pinned_track(this, "44");

    // The compact bar exposes its actual shared volume through a native
    // popover. A keyboard adjustment reaches the same worker and D-Bus value;
    // Escape dismisses only that popover, never the pinned information subview.
    resize_player_window(&ui, 420, 720);
    let volume_menu = this
        .widgets
        .volume
        .ancestor(gtk::MenuButton::static_type())
        .and_downcast::<gtk::MenuButton>()
        .unwrap();
    assert_eq!(volume_menu.widget_name(), "music-volume-menu");
    native_click(&ui.window, &volume_menu);
    let volume_popover = volume_menu.popover().unwrap();
    pump_until(|| volume_popover.is_mapped() && this.widgets.volume.is_mapped());
    assert!(this.widgets.volume.grab_focus());
    let before_volume = original.snapshot().volume;
    assert!(
        std::process::Command::new("xdotool")
            .args(["key", "--clearmodifiers", "Right"])
            .status()
            .unwrap()
            .success()
    );
    pump_until(|| original.snapshot().volume > before_volume + 0.005);
    assert_pinned_track(this, "44");
    assert_eq!(this.player.borrow().serial, presentation_serial);
    assert_eq!(original.snapshot().generation, source_generation);
    capture_native("player-compact-volume-popover-pinned");
    assert!(
        std::process::Command::new("xdotool")
            .args(["key", "--clearmodifiers", "Escape"])
            .status()
            .unwrap()
            .success()
    );
    pump_until(|| !volume_popover.is_visible());
    assert_pinned_track(this, "44");
    resize_player_window(&ui, 900, 650);
    assert_pinned_track(this, "44");
    assert_eq!(fixture.offers(0), presentation_offers);
    desktop_cli("playerctl", &["--player", PLAYER, "volume", "0.23"]);
    pump_until(|| (original.snapshot().volume - 0.23).abs() < 0.001);

    let seeks = Rc::new(RefCell::new(Vec::<i64>::new()));
    let observed = seeks.clone();
    let acknowledgments = Rc::new(RefCell::new(Vec::<(i64, u64, Option<Duration>)>::new()));
    let observed_acknowledgments = acknowledgments.clone();
    let seek_worker = original.clone();
    let subscription = bus.subscribe_to_signal(
        Some(crate::mpris::BUS_NAME),
        Some("org.mpris.MediaPlayer2.Player"),
        Some("Seeked"),
        Some("/org/mpris/MediaPlayer2"),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let position = signal.parameters.get::<(i64,)>().unwrap().0;
            observed.borrow_mut().push(position);
            let snapshot = seek_worker.snapshot();
            observed_acknowledgments.borrow_mut().push((
                position,
                snapshot.seek_serial,
                snapshot.seek_position,
            ));
        },
    );
    pump_until(|| this.widgets.progress.is_sensitive());
    let before_seek = original.snapshot().seek_serial;
    desktop_cli("playerctl", &["--player", PLAYER, "position", "1"]);
    pump_until(|| {
        original.snapshot().position.is_some_and(|position| {
            position.abs_diff(Duration::from_secs(1)) < Duration::from_millis(150)
        })
    });
    pump_until(|| seeks.borrow().contains(&1_000_000));
    assert_pinned_track(this, "44");
    assert!(
        acknowledgments
            .borrow()
            .iter()
            .any(|(position, serial, target)| *position == 1_000_000
                && *serial > before_seek
                && *target == Some(Duration::from_secs(1)))
    );
    eprintln!("Karin synthetic MPRIS phase=near-position-seek-native-ack");
    let before_near_seek = original.snapshot().seek_serial;
    desktop_cli("playerctl", &["--player", PLAYER, "position", "1.05"]);
    pump_until(|| {
        original.snapshot().seek_serial > before_near_seek
            && original.snapshot().seek_position == Some(Duration::from_millis(1_050))
    });
    pump_until(|| seeks.borrow().contains(&1_050_000));
    assert_pinned_track(this, "44");
    assert!(
        acknowledgments
            .borrow()
            .iter()
            .any(|(position, serial, target)| *position == 1_050_000
                && *serial > before_near_seek
                && *target == Some(Duration::from_millis(1_050)))
    );
    assert!(
        original.snapshot().worker_running,
        "desktop commands use the existing GStreamer worker"
    );
    eprintln!("Karin synthetic MPRIS phase=resume-existing-worker");
    desktop_cli("playerctl", &["--player", PLAYER, "play"]);
    wait_playing(this, 0);
    assert_eq!(original.snapshot().state, PlaybackState::Playing);
    assert_pinned_track(this, "44");
    desktop_cli("playerctl", &["--player", PLAYER, "pause"]);
    pump_until(|| {
        original.snapshot().state == PlaybackState::Paused
            && this.widgets.player_status.text() == "Пауза"
    });

    desktop_cli("playerctl", &["--player", PLAYER, "next"]);
    pump_until(|| {
        this.model.borrow().queue.current_index() == Some(1)
            && this
                .music_snapshot()
                .is_some_and(|snapshot| snapshot.state == PlaybackState::Ready)
            && this.widgets.player_status.text() == "Готово к воспроизведению"
    });
    pump_until(|| !original.snapshot().worker_running);
    assert_eq!(
        desktop_cli("playerctl", &["--player", PLAYER, "status"]).trim(),
        "Paused"
    );
    assert_eq!(fixture.offers(1), 1);
    assert_pinned_track(this, "43");
    let seek_count = seeks.borrow().len();
    desktop_method("SetPosition", (original_path, 2_700_000_i64).to_variant());
    assert!(
        this.music_snapshot().unwrap().position.unwrap_or_default() < Duration::from_millis(500)
    );
    assert_eq!(
        seeks.borrow().len(),
        seek_count,
        "a stale desktop track identity cannot seek a replacement"
    );
    desktop_cli("playerctl", &["--player", PLAYER, "previous"]);
    pump_until(|| {
        this.model.borrow().queue.current_index() == Some(0)
            && this
                .music_snapshot()
                .is_some_and(|snapshot| snapshot.state == PlaybackState::Ready)
            && this.widgets.player_status.text() == "Готово к воспроизведению"
    });
    eprintln!("Karin synthetic MPRIS phase=play-prepared-previous");
    assert_pinned_track(this, "42");
    desktop_cli("playerctl", &["--player", PLAYER, "play"]);
    wait_playing(this, 0);
    let stopped = this.player.borrow().controller.clone().unwrap();
    desktop_cli("playerctl", &["--player", PLAYER, "stop"]);
    pump_until(|| this.music_snapshot().is_none() && !stopped.snapshot().worker_running);
    assert_eq!(
        desktop_cli("playerctl", &["--player", PLAYER, "status"]).trim(),
        "Stopped"
    );
    assert_eq!(desktop_property("Position").get::<i64>(), Some(0));
    eprintln!("Karin synthetic MPRIS phase=play-pause-from-stopped");
    desktop_cli("playerctl", &["--player", PLAYER, "play-pause"]);
    wait_playing(this, 0);
    pump_until(|| this.widgets.progress.is_sensitive());

    // Arm an acknowledgment for the old worker, then synchronously Stop before
    // GTK can observe it. Restarting the same queue occurrence must not publish
    // that discarded seek when its new pipeline begins near zero again.
    let seek_count = seeks.borrow().len();
    this.note_music_seek(Duration::ZERO);
    this.stop_music();
    assert!(this.music_snapshot().is_none());
    eprintln!("Karin synthetic MPRIS phase=restart-after-cancelled-seek");
    desktop_cli("playerctl", &["--player", PLAYER, "play-pause"]);
    wait_playing(this, 0);
    pump_until(|| this.widgets.progress.is_sensitive());
    super::super::test_input::wait_for_layout(&ui.window, &this.widgets.progress);
    assert!(desktop_property("Position").get::<i64>().is_some());
    assert_eq!(
        seeks.borrow().len(),
        seek_count,
        "Stop discards a pending seek before the same occurrence restarts"
    );
    desktop_cli("playerctl", &["--player", PLAYER, "stop"]);
    pump_until(|| this.music_snapshot().is_none());
    let offers = fixture.offers(1);
    desktop_cli("playerctl", &["--player", PLAYER, "next"]);
    assert_eq!(this.model.borrow().queue.current_index(), Some(1));
    assert!(this.music_snapshot().is_none());
    assert_eq!(
        fixture.offers(1),
        offers,
        "desktop Next preserves stopped transport"
    );
    eprintln!("Karin synthetic MPRIS phase=play-next-from-stopped");
    desktop_cli("playerctl", &["--player", PLAYER, "play"]);
    wait_playing(this, 1);
    pump_until(|| this.widgets.progress.is_sensitive());
    desktop_cli("playerctl", &["--player", PLAYER, "position", "3.8"]);
    wait_playing(this, 2);
    assert_eq!(
        fixture.offers(2),
        1,
        "actual worker EOS advances the same context once"
    );
    assert_pinned_track(this, "44");
    let closing = this.player.borrow().controller.clone().unwrap();
    this.widgets.logout.emit_clicked();
    pump_until(|| {
        this.logout_confirmation
            .borrow()
            .as_ref()
            .is_some_and(|confirmation| {
                confirmation.dialog.is_mapped() && confirmation.dialog.can_close()
            })
    });
    this.respond_logout_for_test("logout");
    pump_until(|| this.model.borrow().token.is_none() && !closing.snapshot().worker_running);
    assert!(this.track_pane.selected().is_none());
    assert!(!this.info_is_open());
    assert!(
        desktop_property("Metadata")
            .get::<BTreeMap<String, glib::Variant>>()
            .unwrap()
            .is_empty()
    );
    assert_eq!(desktop_property("Position").get::<i64>(), Some(0));
    assert_eq!(desktop_property("CanPlay").get::<bool>(), Some(false));
    assert_eq!(desktop_property("CanGoNext").get::<bool>(), Some(false));
    assert_eq!(
        desktop_cli("playerctl", &["--player", PLAYER, "status"]).trim(),
        "Stopped"
    );
    eprintln!("Karin synthetic MPRIS phase=reject-play-after-logout");
    // playerctl may reject this locally because CanPlay is false. Exercise
    // the server method directly to prove it cannot revive the signed-out
    // session, while preserving the explicit false-capability assertions.
    desktop_method("Play", ().to_variant());
    assert!(this.music_snapshot().is_none());
    assert!(this.model.borrow().queue.items().is_empty());
    drop(subscription);
    drop(volume_subscription);
    ui.window.close();
    pump_until(|| !this.system_media_available());
    let owned = desktop_cli(
        "gdbus",
        &[
            "call",
            "--session",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.NameHasOwner",
            crate::mpris::BUS_NAME,
        ],
    );
    assert_eq!(owned.trim(), "(false,)");
}

#[test]
#[ignore = "requires isolated dbus-run-session, Xvfb, playerctl and GStreamer HTTP/MP3 plugins"]
fn native_music_shuffle_repeat_mpris_shared_policy_and_eos() {
    use crate::queue::{ContextKind, PlaybackContext, RepeatMode};
    use std::collections::{BTreeMap, BTreeSet};
    const PLAYER: &str = "io.github.tracerxbrhd.Karin";
    gtk::init().unwrap();
    adw::init().unwrap();
    let fixture = Fixture::new();
    let (_app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    pump_until(|| this.system_media_available());
    this.play_music_context(
        PlaybackContext::new(
            ContextKind::Playlist,
            Some("synthetic-mode-context".into()),
            vec![track("42"), track("43"), track("42"), track("44")],
            2,
            true,
        )
        .unwrap(),
    );
    wait_playing(this, 2);
    desktop_cli("playerctl", &["--player", PLAYER, "pause"]);
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
    });
    let original = this.player.borrow().controller.clone().unwrap();
    let serial = this.player.borrow().serial;
    let generation = original.snapshot().generation;
    let offers = fixture.offers(0);
    let changed = Rc::new(RefCell::new(BTreeSet::<String>::new()));
    let observed = changed.clone();
    let bus = desktop_background(|| {
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap()
    });
    let subscription = bus.subscribe_to_signal(
        Some(crate::mpris::BUS_NAME),
        Some("org.freedesktop.DBus.Properties"),
        Some("PropertiesChanged"),
        Some("/org/mpris/MediaPlayer2"),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let (interface, properties, _) = signal
                .parameters
                .get::<(String, BTreeMap<String, glib::Variant>, Vec<String>)>()
                .unwrap();
            if interface == "org.mpris.MediaPlayer2.Player" {
                observed.borrow_mut().extend(properties.into_keys());
            }
        },
    );

    let seeked = Rc::new(RefCell::new(Vec::<i64>::new()));
    let observed_seek = seeked.clone();
    let seek_subscription = bus.subscribe_to_signal(
        Some(crate::mpris::BUS_NAME),
        Some("org.mpris.MediaPlayer2.Player"),
        Some("Seeked"),
        Some("/org/mpris/MediaPlayer2"),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            observed_seek
                .borrow_mut()
                .push(signal.parameters.get::<(i64,)>().unwrap().0)
        },
    );

    // External writes and real native input share one policy. Neither may
    // restart/replace the paused worker or make another resolver request.
    desktop_cli("playerctl", &["--player", PLAYER, "shuffle", "On"]);
    assert!(this.model.borrow().queue.shuffle());
    assert!(this.widgets.shuffle.is_active());
    assert_eq!(desktop_property("Shuffle").get::<bool>(), Some(true));
    desktop_cli("playerctl", &["--player", PLAYER, "loop", "Playlist"]);
    assert_eq!(this.model.borrow().queue.repeat_mode(), RepeatMode::Queue);
    assert!(this.widgets.repeat.has_css_class("active"));
    desktop_cli("playerctl", &["--player", PLAYER, "loop", "Track"]);
    assert_eq!(this.model.borrow().queue.repeat_mode(), RepeatMode::One);
    let repeat_badge = super::super::player_bar::repeat_one_badge(&this.widgets.repeat).unwrap();
    assert_eq!(repeat_badge.text(), "1");
    assert!(repeat_badge.is_visible());
    drop(repeat_badge);
    // The logical footer may be narrower than its outer window because the
    // icon rail owns space. Open the actual compact overflow before targeting
    // its shared controls; an unmapped control is not a click regression.
    let mode_popover = if !this.widgets.shuffle.is_mapped() {
        let menu = this
            .widgets
            .shuffle
            .ancestor(gtk::MenuButton::static_type())
            .and_downcast::<gtk::MenuButton>()
            .expect("compact modes have a native menu");
        native_click(&ui.window, &menu);
        let popover = menu.popover().unwrap();
        pump_until(|| popover.is_mapped() && this.widgets.shuffle.is_mapped());
        Some(popover)
    } else {
        None
    };
    eprintln!("Karin synthetic modes phase=native-shuffle-click");
    native_click(&ui.window, &this.widgets.shuffle);
    pump_until(|| !this.model.borrow().queue.shuffle());
    assert_eq!(desktop_property("Shuffle").get::<bool>(), Some(false));
    for expected in [RepeatMode::Off, RepeatMode::Queue, RepeatMode::One] {
        eprintln!("Karin synthetic modes phase=native-repeat-click mode={expected:?}");
        native_click(&ui.window, &this.widgets.repeat);
        pump_until(|| this.model.borrow().queue.repeat_mode() == expected);
    }
    pump_until(|| changed.borrow().contains("Shuffle") && changed.borrow().contains("LoopStatus"));
    assert_eq!(this.player.borrow().serial, serial);
    assert_eq!(original.snapshot().generation, generation);
    assert_eq!(original.snapshot().state, PlaybackState::Paused);
    assert_eq!(fixture.offers(0), offers);
    assert_eq!(this.model.borrow().queue.current_index(), Some(2));
    capture_native("music-player-modes-active-mpris");
    if let Some(popover) = mode_popover {
        assert!(
            std::process::Command::new("xdotool")
                .args(["key", "--clearmodifiers", "Escape"])
                .status()
                .unwrap()
                .success()
        );
        pump_until(|| !popover.is_visible());
    }

    // Manual Next ignores Repeat One. Paused transport stays paused/ready.
    desktop_cli("playerctl", &["--player", PLAYER, "next"]);
    pump_until(|| {
        this.model.borrow().queue.current_index() == Some(3)
            && this.music_snapshot().is_some_and(|snapshot| {
                matches!(snapshot.state, PlaybackState::Ready | PlaybackState::Paused)
            })
    });
    assert_eq!(desktop_property("CanGoNext").get::<bool>(), Some(false));
    desktop_cli("playerctl", &["--player", PLAYER, "previous"]);
    pump_until(|| {
        this.model.borrow().queue.current_index() == Some(2)
            && this.music_snapshot().is_some_and(|snapshot| {
                matches!(snapshot.state, PlaybackState::Ready | PlaybackState::Paused)
            })
    });
    desktop_cli("playerctl", &["--player", PLAYER, "shuffle", "On"]);
    this.set_info_open(true);
    this.select_track_info(track("44"), None, false);
    assert_pinned_track(this, "44");
    let before = this.player.borrow().serial;
    let offers = fixture.offers(0);
    desktop_cli("playerctl", &["--player", PLAYER, "play"]);
    wait_playing(this, 2);
    pump_until(|| this.widgets.progress.is_sensitive());
    desktop_cli("playerctl", &["--player", PLAYER, "position", "3.8"]);
    pump_until(|| {
        this.player.borrow().serial > before
            && this
                .music_snapshot()
                .is_some_and(|snapshot| snapshot.state == PlaybackState::Playing)
    });
    assert_eq!(
        this.model.borrow().queue.current_index(),
        Some(2),
        "EOS repeats exact duplicate occurrence"
    );
    assert_eq!(fixture.offers(0), offers + 1);
    pump_until(|| seeked.borrow().iter().any(|position| *position < 500_000));
    assert_pinned_track(this, "44");
    desktop_cli("playerctl", &["--player", PLAYER, "pause"]);

    // One refused replacement is shown honestly and never turns Repeat One
    // into an automatic request loop. The previous case used actual worker EOS.
    let refused_offers = fixture.offers(0);
    fixture.reject_media.store(true, Ordering::Release);
    this.skip_music(true, true);
    wait_media_forbidden(this);
    assert_eq!(this.model.borrow().queue.current_index(), Some(2));
    let due = Instant::now() + Duration::from_millis(400);
    pump_until(|| Instant::now() >= due);
    assert_eq!(fixture.offers(0), refused_offers + 1);
    assert_pinned_track(this, "44");
    fixture.reject_media.store(false, Ordering::Release);
    desktop_cli("playerctl", &["--player", PLAYER, "stop"]);
    pump_until(|| this.music_snapshot().is_none());

    // Natural EOS wraps the loaded context; no remote collection fetch occurs.
    this.set_music_shuffle(false);
    this.set_music_repeat(RepeatMode::Queue);
    this.play_music_context(
        PlaybackContext::new(
            ContextKind::Album,
            Some("synthetic-loaded-album".into()),
            vec![track("42"), track("43")],
            1,
            true,
        )
        .unwrap(),
    );
    wait_playing(this, 1);
    this.toggle_queue_panel();
    assert_eq!(
        this.panel_mode.get(),
        super::super::workspace::PanelMode::Queue
    );
    pump_until(|| this.widgets.progress.is_sensitive());
    desktop_cli("playerctl", &["--player", PLAYER, "position", "3.8"]);
    wait_playing(this, 0);
    assert_eq!(
        this.panel_mode.get(),
        super::super::workspace::PanelMode::Queue
    );
    this.set_music_shuffle(true);
    let predicted = this.model.borrow().queue.upcoming_indices()[0];
    pump_until(|| this.widgets.progress.is_sensitive());
    desktop_cli("playerctl", &["--player", PLAYER, "position", "3.8"]);
    wait_playing(this, predicted);
    assert_eq!(
        this.panel_mode.get(),
        super::super::workspace::PanelMode::Queue
    );
    assert_eq!(desktop_property("Shuffle").get::<bool>(), Some(true));
    assert_eq!(desktop_property("LoopStatus").str(), Some("Playlist"));
    let source = this.model.borrow();
    let context = source.queue.context().unwrap();
    assert_eq!(
        context
            .items()
            .iter()
            .map(|track| track.id.as_str())
            .collect::<Vec<_>>(),
        ["42", "43"]
    );
    assert!(context.is_partial());
    drop(source);

    // Prove the real 420px control path, rather than treating an unmapped
    // desktop control as usable. The same mode widgets live in the upward
    // native overflow, and pointer/Space input reaches the shared policy.
    desktop_cli("playerctl", &["--player", PLAYER, "pause"]);
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
    });
    let compact_worker = this.player.borrow().controller.clone().unwrap();
    let compact_serial = this.player.borrow().serial;
    let compact_generation = compact_worker.snapshot().generation;
    let compact_index = this.model.borrow().queue.current_index();
    let compact_offers = fixture.counts.lock().unwrap().offers;
    resize_player_window(&ui, 420, 720);
    pump_until(|| {
        this.widgets
            .shuffle
            .ancestor(gtk::MenuButton::static_type())
            .is_some()
            && !this.widgets.shuffle.is_mapped()
    });
    {
        let menu = this
            .widgets
            .shuffle
            .ancestor(gtk::MenuButton::static_type())
            .and_downcast::<gtk::MenuButton>()
            .expect("420px modes use the native overflow");
        assert_eq!(menu.widget_name(), "music-playback-modes");
        native_click(&ui.window, &menu);
        let popover = menu.popover().unwrap();
        pump_until(|| {
            popover.is_mapped()
                && this.widgets.shuffle.is_mapped()
                && this.widgets.repeat.is_mapped()
        });
        native_click(&ui.window, &this.widgets.shuffle);
        pump_until(|| !this.model.borrow().queue.shuffle());
        assert_eq!(desktop_property("Shuffle").get::<bool>(), Some(false));
        native_click(&ui.window, &this.widgets.repeat);
        pump_until(|| this.model.borrow().queue.repeat_mode() == RepeatMode::One);
        assert_eq!(desktop_property("LoopStatus").str(), Some("Track"));
        {
            let badge = super::super::player_bar::repeat_one_badge(&this.widgets.repeat).unwrap();
            assert_eq!(badge.text(), "1");
            assert!(
                badge.is_mapped(),
                "Repeat One is visible inside the actual overflow"
            );
        }
        capture_native("music-player-modes-compact-repeat-one");
        assert!(this.widgets.repeat.grab_focus());
        assert!(
            std::process::Command::new("xdotool")
                .args(["key", "--clearmodifiers", "space"])
                .status()
                .unwrap()
                .success()
        );
        pump_until(|| this.model.borrow().queue.repeat_mode() == RepeatMode::Off);
        assert_eq!(desktop_property("LoopStatus").str(), Some("None"));
        assert!(
            !super::super::player_bar::repeat_one_badge(&this.widgets.repeat)
                .unwrap()
                .is_visible()
        );
        native_click(&ui.window, &this.widgets.repeat);
        pump_until(|| this.model.borrow().queue.repeat_mode() == RepeatMode::Queue);
        assert_eq!(desktop_property("LoopStatus").str(), Some("Playlist"));
        native_click(&ui.window, &this.widgets.shuffle);
        pump_until(|| this.model.borrow().queue.shuffle());
        assert_eq!(desktop_property("Shuffle").get::<bool>(), Some(true));
        assert!(
            std::process::Command::new("xdotool")
                .args(["key", "--clearmodifiers", "Escape"])
                .status()
                .unwrap()
                .success()
        );
        pump_until(|| !popover.is_visible());
    }
    resize_player_window(&ui, 900, 650);
    assert_eq!(
        this.panel_mode.get(),
        super::super::workspace::PanelMode::Queue
    );
    assert_eq!(this.model.borrow().queue.current_index(), compact_index);
    assert_eq!(this.player.borrow().serial, compact_serial);
    assert_eq!(compact_worker.snapshot().generation, compact_generation);
    assert_eq!(compact_worker.snapshot().state, PlaybackState::Paused);
    assert_eq!(
        fixture.counts.lock().unwrap().offers,
        compact_offers,
        "compact mode input and resize cannot resolve or restart audio"
    );
    drop(compact_worker);
    drop(subscription);
    drop(seek_subscription);
    let cleanup = ui.close_for_smoke();
    pump_until(|| cleanup(false));
    assert!(cleanup(true));
}
