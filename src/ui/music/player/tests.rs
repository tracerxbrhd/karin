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
    account_checks: usize,
    searches: usize,
    lookups: usize,
    offers: [usize; 4],
    descriptors: [usize; 4],
    media: [usize; 4],
    range_requests: usize,
    cache_reads: [usize; 4],
    hold_cache_track: Option<usize>,
    cache_hold_started: bool,
    reject_delivery_track: Option<usize>,
    slow_started: bool,
    wave_sessions: usize,
    wave_batches: usize,
    wave_events: Vec<serde_json::Value>,
    wave_hold_feedback: bool,
    wave_hold_begin: bool,
    wave_hold_batch: bool,
    wave_feedback_status: Option<u16>,
    wave_reject_event: Option<&'static str>,
    wave_initial: Option<Vec<u64>>,
    wave_next: Option<Vec<u64>>,
    wave_unavailable: Vec<u64>,
    disliked_tracks: Vec<u64>,
    reaction_posts: usize,
}

pub(in crate::ui::music) struct Fixture {
    origin: String,
    counts: Arc<Mutex<Counts>>,
    reject_media: Arc<AtomicBool>,
    release: Arc<(Mutex<bool>, Condvar)>,
    stop: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<()>>,
}

impl Fixture {
    pub(in crate::ui::music) fn new() -> Self {
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
        "/account/status"
            | "/search"
            | "/tracks"
            | "/users/1234/likes/tracks"
            | "/users/1234/dislikes/tracks"
    ) || url.path().starts_with("/users/1234/dislikes/tracks/")
        || url.path().starts_with("/tracks/")
        || url.path().starts_with("/rotor/");
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
    let body = if first[0] == "POST" {
        let length: usize = header("content-length").unwrap().parse().unwrap();
        assert!(length < 16 * 1024);
        let mut body = vec![0; length];
        stream.read_exact(&mut body).unwrap();
        body
    } else {
        Vec::new()
    };
    if url.path() == "/rotor/session/new" {
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["seeds"], json!(["user:onyourwave"]));
        let ids = {
            let mut counts = counts.lock().unwrap();
            counts.wave_sessions += 1;
            if counts.wave_hold_begin {
                counts.slow_started = true;
            }
            counts
                .wave_initial
                .clone()
                .unwrap_or_else(|| vec![42, 43, 42])
        };
        if counts.lock().unwrap().wave_hold_begin {
            wait_wave_fixture(release);
        }
        json_response(
            &mut stream,
            wave_response("synthetic-wave-batch-1", &ids, true),
        );
    } else if url.path() == "/rotor/session/synthetic-wave-session/tracks" {
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(body["queue"].as_array().unwrap().len() <= 100);
        let (index, hold) = {
            let mut state = counts.lock().unwrap();
            state.wave_batches += 1;
            if state.wave_hold_batch {
                state.slow_started = true;
            }
            (state.wave_batches, state.wave_hold_batch)
        };
        if hold {
            wait_wave_fixture(release);
        }
        let ids = counts
            .lock()
            .unwrap()
            .wave_next
            .clone()
            .unwrap_or_else(|| vec![44, 43]);
        json_response(
            &mut stream,
            wave_response(&format!("synthetic-wave-batch-{}", index + 1), &ids, false),
        );
    } else if url.path() == "/rotor/session/synthetic-wave-session/feedback" {
        let event: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            matches!(
                event["event"]["type"].as_str(),
                Some("radioStarted" | "trackStarted" | "trackFinished" | "skip")
            ),
            "the modern Wave fixture rejects unsupported outgoing events"
        );
        let (hold, status) = {
            let mut state = counts.lock().unwrap();
            let selected = state
                .wave_reject_event
                .is_none_or(|kind| event["event"]["type"] == kind);
            state.wave_events.push(event);
            if state.wave_hold_feedback {
                state.slow_started = true;
            }
            (
                state.wave_hold_feedback,
                selected.then_some(state.wave_feedback_status).flatten(),
            )
        };
        if hold {
            wait_wave_fixture(release);
        }
        if status == Some(0) {
            return;
        }
        if let Some(status) = status {
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Rejected\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            );
        } else {
            json_response(&mut stream, json!({"result":"accepted"}));
        }
    } else if url.path() == "/account/status" {
        counts.lock().unwrap().account_checks += 1;
        json_response(
            &mut stream,
            json!({"result":{"account":{"uid":1234,"displayName":"Synthetic player account"}}}),
        );
    } else if matches!(
        url.path(),
        "/users/1234/likes/tracks" | "/users/1234/dislikes/tracks"
    ) {
        assert_eq!(first[0], "GET");
        let state = counts.lock().unwrap();
        let tracks = if url.path().contains("/dislikes/") {
            state
                .disliked_tracks
                .iter()
                .map(|id| json!({"id": id}))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        json_response(
            &mut stream,
            json!({"result":{"library":{"uid":1234,"revision":state.reaction_posts + 1,"tracks":tracks}}}),
        );
    } else if matches!(
        url.path(),
        "/users/1234/dislikes/tracks/add-multiple" | "/users/1234/dislikes/tracks/remove"
    ) {
        assert_eq!(first[0], "POST");
        assert_eq!(body, b"track-ids=42");
        let mut state = counts.lock().unwrap();
        state.reaction_posts += 1;
        if url.path().ends_with("add-multiple") {
            assert!(!state.disliked_tracks.contains(&42));
            state.disliked_tracks.push(42);
        } else {
            state.disliked_tracks.retain(|id| *id != 42);
        }
        json_response(
            &mut stream,
            json!({"result":{"revision":state.reaction_posts + 1}}),
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
        let form = reqwest::Url::parse(&format!(
            "http://fixture/?{}",
            String::from_utf8(body).unwrap()
        ))
        .unwrap();
        let ids = form
            .query_pairs()
            .filter(|(key, _)| key == "track-ids")
            .map(|(_, value)| value.parse::<u64>().unwrap())
            .collect::<Vec<_>>();
        assert!(!ids.is_empty());
        assert!(ids.iter().all(|id| (42..=45).contains(id)));
        let unavailable = {
            let mut counts = counts.lock().unwrap();
            counts.lookups += 1;
            counts.wave_unavailable.clone()
        };
        json_response(
            &mut stream,
            json!({"result":ids.into_iter().rev().map(|id| json!({"id":id,"title":"Synthetic remote player track","durationMs":4000,
            "available":!unavailable.contains(&id),"artists":[{"name":"Synthetic artist"}]})).collect::<Vec<_>>()}),
        );
    } else if url.path().starts_with("/tracks/") {
        let track: usize = url.path().split('/').nth(2).unwrap().parse().unwrap();
        assert!((42..=45).contains(&track));
        let rejected = {
            let mut counts = counts.lock().unwrap();
            counts.offers[track - 42] += 1;
            counts.reject_delivery_track == Some(track)
        };
        if rejected {
            let _ = write!(
                stream,
                "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            );
            return;
        }
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
        let cache_copy = header("accept") == Some("audio/mpeg");
        let hold_cache = {
            let mut state = counts.lock().unwrap();
            state.media[track - 42] += 1;
            if range.is_some() {
                state.range_requests += 1;
            }
            if cache_copy {
                assert_eq!(first[0], "GET");
                assert!(
                    range.is_none(),
                    "a legacy duplicate copy requests a complete resource"
                );
                state.cache_reads[track - 42] += 1;
            }
            first[0] == "GET" && state.hold_cache_track == Some(track)
        };
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
            if hold_cache {
                // Hold the one actual playback response after enough original
                // MP3 bytes for native preroll/Playing. The cache tap must
                // remain incomplete without starting a second origin GET.
                let prefix_end = (start + 8 * 1024).min(end + 1);
                assert!(
                    prefix_end < end + 1,
                    "the held response must remain incomplete"
                );
                let _ = stream.write_all(&data[start..prefix_end]);
                let _ = stream.flush();
                counts.lock().unwrap().cache_hold_started = true;
                wait_wave_fixture(release);
                let _ = stream.write_all(&data[prefix_end..=end]);
            } else {
                let _ = stream.write_all(&data[start..=end]);
            }
        }
    } else {
        panic!("unexpected synthetic player request");
    }
}

fn wave_response(batch: &str, ids: &[u64], initial: bool) -> serde_json::Value {
    let mut value = json!({"result":{"batchId":batch,"sequence":ids.iter().map(|id| json!({"type":"track","track":{"id":id}})).collect::<Vec<_>>()}});
    if initial {
        value["result"]["radioSessionId"] = "synthetic-wave-session".into();
    }
    value
}
fn wait_wave_fixture(release: &(Mutex<bool>, Condvar)) {
    let released = release.0.lock().unwrap();
    let (released, _) = release
        .1
        .wait_timeout_while(released, Duration::from_secs(8), |ready| !*ready)
        .unwrap();
    assert!(*released, "bounded synthetic Wave response gate released");
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

pub(in crate::ui::music) fn generated_mp3() -> Vec<u8> {
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

pub(in crate::ui::music) fn resize_player_window(
    ui: &crate::ui::MainWindow,
    width: i32,
    height: i32,
) {
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

pub(in crate::ui::music) fn player_window(
    fixture: &Fixture,
) -> (adw::Application, crate::ui::MainWindow) {
    player_window_with_account(fixture, true)
}

fn player_window_with_account(
    fixture: &Fixture,
    authenticated: bool,
) -> (adw::Application, crate::ui::MainWindow) {
    static NEXT_WINDOW: AtomicU64 = AtomicU64::new(0);
    // Each registered application exports its own D-Bus object path, even
    // with NON_UNIQUE. Keep independent windows distinct on one test bus.
    let application_id = format!(
        "io.github.tracerxbrhd.Karin.NativeMusicPlaybackFixture.Window{}",
        NEXT_WINDOW.fetch_add(1, Ordering::Relaxed)
    );
    let service = MusicService::for_test(&fixture.origin);
    let token = AccessToken::for_test("synthetic-native-music-token", Duration::from_secs(300));
    if authenticated {
        service.account(&token).unwrap();
    }
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
    if authenticated {
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
    assert_eq!(
        fixture.counts.lock().unwrap().cache_reads,
        [0; 4],
        "ordinary ungranted playback creates no storage-copy HTTP"
    );
    native_granted_audio_cache_fill_reuse_and_retirement();
}

// Executes inside the existing native player selector. The positive authority
// exists only in cfg(test); its original MP3 cannot establish service rights.
fn native_granted_audio_cache_fill_reuse_and_retirement() {
    use crate::audio_cache::{
        AudioCache, CacheError, CachePolicy, OnlineStreamAuthority, ResourceKey,
        test_support::SyntheticAuthority,
    };
    let fixture = Fixture::new();
    let (_app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    let directory = std::env::temp_dir().join(format!(
        "karin-native-cache-{}-{}",
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
    let authority = Arc::new(SyntheticAuthority::default());
    let token = this.model.borrow().token.as_ref().unwrap().clone();
    let scope = this.service.as_ref().unwrap().cache_scope(&token).unwrap();
    let key = |id| ResourceKey::track(id).unwrap();
    authority.allow(&scope, &key("42"));
    authority.allow(&scope, &key("43"));
    let budget = crate::cache_budget::CacheBudget::new(
        directory.0.join("metadata"),
        directory.0.join("audio"),
        true,
        Some(2 * 1024 * 1024 * 1024),
    )
    .unwrap();
    let cache = AudioCache::open_with_budget(
        directory.0.join("audio"),
        Arc::new(OnlineStreamAuthority::with_storage_permission(authority)),
        // A shared budget supersedes the former separate audio quota.
        CachePolicy { max_bytes: 1 },
        budget.clone(),
    )
    .unwrap();
    this.install_audio_cache_for_test(cache.clone(), scope);
    let lease = cache.lease().unwrap();
    this.model.borrow_mut().queue.replace_context(
        PlaybackContext::new(
            crate::queue::ContextKind::Playlist,
            Some("synthetic-cache-playlist".into()),
            vec![track("42"), track("43"), track("44")],
            0,
            false,
        )
        .unwrap(),
    );
    this.render_queue();
    this.request_music(false, None);
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Ready)
            && this.player.borrow().preparing.is_none()
    });
    assert!(this.player.borrow().cache_fill_pending.is_some());
    assert_eq!(fixture.counts.lock().unwrap().cache_reads[0], 0);
    assert_eq!(cache.stats().unwrap().entries, 0);
    this.widgets.play.emit_clicked();
    wait_playing(this, 0);
    pump_until(|| {
        cache.stats().is_ok_and(|stats| stats.entries == 1)
            && !this.player.borrow().cache_fill_running
    });
    assert_eq!(cache.stats().unwrap().entries, 1);
    assert_eq!(budget.usage().unwrap().bytes, cache.stats().unwrap().bytes);
    assert!(cache.get(&lease, key("42")).unwrap().is_some());
    this.pause_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == PlaybackState::Paused)
    });
    let first_media = fixture.counts.lock().unwrap().media[0];
    let first_offers = fixture.offers(0);
    let retired = this.player.borrow().controller.clone().unwrap();
    this.widgets.stop.emit_clicked();
    pump_until(|| !retired.snapshot().worker_running);
    this.widgets.play.emit_clicked();
    wait_playing(this, 0);
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| !s.playing_time.is_zero())
    });
    assert_eq!(
        fixture.offers(0),
        first_offers + 1,
        "a hit still revalidates the current online resource"
    );
    assert_eq!(
        fixture.counts.lock().unwrap().media[0],
        first_media,
        "a valid hit plays locally without a second media or cache GET"
    );
    assert_eq!(fixture.counts.lock().unwrap().cache_reads[0], 0);
    assert!(
        !this.player.borrow().cache_fill_running
            && this.player.borrow().cache_fill_pending.is_none()
    );
    this.pause_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == PlaybackState::Paused)
    });

    // Recreate the real controller, MusicService account validation, namespace
    // owner and independent storage authority. Persistent bytes alone cannot
    // authorize local playback, and every hit must resolve the online source.
    let checks_before_restart = fixture.counts.lock().unwrap().account_checks;
    this.close();
    ui.window.close();
    drop(cache);
    drop(ui);
    drop(_app);
    let (_app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    assert_eq!(
        fixture.counts.lock().unwrap().account_checks,
        checks_before_restart + 1
    );
    let token = this.model.borrow().token.as_ref().unwrap().clone();
    let scope = this.service.as_ref().unwrap().cache_scope(&token).unwrap();
    let permission = Arc::new(SyntheticAuthority::default());
    permission.allow(&scope, &key("42"));
    permission.allow(&scope, &key("43"));
    let cache = AudioCache::open_with_budget(
        directory.0.join("audio"),
        Arc::new(OnlineStreamAuthority::with_storage_permission(permission)),
        CachePolicy { max_bytes: 1 },
        budget.clone(),
    )
    .unwrap();
    this.install_audio_cache_for_test(cache.clone(), scope);
    let lease = cache.lease().unwrap();
    assert_eq!(cache.stats().unwrap().entries, 1);
    assert!(matches!(
        cache.get(&lease, key("42")),
        Err(CacheError::NoGrant)
    ));

    fixture.counts.lock().unwrap().reject_delivery_track = Some(42);
    let offers_before_denial = fixture.offers(0);
    this.play_music_context(
        PlaybackContext::new(
            crate::queue::ContextKind::Playlist,
            Some("synthetic-cache-playlist".into()),
            vec![track("42"), track("43"), track("44")],
            0,
            false,
        )
        .unwrap(),
    );
    pump_until(|| {
        !this.player.borrow().resolving
            && matches!(this.player.borrow().failure, Some(Failure::Delivery(_)))
    });
    assert_eq!(fixture.offers(0), offers_before_denial + 1);
    assert_eq!(fixture.counts.lock().unwrap().media[0], first_media);
    assert!(matches!(
        cache.get(&lease, key("42")),
        Err(CacheError::NoGrant)
    ));
    assert!(this.player.borrow().controller.is_none());

    fixture.counts.lock().unwrap().reject_delivery_track = None;
    this.widgets.play.emit_clicked();
    wait_playing(this, 0);
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| !s.playing_time.is_zero())
    });
    assert_eq!(fixture.offers(0), offers_before_denial + 2);
    assert_eq!(fixture.counts.lock().unwrap().media[0], first_media);
    assert_eq!(fixture.counts.lock().unwrap().cache_reads[0], 0);
    assert_eq!(budget.usage().unwrap().bytes, cache.stats().unwrap().bytes);
    this.pause_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == PlaybackState::Paused)
    });

    // A previous successful online check must not survive a later denial.
    // The old record stays on disk, but cannot override the current service.
    fixture.counts.lock().unwrap().reject_delivery_track = Some(42);
    let retired = this.player.borrow().controller.clone().unwrap();
    this.widgets.stop.emit_clicked();
    pump_until(|| !retired.snapshot().worker_running);
    this.widgets.play.emit_clicked();
    pump_until(|| {
        !this.player.borrow().resolving
            && matches!(this.player.borrow().failure, Some(Failure::Delivery(_)))
    });
    assert_eq!(fixture.counts.lock().unwrap().media[0], first_media);
    assert!(matches!(
        cache.get(&lease, key("42")),
        Err(CacheError::NoGrant)
    ));
    assert_eq!(cache.stats().unwrap().entries, 1);
    fixture.counts.lock().unwrap().reject_delivery_track = None;
    this.widgets.play.emit_clicked();
    wait_playing(this, 0);
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| !s.playing_time.is_zero())
    });
    assert_eq!(fixture.counts.lock().unwrap().media[0], first_media);
    assert_eq!(fixture.counts.lock().unwrap().cache_reads[0], 0);
    this.pause_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == PlaybackState::Paused)
    });

    // An independently absent grant leaves a perfectly valid stream playable,
    // without touching the cache or fabricating an offline permission.
    this.play_music_queue(2);
    wait_playing(this, 2);
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| !s.playing_time.is_zero())
    });
    assert_eq!(fixture.counts.lock().unwrap().cache_reads[2], 0);
    assert_eq!(cache.stats().unwrap().entries, 1);
    assert!(
        !this.player.borrow().cache_fill_running
            && this.player.borrow().cache_fill_pending.is_none()
    );

    // Hold the actual playback transfer after its preroll prefix. GTK timers,
    // volume and a cached source change remain live while capture awaits the
    // rest of this same HTTP response; no separate cache GET is allowed.
    fixture.hold_descriptor();
    fixture.counts.lock().unwrap().hold_cache_track = Some(43);
    this.play_music_queue(1);
    wait_playing(this, 1);
    pump_until(|| {
        fixture.counts.lock().unwrap().cache_hold_started && this.player.borrow().cache_fill_running
    });
    assert!(this.player.borrow().cache_fill_running);
    let heartbeat = Rc::new(Cell::new(false));
    let observed = heartbeat.clone();
    glib::timeout_add_local_once(Duration::from_millis(30), move || observed.set(true));
    this.set_music_volume(0.31);
    pump_until(|| {
        heartbeat.get()
            && this
                .music_snapshot()
                .is_some_and(|s| (s.volume - 0.31).abs() < 0.001)
    });
    let cancellation = this.player.borrow().cache_fill_cancel.clone().unwrap();
    this.play_music_queue(0);
    wait_playing(this, 0);
    assert!(cancellation.is_cancelled());
    assert_eq!(
        fixture.counts.lock().unwrap().cache_reads[1],
        0,
        "capture must not start a duplicate origin GET beside the held playback transfer"
    );
    fixture.release_descriptor();
    pump_until(|| !this.player.borrow().cache_fill_running);
    assert!(
        matches!(
            cache.get(&lease, key("43")),
            Ok(None) | Err(CacheError::NoGrant)
        ),
        "a cancelled response cannot publish late bytes"
    );
    assert_eq!(cache.stats().unwrap().entries, 1);
    assert_eq!(fixture.counts.lock().unwrap().media[0], first_media);

    fixture.hold_descriptor();
    fixture.counts.lock().unwrap().cache_hold_started = false;
    this.play_music_queue(1);
    wait_playing(this, 1);
    pump_until(|| {
        fixture.counts.lock().unwrap().cache_hold_started && this.player.borrow().cache_fill_running
    });
    let cancellation = this.player.borrow().cache_fill_cancel.clone().unwrap();
    let retired = this.player.borrow().controller.clone().unwrap();
    this.logout();
    assert!(cancellation.is_cancelled());
    fixture.release_descriptor();
    pump_until(|| !this.player.borrow().cache_fill_running && !retired.snapshot().worker_running);
    assert!(this.model.borrow().token.is_none());
    assert!(this.model.borrow().queue.items().is_empty());
    assert!(
        cache.get(&lease, key("42")).is_err(),
        "logout fences old local hits"
    );
    pump_until(|| cache.stats().is_ok_and(|stats| stats.entries == 0));
    assert_eq!(fixture.counts.lock().unwrap().cache_reads, [0; 4]);
    assert_eq!(
        fixture.counts.lock().unwrap().media[1],
        2,
        "each interrupted playback uses just its one origin transfer"
    );
    this.close();
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
    // Neither idle startup nor a merely selected queue entry is playback.
    pump_until(|| ui.window.is_mapped() && !this.widgets.player_revealer.reveals_child());
    assert!(!this.widgets.player_revealer.is_child_revealed());
    this.enqueue(track("42"));
    this.render_queue();
    assert!(!this.widgets.player_revealer.reveals_child());
    assert!(!this.widgets.current_art_button.is_mapped());
    assert_eq!(fixture.offers(0), 0);
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
    // Actual first reveal under reduced motion maps the direct Like control
    // synchronously. Its GET must be deferred until render borrows unwind.
    let settings = gtk::Settings::default().unwrap();
    let animations_before = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);
    eprintln!("Karin synthetic context phase=play-clicked-duplicate-reduced-motion");
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
    pump_until(|| this.widgets.player_revealer.is_child_revealed());
    settings.set_gtk_enable_animations(animations_before);
    assert!(this.widgets.current_art_button.is_mapped());
    assert_eq!(
        this.widgets.player_revealer.transition_type(),
        gtk::RevealerTransitionType::SlideUp
    );
    capture_native("footer-real-playback-visible");
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
    assert!(
        this.widgets.player_revealer.reveals_child(),
        "Pause retains the actual active footer"
    );
    let input_serial = this.player.borrow().serial;
    let input_generation = this
        .player
        .borrow()
        .controller
        .as_ref()
        .unwrap()
        .snapshot()
        .generation;
    let input_offers = fixture.counts.lock().unwrap().offers;
    // Click the actual title and artist regions, not just the small cover.
    this.set_info_open(false);
    native_click(&ui.window, &this.widgets.current);
    pump_until(|| this.info_is_open());
    assert_pinned_track(this, "42");
    this.toggle_queue_panel();
    assert_eq!(
        this.panel_mode.get(),
        super::super::workspace::PanelMode::Queue
    );
    native_click(&ui.window, &this.widgets.current_artist);
    pump_until(|| this.panel_mode.get() == super::super::workspace::PanelMode::TrackInfo);
    assert_pinned_track(this, "42");
    for key in ["Return", "space"] {
        this.set_info_open(false);
        assert!(this.widgets.current_art_button.grab_focus());
        assert!(
            std::process::Command::new("xdotool")
                .args(["key", "--clearmodifiers", key])
                .status()
                .unwrap()
                .success()
        );
        pump_until(|| this.info_is_open());
        assert_pinned_track(this, "42");
    }
    assert_eq!(this.player.borrow().serial, input_serial);
    assert_eq!(
        this.player
            .borrow()
            .controller
            .as_ref()
            .unwrap()
            .snapshot()
            .generation,
        input_generation
    );
    assert_eq!(fixture.counts.lock().unwrap().offers, input_offers);
    assert_eq!(this.music_snapshot().unwrap().state, PlaybackState::Paused);
    assert!(this.widgets.player_revealer.reveals_child());
    capture_native("footer-paused-whole-metadata-input");
    this.select_track_info(track("43"), None, false);
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
    pump_until(|| {
        !this.widgets.player_revealer.is_child_revealed()
            && !this.widgets.current_art_button.is_mapped()
            && ui.music.player_bar.height() <= 1
    });
    assert!(!this.widgets.player_revealer.reveals_child());
    capture_native("footer-confirmed-stop-hidden");
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
    assert!(!this.widgets.player_revealer.reveals_child());
    eprintln!("Karin synthetic context phase=play-next-from-stopped");
    this.play_music();
    wait_playing(this, 2);
    pump_until(|| this.widgets.player_revealer.is_child_revealed());

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
    {
        let spinner = super::super::player_bar::play_loading_spinner(&this.widgets.play).unwrap();
        pump_until(|| spinner.is_mapped() && spinner.is_spinning());
        assert!(this.widgets.player_revealer.reveals_child());
        assert!(!this.widgets.play.is_sensitive());
        assert!(
            !this.widgets.player_status.is_visible(),
            "loading must not add a row that disappears during reveal"
        );
        assert_eq!(
            this.widgets.play.tooltip_text().as_deref(),
            Some("Получаем аудио…")
        );
    }
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
    assert!(
        !super::super::player_bar::play_loading_spinner(&this.widgets.play)
            .unwrap()
            .is_spinning()
    );
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

