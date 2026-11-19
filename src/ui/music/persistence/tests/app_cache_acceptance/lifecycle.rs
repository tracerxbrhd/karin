// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic application/queue/cache lifecycle races with original audio.
use super::*;
use crate::{
    audio_cache::ResourceKey,
    queue::{ContextKind, PlaybackContext},
};
use std::{collections::BTreeMap, sync::Condvar};

const CASES: [&str; 8] = [
    "lifecycle-eos",
    "lifecycle-switch",
    "lifecycle-fast",
    "lifecycle-skip",
    "lifecycle-seek",
    "lifecycle-clear",
    "lifecycle-logout",
    "lifecycle-wave",
];

pub(super) fn run_parent() {
    let mut reports = Vec::new();
    for phase in CASES {
        let directory = Directory::new();
        for kind in ["config", "cache", "data", "state"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(directory.0.join(kind))
                .unwrap();
        }
        let input = Input {
            directory: directory.0.clone(),
            origin: String::new(),
            sha256: [0; 32],
            bytes: 0,
            license: "CC0-1.0".into(),
        };
        let path = directory.0.join("lifecycle-input.json");
        fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
        let phase_report = run_phase(&path, phase, &directory.0);
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.0.join("lifecycle-result.json")).unwrap())
                .unwrap();
        assert_eq!(report["phase"], phase);
        assert_eq!(report["pid"], phase_report.pid);
        assert_eq!(report["passed"], true);
        reports.push(report);
    }
    let pids = reports
        .iter()
        .map(|r| r["pid"].as_u64().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(pids.len(), CASES.len());
    if let Some(path) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
        fs::create_dir_all(&path).unwrap();
        fs::write(
            PathBuf::from(path).join("audio-cache-lifecycle-receipt.json"),
            serde_json::to_vec_pretty(&json!({
                "scope":"ordinary-application-cache-lifecycle", "mechanism":"independent-full-response", "real_yandex_service":false,
                "cases":reports, "processes":CASES.len(),
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

#[derive(Default)]
pub(super) struct Gate {
    pub(super) entered: AtomicBool,
    released: Mutex<bool>,
    changed: Condvar,
}
impl Gate {
    pub(super) fn wait(&self) {
        self.entered.store(true, Ordering::Release);
        let value = self.released.lock().unwrap();
        let (value, _) = self
            .changed
            .wait_timeout_while(value, Duration::from_secs(8), |v| !*v)
            .unwrap();
        assert!(*value, "bounded lifecycle HTTP gate was released");
    }
    pub(super) fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

#[derive(Clone, Default, serde::Serialize)]
struct WireCounts {
    offers: [u64; 4],
    media_gets: [u64; 4],
    native_gets: [u64; 4],
    background_gets: [u64; 4],
    media_heads: [u64; 4],
    sent_bytes: [u64; 4],
    range_gets: u64,
    sessions: u64,
    batches: u64,
    #[serde(skip)]
    feedback: Vec<serde_json::Value>,
}

struct Server {
    origin: String,
    counts: Arc<Mutex<WireCounts>>,
    hold: Arc<Gate>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(
        media: BTreeMap<String, Arc<Vec<u8>>>,
        hold_first: bool,
        seekable: bool,
        hold_fourth: bool,
    ) -> Self {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let origin = format!("http://{}/", socket.local_addr().unwrap());
        let media = Arc::new(media);
        let counts = Arc::new(Mutex::new(WireCounts::default()));
        let hold = Arc::new(Gate::default());
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let (origin, media, counts, hold, stop) = (
                origin.clone(),
                media.clone(),
                counts.clone(),
                hold.clone(),
                stop.clone(),
            );
            thread::spawn(move || {
                let mut workers = Vec::new();
                while !stop.load(Ordering::Acquire) {
                    match socket.accept() {
                        Ok((stream, _)) => {
                            assert!(workers.len() < 128, "bounded lifecycle request count");
                            let (origin, media, counts, hold) =
                                (origin.clone(), media.clone(), counts.clone(), hold.clone());
                            workers.push(thread::spawn(move || {
                                serve(
                                    stream,
                                    &origin,
                                    &media,
                                    &counts,
                                    &hold,
                                    (hold_first, seekable, hold_fourth),
                                )
                            }));
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(_) => panic!("lifecycle listener failed"),
                    }
                }
                for worker in workers {
                    worker.join().unwrap();
                }
            })
        };
        Self {
            origin,
            counts,
            hold,
            stop,
            worker: Some(worker),
        }
    }
    fn counts(&self) -> WireCounts {
        self.counts.lock().unwrap().clone()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.hold.release();
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn index(id: &str) -> usize {
    match id {
        "42" => 0,
        "43" => 1,
        "44" => 2,
        "45" => 3,
        _ => panic!("unexpected lifecycle resource"),
    }
}
fn track_value(id: &str) -> serde_json::Value {
    json!({"id":id,"title":"Original lifecycle audio","durationMs":1200,"available":true,"artists":[{"id":7,"name":"Karin original fixture"}]})
}
fn wave_value(batch: u64, ids: &[&str], initial: bool) -> serde_json::Value {
    let mut value = json!({"result":{"batchId":format!("lifecycle-batch-{batch}"),"sequence":ids.iter().map(|id|json!({"type":"track","track":{"id":id}})).collect::<Vec<_>>()}});
    if initial {
        value["result"]["radioSessionId"] = "lifecycle-session".into();
    }
    value
}

fn serve(
    mut stream: TcpStream,
    origin: &str,
    media: &BTreeMap<String, Arc<Vec<u8>>>,
    counts: &Mutex<WireCounts>,
    gate: &Gate,
    mode: (bool, bool, bool),
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut raw = Vec::new();
    while !raw.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            return;
        }
        raw.push(byte[0]);
        assert!(raw.len() < 16 * 1024);
    }
    let raw = String::from_utf8(raw).unwrap();
    let first = raw
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{}", first[1])).unwrap();
    let header = |key: &str| {
        raw.lines().skip(1).find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case(key).then_some(value.trim())
        })
    };
    let path = url.path();
    let transport = path.starts_with("/download-info/") || path.starts_with("/get-mp3/");
    if transport {
        for name in ["authorization", "cookie", "origin", "referer"] {
            assert!(header(name).is_none());
        }
    } else {
        assert_eq!(
            header("authorization"),
            Some("OAuth synthetic-storage-ui-token")
        );
    }
    let body = if first[0] == "POST" {
        let length: usize = header("content-length").unwrap().parse().unwrap();
        assert!(length < 16 * 1024);
        let mut body = vec![0; length];
        stream.read_exact(&mut body).unwrap();
        body
    } else {
        Vec::new()
    };
    let reply = if path == "/account/status" {
        json!({"result":{"account":{"uid":1234,"displayName":"Lifecycle fixture"}}})
    } else if path == "/users/1234/likes/tracks" {
        json!({"result":{"library":{"uid":1234,"revision":1,"tracks":[{"id":42},{"id":43}]}}})
    } else if path == "/users/1234/dislikes/tracks" {
        json!({"result":{"library":{"uid":1234,"revision":1,"tracks":[]}}})
    } else if matches!(
        path,
        "/users/1234/playlists/list" | "/users/1234/likes/playlists"
    ) {
        json!({"result":[]})
    } else if path == "/tracks" {
        let pairs = reqwest::Url::parse(&format!(
            "http://127.0.0.1/?{}",
            std::str::from_utf8(&body).unwrap()
        ))
        .unwrap();
        // MusicService posts one form pair per distinct ID, not a comma list.
        // Return every requested canonical track so the Wave fixture exercises
        // both initial occurrences and the real next-batch rollover.
        let ids = pairs
            .query_pairs()
            .filter(|(key, _)| key == "track-ids")
            .map(|(_, value)| value.into_owned())
            .collect::<Vec<_>>();
        assert!(!ids.is_empty() && ids.len() <= 4);
        json!({"result":ids.iter().map(|id| { index(id); track_value(id) }).collect::<Vec<_>>()})
    } else if path == "/rotor/session/new" {
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["seeds"], json!(["user:onyourwave"]));
        counts.lock().unwrap().sessions += 1;
        wave_value(1, &["42", "43"], true)
    } else if path == "/rotor/session/lifecycle-session/tracks" {
        let mut state = counts.lock().unwrap();
        state.batches += 1;
        wave_value(state.batches + 1, &["44", "45"], false)
    } else if path == "/rotor/session/lifecycle-session/feedback" {
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(matches!(
            value["event"]["type"].as_str(),
            Some("radioStarted" | "trackStarted" | "trackFinished" | "skip")
        ));
        counts.lock().unwrap().feedback.push(value);
        json!({"result":"accepted"})
    } else if let Some(id) = path
        .strip_prefix("/tracks/")
        .and_then(|tail| tail.strip_suffix("/download-info"))
    {
        counts.lock().unwrap().offers[index(id)] += 1;
        json!({"result":[{"codec":"mp3","bitrateInKbps":128,"preview":false,"direct":false,"downloadInfoUrl":format!("{origin}download-info/{id}")} ]})
    } else if let Some(id) = path.strip_prefix("/download-info/") {
        index(id);
        let value = format!(
            "<download-info><host>s268myt.storage.yandex.net</host><path>/lifecycle-{id}.mp3</path><ts>1700000000</ts><s>lifecycle-fixture</s></download-info>"
        );
        write_response(&mut stream, "application/xml", value.as_bytes());
        return;
    } else if path.starts_with("/get-mp3/") {
        let file = path.rsplit('/').next().unwrap();
        let id = file
            .strip_prefix("lifecycle-")
            .unwrap()
            .strip_suffix(".mp3")
            .unwrap();
        let n = index(id);
        let data = media.get(id).unwrap();
        let etag = format!("\"lifecycle-{id}\"");
        let policy = if mode.2 && id == "45" {
            "no-store"
        } else {
            "private, no-cache, must-revalidate"
        };
        if first[0] == "HEAD" {
            counts.lock().unwrap().media_heads[n] += 1;
            let status = if header("if-none-match") == Some(etag.as_str()) {
                "304 Not Modified"
            } else {
                "200 OK"
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: audio/mpeg\r\nContent-Length: {}\r\nCache-Control: {policy}\r\nETag: {etag}\r\nConnection: close\r\n\r\n",
                data.len()
            );
            return;
        }
        assert_eq!(first[0], "GET");
        let background = header("cache-control") == Some("no-cache");
        let range = header("range").map(|value| {
            assert!(mode.1, "nonseekable fast fixture must not request a range");
            let (start, end) = value
                .strip_prefix("bytes=")
                .unwrap()
                .split_once('-')
                .unwrap();
            let start = start.parse::<usize>().unwrap();
            let end = if end.is_empty() {
                data.len() - 1
            } else {
                end.parse::<usize>().unwrap().min(data.len() - 1)
            };
            assert!(start <= end && end < data.len());
            (start, end)
        });
        let (start, end) = range.unwrap_or((0, data.len() - 1));
        {
            let mut state = counts.lock().unwrap();
            if start == 0 {
                state.media_gets[n] += 1;
                if background {
                    state.background_gets[n] += 1;
                } else {
                    state.native_gets[n] += 1;
                }
            } else {
                state.range_gets += 1;
            }
        }
        let status = if range.is_some() {
            "206 Partial Content"
        } else {
            "200 OK"
        };
        let ranges = if mode.1 { "bytes" } else { "none" };
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: audio/mpeg\r\nContent-Length: {}\r\nCache-Control: {policy}\r\nETag: {etag}\r\nAccept-Ranges: {ranges}\r\nConnection: close\r\n",
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
        if mode.2 && id == "45" && !background {
            gate.wait();
        }
        if mode.0 && id == "42" && start == 0 && background {
            assert!(end >= 8192 && data.len() > 8192);
            if stream.write_all(&data[..8192]).is_err() {
                return;
            }
            counts.lock().unwrap().sent_bytes[n] += 8192;
            gate.wait();
            if stream.write_all(&data[8192..=end]).is_ok() {
                counts.lock().unwrap().sent_bytes[n] += (end + 1 - 8192) as u64;
            }
        } else if stream.write_all(&data[start..=end]).is_ok() {
            counts.lock().unwrap().sent_bytes[n] += (end - start + 1) as u64;
        }
        return;
    } else if first[0] == "GET"
        && let Some(response) = crate::ui::music::tests::empty_similar_response(path)
    {
        response
    } else {
        panic!("unexpected lifecycle route")
    };
    write_response(
        &mut stream,
        "application/json",
        reply.to_string().as_bytes(),
    );
}

fn original_with_large_tag(audio: &[u8]) -> Vec<u8> {
    let size = 5 * 512 * 1024;
    let mut bytes = vec![b'I', b'D', b'3', 4, 0, 0];
    bytes.extend(
        (0..4)
            .rev()
            .map(|shift| ((size >> (shift * 7)) & 0x7f) as u8),
    );
    bytes.resize(10 + size, 0);
    bytes.extend_from_slice(audio);
    bytes
}

pub(super) fn normal_window(
    input: &Input,
    origin: &str,
) -> (adw::Application, crate::ui::MainWindow) {
    let settings = SettingsStore::for_application().unwrap();
    settings.save(&Preferences::default()).unwrap();
    let root = input.directory.join("cache").join(crate::config::APP_ID);
    let budget = CacheBudget::new(
        root.join("persistent"),
        root.join("audio"),
        true,
        Preferences::default().cache_limit.bytes(),
    )
    .unwrap();
    let cache =
        Cache::new_with_budget(root.join("persistent"), CachePolicy::default(), budget).unwrap();
    let mut state = State::for_testing(
        settings,
        CredentialStore::with_backend(Backend::new(Some(TOKEN))),
        Some(cache),
    );
    state.audio_root = Some(root.join("audio"));
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.CacheLifecycle")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let ui = crate::ui::build_with_music_pane(
        &app,
        crate::application::ConnectionService::new(crate::api::YandexApi::new().unwrap()),
        |app, window| {
            MusicPane::with_provider_state(
                app,
                window,
                Some(MusicService::for_test(origin)),
                Arc::new(OfflineProvider),
                launch_system_browser,
                state,
            )
        },
    );
    ui.window.set_title(Some(TITLE));
    ui.window.set_default_size(900, 1000);
    ui.window.present();
    let this = &ui.music.controller;
    pump_until(|| this.model.borrow().account_valid && settled(this));
    this.set_fixture_resolver_for_test(StreamResolver::for_test(origin, origin, origin));
    assert_idle_wave_after_startup(this, true);
    (app, ui)
}
fn track(id: &str) -> crate::music::Track {
    crate::music::Track {
        id: id.into(),
        title: "Original lifecycle audio".into(),
        artists: vec!["Karin original fixture".into()],
        artist_links: Vec::new(),
        album_id: None,
        album_title: None,
        duration_ms: Some(1200),
        cover: None,
        explicit: false,
        available: true,
    }
}
pub(super) fn start(this: &Rc<Controller>, ids: &[&str], autoplay: bool) {
    let context = PlaybackContext::new(
        ContextKind::Playlist,
        Some("lifecycle-context".into()),
        ids.iter().map(|id| track(id)).collect(),
        0,
        false,
    )
    .unwrap();
    this.model.borrow_mut().queue.replace_context(context);
    this.render_queue();
    this.request_music(autoplay, None);
}
pub(super) fn playing(this: &Controller, expected: &str) {
    pump_until(|| {
        this.model
            .borrow()
            .queue
            .current()
            .is_some_and(|v| v.id == expected)
            && this.music_snapshot().is_some_and(|s| {
                s.state == PlaybackState::Playing
                    && !s.playing_time.is_zero()
                    && s.source_kind == PlaybackSourceKind::RemoteHttps
            })
    });
}
pub(super) fn wait_records(this: &Controller, input: &Input, expected: &[Arc<Vec<u8>>]) {
    let (cache, _) = this.audio_cache_binding().unwrap();
    let root = input
        .directory
        .join("cache")
        .join(crate::config::APP_ID)
        .join("audio");
    let hashes = expected
        .iter()
        .map(|v| {
            (
                v.len() as u64,
                digest(&SHA256, v).as_ref().try_into().unwrap(),
            )
        })
        .collect::<Vec<(u64, [u8; 32])>>();
    let ready = Arc::new(AtomicBool::new(false));
    let observed = ready.clone();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(7);
        while Instant::now() < deadline {
            if cache.stats().is_ok_and(|s| s.entries == hashes.len())
                && hashes
                    .iter()
                    .all(|(bytes, sha)| completed_media_matches(&root, *bytes, *sha))
            {
                observed.store(true, Ordering::Release);
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    });
    pump_until(|| ready.load(Ordering::Acquire));
    worker.join().unwrap();
    if !this.is_wave_queue() {
        pump_until(|| this.cache_work_settled_for_test());
    }
}

pub(super) fn run_child(input: &Input, phase: &str) {
    assert!(CASES.contains(&phase));
    gtk::init().unwrap();
    adw::init().unwrap();
    let partial = matches!(phase, "lifecycle-skip" | "lifecycle-seek");
    let fast = phase == "lifecycle-fast";
    let first = Arc::new(encode_audio(
        &input.directory,
        440.0,
        if partial || matches!(phase, "lifecycle-clear" | "lifecycle-logout") {
            600
        } else {
            120
        },
    ));
    let second = Arc::new(encode_audio(&input.directory, 660.0, 120));
    let third = Arc::new(encode_audio(&input.directory, 880.0, 120));
    let first = if fast {
        Arc::new(original_with_large_tag(&first))
    } else {
        first
    };
    let server = Server::new(
        BTreeMap::from([
            ("42".into(), first.clone()),
            ("43".into(), second.clone()),
            ("44".into(), third.clone()),
            ("45".into(), second.clone()),
        ]),
        partial,
        !fast,
        phase == "lifecycle-wave",
    );
    let (_app, ui) = normal_window(input, &server.origin);
    let this = &ui.music.controller;
    let (cache, _) = this.audio_cache_binding().unwrap();
    let root = input
        .directory
        .join("cache")
        .join(crate::config::APP_ID)
        .join("audio");
    let mut gate = None;
    let mut published = 0usize;
    let mut next_source = "none";
    if matches!(
        phase,
        "lifecycle-eos"
            | "lifecycle-switch"
            | "lifecycle-clear"
            | "lifecycle-logout"
            | "lifecycle-wave"
    ) {
        gate = Some(cache.set_commit_gate_for_test(ResourceKey::track("42").unwrap()));
    }
    if phase == "lifecycle-wave" {
        this.start_wave();
    } else {
        start(this, &["42", "43"], !fast);
    }
    if fast {
        pump_until(|| {
            this.music_snapshot().is_some_and(|s| {
                s.state == PlaybackState::Ready && s.source_kind == PlaybackSourceKind::RemoteHttps
            })
        });
        pump_until(|| server.counts().sent_bytes[0] > 2 * 1024 * 1024);
        assert_eq!(
            server.counts().background_gets[0],
            0,
            "preparation alone must not request a cache copy"
        );
        assert!(this.cache_work_settled_for_test());
        assert!(
            media_files(&root).is_empty(),
            "preparation without Play intent must not create cache audio"
        );
        this.play_music();
        playing(this, "42");
        wait_records(this, input, std::slice::from_ref(&first));
        published = 1;
        assert_eq!(server.counts().range_gets, 0);
        assert_eq!(server.counts().media_gets[0], 2);
        assert_eq!(server.counts().native_gets[0], 1);
        assert_eq!(server.counts().background_gets[0], 1);
    } else {
        playing(this, "42");
        if let Some(gate) = &gate {
            pump_until(|| gate.entered());
        }
        match phase {
            "lifecycle-eos" | "lifecycle-switch" => {
                if phase == "lifecycle-switch" {
                    this.skip_music(true, false);
                }
                playing(this, "43");
                next_source = "remote";
                assert_eq!(
                    server.counts().offers[1],
                    1,
                    "next admission must happen before old commit is released"
                );
                gate.as_ref().unwrap().release();
                wait_records(this, input, &[first.clone(), second.clone()]);
                published = 2;
            }
            "lifecycle-skip" => {
                pump_until(|| server.hold.entered.load(Ordering::Acquire));
                this.skip_music(true, false);
                playing(this, "43");
                next_source = "remote";
                server.hold.release();
                wait_records(this, input, &[first.clone(), second.clone()]);
                published = 2;
            }
            "lifecycle-seek" => {
                pump_until(|| server.hold.entered.load(Ordering::Acquire));
                let serial = this.music_snapshot().unwrap().seek_serial;
                this.seek_music(Duration::from_secs(3));
                // The worker acknowledges the seek before soup's asynchronous
                // range request necessarily reaches the server. Observe both
                // independent events before releasing the incomplete response.
                pump_until(|| {
                    this.music_snapshot()
                        .is_some_and(|s| s.seek_serial > serial)
                        && server.counts().range_gets > 0
                });
                assert!(
                    server.counts().range_gets > 0,
                    "seek must exercise a real range response"
                );
                server.hold.release();
                wait_records(this, input, std::slice::from_ref(&first));
                published = 1;
                let before = server.counts().media_gets[0];
                start(this, &["42"], true);
                wait_source(this, PlaybackSourceKind::CachedFile);
                assert_eq!(
                    server.counts().media_gets[0],
                    before,
                    "seek cannot damage independent whole-file cache reuse"
                );
            }
            "lifecycle-clear" => {
                this.clear_persistent_cache();
                pump_until(|| {
                    !this.persistence.borrow().cache_busy
                        && !this.persistence.borrow().audio_purge_pending
                });
                let before = this.music_snapshot().unwrap();
                gate.as_ref().unwrap().release();
                pump_until(|| this.cache_work_settled_for_test());
                assert!(media_files(&root).is_empty());
                assert!(this.model.borrow().account_valid);
                pump_until(|| {
                    this.music_snapshot().is_some_and(|s| {
                        s.state == PlaybackState::Playing
                            && s.generation == before.generation
                            && s.playing_time > before.playing_time + Duration::from_millis(40)
                    })
                });
            }
            "lifecycle-logout" => {
                this.logout();
                gate.as_ref().unwrap().release();
                pump_until(|| {
                    this.model.borrow().token.is_none()
                        && this.cache_work_settled_for_test()
                        && !this.persistence.borrow().audio_purge_pending
                });
                assert!(media_files(&root).is_empty());
                assert!(this.music_snapshot().is_none());
            }
            "lifecycle-wave" => {
                playing(this, "43");
                next_source = "remote";
                gate.as_ref().unwrap().release();
                playing(this, "44");
                wait_records(this, input, &[first.clone(), second.clone(), third.clone()]);
                published = 3;
                // Native playback and the serialized Wave feedback lane
                // are independent workers. Observe its actual acknowledgements
                // rather than racing the next GTK ledger tick.
                pump_until(|| {
                    let state = server.counts();
                    state
                        .feedback
                        .iter()
                        .filter(|e| e["event"]["type"] == "trackStarted")
                        .count()
                        >= 3
                        && state
                            .feedback
                            .iter()
                            .filter(|e| e["event"]["type"] == "trackFinished")
                            .count()
                            >= 2
                });
                let state = server.counts();
                assert_eq!(state.sessions, 1);
                assert!(state.batches >= 1);
                let started = state
                    .feedback
                    .iter()
                    .filter(|e| e["event"]["type"] == "trackStarted")
                    .collect::<Vec<_>>();
                assert!(started.len() >= 3);
                assert_eq!(started[0]["batchId"], "lifecycle-batch-1");
                assert_eq!(started[1]["batchId"], "lifecycle-batch-1");
                assert_eq!(started[2]["batchId"], "lifecycle-batch-2");
                assert!(
                    state
                        .feedback
                        .iter()
                        .filter(|e| e["event"]["type"] == "trackFinished")
                        .count()
                        >= 2
                );
            }
            _ => unreachable!(),
        }
    }
    if let Some(gate) = &gate {
        gate.release();
    }
    if phase == "lifecycle-wave" {
        this.stop_music();
    }
    server.hold.release();
    let stats = server.counts();
    if matches!(
        phase,
        "lifecycle-eos" | "lifecycle-switch" | "lifecycle-wave"
    ) {
        assert!(
            stats.background_gets.iter().all(|n| *n <= 1),
            "one independent full response per selected uncached resource"
        );
        assert!(
            stats.native_gets.iter().all(|n| *n <= 1),
            "cache jobs cannot reconnect the player"
        );
        assert_eq!(stats.background_gets[0], 1);
        assert_eq!(stats.native_gets[0], 1);
    }
    close_child(&ui, this);
    fs::write(input.directory.join("lifecycle-result.json"),serde_json::to_vec_pretty(&json!({
        "phase":phase,"pid":std::process::id(),"passed":true,"mechanism":"independent-full-response","first_source":"remote","next_source":next_source,"published_records":published,"counters":stats,"fast_bytes":if fast {first.len()} else {0},"wave_original_markers":phase=="lifecycle-wave","commit_overlap":matches!(phase,"lifecycle-eos"|"lifecycle-switch"|"lifecycle-wave"),
    })).unwrap()).unwrap();
    write_phase(
        input,
        PhaseReport {
            phase: phase.into(),
            pid: std::process::id(),
            account_revalidated: true,
            cache_policy: "private-http".into(),
            first_source: "remote-https".into(),
            replay_source: None,
            clear_kept_playing: phase == "lifecycle-clear",
            default_limit_bytes: 2 * 1024 * 1024 * 1024,
        },
    );
}
