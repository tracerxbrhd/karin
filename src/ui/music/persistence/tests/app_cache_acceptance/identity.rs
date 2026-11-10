// SPDX-License-Identifier: GPL-3.0-or-later
//! Ordinary application replay with one logical MDS object and changing replicas.
//! Audio is original CC0 fixture data; opaque fixture ETags are deliberately NOT
//! hashes of that audio and are never treated as globally meaningful.
use super::*;
use crate::audio_cache::{Presence, http_policy::Metadata};
use base64::{Engine as _, engine::general_purpose::STANDARD};

mod presence;

const CASES: [&str; 13] = [
    "identity-first",
    "identity-upgrade-failed",
    "identity-upgrade",
    "identity-signed",
    "identity-host",
    "identity-path",
    "identity-unknown",
    "identity-unknown-restart",
    "identity-digest",
    "identity-version",
    "identity-variant",
    "identity-codec",
    "identity-denied",
];

#[derive(Clone)]
struct Mode {
    host: bool,
    path: u8,
    object: bool,
    timestamp: u64,
    version: u8,
    bitrate: u32,
    digest: bool,
    codec: &'static str,
    denied: bool,
    broken_background: bool,
    account: u64,
}
impl Default for Mode {
    fn default() -> Self {
        Self {
            host: false,
            path: 0,
            object: false,
            timestamp: 1_700_000_000,
            version: 1,
            bitrate: 128,
            digest: false,
            codec: "mp3",
            denied: false,
            broken_background: false,
            account: 1234,
        }
    }
}
#[derive(Clone, Default, serde::Serialize)]
struct Wire {
    account: u64,
    offers: u64,
    descriptors: u64,
    heads: u64,
    conditional_heads: u64,
    native_gets: u64,
    background_gets: u64,
    media_bytes: u64,
}
impl Wire {
    fn gets(&self) -> u64 {
        self.native_gets + self.background_gets
    }
}