fn inline_transport_controls(this: &Controller) -> [gtk::Widget; 5] {
    [
        this.widgets.shuffle.clone().upcast(),
        this.widgets.previous.clone().upcast(),
        this.widgets.play.clone().upcast(),
        this.widgets.next.clone().upcast(),
        this.widgets.repeat.clone().upcast(),
    ]
}

fn inline_transport_target_ready(ui: &crate::ui::MainWindow, widget: &gtk::Widget) -> bool {
    // GtkWidget::width/height exclude CSS padding; the actual native button
    // bounds, also used by pointer input, define its full 44px hit target.
    widget.is_mapped()
        && super::super::test_input::bounds_in_window(&ui.window, widget)
            .is_some_and(|bounds| bounds.width() >= 44.0 && bounds.height() >= 44.0)
}

fn assert_inline_transport_target(ui: &crate::ui::MainWindow, widget: &gtk::Widget) {
    let bounds = super::super::test_input::bounds_in_window(&ui.window, widget);
    assert!(
        inline_transport_target_ready(ui, widget),
        "inline control name={} mapped={} content={}x{} bounds={bounds:?}",
        widget.widget_name(),
        widget.is_mapped(),
        widget.width(),
        widget.height()
    );
    let bounds = bounds.unwrap();
    let flags = if widget.is_sensitive() {
        gtk::PickFlags::DEFAULT
    } else {
        gtk::PickFlags::INSENSITIVE
    };
    let target = ui.window.pick(
        f64::from(bounds.x() + bounds.width() / 2.0),
        f64::from(bounds.y() + bounds.height() / 2.0),
        flags,
    );
    assert!(
        target.is_some_and(|target| target == *widget || target.is_ancestor(widget)),
        "inline control has no native pointer target: name={} bounds={bounds:?}",
        widget.widget_name()
    );
}

