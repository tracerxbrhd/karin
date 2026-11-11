// SPDX-License-Identifier: GPL-3.0-or-later
//! Ordinary application replay with one logical MDS object and changing replicas.
//! Audio is original CC0 fixture data; opaque fixture ETags are deliberately NOT
//! hashes of that audio and are never treated as globally meaningful.
use super::*;
use crate::audio_cache::{Presence, http_policy::Metadata};
use base64::{Engine as _, engine::general_purpose::STANDARD};

mod localfirst;
mod presence;

const CASES: [&str; 17] = [
    "identity-first",
    "identity-legacy-unproved",
    "identity-legacy-upgrade",
    "identity-rebased",
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
    "identity-lm-first",
    "identity-lm-replica",
    "identity-lm-repeat",
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
    modified_only: bool,
    codec: &'static str,
    denied: bool,
    broken_background: bool,
    account: u64,
    cache_policy: &'static str,
    current_dates: bool,
    offer_denied: bool,
    preview: bool,
    protection: bool,
    unavailable: bool,
    gain: bool,
    omit_validators: bool,
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
            modified_only: false,
            codec: "mp3",
            denied: false,
            broken_background: false,
            account: 1234,
            cache_policy: "private, no-cache, must-revalidate",
            current_dates: false,
            offer_denied: false,
            preview: false,
            protection: false,
            unavailable: false,
            gain: false,
            omit_validators: false,
        }
    }
}
#[derive(Clone, Default, serde::Serialize)]
struct Wire {
    account: u64,
    tracks: u64,
    offers: u64,
    descriptors: u64,
    heads: u64,
    conditional_heads: u64,
    etag_conditionals: u64,
    modified_conditionals: u64,
    not_modified: u64,
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
        Self::new_on_port(directory, 0)
    }

    fn new_on_origin(directory: &Path, origin: &str) -> Self {
        let origin = reqwest::Url::parse(origin).unwrap();
        assert_eq!(origin.scheme(), "http");
        assert_eq!(origin.host_str(), Some("127.0.0.1"));
        assert!(origin.username().is_empty() && origin.password().is_none());
        assert!(origin.query().is_none() && origin.fragment().is_none());
        assert_eq!(origin.path(), "/");
        let port = origin.port().filter(|port| *port != 0).unwrap();
        Self::new_on_port(directory, port)
    }

    fn new_on_port(directory: &Path, port: u16) -> Self {
        use std::os::fd::AsFd;

        let media = Arc::new(vec![
            permitted_audio(directory, 440.0),
            permitted_audio(directory, 880.0),
            encode_audio_with_bitrate(directory, 660.0, 600, 192),
        ]);
        // Preserve the exact fixture object origin across isolated application
        // restarts. SO_REUSEADDR handles closed connections in TIME_WAIT; an
        // active listener still prevents accidental reuse by another fixture.
        let listener = gio::Socket::new(
            gio::SocketFamily::Ipv4,
            gio::SocketType::Stream,
            gio::SocketProtocol::Tcp,
        )
        .unwrap();
        let address =
            gio::InetSocketAddress::new(&gio::InetAddress::from_string("127.0.0.1").unwrap(), port);
        listener.bind(&address, true).unwrap();
        listener.listen().unwrap();
        let socket = TcpListener::from(listener.as_fd().try_clone_to_owned().unwrap());
        drop(listener);
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
fn track(id: &str, mode: &Mode) -> serde_json::Value {
    json!({"id":id,"title":format!("Original permitted audio {id}"),"durationMs":6000,"available":!mode.unavailable,"artists":[{"id":7,"name":"Karin original fixture"}]})
}
fn replica_etag(mode: &Mode) -> Option<String> {
    (!mode.modified_only && !mode.omit_validators).then(|| {
        // A replica-local opaque tag is deliberately independent of the audio
        // hash. Identical bytes on another placement have a different tag.
        format!(
            "\"original-replica-{}-{}-version-{}\"",
            u8::from(mode.host),
            mode.path,
            mode.version
        )
    })
}
fn replica_modified(mode: &Mode) -> &'static str {
    match (mode.host, mode.path) {
        (false, 0) => "Mon, 01 Jan 2024 00:00:00 GMT",
        (true, 1) => "Tue, 02 Jan 2024 00:00:00 GMT",
        (false, 1) => "Wed, 03 Jan 2024 00:00:00 GMT",
        _ => "Thu, 04 Jan 2024 00:00:00 GMT",
    }
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
            json!({"result":{"uid":m.account,"kind":77,"title":"Original fixture playlist","trackCount":2,"revision":1,"tracks":[{"id":42,"track":track("42", &m)},{"id":43,"track":track("43", &m)}]}})
        }
        "/tracks" => {
            counts.lock().unwrap().tracks += 1;
            assert_eq!(first[0], "POST");
            let form = reqwest::Url::parse(&format!("http://127.0.0.1/?{post}")).unwrap();
            let ids = form
                .query_pairs()
                .filter(|(key, _)| key == "track-ids")
                .map(|(_, id)| id.into_owned())
                .collect::<Vec<_>>();
            assert!(!ids.is_empty());
            json!({"result":ids.iter().map(|id|track(id, &m)).collect::<Vec<_>>()})
        }
        "/tracks/42" => json!({"result":[track("42", &m)]}),
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
            if m.offer_denied {
                let _ = stream.write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                );
                return;
            }
            let object = if m.object {
                "other-object"
            } else {
                "original-object"
            };
            json!({"result":[{"codec":m.codec,"bitrateInKbps":m.bitrate,"gain":m.gain,"preview":m.preview,"drm":m.protection,"direct":false,"downloadInfoUrl":format!("{origin}download-info/original-group/{object}?ns=original-audio&sign={:064x}&ts={:x}",m.timestamp,m.timestamp)}]})
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
            let etag = replica_etag(&m);
            let modified = if m.current_dates {
                localfirst::http_date(86_400)
            } else {
                replica_modified(&m).to_owned()
            };
            {
                let mut c = counts.lock().unwrap();
                if is_head {
                    c.heads += 1;
                    let etag_conditional = header("if-none-match").is_some();
                    let modified_conditional = header("if-modified-since").is_some();
                    c.conditional_heads += u64::from(etag_conditional || modified_conditional);
                    c.etag_conditionals += u64::from(etag_conditional);
                    c.modified_conditionals += u64::from(modified_conditional);
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
            let not_modified = is_head
                && if let Some(condition) = header("if-none-match") {
                    etag.as_deref() == Some(condition)
                } else {
                    header("if-modified-since") == Some(modified.as_str())
                };
            counts.lock().unwrap().not_modified += u64::from(not_modified);
            let status = if not_modified {
                "304 Not Modified"
            } else {
                "200 OK"
            };
            let date = if m.current_dates {
                localfirst::http_date(0)
            } else {
                "Wed, 10 Jan 2024 00:00:00 GMT".to_owned()
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: audio/mpeg\r\nContent-Length: {}\r\nDate: {date}\r\nCache-Control: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n",
                bytes.len(),
                m.cache_policy
            );
            if !m.omit_validators {
                let _ = write!(stream, "Last-Modified: {modified}\r\n");
            }
            if let Some(etag) = etag {
                let _ = write!(stream, "ETag: {etag}\r\n");
            }
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

fn cached_metadata(input: &Input) -> Option<Metadata> {
    let records = media_files(&audio_root(input));
    assert!(records.len() <= 1);
    records.first().map(|path| {
        Metadata::decode(&fs::read(path.parent().unwrap().join("http")).unwrap()).unwrap()
    })
}

fn same_validator_policy(before: &Option<Metadata>, after: &Option<Metadata>) -> bool {
    // A real successful validation can renew the persisted reuse clock without
    // changing the validator, its resource scope or the storage policy.
    let without_clock = |value: &Option<Metadata>| {
        value.clone().map(|mut value| {
            value.observed = None;
            value
        })
    };
    without_clock(before) == without_clock(after)
}

fn retained_candidate(phase: &str) -> bool {
    matches!(
        phase,
        "identity-host" | "identity-path" | "identity-unknown-restart" | "identity-lm-replica"
    )
}
fn cached_phase(phase: &str) -> bool {
    matches!(
        phase,
        "identity-legacy-upgrade" | "identity-rebased" | "identity-signed" | "identity-lm-repeat"
    )
}
fn first_save(phase: &str) -> bool {
    matches!(phase, "identity-first" | "identity-lm-first")
}
fn repeat_after_refresh(phase: &str) -> bool {
    matches!(phase, "identity-legacy-unproved" | "identity-unknown")
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
    let mut stored_mode: Option<Mode> = None;
    for phase in CASES {
        if matches!(
            phase,
            "identity-legacy-unproved" | "identity-legacy-upgrade"
        ) {
            let input = make_input(&directory.0, &server);
            let http = record(&input).parent().unwrap().join("http");
            let mut metadata = Metadata::decode(&fs::read(&http).unwrap()).unwrap();
            metadata.binding = None;
            fs::write(http, metadata.encode().unwrap()).unwrap();
        }
        let current_mode = {
            let mut m = server.mode.lock().unwrap();
            m.timestamp += 1;
            match phase {
                "identity-legacy-unproved" => {
                    m.host = true;
                    m.path = 1;
                }
                "identity-legacy-upgrade" => m.digest = true,
                "identity-rebased" => {
                    // Return the exact URI established by the metadata-only
                    // upgrade. A new replica's tag must not be replaced with
                    // the old replica tag when the binding is refreshed.
                    *m = stored_mode.clone().unwrap();
                    m.digest = false;
                }
                "identity-signed" => m.digest = true,
                "identity-host" => {
                    m.host = false;
                    m.digest = false;
                }
                "identity-path" => m.path = 2,
                "identity-unknown" => {
                    // A different current logical object selects a different
                    // application representation despite equal opaque tags.
                    let saved = stored_mode.as_ref().unwrap();
                    m.host = saved.host;
                    m.path = saved.path;
                    m.object = !saved.object;
                }
                "identity-unknown-restart" => {
                    // Matching bytes/tags and physical placement still do not
                    // prove a newly signed provider URI without full digest.
                    let timestamp = m.timestamp;
                    *m = stored_mode.clone().unwrap();
                    m.timestamp = timestamp;
                    m.digest = false;
                }
                "identity-digest" => {
                    let saved = stored_mode.as_ref().unwrap();
                    m.host = !saved.host;
                    m.path = 2;
                    m.object = !saved.object;
                    m.digest = true;
                }
                "identity-version" => {
                    // A real content/version change at the stored exact URI,
                    // not incomparable tags at different replica addresses.
                    *m = stored_mode.clone().unwrap();
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
                "identity-lm-first" => {
                    let timestamp = m.timestamp;
                    *m = Mode::default();
                    m.timestamp = timestamp;
                    m.modified_only = true;
                }
                "identity-lm-replica" => {
                    m.host = true;
                    m.path = 1;
                }
                "identity-lm-repeat" => {
                    // The retained original LM-only candidate can still be
                    // proved when its exact resource URI returns after restart.
                    *m = stored_mode.clone().unwrap();
                }
                _ => {}
            }
            m.clone()
        };
        let input = make_input(&directory.0, &server);
        fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
        let before = server.counts();
        let before_files = file_facts(&input);
        let before_metadata = cached_metadata(&input);
        let result = run_phase(&path, phase, &directory.0);
        let after = server.counts();
        let extra: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.0.join("identity-result.json")).unwrap())
                .unwrap();
        assert_eq!(extra["phase"], phase);
        assert_eq!(extra["pid"], result.pid);
        assert_eq!(extra["passed"], true);
        let (native, background) = match phase {
            "identity-first" | "identity-version" | "identity-variant" | "identity-lm-first" => {
                (1, 1)
            }
            "identity-legacy-unproved" => (1, 1),
            "identity-unknown" | "identity-digest" => (1, 1),
            "identity-host"
            | "identity-path"
            | "identity-unknown-restart"
            | "identity-denied"
            | "identity-lm-replica" => (1, 0),
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
        let offers = if repeat_after_refresh(phase) { 2 } else { 1 };
        let descriptors = if phase == "identity-codec" { 0 } else { offers };
        let heads = if first_save(phase)
            || matches!(
                phase,
                "identity-codec" | "identity-variant" | "identity-digest"
            ) {
            0
        } else if phase == "identity-unknown" {
            1
        } else {
            offers
        };
        assert_eq!(after.account - before.account, 1);
        assert_eq!(after.offers - before.offers, offers);
        assert_eq!(after.descriptors - before.descriptors, descriptors);
        assert_eq!(
            after.heads - before.heads,
            heads,
            "strict no-cache candidates require HTTP validation; changed selections are replaced for {phase}"
        );
        let media_bytes = if phase == "identity-denied" {
            0
        } else {
            (native + background) * input.bytes
        };
        assert_eq!(
            after.media_bytes - before.media_bytes,
            media_bytes,
            "exact audio response bytes for {phase}"
        );
        let after_metadata = cached_metadata(&input);
        let metadata_unchanged = same_validator_policy(&before_metadata, &after_metadata);
        if phase == "identity-denied" {
            assert!(media_files(&audio_root(&input)).is_empty());
        } else {
            let unchanged = !first_save(phase)
                && !matches!(
                    phase,
                    "identity-version"
                        | "identity-variant"
                        | "identity-legacy-unproved"
                        | "identity-unknown"
                        | "identity-digest"
                );
            assert_saved(&input, unchanged.then_some(original_inode));
            if !unchanged {
                original_inode = fs::metadata(record(&input)).unwrap().ino();
            }
        }
        if retained_candidate(phase) {
            assert!(
                metadata_unchanged,
                "unproved relocation must not rebind cached validators"
            );
            assert_eq!(
                after.conditional_heads, before.conditional_heads,
                "independent resources have no common opaque validator scope"
            );
        }
        if phase == "identity-legacy-upgrade" {
            assert!(
                before_metadata
                    .as_ref()
                    .is_some_and(|value| value.binding.is_none())
            );
            let migrated = after_metadata.as_ref().unwrap();
            assert!(
                migrated.binding.is_some(),
                "confirmed local hit upgrades the legacy sidecar"
            );
            assert!(
                migrated.etag == replica_etag(&current_mode),
                "metadata upgrade records the proven current response validator"
            );
            assert!(
                !metadata_unchanged,
                "legacy migration must publish current scoped metadata"
            );
        }
        if phase == "identity-rebased" {
            assert!(
                metadata_unchanged,
                "a confirmed legacy migration is idempotent"
            );
        }
        if phase == "identity-signed" {
            assert!(
                !metadata_unchanged,
                "full representation proof rebases validators to the proven current resource"
            );
        }
        let etag_conditions = u64::from(matches!(
            phase,
            "identity-rebased"
                | "identity-version"
                | "identity-legacy-unproved"
                | "identity-unknown"
        ));
        let modified_conditions = u64::from(phase == "identity-lm-repeat");
        assert_eq!(
            after.etag_conditionals - before.etag_conditionals,
            etag_conditions,
            "exact-URI ETag conditions for {phase}"
        );
        assert_eq!(
            after.modified_conditionals - before.modified_conditionals,
            modified_conditions,
            "exact-URI Last-Modified conditions for {phase}"
        );
        assert_eq!(
            after.conditional_heads - before.conditional_heads,
            etag_conditions + modified_conditions
        );
        let not_modified = u64::from(matches!(
            phase,
            "identity-rebased"
                | "identity-lm-repeat"
                | "identity-legacy-unproved"
                | "identity-unknown"
        ));
        assert_eq!(
            after.not_modified - before.not_modified,
            not_modified,
            "304 only follows a valid exact-resource condition for {phase}"
        );
        if after_metadata.is_some() && !metadata_unchanged {
            stored_mode = Some(current_mode.clone());
        }
        reports.push(json!({"phase":phase,"pid":result.pid,"passed":true,"source":result.first_source,"replay":result.replay_source,"before":before,"after":after,"before_files":before_files,"after_files":file_facts(&input),"expected_bytes":input.bytes,"expected_payload_sha256":input.sha256.iter().map(|byte|format!("{byte:02x}")).collect::<String>(),"validator_kind":if current_mode.modified_only {"last-modified"} else {"etag"},"full_repr_digest":current_mode.digest,"metadata_unchanged":metadata_unchanged,"binding_before":before_metadata.as_ref().is_some_and(|value|value.binding.is_some()),"binding_after":after_metadata.as_ref().is_some_and(|value|value.binding.is_some()),"details":extra}));
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
        fs::write(PathBuf::from(path).join("audio-cache-identity-receipt.json"),serde_json::to_vec_pretty(&json!({"scope":"ordinary-application-replica-validator-replay","real_yandex_service":false,"cases":reports,"processes":CASES.len()})).unwrap()).unwrap();
    }
    presence::run_parent();
    localfirst::run_parent();
}

pub(super) fn run_child(input: &Input, phase: &str) {
    if phase.starts_with("localfirst-") {
        localfirst::run_child(input, phase);
        return;
    }
    if phase.starts_with("presence-") {
        presence::run_child(input, phase);
        return;
    }
    let (_app, ui) = application(input);
    let this = &ui.music.controller;
    let row = this.widgets.library_rows.row_at_index(0).unwrap();
    let expected_cached = cached_phase(phase);
    let existing = !first_save(phase);
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
        "identity-first"
            | "identity-version"
            | "identity-variant"
            | "identity-lm-first"
            | "identity-legacy-unproved"
            | "identity-unknown"
            | "identity-digest"
    ) {
        wait_publication(this, input);
    }
    pump_until(|| this.cache_work_settled_for_test());
    let mut replay = None;
    if repeat_after_refresh(phase) {
        let serial = this.music_serial();
        assert!(row.grab_focus());
        key(&ui.window, "Return");
        pump_until(|| this.music_serial() > serial);
        let source = PlaybackSourceKind::CachedFile;
        wait_source(this, source);
        pump_until(|| this.cache_work_settled_for_test());
        replay = Some(source_name(source).to_owned());
    }
    // Same proofs rebase HTTP metadata on the existing lookup/LRU worker.
    // Observe that actual worker completion before AppClose can fence its
    // lease; do not substitute a delay or a second playback request.
    let (cache, _) = this.audio_cache_binding().unwrap();
    pump_until(|| cache.lookup_settled_for_test());
    drop(cache);
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
    let details = json!({"phase":phase,"pid":std::process::id(),"passed":true,"source":source,"startup_stored_without_player":existing,"retained_good_file":retained_candidate(phase),"local_replay":expected_cached});
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