struct Server {
    origin: String,
    mode: Arc<Mutex<Mode>>,
    counts: Arc<Mutex<Wire>>,
    media: Arc<Vec<Vec<u8>>>,
    hold: Arc<lifecycle::Gate>,
    hold_background: Arc<AtomicBool>,
    hold_head: Arc<lifecycle::Gate>,
    hold_heads: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(directory: &Path) -> Self {
        let media = Arc::new(vec![
            permitted_audio(directory, 440.0),
            permitted_audio(directory, 880.0),
            encode_audio_with_bitrate(directory, 660.0, 600, 192),
        ]);
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let origin = format!("http://{}/", socket.local_addr().unwrap());
        let mode = Arc::new(Mutex::new(Mode::default()));
        let counts = Arc::new(Mutex::new(Wire::default()));
        let hold = Arc::new(lifecycle::Gate::default());
        let hold_background = Arc::new(AtomicBool::new(false));
        let hold_head = Arc::new(lifecycle::Gate::default());
        let hold_heads = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let (origin, mode, counts, media, hold, hold_background, hold_head, hold_heads, stop) = (
                origin.clone(),
                mode.clone(),
                counts.clone(),
                media.clone(),
                hold.clone(),
                hold_background.clone(),
                hold_head.clone(),
                hold_heads.clone(),
                stop.clone(),
            );
            thread::spawn(move || {
                let mut workers = Vec::new();
                while !stop.load(Ordering::Acquire) {
                    match socket.accept() {
                        Ok((stream, _)) => {
                            assert!(workers.len() < 256, "bounded replay fixture requests");
                            let (
                                origin,
                                mode,
                                counts,
                                media,
                                hold,
                                hold_background,
                                hold_head,
                                hold_heads,
                            ) = (
                                origin.clone(),
                                mode.clone(),
                                counts.clone(),
                                media.clone(),
                                hold.clone(),
                                hold_background.clone(),
                                hold_head.clone(),
                                hold_heads.clone(),
                            );
                            workers.push(thread::spawn(move || {
                                serve(
                                    stream,
                                    &origin,
                                    &mode,
                                    &counts,
                                    &media,
                                    &hold,
                                    &hold_background,
                                    &hold_head,
                                    &hold_heads,
                                )
                            }));
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(_) => panic!("replay fixture listener failed"),
                    }
                }
                for worker in workers {
                    worker.join().unwrap();
                }
            })
        };
        Self {
            origin,
            mode,
            counts,
            media,
            hold,
            hold_background,
            hold_head,
            hold_heads,
            stop,
            worker: Some(worker),
        }
    }
    fn counts(&self) -> Wire {
        self.counts.lock().unwrap().clone()
    }
    fn current(&self) -> Vec<u8> {
        self.media[usize::from(self.mode.lock().unwrap().version - 1)].clone()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.hold.release();
        self.hold_head.release();
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn track(id: &str) -> serde_json::Value {
    json!({"id":id,"title":format!("Original permitted audio {id}"),"durationMs":6000,"available":true,"artists":[{"id":7,"name":"Karin original fixture"}]})
}
#[allow(clippy::too_many_arguments)]
fn serve(
    mut stream: TcpStream,
    origin: &str,
    mode: &Mutex<Mode>,
    counts: &Mutex<Wire>,
    media: &[Vec<u8>],
    hold: &lifecycle::Gate,
    hold_background: &AtomicBool,
    hold_head: &lifecycle::Gate,
    hold_heads: &AtomicBool,
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut raw = Vec::new();
    while !raw.ends_with(b"\r\n\r\n") {
        let mut b = [0];
        if stream.read(&mut b).unwrap_or(0) == 0 {
            return;
        }
        raw.push(b[0]);
        assert!(raw.len() < 16 * 1024);
    }
    let request = String::from_utf8(raw).unwrap();
    let first = request
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{}", first[1])).unwrap();
    let header = |key: &str| {
        request.lines().skip(1).find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case(key).then_some(value.trim())
        })
    };
    let m = mode.lock().unwrap().clone();
    let resource = url.path().starts_with("/download-info/") || url.path().starts_with("/get-mp3/");
    if resource {
        for forbidden in ["authorization", "cookie", "origin", "referer"] {
            assert!(header(forbidden).is_none());
        }
    } else {
        assert!(matches!(
            header("authorization"),
            Some("OAuth synthetic-storage-ui-token" | "OAuth synthetic-storage-other-token")
        ));
    }
    let mut post = String::new();
    if first[0] == "POST" {
        let length = header("content-length").unwrap().parse::<usize>().unwrap();
        assert!(length < 16 * 1024);
        let mut body = vec![0; length];
        stream.read_exact(&mut body).unwrap();
        post = String::from_utf8(body).unwrap();
    }
    let json = match url.path() {
        "/account/status" => {
            counts.lock().unwrap().account += 1;
            json!({"result":{"account":{"uid":m.account,"displayName":"Original replay fixture"}}})
        }
        "/users/1234/likes/tracks" | "/users/5678/likes/tracks" => {
            json!({"result":{"library":{"uid":m.account,"revision":1,"tracks":[{"id":42},{"id":43}]}}})
        }
        "/users/1234/dislikes/tracks" | "/users/5678/dislikes/tracks" => {
            json!({"result":{"library":{"uid":m.account,"revision":1,"tracks":[]}}})
        }
        "/users/1234/playlists/list" | "/users/5678/playlists/list" => {
            json!({"result":[{"uid":m.account,"kind":77,"title":"Original fixture playlist","trackCount":2,"revision":1}]})
        }
        "/users/1234/likes/playlists" | "/users/5678/likes/playlists" => json!({"result":[]}),
        "/users/1234/playlists/77" | "/users/5678/playlists/77" => {
            json!({"result":{"uid":m.account,"kind":77,"title":"Original fixture playlist","trackCount":2,"revision":1,"tracks":[{"id":42,"track":track("42")},{"id":43,"track":track("43")}]}})
        }
        "/tracks" => {
            assert_eq!(first[0], "POST");
            let form = reqwest::Url::parse(&format!("http://127.0.0.1/?{post}")).unwrap();
            let ids = form
                .query_pairs()
                .filter(|(key, _)| key == "track-ids")
                .map(|(_, id)| id.into_owned())
                .collect::<Vec<_>>();
            assert!(!ids.is_empty());
            json!({"result":ids.iter().map(|id|track(id)).collect::<Vec<_>>()})
        }
        "/tracks/42" => json!({"result":[track("42")]}),
        "/tracks/42/supplement" => json!({"result":{}}),
        "/search" => {
            let kind = url
                .query_pairs()
                .find(|(key, _)| key == "type")
                .unwrap()
                .1
                .into_owned();
            match kind.as_str() {
                "all" => {
                    json!({"result":{"tracks":{"total":2,"perPage":20,"results":[{"id":42},{"id":43}]},"albums":{"total":0,"perPage":20,"results":[]},"artists":{"total":0,"perPage":20,"results":[]},"playlists":{"total":0,"perPage":20,"results":[]}}})
                }
                "track" => {
                    json!({"result":{"tracks":{"total":2,"perPage":20,"results":[{"id":42},{"id":43}]}}})
                }
                "album" => json!({"result":{"albums":{"total":0,"perPage":20,"results":[]}}}),
                "artist" => json!({"result":{"artists":{"total":0,"perPage":20,"results":[]}}}),
                "playlist" => json!({"result":{"playlists":{"total":0,"perPage":20,"results":[]}}}),
                _ => panic!("unexpected replay search category"),
            }
        }
        "/tracks/42/download-info" | "/tracks/43/download-info" => {
            counts.lock().unwrap().offers += 1;
            let object = if m.object {
                "other-object"
            } else {
                "original-object"
            };
            json!({"result":[{"codec":m.codec,"bitrateInKbps":m.bitrate,"preview":false,"direct":false,"downloadInfoUrl":format!("{origin}download-info/original-group/{object}?ns=original-audio&sign={:064x}&ts={:x}",m.timestamp,m.timestamp)}]})
        }
        path if path.starts_with("/download-info/original-group/") => {
            counts.lock().unwrap().descriptors += 1;
            thread::sleep(Duration::from_millis(30));
            let host = if m.host {
                "server-2026.storage.yandex.net"
            } else {
                "s268myt.storage.yandex.net"
            };
            let path = match m.path {
                0 => "/original-cc0.mp3",
                1 => "/replica-cc0.mp3",
                _ => "/moved-cc0.mp3",
            };
            let body = format!(
                "<download-info><host>{host}</host><path>{path}</path><ts>{:x}</ts><s>original-fixture-input</s></download-info>",
                m.timestamp
            );
            write_response(&mut stream, "application/xml", body.as_bytes());
            return;
        }
        path if path.starts_with("/get-mp3/") => {
            let is_head = first[0] == "HEAD";
            let background = !is_head && header("cache-control") == Some("no-cache");
            let bytes = &media[usize::from(m.version - 1)];
            let etag = match m.version {
                1 => "\"scoped-original-version-1\"",
                2 => "\"scoped-original-version-2\"",
                _ => "\"scoped-original-version-3\"",
            };
            {
                let mut c = counts.lock().unwrap();
                if is_head {
                    c.heads += 1;
                    c.conditional_heads += u64::from(header("if-none-match").is_some());
                } else if background {
                    c.background_gets += 1;
                } else {
                    c.native_gets += 1;
                }
            }
            if is_head && hold_heads.load(Ordering::Acquire) {
                hold_head.wait();
            }
            if background && hold_background.load(Ordering::Acquire) {
                hold.wait();
            }
            if m.denied {
                let _ = stream.write_all(
                    b"HTTP/1.1 451 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                return;
            }
            let not_modified = is_head && header("if-none-match") == Some(etag);
            let status = if not_modified {
                "304 Not Modified"
            } else {
                "200 OK"
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: audio/mpeg\r\nContent-Length: {}\r\nETag: {etag}\r\nCache-Control: private, no-cache, must-revalidate\r\nAccept-Ranges: bytes\r\nConnection: close\r\n",
                bytes.len()
            );
            if m.digest {
                let _ = write!(
                    stream,
                    "Repr-Digest: sha-256=:{}:\r\n",
                    STANDARD.encode(digest(&SHA256, bytes).as_ref())
                );
            }
            let _ = stream.write_all(b"\r\n");
            if !is_head {
                let end = if background && m.broken_background {
                    bytes.len() / 2
                } else {
                    bytes.len()
                };
                counts.lock().unwrap().media_bytes += end as u64;
                let _ = stream.write_all(&bytes[..end]);
            }
            return;
        }
        _ => panic!("unexpected original replay fixture route"),
    };
    write_response(&mut stream, "application/json", json.to_string().as_bytes());
}

fn make_input(directory: &Path, server: &Server) -> Input {
    let bytes = server.current();
    Input {
        directory: directory.to_owned(),
        origin: server.origin.clone(),
        sha256: digest(&SHA256, &bytes).as_ref().try_into().unwrap(),
        bytes: bytes.len() as u64,
        license: "CC0-1.0".into(),
    }
}
fn audio_root(input: &Input) -> PathBuf {
    input
        .directory
        .join("cache")
        .join(crate::config::APP_ID)
        .join("audio")
}
fn record(input: &Input) -> PathBuf {
    let paths = media_files(&audio_root(input));
    assert_eq!(paths.len(), 1);
    paths[0].clone()
}
fn assert_saved(input: &Input, inode: Option<u64>) {
    let path = record(input);
    assert_original_file(&path, input);
    if let Some(inode) = inode {
        assert_eq!(fs::metadata(path).unwrap().ino(), inode);
    }
}

pub(super) fn run_parent() {
    let directory = Directory::new();
    for kind in ["config", "cache", "data", "state"] {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(directory.0.join(kind))
            .unwrap();
    }
    let server = Server::new(&directory.0);
    let path = directory.0.join("replay-input.json");
    let mut reports = Vec::new();
    let mut original_inode = 0;
    for phase in CASES {
        if phase == "identity-upgrade-failed" {
            let input = make_input(&directory.0, &server);
            let http = record(&input).parent().unwrap().join("http");
            let mut metadata = Metadata::decode(&fs::read(&http).unwrap()).unwrap();
            metadata.binding = None;
            fs::write(http, metadata.encode().unwrap()).unwrap();
        }
        {
            let mut m = server.mode.lock().unwrap();
            m.timestamp += 1;
            match phase {
                "identity-upgrade-failed" => {
                    m.host = true;
                    m.path = 1;
                    m.broken_background = true;
                }
                "identity-upgrade" => m.broken_background = false,
                "identity-host" => m.host = false,
                "identity-path" => m.path = 2,
                "identity-unknown" | "identity-unknown-restart" => m.object = true,
                "identity-digest" => m.digest = true,
                "identity-version" => {
                    m.object = false;
                    m.digest = false;
                    m.version = 2;
                }
                "identity-variant" => {
                    m.version = 3;
                    m.bitrate = 192;
                }
                "identity-codec" => m.codec = "opus",
                "identity-denied" => {
                    m.codec = "mp3";
                    m.denied = true;
                }
                _ => {}
            }
        }
        let input = make_input(&directory.0, &server);
        fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
        let before = server.counts();
        let before_files = file_facts(&input);
        let result = run_phase(&path, phase, &directory.0);
        let after = server.counts();
        let extra: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.0.join("identity-result.json")).unwrap())
                .unwrap();
        assert_eq!(extra["phase"], phase);
        assert_eq!(extra["pid"], result.pid);
        assert_eq!(extra["passed"], true);
        let (native, background) = match phase {
            "identity-first"
            | "identity-upgrade-failed"
            | "identity-upgrade"
            | "identity-version"
            | "identity-variant" => (1, 1),
            "identity-unknown" => (2, 0),
            "identity-unknown-restart" | "identity-denied" => (1, 0),
            _ => (0, 0),
        };
        assert_eq!(
            after.native_gets - before.native_gets,
            native,
            "native transfer count for {phase}"
        );
        assert_eq!(
            after.background_gets - before.background_gets,
            background,
            "background transfer count for {phase}"
        );
        if phase == "identity-denied" {
            assert!(media_files(&audio_root(&input)).is_empty());
        } else {
            let old = matches!(
                phase,
                "identity-upgrade-failed"
                    | "identity-signed"
                    | "identity-host"
                    | "identity-path"
                    | "identity-unknown"
                    | "identity-unknown-restart"
                    | "identity-digest"
                    | "identity-codec"
            );
            assert_saved(&input, old.then_some(original_inode));
            if !old {
                original_inode = fs::metadata(record(&input)).unwrap().ino();
            }
        }
        if matches!(phase, "identity-signed" | "identity-host" | "identity-path") {
            assert_eq!(
                after.conditional_heads - before.conditional_heads,
                1,
                "ETag must be scoped to the stable logical object"
            );
        }
        if matches!(
            phase,
            "identity-unknown" | "identity-unknown-restart" | "identity-digest"
        ) {
            assert_eq!(
                after.conditional_heads, before.conditional_heads,
                "a coincidentally equal tag on a different object is not a conditional validator"
            );
        }
        reports.push(json!({"phase":phase,"pid":result.pid,"passed":true,"source":result.first_source,"replay":result.replay_source,"before":before,"after":after,"before_files":before_files,"after_files":file_facts(&input),"expected_bytes":input.bytes,"expected_payload_sha256":input.sha256.iter().map(|byte|format!("{byte:02x}")).collect::<String>(),"details":extra}));
    }
    assert_eq!(
        reports
            .iter()
            .map(|r| r["pid"].as_u64().unwrap())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        CASES.len()
    );
    if let Some(path) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
        fs::create_dir_all(&path).unwrap();
        fs::write(PathBuf::from(path).join("audio-cache-identity-receipt.json"),serde_json::to_vec_pretty(&json!({"scope":"ordinary-application-logical-mds-replay","real_yandex_service":false,"cases":reports,"processes":CASES.len()})).unwrap()).unwrap();
    }
    presence::run_parent();
}