fn inline_transport_layout_ready(ui: &crate::ui::MainWindow) -> bool {
    let this = &ui.music.controller;
    this.widgets.player_revealer.is_child_revealed()
        && this.widgets.player_revealer.child().is_some_and(|content| {
            let available =
                ui.music.player_bar.width() - content.margin_start() - content.margin_end();
            available > 0 && content.has_css_class("music-player-compact") == (available < 720)
        })
        && inline_transport_controls(this)
            .iter()
            .all(|widget| inline_transport_target_ready(ui, widget))
        && ui.verify_layout().is_ok()
}

/// A GDK resize notification precedes its descendant allocation. Observe the
/// actual responsive mode, usable controls and completed native frames before
/// asserting or sending pointer input; mapping alone is already true in wide.
fn wait_inline_transport_layout(ui: &crate::ui::MainWindow) {
    let this = &ui.music.controller;
    let previous = RefCell::new(None);
    pump_until(|| {
        let ready = inline_transport_layout_ready(ui);
        let controls = inline_transport_controls(this)
            .iter()
            .map(|widget| {
                format!(
                    "{} mapped={} content={}x{} bounds={:?}",
                    widget.widget_name(),
                    widget.is_mapped(),
                    widget.width(),
                    widget.height(),
                    super::super::test_input::bounds_in_window(&ui.window, widget),
                )
            })
            .collect::<Vec<_>>();
        if previous.borrow().as_ref() != Some(&controls) {
            eprintln!(
                "Karin synthetic inline transport: ready={ready} window={}x{} controls={controls:?}",
                ui.window.width(),
                ui.window.height()
            );
            previous.replace(Some(controls));
        }
        ready
    });
    for widget in inline_transport_controls(this) {
        super::super::test_input::wait_for_layout(&ui.window, &widget);
        assert_inline_transport_target(ui, &widget);
    }
    pump_until(|| inline_transport_layout_ready(ui));
}

/// Wait on actual native state, not an arbitrary delay: the panel must finish
/// sliding and the footer must finish its responsive timeline allocation.
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
        panel_ready && inline_transport_layout_ready(ui)
    };
    pump_until(ready);
    wait_inline_transport_layout(ui);
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
    // The new footer is honestly absent before any playback attempt. Establish
    // the anchoring baseline from the ordinary real MP3/Pause path, not an
    // unmapped control or a forced reveal, before retrying with a refused source.
    assert!(!this.widgets.player_revealer.reveals_child());
    this.widgets.play.emit_clicked();
    wait_playing(this, 0);
    this.pause_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
    });
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
    this.stop_music();
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

/// Exercise the production queue bridge with actual MP3 preroll and session
/// D-Bus. Equal service IDs are different manual occurrences; reindexing a
/// retained occurrence must not change its public opaque path or restart audio.
fn manual_occurrence_mpris_reindexing(this: &Rc<Controller>) {
    use std::collections::BTreeMap;

    let metadata_path = || {
        desktop_property("Metadata")
            .get::<BTreeMap<String, glib::Variant>>()
            .unwrap()["mpris:trackid"]
            .get::<glib::variant::ObjectPath>()
            .unwrap()
    };
    this.pause_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
    });
    {
        let mut model = this.model.borrow_mut();
        model.queue.append(track("42")).unwrap();
        model.queue.append(track("42")).unwrap();
        model.queue.append(track("43")).unwrap();
    }
    this.render_queue();
    this.select_music_queue(4);
    pump_until(|| {
        this.model.borrow().queue.current_index() == Some(4)
            && this
                .music_snapshot()
                .is_some_and(|snapshot| snapshot.state == PlaybackState::Ready && snapshot.seekable)
    });
    assert_pinned_track(this, "42");
    let worker = this.player.borrow().controller.clone().unwrap();
    let generation = worker.snapshot().generation;
    let playback_serial = this.player.borrow().serial;
    let path = metadata_path();
    this.select_track_info(track("43"), None, false);

    let bus = desktop_background(|| {
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap()
    });
    let paths = Rc::new(RefCell::new(Vec::new()));
    let observed = paths.clone();
    let subscription = bus.subscribe_to_signal(
        Some(crate::mpris::BUS_NAME),
        Some("org.freedesktop.DBus.Properties"),
        Some("PropertiesChanged"),
        Some("/org/mpris/MediaPlayer2"),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let (interface, changed, _) = signal
                .parameters
                .get::<(String, BTreeMap<String, glib::Variant>, Vec<String>)>()
                .unwrap();
            if interface == "org.mpris.MediaPlayer2.Player"
                && let Some(metadata) = changed
                    .get("Metadata")
                    .and_then(|value| value.get::<BTreeMap<String, glib::Variant>>())
                && let Some(path) = metadata
                    .get("mpris:trackid")
                    .and_then(|value| value.get::<glib::variant::ObjectPath>())
            {
                observed.borrow_mut().push(path);
            }
        },
    );

    this.remove_music_queue(0);
    assert_eq!(this.model.borrow().queue.current_index(), Some(3));
    assert_eq!(metadata_path(), path, "reindexing is not a track change");
    assert_pinned_track(this, "43");
    this.remove_music_queue(4);
    assert_eq!(
        metadata_path(),
        path,
        "removing the next row keeps identity"
    );
    assert_pinned_track(this, "43");
    assert_eq!(this.player.borrow().serial, playback_serial);
    assert_eq!(worker.snapshot().generation, generation);
    assert_eq!(worker.snapshot().state, PlaybackState::Ready);
    assert!(worker.snapshot().worker_running);
    assert!(paths.borrow().iter().all(|observed| observed == &path));

    // A previously published SetPosition remains valid after removing a row
    // before this manual occurrence, and is acknowledged by the same worker.
    let seek_serial = worker.snapshot().seek_serial;
    desktop_method("SetPosition", (path.clone(), 1_000_000_i64).to_variant());
    pump_until(|| worker.snapshot().seek_serial > seek_serial);
    assert_eq!(
        worker.snapshot().seek_position,
        Some(Duration::from_secs(1))
    );
    assert_eq!(metadata_path(), path);
    assert_pinned_track(this, "43");

    this.skip_music(false, false);
    pump_until(|| {
        this.model.borrow().queue.current_index() == Some(2)
            && this
                .music_snapshot()
                .is_some_and(|snapshot| snapshot.state == PlaybackState::Ready && snapshot.seekable)
    });
    let previous_path = metadata_path();
    assert_ne!(previous_path, path, "equal IDs remain distinct occurrences");
    pump_until(|| paths.borrow().contains(&previous_path));
    assert_pinned_track(this, "42");
    let previous_worker = this.player.borrow().controller.clone().unwrap();
    let seek_serial = previous_worker.snapshot().seek_serial;
    desktop_method("SetPosition", (path, 2_000_000_i64).to_variant());
    assert_eq!(previous_worker.snapshot().seek_serial, seek_serial);

    this.remove_music_queue(2);
    pump_until(|| {
        !previous_worker.snapshot().worker_running
            && this
                .music_snapshot()
                .is_some_and(|snapshot| snapshot.state == PlaybackState::Ready && snapshot.seekable)
    });
    let successor_path = metadata_path();
    assert_ne!(successor_path, previous_path);
    pump_until(|| paths.borrow().contains(&successor_path));
    assert_pinned_track(this, "42");
    drop(subscription);
    eprintln!("Karin synthetic MPRIS phase=manual-occurrence-reindexing-and-removal");
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
    manual_occurrence_mpris_reindexing(this);
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

    // Prove the actual 420px path: all five transport controls now fit one
    // native group. Pointer/Space still dispatch to the same policy and never
    // replace or restart the paused worker while the layout adapts.
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
    wait_inline_transport_layout(&ui);
    {
        let transport = this.widgets.shuffle.parent().unwrap();
        assert_eq!(transport.widget_name(), "music-transport");
        assert_eq!(this.widgets.repeat.parent().as_ref(), Some(&transport));
        for widget in inline_transport_controls(this) {
            assert_inline_transport_target(&ui, &widget);
        }
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
                "Repeat One remains visible in the compact transport"
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
        assert_eq!(
            this.panel_mode.get(),
            super::super::workspace::PanelMode::Queue
        );
        assert_eq!(this.widgets.shuffle.parent().as_ref(), Some(&transport));
        assert!(this.widgets.shuffle.is_mapped() && this.widgets.repeat.is_mapped());
    }
    resize_player_window(&ui, 900, 650);
    wait_inline_transport_layout(&ui);
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
    // The seek control is the same native widget in both timeline placements.
    // Resizing while it owns keyboard focus must neither drop that focus nor
    // resolve/restart the already paused source.
    resize_player_window(&ui, 850, 750);
    wait_inline_transport_layout(&ui);
    assert!(this.widgets.progress.is_sensitive());
    assert!(this.widgets.progress.grab_focus());
    super::super::test_input::wait_for_layout(&ui.window, &this.widgets.progress);
    for (width, height) in [(420, 720), (850, 750)] {
        resize_player_window(&ui, width, height);
        wait_inline_transport_layout(&ui);
        super::super::test_input::wait_for_layout(&ui.window, &this.widgets.progress);
        assert!(
            gtk::prelude::GtkWindowExt::focus(&ui.window).is_some_and(|focus| {
                focus == this.widgets.progress || focus.is_ancestor(&this.widgets.progress)
            }),
            "responsive seek lost focus at {width}x{height}: focus={:?}",
            gtk::prelude::GtkWindowExt::focus(&ui.window)
                .map(|focus| (focus.type_().name(), focus.widget_name().to_string()))
        );
        assert_eq!(this.player.borrow().serial, compact_serial);
        assert_eq!(compact_worker.snapshot().generation, compact_generation);
        assert_eq!(compact_worker.snapshot().state, PlaybackState::Paused);
        assert_eq!(this.model.borrow().queue.current_index(), compact_index);
        assert_eq!(fixture.counts.lock().unwrap().offers, compact_offers);
    }
    drop(compact_worker);
    drop(subscription);
    drop(seek_subscription);
    let cleanup = ui.close_for_smoke();
    pump_until(|| cleanup(false));
    assert!(cleanup(true));
}