pub(super) fn run_child(input: &Input, phase: &str) {
    if phase.starts_with("presence-") {
        presence::run_child(input, phase);
        return;
    }
    let (_app, ui) = application(input);
    let this = &ui.music.controller;
    let row = this.widgets.library_rows.row_at_index(0).unwrap();
    let expected_cached = matches!(
        phase,
        "identity-signed" | "identity-host" | "identity-path" | "identity-digest"
    );
    let existing = phase != "identity-first";
    if existing {
        wait_presence(this, "42", Presence::Stored);
    } else {
        wait_presence(this, "42", Presence::Missing);
    }
    assert!(
        this.model.borrow().queue.current().is_none(),
        "restored row status must not depend on a current player"
    );
    pointer(&ui.window, &row);
    let denied = matches!(phase, "identity-codec" | "identity-denied");
    if denied {
        pump_until(|| {
            !this.music_requested()
                && (this.widgets.status.title() == "Не удалось получить источник аудио"
                    || this
                        .music_snapshot()
                        .is_some_and(|s| s.state == PlaybackState::Error))
        });
        assert!(
            this.music_snapshot()
                .is_none_or(|s| s.state != PlaybackState::Playing
                    && s.source_kind != PlaybackSourceKind::CachedFile)
        );
    } else {
        wait_source(
            this,
            if expected_cached {
                PlaybackSourceKind::CachedFile
            } else {
                PlaybackSourceKind::RemoteHttps
            },
        );
    }
    if matches!(
        phase,
        "identity-first" | "identity-upgrade" | "identity-version" | "identity-variant"
    ) {
        wait_publication(this, input);
    }
    pump_until(|| this.cache_work_settled_for_test());
    let mut replay = None;
    if phase == "identity-unknown" {
        let serial = this.music_serial();
        assert!(row.grab_focus());
        key(&ui.window, "Return");
        pump_until(|| this.music_serial() > serial);
        wait_source(this, PlaybackSourceKind::RemoteHttps);
        pump_until(|| this.cache_work_settled_for_test());
        replay = Some("remote-https".to_owned());
    }
    if phase == "identity-denied" {
        wait_presence(this, "42", Presence::Missing);
    } else {
        wait_presence(this, "42", Presence::Stored);
    }
    let source = if denied {
        "none"
    } else if expected_cached {
        "cached-file"
    } else {
        "remote-https"
    };
    let details = json!({"phase":phase,"pid":std::process::id(),"passed":true,"source":source,"startup_stored_without_player":existing,"retained_good_file":matches!(phase,"identity-unknown"|"identity-unknown-restart"|"identity-upgrade-failed"),"local_replay":expected_cached});
    close_child(&ui, this);
    fs::write(
        input.directory.join("identity-result.json"),
        serde_json::to_vec(&details).unwrap(),
    )
    .unwrap();
    write_phase(
        input,
        PhaseReport {
            phase: phase.into(),
            pid: std::process::id(),
            account_revalidated: true,
            cache_policy: "private-http".into(),
            first_source: source.into(),
            replay_source: replay,
            clear_kept_playing: false,
            default_limit_bytes: 2 * 1024 * 1024 * 1024,
        },
    );
}