#[test]
#[ignore = "requires isolated session D-Bus, Xvfb, playerctl and actual generated MP3 playback"]
fn native_wave_shared_player_feedback_batches_and_cleanup() {
    use super::super::radio::{RadioFailure, RadioPhase, RadioStage};
    use crate::queue::{ContextKind, RepeatMode};
    gtk::init().unwrap();
    adw::init().unwrap();
    // Normal startup needs no hidden Wave enable call. Without a validated
    // account, neither startup nor a late activation may admit radio requests.
    let anonymous = Fixture::new();
    let (_anonymous_app, anonymous_ui) = player_window_with_account(&anonymous, false);
    let signed_out = &anonymous_ui.music.controller;
    super::super::test_input::wait_for_layout(
        &anonymous_ui.window,
        &signed_out.widgets.home_signedout,
    );
    assert_eq!(signed_out.radio_status().phase, RadioPhase::Ready);
    assert!(!signed_out.radio_status().authenticated);
    assert!(!signed_out.widgets.home_wave.primary.is_sensitive());
    signed_out.start_wave();
    signed_out.activate_wave_hero();
    assert!(signed_out.radio_idle_for_test());
    assert!(signed_out.wave_session_for_test().is_none());
    {
        let counts = anonymous.counts.lock().unwrap();
        assert_eq!((counts.wave_sessions, counts.wave_batches), (0, 0));
        assert!(counts.wave_events.is_empty());
    }
    signed_out.close();
    anonymous_ui.window.close();

    let fixture = Fixture::new();
    let (_app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    pump_until(|| this.widgets.home_wave.primary.is_mapped());
    assert_eq!(this.radio_status().phase, RadioPhase::Ready);
    assert!(this.radio_status().authenticated);
    assert!(this.widgets.home_wave.primary.is_sensitive());
    assert!(this.wave_session_for_test().is_none());
    assert!(this.model.borrow().queue.items().is_empty());
    assert!(this.music_snapshot().is_none());
    {
        let counts = fixture.counts.lock().unwrap();
        assert_eq!((counts.wave_sessions, counts.wave_batches), (0, 0));
        assert!(
            counts.wave_events.is_empty(),
            "ordinary Home does not auto-start Wave"
        );
    }
    this.set_music_shuffle(true);
    this.set_music_repeat(RepeatMode::Queue);
    native_click(&ui.window, &this.widgets.home_wave.primary);
    wait_playing(this, 0);
    pump_until(|| {
        let counts = fixture.counts.lock().unwrap();
        counts.wave_events.len() >= 2
            && counts.wave_batches == 1
            && this.model.borrow().queue.items().len() == 5
    });
    assert_eq!(
        this.model.borrow().queue.context().unwrap().source_kind(),
        ContextKind::Wave
    );
    assert!(!this.model.borrow().queue.shuffle());
    assert_eq!(this.model.borrow().queue.repeat_mode(), RepeatMode::Off);
    assert!(!this.widgets.shuffle.is_sensitive());
    assert!(!this.widgets.repeat.is_sensitive());
    this.set_music_shuffle(true);
    this.set_music_repeat(RepeatMode::One);
    assert!(!this.model.borrow().queue.shuffle());
    assert_eq!(this.model.borrow().queue.repeat_mode(), RepeatMode::Off);
    let first_occurrence = this
        .model
        .borrow()
        .queue
        .current_occurrence_id()
        .unwrap()
        .clone();
    let first_context = this.model.borrow().queue.context().unwrap().clone();
    let first_serial = this.music_serial();
    let engine = this.player.borrow().controller.clone().unwrap();
    let metadata_path = || {
        let metadata = desktop_property("Metadata")
            .get::<std::collections::BTreeMap<String, glib::Variant>>()
            .unwrap();
        assert!(!metadata.contains_key("xesam:url"));
        metadata["mpris:trackid"]
            .get::<glib::variant::ObjectPath>()
            .unwrap()
    };
    pump_until(|| this.system_media_available());
    let first_path = metadata_path();
    assert!(!desktop_property("Shuffle").get::<bool>().unwrap());
    assert_eq!(
        desktop_property("LoopStatus").get::<String>().unwrap(),
        "None"
    );
    let home_serial = this.music_serial();
    let home_sessions = fixture.counts.lock().unwrap().wave_sessions;
    native_click(&ui.window, &this.widgets.nav_liked);
    pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("library"));
    native_click(&ui.window, &this.widgets.nav_wave);
    pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("home"));
    assert_eq!(this.music_serial(), home_serial);
    assert_eq!(fixture.counts.lock().unwrap().wave_sessions, home_sessions);
    let hero_clicks = Rc::new(Cell::new(0));
    let observed_clicks = hero_clicks.clone();
    this.widgets.home_wave.primary.connect_clicked(move |_| {
        observed_clicks.set(observed_clicks.get() + 1);
    });
    assert_inline_transport_target(&ui, this.widgets.home_wave.primary.upcast_ref());
    native_click(&ui.window, &this.widgets.home_wave.primary);
    pump_until(|| hero_clicks.get() == 1);
    pump_until(|| engine.snapshot().state == PlaybackState::Paused);
    let paused = engine.snapshot().playing_time;
    let until = Instant::now() + Duration::from_millis(300);
    pump_until(|| Instant::now() >= until);
    assert_eq!(
        engine.snapshot().playing_time,
        paused,
        "pause cannot add heard time"
    );
    let seek = engine.snapshot().seek_serial;
    this.seek_music(Duration::from_secs(2));
    pump_until(|| engine.snapshot().seek_serial > seek);
    assert_eq!(
        engine.snapshot().playing_time,
        paused,
        "seek distance is not listening"
    );
    this.set_info_open(true);
    let selected = this.model.borrow().queue.current().unwrap().clone();
    this.handle_track_pane_action(super::super::track_pane::PaneAction::Play(selected));
    pump_until(|| engine.snapshot().state == PlaybackState::Playing);
    assert_eq!(
        this.music_serial(),
        first_serial,
        "Track Info resumes the existing worker"
    );
    assert_eq!(
        this.model.borrow().queue.current_occurrence_id(),
        Some(&first_occurrence)
    );
    assert_eq!(metadata_path(), first_path);
    this.set_music_volume(0.4);
    pump_until(|| (engine.snapshot().volume - 0.4).abs() < f64::EPSILON);
    this.seek_music(Duration::from_millis(3_800));
    wait_playing(this, 1);
    pump_until(|| {
        fixture
            .counts
            .lock()
            .unwrap()
            .wave_events
            .iter()
            .any(|event| event["event"]["type"] == "trackFinished")
    });
    {
        let counts = fixture.counts.lock().unwrap();
        assert_eq!(counts.wave_sessions, 1);
        let finished = counts
            .wave_events
            .iter()
            .filter(|event| event["event"]["type"] == "trackFinished")
            .collect::<Vec<_>>();
        assert_eq!(
            finished.len(),
            1,
            "EOS is consumed once before selecting another occurrence"
        );
        assert_eq!(finished[0]["batchId"], "synthetic-wave-batch-1");
        assert!(
            finished[0]["event"]["totalPlayedSeconds"].as_f64().unwrap() < 3.5,
            "seeked-over audio and pause are not reported as heard"
        );
    }
    this.play_music_queue(2);
    wait_playing(this, 2);
    assert_ne!(
        this.model.borrow().queue.current_occurrence_id(),
        Some(&first_occurrence)
    );
    assert_ne!(
        metadata_path(),
        first_path,
        "same track ID in another occurrence has another opaque path"
    );
    // A confirmed account Dislike uses this same explicit Next path. The
    // duplicate provider ID still belongs to occurrence2 and its original
    // batch; no recommendation-only dislike/radio event is introduced.
    let dislike = {
        let slot = this.widgets.current_like_slot.first_child().unwrap();
        let button = slot.first_child().unwrap().next_sibling().unwrap();
        assert_eq!(button.widget_name(), "music-current-dislike");
        button.downcast::<gtk::Button>().unwrap()
    };
    pump_until(|| dislike.is_mapped() && dislike.is_sensitive());
    pump_until(|| {
        fixture
            .counts
            .lock()
            .unwrap()
            .wave_events
            .iter()
            .filter(|event| {
                event["event"]["type"] == "trackStarted" && event["event"]["trackId"] == "42"
            })
            .count()
            == 2
            && this.radio_idle_for_test()
    });
    let skips_before = fixture
        .counts
        .lock()
        .unwrap()
        .wave_events
        .iter()
        .filter(|event| event["event"]["type"] == "skip")
        .count();
    native_click(&ui.window, &dislike);
    wait_playing(this, 3);
    pump_until(|| {
        fixture
            .counts
            .lock()
            .unwrap()
            .wave_events
            .iter()
            .any(|event| {
                event["event"]["type"] == "trackStarted" && event["event"]["trackId"] == "44"
            })
            && this.radio_idle_for_test()
    });
    {
        let counts = fixture.counts.lock().unwrap();
        assert_eq!(counts.reaction_posts, 1, "one deliberate Dislike POST");
        assert_eq!(counts.disliked_tracks, vec![42]);
        assert_eq!(counts.wave_sessions, 1, "Dislike keeps the existing Wave");
        let skips = counts
            .wave_events
            .iter()
            .filter(|event| event["event"]["type"] == "skip")
            .collect::<Vec<_>>();
        assert_eq!(skips.len(), skips_before + 1, "one Next reports one skip");
        let skip = skips.last().unwrap();
        assert_eq!(skip["batchId"], "synthetic-wave-batch-1");
        assert_eq!(skip["event"]["trackId"], "42");
        assert!(skip["event"]["totalPlayedSeconds"].as_f64().unwrap() > 0.0);
    }
    this.play_music_context(
        PlaybackContext::new(
            ContextKind::Playlist,
            Some("synthetic-normal".into()),
            vec![track("43")],
            0,
            false,
        )
        .unwrap(),
    );
    wait_playing(this, 0);
    pump_until(|| this.radio_idle_for_test() && this.radio_status().phase == RadioPhase::Ready);
    assert!(this.model.borrow().queue.shuffle());
    assert_eq!(this.model.borrow().queue.repeat_mode(), RepeatMode::Queue);
    let ordinary_serial = this.music_serial();
    this.play_music_context(first_context);
    assert_eq!(
        this.music_serial(),
        ordinary_serial,
        "a saved Wave snapshot cannot recreate its retired session"
    );
    assert_eq!(fixture.counts.lock().unwrap().wave_sessions, 1);
    // Hold the first feedback of another session. Logout may cancel it locally,
    // but cannot unsend the HTTP already admitted by the worker.
    fixture.hold_descriptor();
    fixture.counts.lock().unwrap().wave_hold_feedback = true;
    this.start_wave();
    wait_playing(this, 0);
    pump_until(|| fixture.counts.lock().unwrap().slow_started);
    let sent = fixture.counts.lock().unwrap().wave_events.len();
    let closing = this.player.borrow().controller.clone().unwrap();
    this.logout();
    fixture.release_descriptor();
    pump_until(|| !closing.snapshot().worker_running && this.model.borrow().token.is_none());
    let until = Instant::now() + Duration::from_millis(250);
    pump_until(|| Instant::now() >= until);
    assert_eq!(
        fixture.counts.lock().unwrap().wave_events.len(),
        sent,
        "no queued feedback after logout"
    );
    assert!(this.model.borrow().queue.items().is_empty());
    this.close();
    ui.window.close();

    // A source switch during an admitted uncertain POST must keep its primary
    // category and must not dispatch final feedback with the cancelled session.
    let uncertain = Fixture::new();
    uncertain.hold_descriptor();
    {
        let mut counts = uncertain.counts.lock().unwrap();
        counts.wave_hold_feedback = true;
        counts.wave_feedback_status = Some(0);
    }
    let (_other_app, other_ui) = player_window(&uncertain);
    let other = &other_ui.music.controller;
    other.start_wave();
    wait_playing(other, 0);
    pump_until(|| uncertain.counts.lock().unwrap().slow_started);
    other.play_music_context(
        PlaybackContext::new(ContextKind::Playlist, None, vec![track("43")], 0, false).unwrap(),
    );
    uncertain.release_descriptor();
    pump_until(|| {
        matches!(
            other.radio_status().phase,
            RadioPhase::Failed {
                stage: RadioStage::Feedback,
                failure: RadioFailure::OutcomeUnknown
            }
        )
    });
    wait_playing(other, 0);
    assert_eq!(uncertain.counts.lock().unwrap().wave_events.len(), 1);
    other.close();
    other_ui.window.close();

    // Bounded history must be pruned before the next request's capacity gate.
    let rolling = Fixture::new();
    {
        let mut counts = rolling.counts.lock().unwrap();
        let mut initial = vec![42];
        initial.extend(std::iter::repeat_n(43, 49));
        counts.wave_initial = Some(initial);
        counts.wave_next = Some(vec![44; 50]);
        counts.wave_unavailable = vec![43];
    }
    let (_rolling_app, rolling_ui) = player_window(&rolling);
    let rolling_this = &rolling_ui.music.controller;
    rolling_this.start_wave();
    wait_playing(rolling_this, 0);
    pump_until(|| rolling_this.model.borrow().queue.items().len() == 100);
    rolling.hold_descriptor();
    rolling.counts.lock().unwrap().wave_hold_batch = true;
    rolling_this.play_music_queue(99);
    pump_until(|| rolling.counts.lock().unwrap().slow_started);
    assert_eq!(rolling_this.model.borrow().queue.items().len(), 26);
    assert!(
        rolling_this
            .widgets
            .queue_count
            .text()
            .contains(&track_count_text(26)),
        "held-batch UI count follows the trimmed queue"
    );
    assert_eq!(
        rolling_this.widgets.queue_rows.observe_children().n_items(),
        26,
        "held-batch native rows follow the trimmed queue"
    );
    rolling_this.skip_music(true, false);
    rolling_this.skip_music(true, false);
    assert_eq!(
        rolling.counts.lock().unwrap().wave_batches,
        2,
        "two Next intents share the admitted batch"
    );
    rolling_this.play_music_queue(24);
    let retained = rolling_this
        .model
        .borrow()
        .queue
        .current_occurrence_id()
        .unwrap()
        .clone();
    pump_until(|| {
        rolling_this
            .music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Playing)
    });
    let resolved = rolling.counts.lock().unwrap().offers[2];
    rolling.release_descriptor();
    pump_until(|| {
        rolling_this.model.borrow().queue.items().len() == 76 && rolling_this.radio_idle_for_test()
    });
    assert_eq!(
        rolling_this.model.borrow().queue.current_occurrence_id(),
        Some(&retained),
        "batch arrival cannot override a later row selection"
    );
    assert_eq!(
        rolling.counts.lock().unwrap().offers[2],
        resolved,
        "stale waiting Next cannot re-resolve another occurrence"
    );
    rolling_this.skip_music(true, false);
    rolling_this.skip_music(true, false);
    pump_until(|| {
        let model = rolling_this.model.borrow();
        model
            .queue
            .current_index()
            .is_some_and(|index| model.queue.context().unwrap().source_position(index) == Some(100))
            && rolling_this
                .music_snapshot()
                .is_some_and(|snapshot| snapshot.state == PlaybackState::Playing)
    });
    assert_eq!(
        rolling.counts.lock().unwrap().wave_batches,
        2,
        "no redundant queued batch after success"
    );
    assert!(!matches!(
        rolling_this.radio_status().phase,
        RadioPhase::Failed { .. }
    ));
    let current = rolling_this
        .model
        .borrow()
        .queue
        .current_occurrence_id()
        .unwrap()
        .clone();
    rolling_this.seek_music(Duration::from_millis(3_800));
    pump_until(|| {
        rolling_this.model.borrow().queue.current_occurrence_id() != Some(&current)
            && rolling_this
                .music_snapshot()
                .is_some_and(|s| s.state == PlaybackState::Playing)
    });
    rolling_this.cancel_wave();
    assert!(rolling_this.model.borrow().queue.items().is_empty());
    rolling_this.close();
    rolling_ui.window.close();

    // An explicit fresh session after a feedback refusal must preserve the
    // user's modes from before the first Wave, not save its forced Off values.
    let modes = Fixture::new();
    modes.counts.lock().unwrap().wave_feedback_status = Some(403);
    let (_modes_app, modes_ui) = player_window(&modes);
    let modes_this = &modes_ui.music.controller;
    modes_this.set_music_shuffle(true);
    modes_this.set_music_repeat(RepeatMode::Queue);
    modes_this.start_wave();
    wait_playing(modes_this, 0);
    pump_until(|| {
        matches!(
            modes_this.radio_status().phase,
            RadioPhase::Failed {
                stage: RadioStage::Feedback,
                ..
            }
        )
    });
    assert!(!modes_this.widgets.next.is_sensitive());
    for index in 0..modes_this.model.borrow().queue.items().len() {
        let row = modes_this.queue_row_at_physical(index).unwrap();
        assert!(
            !row.is_activatable(),
            "retired Wave rows cannot offer playback"
        );
        let mut child = row.child().unwrap().first_child();
        let mut play_checked = false;
        while let Some(widget) = child {
            child = widget.next_sibling();
            if widget.widget_name() == "music-queue-play" {
                assert!(!widget.is_sensitive());
                play_checked = true;
            } else if widget.widget_name() == "music-queue-more" {
                assert!(widget.is_sensitive(), "Details/remove remain available");
            }
        }
        assert!(play_checked);
    }

    assert!(!modes_this.widgets.previous.is_sensitive());
    modes.counts.lock().unwrap().wave_feedback_status = None;
    modes_this.start_wave();
    pump_until(|| modes.counts.lock().unwrap().wave_sessions == 2);
    wait_playing(modes_this, 0);
    desktop_method("Stop", ().to_variant());
    pump_until(|| modes_this.model.borrow().queue.items().is_empty());
    pump_until(|| matches!(modes_this.radio_status().phase, RadioPhase::Ready));
    assert!(modes_this.model.borrow().queue.shuffle());
    assert_eq!(
        modes_this.model.borrow().queue.repeat_mode(),
        RepeatMode::Queue
    );
    modes_this.close();
    modes_ui.window.close();

    let pending = Fixture::new();
    pending.hold_descriptor();
    pending.counts.lock().unwrap().wave_hold_begin = true;
    let (_pending_app, pending_ui) = player_window(&pending);
    let pending_this = &pending_ui.music.controller;
    let ordinary = || {
        PlaybackContext::new(
            ContextKind::Playlist,
            Some("ordinary-pending-wave".into()),
            vec![track("42"), track("43")],
            0,
            false,
        )
        .unwrap()
    };
    pending_this.play_music_context(ordinary());
    wait_playing(pending_this, 0);
    pending_this.start_wave();
    pump_until(|| pending.counts.lock().unwrap().slow_started);
    let ordinary_serial = pending_this.music_serial();
    let ordinary_worker = pending_this.player.borrow().controller.clone().unwrap();
    let ordinary_occurrence = pending_this
        .model
        .borrow()
        .queue
        .current_occurrence_id()
        .cloned();
    pending_this.cancel_wave_start();
    pending.release_descriptor();
    pump_until(|| pending_this.radio_idle_for_test());
    assert_eq!(pending_this.music_serial(), ordinary_serial);
    assert_eq!(
        pending_this
            .model
            .borrow()
            .queue
            .current_occurrence_id()
            .cloned(),
        ordinary_occurrence
    );
    assert_eq!(ordinary_worker.snapshot().state, PlaybackState::Playing);
    assert!(pending.counts.lock().unwrap().wave_events.is_empty());
    pending.hold_descriptor();
    pending_this.start_wave();
    pump_until(|| pending.counts.lock().unwrap().slow_started);
    pending_this.play_music_queue(1);
    wait_playing(pending_this, 1);
    let selected = pending_this
        .model
        .borrow()
        .queue
        .current_occurrence_id()
        .unwrap()
        .clone();
    pending.release_descriptor();
    pump_until(|| pending_this.radio_idle_for_test());
    assert_eq!(
        pending_this.model.borrow().queue.current_occurrence_id(),
        Some(&selected)
    );
    assert_eq!(
        pending_this
            .model
            .borrow()
            .queue
            .context()
            .unwrap()
            .source_kind(),
        ContextKind::Playlist
    );
    assert!(
        pending.counts.lock().unwrap().wave_events.is_empty(),
        "cancelled begin cannot create listening feedback"
    );

    // Natural completion belongs to the old ordinary intent; it must not
    // cancel a newer deliberately requested radio start.
    pending_this.play_music_context(ordinary());
    wait_playing(pending_this, 0);
    pending.hold_descriptor();
    pending_this.start_wave();
    pump_until(|| pending.counts.lock().unwrap().slow_started);
    pump_until(|| {
        pending_this
            .music_snapshot()
            .is_some_and(|snapshot| snapshot.seekable)
    });
    pending_this.seek_music(Duration::from_millis(3_800));
    wait_playing(pending_this, 1);
    assert_eq!(pending_this.radio_status().phase, RadioPhase::Starting);
    pending.release_descriptor();
    pump_until(|| {
        pending_this
            .model
            .borrow()
            .queue
            .context()
            .is_some_and(|context| context.source_kind() == ContextKind::Wave)
    });
    wait_playing(pending_this, 0);
    pending_this.cancel_wave();
    pending_this.close();
    pending_ui.window.close();

    // Force the exact worker-error/UI-callback gap: an admitted feedback will
    // return403, while its local session has already been marked cancelled.
    // A timer observation and an ordinary source switch must retain that job's
    // typed primary result instead of inventing a local outbox-capacity error.
    let poisoned = Fixture::new();
    poisoned.hold_descriptor();
    {
        let mut counts = poisoned.counts.lock().unwrap();
        counts.wave_hold_feedback = true;
        counts.wave_feedback_status = Some(403);
    }
    let (_poison_app, poison_ui) = player_window(&poisoned);
    let poison_this = &poison_ui.music.controller;
    poison_this.start_wave();
    wait_playing(poison_this, 0);
    pump_until(|| poisoned.counts.lock().unwrap().slow_started);
    poison_this.poison_wave_session_for_test();
    poison_this.tick_radio(poison_this.music_snapshot().as_ref());
    assert!(
        !matches!(poison_this.radio_status().phase, RadioPhase::Failed { .. }),
        "pending typed provider failure owns the diagnosis"
    );
    poison_this.play_music_context(
        PlaybackContext::new(ContextKind::Playlist, None, vec![track("43")], 0, false).unwrap(),
    );
    assert_eq!(
        poison_this.radio_status().phase,
        RadioPhase::Finishing,
        "retain the pending epoch even though coordinator finish is cancelled"
    );
    poisoned.release_descriptor();
    pump_until(|| {
        matches!(
            poison_this.radio_status().phase,
            RadioPhase::Failed {
                stage: RadioStage::Feedback,
                failure: RadioFailure::Forbidden
            }
        )
    });
    wait_playing(poison_this, 0);
    assert_eq!(
        poisoned.counts.lock().unwrap().wave_events.len(),
        1,
        "no queued final feedback after the primary refusal"
    );
    assert_eq!(poisoned.counts.lock().unwrap().wave_batches, 0);
    poison_this.close();
    poison_ui.window.close();

    use crate::music::wave::WaveEventKind;
    for (kind, finish) in [
        (WaveEventKind::RadioStarted, None),
        (WaveEventKind::TrackStarted, None),
        (WaveEventKind::Skip, None),
        (WaveEventKind::TrackFinished, None),
        (WaveEventKind::Skip, Some("source-change")),
        (WaveEventKind::Skip, Some("stop")),
    ] {
        let refusal = Fixture::new();
        {
            let mut counts = refusal.counts.lock().unwrap();
            counts.wave_feedback_status = Some(400);
            counts.wave_reject_event = Some(kind.as_str());
        }
        let (_refusal_app, refusal_ui) = player_window(&refusal);
        let radio = &refusal_ui.music.controller;
        radio.start_wave();
        wait_playing(radio, 0);
        if !matches!(
            kind,
            WaveEventKind::RadioStarted | WaveEventKind::TrackStarted
        ) {
            pump_until(|| {
                refusal
                    .counts
                    .lock()
                    .unwrap()
                    .wave_events
                    .iter()
                    .any(|event| event["event"]["type"] == "trackStarted")
            });
            match (kind, finish) {
                (WaveEventKind::Skip, None) => radio.skip_music(true, false),
                (WaveEventKind::TrackFinished, None) => {
                    pump_until(|| {
                        radio
                            .music_snapshot()
                            .is_some_and(|snapshot| snapshot.seekable)
                    });
                    radio.seek_music(Duration::from_millis(3_800));
                }
                (WaveEventKind::Skip, Some("stop")) => {
                    pump_until(|| radio.system_media_available());
                    desktop_method("Stop", ().to_variant());
                }
                (WaveEventKind::Skip, Some("source-change")) => radio.play_music_context(
                    PlaybackContext::new(
                        ContextKind::LikedTracks,
                        None,
                        vec![track("43")],
                        0,
                        false,
                    )
                    .unwrap(),
                ),
                _ => unreachable!(),
            }
        }
        pump_until(|| {
            matches!(
                radio.radio_status().phase,
                RadioPhase::Failed {
                    stage: RadioStage::Feedback,
                    failure: RadioFailure::Http
                }
            )
        });
        pump_until(|| radio.radio_idle_for_test());
        let line = radio.wave_diagnostic_for_test().unwrap();
        assert!(
            line.starts_with("Karin wave: stage=feedback category=http-rejected http=400 event=")
        );
        assert!(line.contains(&format!(" event={} ", kind.as_str())));
        let origin = match (kind, finish) {
            (_, Some(_)) => "retirement",
            (WaveEventKind::Skip, None) => "selection",
            _ => "playback",
        };
        assert!(line.contains(&format!(" origin={origin} ")));
        assert!(line.ends_with(&format!("finish={}", finish.unwrap_or("none"))));
        assert!(line.contains(if finish.is_some() {
            " phase=retiring "
        } else {
            " phase=active "
        }));
        let sent = refusal.counts.lock().unwrap().wave_events.len();
        assert_eq!(
            refusal.counts.lock().unwrap().wave_events.last().unwrap()["event"]["type"],
            kind.as_str()
        );
        for _ in 0..3 {
            radio.tick_radio(radio.music_snapshot().as_ref());
        }
        assert_eq!(
            refusal.counts.lock().unwrap().wave_events.len(),
            sent,
            "confirmed400 is not success, repeated or followed by another event"
        );
        if finish == Some("source-change") {
            wait_playing(radio, 0);
            assert_eq!(
                radio.model.borrow().queue.context().unwrap().source_kind(),
                ContextKind::LikedTracks
            );
        }
        radio.close();
        refusal_ui.window.close();
    }
}

#[test]
#[ignore = "requires isolated session D-Bus, Xvfb, playerctl and actual generated MP3 playback"]
fn native_wave_retirement_supported_feedback_and_cleanup() {
    use super::super::radio::RadioPhase;
    use crate::queue::{ContextKind, RepeatMode};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Exit {
        SourcePlaying,
        SourcePaused,
        SourceEos,
        StopPlaying,
        StopPaused,
        BeforePlaying,
        Logout,
        Close,
        PendingFeedback,
        PendingBatch,
        Restart,
    }

    gtk::init().unwrap();
    adw::init().unwrap();
    for exit in [
        Exit::SourcePlaying,
        Exit::SourcePaused,
        Exit::SourceEos,
        Exit::StopPlaying,
        Exit::StopPaused,
        Exit::BeforePlaying,
        Exit::Logout,
        Exit::Close,
        Exit::PendingFeedback,
        Exit::PendingBatch,
        Exit::Restart,
    ] {
        eprintln!("Karin synthetic Wave retirement: case={exit:?}");
        let fixture = Fixture::new();
        let held_feedback = matches!(exit, Exit::PendingFeedback | Exit::Logout | Exit::Close);
        let held_batch = exit == Exit::PendingBatch;
        let never_played = exit == Exit::BeforePlaying;
        {
            let mut counts = fixture.counts.lock().unwrap();
            // Five initial positions avoid an unrelated prefetch. The pending
            // batch case deliberately starts at the actual three-item gate.
            counts.wave_initial = Some(if held_batch {
                vec![42, 43, 44]
            } else if never_played {
                vec![45, 43, 44, 42, 43]
            } else if exit == Exit::SourceEos {
                vec![42, 45, 44, 42, 43]
            } else {
                vec![42, 43, 44, 42, 43]
            });
            counts.wave_hold_feedback = held_feedback;
            counts.wave_hold_batch = held_batch;
        }
        let (_app, ui) = player_window(&fixture);
        let this = &ui.music.controller;
        this.set_music_shuffle(true);
        this.set_music_repeat(RepeatMode::Queue);
        this.start_wave();
        if never_played {
            pump_until(|| fixture.counts.lock().unwrap().slow_started);
            assert!(this.music_snapshot().is_none());
            assert!(fixture.counts.lock().unwrap().wave_events.is_empty());
        } else {
            wait_playing(this, 0);
            pump_until(|| {
                this.music_snapshot()
                    .is_some_and(|snapshot| !snapshot.playing_time.is_zero())
            });
            if held_feedback || held_batch {
                pump_until(|| fixture.counts.lock().unwrap().slow_started);
            } else {
                pump_until(|| {
                    fixture.counts.lock().unwrap().wave_events.len() == 2
                        && this.radio_idle_for_test()
                });
            }
        }
        let session = this.wave_session_for_test().unwrap();
        assert!(!session.is_cancelled());
        let worker = this.player.borrow().controller.clone();
        let mut paused_heard = None;
        if matches!(exit, Exit::SourcePaused | Exit::StopPaused) {
            this.pause_music();
            pump_until(|| {
                this.music_snapshot()
                    .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
            });
            paused_heard = this.music_snapshot().map(|snapshot| snapshot.playing_time);
        }
        if exit == Exit::SourceEos {
            pump_until(|| {
                this.music_snapshot()
                    .is_some_and(|snapshot| snapshot.seekable)
            });
            this.seek_music(Duration::from_millis(3_800));
            pump_until(|| {
                let counts = fixture.counts.lock().unwrap();
                counts.slow_started
                    && counts
                        .wave_events
                        .iter()
                        .any(|event| event["event"]["type"] == "trackFinished")
            });
            pump_until(|| !worker.as_ref().unwrap().snapshot().worker_running);
            assert_eq!(
                worker.as_ref().unwrap().snapshot().state,
                PlaybackState::Stopped
            );
            assert!(this.music_snapshot().is_none());
        }
        let before_exit = fixture.counts.lock().unwrap().wave_events.len();
        match exit {
            Exit::StopPlaying | Exit::StopPaused => {
                pump_until(|| this.system_media_available());
                desktop_method("Stop", ().to_variant());
            }
            Exit::Logout => this.logout(),
            Exit::Close => this.close(),
            _ => this.play_music_context(
                PlaybackContext::new(
                    ContextKind::Playlist,
                    Some("synthetic-retirement-ordinary".into()),
                    vec![track("43")],
                    0,
                    false,
                )
                .unwrap(),
            ),
        }
        if never_played {
            // Retirement with an empty outbox is independent of the still
            // blocked old audio resolver and must cancel its identity locally.
            pump_until(|| {
                this.radio_idle_for_test() && this.radio_status().phase == RadioPhase::Ready
            });
            assert!(session.is_cancelled());
        }
        fixture.release_descriptor();
        pump_until(|| this.radio_idle_for_test());
        assert!(session.is_cancelled(), "retired local identity: {exit:?}");
        if let Some(worker) = &worker {
            pump_until(|| !worker.snapshot().worker_running);
        }
        if !matches!(exit, Exit::Logout | Exit::Close) {
            assert_eq!(this.radio_status().phase, RadioPhase::Ready, "{exit:?}");
            assert!(this.model.borrow().queue.shuffle(), "{exit:?}");
            assert_eq!(this.model.borrow().queue.repeat_mode(), RepeatMode::Queue);
        }
        if matches!(
            exit,
            Exit::StopPlaying | Exit::StopPaused | Exit::Logout | Exit::Close
        ) {
            assert!(this.model.borrow().queue.items().is_empty());
        } else {
            wait_playing(this, 0);
            let model = this.model.borrow();
            assert_eq!(
                model.queue.context().unwrap().source_kind(),
                ContextKind::Playlist
            );
            assert_eq!(
                model.queue.items().len(),
                1,
                "late batch cannot append: {exit:?}"
            );
            assert_eq!(model.queue.current().unwrap().id, "43");
        }
        let events = fixture.counts.lock().unwrap().wave_events.clone();
        assert!(
            events
                .iter()
                .all(|event| event["event"]["type"] != "radioFinished")
        );
        let terminals = events
            .iter()
            .filter(|event| {
                matches!(
                    event["event"]["type"].as_str(),
                    Some("skip" | "trackFinished")
                )
            })
            .collect::<Vec<_>>();
        if never_played || matches!(exit, Exit::Logout | Exit::Close) {
            assert!(terminals.is_empty(), "no invented terminal: {exit:?}");
            assert_eq!(
                events.len(),
                before_exit,
                "hard retirement cannot drain: {exit:?}"
            );
        } else {
            assert_eq!(terminals.len(), 1, "exactly one terminal: {exit:?}");
            assert_eq!(
                terminals[0]["event"]["type"],
                if exit == Exit::SourceEos {
                    "trackFinished"
                } else {
                    "skip"
                }
            );
            assert_eq!(terminals[0]["batchId"], "synthetic-wave-batch-1");
            assert_eq!(terminals[0]["event"]["trackId"], "42");
            let heard = terminals[0]["event"]["totalPlayedSeconds"]
                .as_f64()
                .unwrap();
            assert!(heard > 0.0, "terminal requires actual Playing: {exit:?}");
            let actual = worker
                .as_ref()
                .unwrap()
                .snapshot()
                .playing_time
                .as_secs_f64();
            assert!(
                heard <= actual,
                "reported time cannot exceed worker ledger: {exit:?}"
            );
            if let Some(paused) = paused_heard {
                assert!(
                    (heard - paused.as_secs_f64()).abs() <= 1e-9,
                    "paused time is excluded: {exit:?}"
                );
            }
            assert_eq!(events.len(), 3, "no extra session-end report: {exit:?}");
            assert_eq!(events[0]["event"]["type"], "radioStarted");
            assert_eq!(events[1]["event"]["type"], "trackStarted");
        }
        let sent = events.len();
        if !matches!(exit, Exit::Logout | Exit::Close) {
            for _ in 0..3 {
                this.tick_radio(this.music_snapshot().as_ref());
            }
            assert_eq!(
                fixture.counts.lock().unwrap().wave_events.len(),
                sent,
                "retired work cannot replay: {exit:?}"
            );
        }
        if held_batch {
            assert_eq!(fixture.counts.lock().unwrap().wave_batches, 1);
        }
        if exit == Exit::Restart {
            this.start_wave();
            pump_until(|| fixture.counts.lock().unwrap().wave_sessions == 2);
            wait_playing(this, 0);
            pump_until(|| {
                fixture.counts.lock().unwrap().wave_events.len() == sent + 2
                    && this.radio_idle_for_test()
            });
            let fresh = this.wave_session_for_test().unwrap();
            assert!(!fresh.is_cancelled());
            assert!(session.is_cancelled());
            assert_eq!(
                this.model.borrow().queue.context().unwrap().source_kind(),
                ContextKind::Wave
            );
            assert!(!this.model.borrow().queue.shuffle());
            assert_eq!(this.model.borrow().queue.repeat_mode(), RepeatMode::Off);
        }
        this.close();
        ui.window.close();
    }
}