fn application(input: &Input) -> (adw::Application, crate::ui::MainWindow) {
    gtk::init().unwrap();
    adw::init().unwrap();
    assert_eq!(input.license, "CC0-1.0");
    let settings = SettingsStore::for_application().unwrap();
    let root = glib::user_cache_dir().join(crate::config::APP_ID);
    let budget = CacheBudget::new(
        root.join("persistent"),
        root.join("audio"),
        true,
        Preferences::default().cache_limit.bytes(),
    )
    .unwrap();
    let cache =
        Cache::new_with_budget(root.join("persistent"), CachePolicy::default(), budget).unwrap();
    let backend = Backend::new(Some(TOKEN));
    let mut state = State::for_testing(
        settings,
        CredentialStore::with_backend(backend.clone()),
        Some(cache),
    );
    state.audio_root = Some(root.join("audio"));
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.CacheAcceptance")
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
                Some(MusicService::for_test(&input.origin)),
                Arc::new(OfflineProvider),
                launch_system_browser,
                state,
            )
        },
    );
    ui.window.set_title(Some(TITLE));
    ui.window.set_default_size(900, 650);
    ui.window.present();
    let this = &ui.music.controller;
    pump_until(|| this.model.borrow().account_valid && settled(this));
    assert_eq!(backend.loads.load(Ordering::Acquire), 1);
    assert_idle_wave_after_startup(this, true);
    this.set_fixture_resolver_for_test(StreamResolver::for_test(
        &input.origin,
        &input.origin,
        &input.origin,
    ));
    pointer(&ui.window, &this.widgets.nav_liked);
    pump_until(|| {
        this.model
            .borrow()
            .liked
            .as_ref()
            .is_some_and(|r| r.as_ref().is_ok_and(|p| p.items.len() == 2))
    });
    (app, ui)
}
#[track_caller]
fn wait_presence(this: &Controller, id: &str, expected: Presence) {
    wait_presence_at(this, id, expected, "ordinary-state");
}
#[track_caller]
fn wait_presence_at(this: &Controller, id: &str, expected: Presence, step: &'static str) {
    let line = std::panic::Location::caller().line();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if !this.cache_presence_query_running_for_test()
            && this.cache_presence_for_test(id) == expected
            && !this.cache_indicators_for_test(id).is_empty()
        {
            break;
        }
        if Instant::now() >= deadline {
            let core = this.audio_cache_binding().map(|(cache, lease)| {
                let keys =
                    ["42", "43"].map(|key| crate::audio_cache::ResourceKey::track(key).unwrap());
                thread::spawn(move || cache.presence(&lease, &keys))
                    .join()
                    .unwrap()
            });
            let current = this.model.borrow().queue.current().map_or("none", |track| {
                match track.id.as_str() {
                    "42" => "primary",
                    "43" => "secondary",
                    _ => "other",
                }
            });
            let source = this
                .music_snapshot()
                .map(|snapshot| (snapshot.state, snapshot.source_kind));
            panic!(
                "native cache indicator step={step} line={line} expected={expected:?} observed={:?} query-running={} bindings={} current={current} source={source:?} serial={} core-primary-secondary={core:?}",
                this.cache_presence_for_test(id),
                this.cache_presence_query_running_for_test(),
                this.cache_indicators_for_test(id).len(),
                this.music_serial(),
            );
        }
        let context = glib::MainContext::default();
        if context.pending() {
            context.iteration(false);
        } else {
            thread::sleep(Duration::from_millis(5));
        }
    }
    let slots = this.cache_indicators_for_test(id);
    assert!(!slots.is_empty());
    for slot in slots {
        assert_eq!(slot.is_visible(), expected != Presence::Missing);
        let spinner = slot.first_child().and_downcast::<gtk::Spinner>().unwrap();
        let image = spinner.next_sibling().and_downcast::<gtk::Image>().unwrap();
        assert_eq!(spinner.is_visible(), expected == Presence::Downloading);
        assert_eq!(spinner.is_spinning(), expected == Presence::Downloading);
        assert_eq!(image.is_visible(), expected == Presence::Stored);
        if expected == Presence::Stored {
            assert!(
                slot.tooltip_text()
                    .unwrap()
                    .starts_with("Аудио сохранено в кеше. При воспроизведении проверяется доступ.")
            );
        }
        if expected == Presence::Downloading {
            assert_eq!(
                slot.tooltip_text().as_deref(),
                Some("Сохраняем аудио в кеш…")
            );
        }
    }
}

fn file_facts(input: &Input) -> Vec<serde_json::Value> {
    media_files(&audio_root(input)).into_iter().map(|path| {
        let bytes = fs::read(&path).unwrap();
        let metadata = fs::metadata(&path).unwrap();
        json!({"bytes":bytes.len(),"inode":metadata.ino(),"sha256":digest(&SHA256,&bytes).as_ref().iter().map(|byte|format!("{byte:02x}")).collect::<String>(),"completed":path.parent().unwrap().join("header").is_file()})
    }).collect()
}

pub(super) fn run_presence_only() {
    presence::run_parent();
}
