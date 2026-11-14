// SPDX-License-Identifier: GPL-3.0-or-later
//! Normal application playback with an independent, bounded whole-file GET.
//!
//! Native GStreamer receives the complete original CC0 MP3. Only the separate
//! cache response is gated, refused, changed, truncated or timed out. These
//! local HTTP cases do not claim to reproduce Yandex's live CDN behavior.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::collections::BTreeSet;

const CASES: [&str; 16] = [
    "completion-eos",
    "completion-no-range",
    "completion-wave",
    "completion-changed",
    "completion-expired",
    "completion-no-store",
    "completion-skip",
    "completion-seek",
    "completion-stop",
    "completion-disable",
    "completion-clear",
    "completion-logout",
    "completion-rapid",
    "completion-forbidden",
    "completion-broken-body",
    "completion-timeout",
];
const FIXTURE: &str = "independent-full-response";

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct Counts {
    account: u64,
    offers: [u64; 4],
    descriptors: [u64; 4],
    media_gets: [u64; 4],
    native_full_gets: [u64; 4],
    background_full_gets: [u64; 4],
    media_heads: [u64; 4],
    background_requests: [u64; 4],
    sent_bytes: [u64; 4],
    range_gets: u64,
    partial_responses: u64,
    expired_old_requests: u64,
    sessions: u64,
    batches: u64,
    started_batches: Vec<u64>,
    track_finished: u64,
}

struct Source {
    origin: String,
    directory: PathBuf,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Source {
    fn new(directory: &Path, profile: &str, media: [Arc<Vec<u8>>; 4], prefix: [usize; 4]) -> Self {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let origin = format!("http://{}/", socket.local_addr().unwrap());
        let counts = Arc::new(Mutex::new(Counts::default()));
        let stop = Arc::new(AtomicBool::new(false));
        save_counts(directory, &counts.lock().unwrap());
        let worker = {
            let directory = directory.to_owned();
            let profile = profile.to_owned();
            let origin = origin.clone();
            let stop = stop.clone();
            thread::spawn(move || {
                let mut children = Vec::new();
                while !stop.load(Ordering::Acquire) {
                    match socket.accept() {
                        Ok((stream, _)) => {
                            assert!(children.len() < 160, "bounded completion fixture requests");
                            let (directory, profile, origin, counts, stop, media) = (
                                directory.clone(),
                                profile.clone(),
                                origin.clone(),
                                counts.clone(),
                                stop.clone(),
                                media.clone(),
                            );
                            children.push(thread::spawn(move || {
                                serve(
                                    stream, &directory, &profile, &origin, &counts, &stop, &media,
                                    prefix,
                                );
                            }));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => panic!("completion fixture listener failed"),
                    }
                }
                for child in children {
                    child.join().unwrap();
                }
            })
        };
        Self {
            origin,
            directory: directory.to_owned(),
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for n in 0..4 {
            release(&self.directory, &format!("primary-{n}"));
            release(&self.directory, &format!("background-{n}"));
        }
        self.worker.take().unwrap().join().unwrap();
    }
}
fn save_counts(directory: &Path, counts: &Counts) {
    let temporary = directory.join("completion-wire.tmp");
    fs::write(&temporary, serde_json::to_vec(counts).unwrap()).unwrap();
    fs::rename(temporary, directory.join("completion-wire.json")).unwrap();
}
fn update<T>(directory: &Path, counts: &Mutex<Counts>, f: impl FnOnce(&mut Counts) -> T) -> T {
    let mut value = counts.lock().unwrap();
    let result = f(&mut value);
    save_counts(directory, &value);
    result
}
fn counts(input: &Input) -> Counts {
    serde_json::from_slice(&fs::read(input.directory.join("completion-wire.json")).unwrap())
        .unwrap()
}
fn release(directory: &Path, name: &str) {
    fs::write(directory.join(format!("release-{name}")), b"release").unwrap();
}
fn held(directory: &Path, name: &str) -> bool {
    directory.join(format!("entered-{name}")).exists()
}
fn gate(directory: &Path, stop: &AtomicBool, name: &str) {
    fs::write(directory.join(format!("entered-{name}")), b"entered").unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    while !directory.join(format!("release-{name}")).exists() && !stop.load(Ordering::Acquire) {
        assert!(
            Instant::now() < deadline,
            "completion HTTP gate was not released"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn index(id: &str) -> usize {
    match id {
        "42" => 0,
        "43" => 1,
        "44" => 2,
        "45" => 3,
        _ => panic!("unknown fixture track"),
    }
}
fn track_value(id: &str) -> serde_json::Value {
    json!({"id":id,"title":"Original completion fixture","durationMs":1200,"available":true,"artists":[{"id":7,"name":"Karin original CC0"}]})
}
fn wave_value(batch: u64, ids: &[&str], initial: bool) -> serde_json::Value {
    let mut value = json!({"result":{"batchId":format!("completion-batch-{batch}"),"sequence":ids.iter().map(|id|json!({"type":"track","track":{"id":id}})).collect::<Vec<_>>()}});
    if initial {
        value["result"]["radioSessionId"] = "completion-session".into();
    }
    value
}

#[allow(clippy::too_many_arguments)]
fn serve(
    mut stream: TcpStream,
    directory: &Path,
    profile: &str,
    origin: &str,
    counts: &Mutex<Counts>,
    stop: &AtomicBool,
    media: &[Arc<Vec<u8>>; 4],
    prefix: [usize; 4],
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            return;
        }
        request.push(byte[0]);
        assert!(request.len() < 16 * 1024);
    }
    let request = String::from_utf8(request).unwrap();
    let first = request
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{}", first[1])).unwrap();
    let header = |name: &str| {
        request.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then_some(value.trim())
        })
    };
    let path = url.path();
    if path.starts_with("/download-info/") || path.starts_with("/get-mp3/") {
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
        let mut value = vec![0; length];
        stream.read_exact(&mut value).unwrap();
        value
    } else {
        Vec::new()
    };
    let reply = if path == "/account/status" {
        update(directory, counts, |c| c.account += 1);
        json!({"result":{"account":{"uid":1234,"displayName":"Completion fixture"}}})
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
        let ids = pairs
            .query_pairs()
            .filter(|(key, _)| key == "track-ids")
            .map(|(_, value)| value.into_owned())
            .collect::<Vec<_>>();
        assert!(!ids.is_empty() && ids.len() <= 4);
        json!({"result":ids.iter().map(|id| {index(id);track_value(id)}).collect::<Vec<_>>()})
    } else if path == "/rotor/session/new" {
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["seeds"], json!(["user:onyourwave"]));
        update(directory, counts, |c| c.sessions += 1);
        wave_value(1, &["42", "43"], true)
    } else if path == "/rotor/session/completion-session/tracks" {
        let batch = update(directory, counts, |c| {
            c.batches += 1;
            c.batches + 1
        });
        wave_value(batch, &["44", "45"], false)
    } else if path == "/rotor/session/completion-session/feedback" {
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        update(directory, counts, |c| {
            match value["event"]["type"].as_str().unwrap() {
                "trackStarted" => {
                    c.started_batches
                        .push(match value["batchId"].as_str().unwrap() {
                            "completion-batch-1" => 1,
                            "completion-batch-2" => 2,
                            _ => panic!("unknown original batch"),
                        })
                }
                "trackFinished" => c.track_finished += 1,
                "radioStarted" | "skip" => (),
                _ => panic!("unsupported feedback"),
            }
        });
        json!({"result":"accepted"})
    } else if let Some(id) = path
        .strip_prefix("/tracks/")
        .and_then(|p| p.strip_suffix("/download-info"))
    {
        update(directory, counts, |c| c.offers[index(id)] += 1);
        json!({"result":[{"codec":"mp3","bitrateInKbps":128,"preview":false,"direct":false,"downloadInfoUrl":format!("{origin}download-info/{id}")} ]})
    } else if let Some(id) = path.strip_prefix("/download-info/") {
        let n = index(id);
        let descriptor = update(directory, counts, |c| {
            c.descriptors[n] += 1;
            c.descriptors[n]
        });
        let timestamp = if profile == "completion-expired" {
            1_700_000_000 + descriptor - 1
        } else {
            1_700_000_000
        };
        let body = format!(
            "<download-info><host>s268myt.storage.yandex.net</host><path>/completion-{id}.mp3</path><ts>{timestamp}</ts><s>completion-fixture</s></download-info>"
        );
        write_response(&mut stream, "application/xml", body.as_bytes());
        return;
    } else if path.starts_with("/get-mp3/") {
        let id = path
            .rsplit('/')
            .next()
            .unwrap()
            .strip_prefix("completion-")
            .unwrap()
            .strip_suffix(".mp3")
            .unwrap();
        let n = index(id);
        let background = header("cache-control") == Some("no-cache");
        let changed =
            profile == "completion-changed" && n == 0 && (background || first[0] == "HEAD");
        let data = if changed { &media[1] } else { &media[n] };
        let etag = if changed {
            "\"completion-v2\""
        } else {
            "\"completion-v1\""
        };
        let allowed = n == 0
            || (profile == "completion-wave" && n < 3)
            || profile == "completion-rapid"
            || (profile == "completion-forbidden" && n == 1);
        let no_store = !allowed || profile == "completion-no-store";
        let policy = if no_store {
            "no-store"
        } else {
            "private, no-cache, must-revalidate"
        };
        if first[0] == "HEAD" {
            update(directory, counts, |c| c.media_heads[n] += 1);
            let status = if header("if-none-match") == Some(etag) {
                "304 Not Modified"
            } else {
                "200 OK"
            };
            // The expiry case deliberately renews the signed provider URI on
            // every descriptor response. Its opaque tag cannot span those
            // resources; supply a digest of this entire original representation
            // so the later local replay has actual independent byte proof.
            let digest_header = if profile == "completion-expired" {
                format!(
                    "Repr-Digest: sha-256=:{}:\r\n",
                    STANDARD.encode(digest(&SHA256, data).as_ref())
                )
            } else {
                String::new()
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: audio/mpeg\r\nContent-Length: {}\r\nETag: {etag}\r\nCache-Control: {policy}\r\nConnection: close\r\n{digest_header}\r\n",
                data.len()
            );
            return;
        }
        assert_eq!(first[0], "GET");
        let range = header("range").map(|value| {
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
        let request = update(directory, counts, |c| {
            if range.is_some() {
                c.range_gets += 1;
                c.partial_responses += 1;
            } else {
                c.media_gets[n] += 1;
                if background {
                    c.background_full_gets[n] += 1;
                    c.background_requests[n] += 1;
                } else {
                    c.native_full_gets[n] += 1;
                }
            }
            c.background_full_gets[n]
        });
        if background
            && n == 0
            && ((profile == "completion-expired" && path.contains("/1700000000/"))
                || profile == "completion-forbidden")
        {
            if profile == "completion-expired" {
                update(directory, counts, |c| c.expired_old_requests += 1);
            }
            let _ = stream.write_all(
                b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            return;
        }
        let refused_partial = background && profile == "completion-forbidden" && n == 1;
        let (start, end) = if refused_partial {
            (0, data.len() / 2)
        } else {
            range.unwrap_or((0, data.len() - 1))
        };
        let partial = range.is_some() || refused_partial;
        if refused_partial {
            update(directory, counts, |c| c.partial_responses += 1);
        }
        let status = if partial {
            "206 Partial Content"
        } else {
            "200 OK"
        };
        let ranges = if profile == "completion-no-range" {
            "none"
        } else {
            "bytes"
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: audio/mpeg\r\nContent-Length: {}\r\nETag: {etag}\r\nCache-Control: {policy}\r\nAccept-Ranges: {ranges}\r\nConnection: close\r\n",
            end - start + 1
        );
        if partial {
            let _ = write!(
                stream,
                "Content-Range: bytes {start}-{end}/{}\r\n",
                data.len()
            );
        }
        let _ = stream.write_all(b"\r\n");
        if background && profile == "completion-broken-body" && n == 0 {
            let boundary = 1024.min(data.len() / 2);
            if stream.write_all(&data[..boundary]).is_ok() {
                update(directory, counts, |c| c.sent_bytes[n] += boundary as u64);
            }
            return;
        }
        // This barrier applies only to the background HTTP client. Native
        // playback stays independent and receives its complete MP3 immediately.
        let held_background = background
            && n == 0
            && request == 1
            && matches!(
                profile,
                "completion-eos"
                    | "completion-skip"
                    | "completion-seek"
                    | "completion-stop"
                    | "completion-disable"
                    | "completion-clear"
                    | "completion-logout"
                    | "completion-rapid"
                    | "completion-timeout"
            );
        // Keep the fourth real Wave source pending until Stop retires it, so
        // observing the first three natural EOS events cannot race batch three.
        let held_fourth = !background && profile == "completion-wave" && n == 3;
        let boundary = if held_fourth {
            prefix[n]
        } else {
            1024.min(data.len() / 2)
        };
        if held_background || held_fourth {
            if stream.write_all(&data[start..boundary]).is_err() {
                return;
            }
            update(directory, counts, |c| {
                c.sent_bytes[n] += (boundary - start) as u64
            });
            gate(
                directory,
                stop,
                &format!("{}-{n}", if held_fourth { "primary" } else { "background" }),
            );
            if stream.write_all(&data[boundary..=end]).is_ok() {
                update(directory, counts, |c| {
                    c.sent_bytes[n] += (end - boundary + 1) as u64
                });
            }
        } else if stream.write_all(&data[start..=end]).is_ok() {
            update(directory, counts, |c| {
                c.sent_bytes[n] += (end - start + 1) as u64
            });
        }
        return;
    } else if first[0] == "GET"
        && let Some(response) = crate::ui::music::tests::empty_similar_response(path)
    {
        response
    } else {
        panic!("unexpected completion fixture route")
    };
    write_response(
        &mut stream,
        "application/json",
        reply.to_string().as_bytes(),
    );
}

pub(super) fn run_parent() {
    run_profiles(None);
}
pub(super) fn run_parent_only(phase: &str) {
    assert!(CASES.contains(&phase));
    run_profiles(Some(phase));
}
fn run_profiles(only: Option<&str>) {
    let mut reports = Vec::new();
    for phase in CASES {
        if only.is_some_and(|selected| selected != phase) {
            continue;
        }
        let directory = Directory::new();
        for kind in ["config", "cache", "data", "state"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(directory.0.join(kind))
                .unwrap();
        }
        let first = encode_audio(
            &directory.0,
            440.0,
            if matches!(phase, "completion-eos" | "completion-wave") {
                120
            } else {
                600
            },
        );
        let second = encode_audio(
            &directory.0,
            660.0,
            if phase == "completion-wave" { 120 } else { 600 },
        );
        let third = encode_audio(&directory.0, 880.0, 120);
        let fourth = encode_audio(&directory.0, 990.0, 600);
        let prefix = [
            first.len() / 2,
            second.len() / 2,
            third.len() / 2,
            fourth.len() / 2,
        ];
        let media = [first, second, third, fourth].map(Arc::new);
        let expected = media
            .iter()
            .map(|value| {
                (
                    value.len() as u64,
                    <[u8; 32]>::try_from(digest(&SHA256, value).as_ref()).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        fs::write(
            directory.0.join("completion-expected-media.json"),
            serde_json::to_vec(&expected).unwrap(),
        )
        .unwrap();
        let source = Source::new(&directory.0, phase, media.clone(), prefix);
        let input = Input {
            directory: directory.0.clone(),
            origin: source.origin.clone(),
            sha256: digest(&SHA256, &media[usize::from(phase == "completion-changed")])
                .as_ref()
                .try_into()
                .unwrap(),
            bytes: media[usize::from(phase == "completion-changed")].len() as u64,
            license: "CC0-1.0".into(),
        };
        let path = directory.0.join("completion-input.json");
        fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
        let result = run_phase(&path, phase, &directory.0);
        let report: serde_json::Value = serde_json::from_slice(
            &fs::read(directory.0.join(format!("{phase}-result.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(report["pid"], result.pid);
        assert_eq!(report["passed"], true);
        reports.push(report);
        if phase == "completion-eos" {
            let before = counts(&input);
            let paths = media_files(
                &directory
                    .0
                    .join("cache")
                    .join(crate::config::APP_ID)
                    .join("audio"),
            );
            assert_eq!(paths.len(), 1);
            let inode = fs::metadata(&paths[0]).unwrap().ino();
            let restarted = run_phase(&path, "completion-restart", &directory.0);
            assert_ne!(result.pid, restarted.pid);
            let after = counts(&input);
            assert_eq!(after.media_gets, before.media_gets);
            assert!(after.account > before.account);
            assert!(after.media_heads[0] > before.media_heads[0]);
            assert_eq!(fs::metadata(&paths[0]).unwrap().ino(), inode);
            assert_original_file(&paths[0], &input);
            reports.push(
                serde_json::from_slice(
                    &fs::read(directory.0.join("completion-restart-result.json")).unwrap(),
                )
                .unwrap(),
            );
        }
    }
    let pids = reports
        .iter()
        .map(|r| r["pid"].as_u64().unwrap())
        .collect::<BTreeSet<_>>();
    if let Some(phase) = only {
        assert_eq!(pids.len(), if phase == "completion-eos" { 2 } else { 1 });
        return;
    }
    assert_eq!(pids.len(), 17);
    if let Some(path) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
        fs::create_dir_all(&path).unwrap();
        fs::write(PathBuf::from(path).join("audio-cache-completion-receipt.json"),serde_json::to_vec_pretty(&json!({"scope":"ordinary-application-bounded-cache-completion","real_yandex_service":false,"mechanism":"independent-full-response","processes":17,"cases":reports})).unwrap()).unwrap();
    }
}

fn no_records(this: &Controller, input: &Input) {
    pump_until(|| this.cache_work_settled_for_test());
    let root = input
        .directory
        .join("cache")
        .join(crate::config::APP_ID)
        .join("audio");
    assert!(
        media_files(&root).is_empty(),
        "refused or cancelled background response must leave no partial or completed audio"
    );
}
fn expected_media(input: &Input) -> Vec<(u64, [u8; 32])> {
    serde_json::from_slice(
        &fs::read(input.directory.join("completion-expected-media.json")).unwrap(),
    )
    .unwrap()
}
fn wait_expected(this: &Controller, input: &Input, indexes: &[usize]) {
    let (cache, _) = this.audio_cache_binding().unwrap();
    let root = input
        .directory
        .join("cache")
        .join(crate::config::APP_ID)
        .join("audio");
    let expected = expected_media(input);
    let expected = indexes.iter().map(|&n| expected[n]).collect::<Vec<_>>();
    let ready = Arc::new(AtomicBool::new(false));
    let observed = ready.clone();
    let worker = thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(7);
        while Instant::now() < end {
            if cache
                .stats()
                .is_ok_and(|stats| stats.entries == expected.len())
                && expected
                    .iter()
                    .all(|(bytes, hash)| completed_media_matches(&root, *bytes, *hash))
            {
                observed.store(true, Ordering::Release);
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    });
    pump_until(|| ready.load(Ordering::Acquire));
    worker.join().unwrap();
}
pub(super) fn run_child(input: &Input, phase: &str) {
    assert!(CASES.contains(&phase) || phase == "completion-restart");
    gtk::init().unwrap();
    adw::init().unwrap();
    let (_app, ui) = lifecycle::normal_window(input, &input.origin);
    let this = &ui.music.controller;
    let root = input
        .directory
        .join("cache")
        .join(crate::config::APP_ID)
        .join("audio");
    let mut cached_source = false;
    let mut replay_source = "none";
    let mut next_while_pending = false;
    let mut background_held = false;
    let mut paused_while_pending = false;
    let mut seek_while_pending = false;
    if phase == "completion-restart" {
        lifecycle::start(this, &["42"], true);
        wait_source(this, PlaybackSourceKind::CachedFile);
        cached_source = true;
        replay_source = "cached-file";
        let files = media_files(&root);
        assert_eq!(files.len(), 1);
        assert_original_file(&files[0], input);
    } else if phase == "completion-wave" {
        this.start_wave();
        for id in ["42", "43", "44"] {
            lifecycle::playing(this, id);
        }
        pump_until(|| held(&input.directory, "primary-3"));
        wait_expected(this, input, &[0, 1, 2]);
        pump_until(|| {
            counts(input).started_batches.len() >= 3 && counts(input).track_finished >= 3
        });
        // The fourth selected source may still start its independent no-store
        // job after native Loading. Finish that known job before measuring
        // whether passive queue markers introduce any NEW media requests.
        pump_until(|| this.cache_work_settled_for_test());
        let wire = counts(input);
        assert_eq!(&wire.started_batches[..3], &[1, 1, 2]);
        assert_eq!(&wire.native_full_gets[..3], &[1, 1, 1]);
        assert_eq!(&wire.background_full_gets[..3], &[1, 1, 1]);
        // Wave uses the same passive queue markers, backed by the three
        // complete records above. Opening the view must not issue API/media
        // calls, alter original batch markers or mark the no-store fourth
        // source saved while its native response is still pending.
        this.toggle_queue_panel();
        pump_until(|| this.panel_mode.get() == crate::ui::music::workspace::PanelMode::Queue);
        for id in ["42", "43", "44"] {
            pump_until(|| {
                !this.cache_presence_query_running_for_test()
                    && this.cache_presence_for_test(id) == crate::audio_cache::Presence::Stored
            });
            let indicator = this
                .cache_indicators_for_test(id)
                .into_iter()
                .find(|slot| slot.widget_name() == "music-queue-cache" && slot.is_mapped())
                .expect("the completed Wave resource has a mapped native queue marker");
            assert!(indicator.is_visible());
            assert!(
                indicator
                    .last_child()
                    .and_downcast::<gtk::Image>()
                    .unwrap()
                    .is_visible()
            );
        }
        pump_until(|| {
            this.cache_work_settled_for_test()
                && this.cache_presence_for_test("45") == crate::audio_cache::Presence::Missing
        });
        for indicator in this.cache_indicators_for_test("45") {
            assert!(
                !indicator
                    .last_child()
                    .and_downcast::<gtk::Image>()
                    .unwrap()
                    .is_visible(),
                "a refused pending Wave resource has no saved marker"
            );
        }
        assert_eq!(
            counts(input).media_gets,
            wire.media_gets,
            "queue presence must not fetch audio"
        );
        assert_eq!(counts(input).started_batches, wire.started_batches);
        assert_eq!(counts(input).track_finished, wire.track_finished);
        this.set_panel_mode(crate::ui::music::workspace::PanelMode::Closed);
        // Do not release the fourth real source until the ordinary Stop intent
        // retires the Wave session and all its remaining cache work.
        this.stop_music();
        release(&input.directory, "primary-3");
        pump_until(|| this.cache_work_settled_for_test());
    } else {
        lifecycle::start(
            this,
            if phase == "completion-rapid" {
                &["42", "43", "44", "45"]
            } else if phase == "completion-eos" {
                &["42", "43"]
            } else {
                &["42"]
            },
            true,
        );
        lifecycle::playing(this, "42");
        if matches!(
            phase,
            "completion-eos"
                | "completion-skip"
                | "completion-seek"
                | "completion-stop"
                | "completion-disable"
                | "completion-clear"
                | "completion-logout"
                | "completion-rapid"
                | "completion-timeout"
        ) {
            pump_until(|| held(&input.directory, "background-0"));
            background_held = true;
        }
        match phase {
            "completion-eos" => {
                // The complete real MP3 naturally reaches EOS and starts
                // the next context item while its separate GET is still held.
                lifecycle::playing(this, "43");
                next_while_pending = true;
                release(&input.directory, "background-0");
                wait_publication(this, input);
            }
            "completion-skip" | "completion-rapid" => {
                let ids: &[&str] = if phase == "completion-rapid" {
                    &["43", "44", "45"]
                } else {
                    &["43"]
                };
                for id in ids {
                    // Context replacement is an ordinary user action. It must
                    // start the next player while the first full GET is held.
                    lifecycle::start(this, &[*id], true);
                    lifecycle::playing(this, id);
                }
                next_while_pending = true;
                release(&input.directory, "background-0");
                if phase == "completion-rapid" {
                    wait_expected(this, input, &[0, 1, 2, 3]);
                    assert_eq!(counts(input).background_full_gets, [1, 1, 1, 1]);
                } else {
                    wait_publication(this, input);
                }
            }
            "completion-seek" => {
                this.pause_music();
                pump_until(|| {
                    this.music_snapshot()
                        .is_some_and(|s| s.state == PlaybackState::Paused)
                });
                paused_while_pending = true;
                let serial = this.music_snapshot().unwrap().seek_serial;
                this.seek_music(Duration::from_secs(3));
                pump_until(|| {
                    this.music_snapshot()
                        .is_some_and(|s| s.seek_serial > serial)
                        && counts(input).range_gets > 0
                });
                seek_while_pending = true;
                this.play_music();
                lifecycle::playing(this, "42");
                release(&input.directory, "background-0");
                wait_publication(this, input);
            }
            "completion-stop" | "completion-disable" | "completion-clear" | "completion-logout" => {
                lifecycle::start(this, &["43"], true);
                lifecycle::playing(this, "43");
                next_while_pending = true;
                let before = this.music_snapshot().unwrap();
                match phase {
                    "completion-stop" => this.stop_music(),
                    "completion-disable" => {
                        let mut preferences = this.persistence.borrow().values.clone();
                        preferences.cache_enabled = false;
                        this.preferences_changed(preferences);
                        assert!(this.audio_cache_binding().is_none());
                    }
                    "completion-clear" => this.clear_persistent_cache(),
                    "completion-logout" => this.logout(),
                    _ => unreachable!(),
                }
                release(&input.directory, "background-0");
                no_records(this, input);
                if matches!(phase, "completion-clear" | "completion-disable") {
                    assert!(this.model.borrow().account_valid);
                    pump_until(|| {
                        this.music_snapshot().is_some_and(|s| {
                            s.state == PlaybackState::Playing
                                && s.generation == before.generation
                                && s.playing_time > before.playing_time + Duration::from_millis(40)
                        })
                    });
                }
                if phase == "completion-logout" {
                    assert!(this.model.borrow().token.is_none());
                    assert!(this.music_snapshot().is_none());
                }
            }
            "completion-forbidden" => {
                no_records(this, input);
                assert_eq!(
                    counts(input).background_full_gets[0],
                    1,
                    "unchanged descriptor must not retry a forbidden CDN URI"
                );
                assert_eq!(this.music_snapshot().unwrap().state, PlaybackState::Playing);
                lifecycle::start(this, &["43"], true);
                lifecycle::playing(this, "43");
                no_records(this, input);
                assert_eq!(counts(input).background_full_gets[1], 1);
                assert_eq!(
                    counts(input).partial_responses,
                    1,
                    "unexpected full-download 206 must not publish a partial file"
                );
            }
            "completion-timeout" => {
                no_records(this, input);
                assert_eq!(this.music_snapshot().unwrap().state, PlaybackState::Playing);
                release(&input.directory, "background-0");
            }
            "completion-no-store" | "completion-broken-body" => {
                no_records(this, input);
                assert_eq!(
                    this.music_snapshot().unwrap().state,
                    PlaybackState::Playing,
                    "failed or refused caching must preserve online playback"
                );
            }
            "completion-no-range" | "completion-changed" | "completion-expired" => {
                wait_publication(this, input)
            }
            _ => unreachable!(),
        }
        if matches!(
            phase,
            "completion-eos"
                | "completion-no-range"
                | "completion-changed"
                | "completion-expired"
                | "completion-skip"
                | "completion-seek"
        ) {
            let files = media_files(&root);
            assert_eq!(files.len(), 1);
            assert_original_file(&files[0], input);
            let before = counts(input).media_gets;
            lifecycle::start(this, &["42"], true);
            wait_source(this, PlaybackSourceKind::CachedFile);
            cached_source = true;
            replay_source = "cached-file";
            let wire = counts(input);
            assert_eq!(
                wire.media_gets, before,
                "replay must use the complete local file without another media GET"
            );
            assert_eq!(wire.native_full_gets[0], 1);
            assert_eq!(
                wire.background_full_gets[0],
                if phase == "completion-expired" { 2 } else { 1 }
            );
            assert_eq!(
                wire.expired_old_requests,
                u64::from(phase == "completion-expired")
            );
            if phase == "completion-expired" {
                assert!(wire.descriptors[0] >= 3);
            }
        }
    }
    let files = media_files(&root);
    let records = files.len();
    let wire = counts(input);
    let cached_files = files.iter().map(|path| {
        let bytes = fs::read(path).unwrap();
        json!({"bytes":bytes.len(),"sha256":digest(&SHA256,&bytes).as_ref().iter().map(|byte|format!("{byte:02x}")).collect::<String>(),"inode":fs::metadata(path).unwrap().ino()})
    }).collect::<Vec<_>>();
    for n in 0..4 {
        release(&input.directory, &format!("primary-{n}"));
        release(&input.directory, &format!("background-{n}"));
    }
    close_child(&ui, this);
    let expected_sha256 = input
        .sha256
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    fs::write(input.directory.join(format!("{phase}-result.json")),serde_json::to_vec_pretty(&json!({"phase":phase,"pid":std::process::id(),"passed":true,"mechanism":FIXTURE,"counters":wire,"cached_records":records,"cached_files":cached_files,"cached_source":cached_source,"replay_source":replay_source,"expected_sha256":expected_sha256,"expected_bytes":input.bytes,"background_held":background_held,"next_while_pending":next_while_pending,"paused_while_pending":paused_while_pending,"seek_while_pending":seek_while_pending,"wave_original_markers":phase=="completion-wave"})).unwrap()).unwrap();
    write_phase(
        input,
        PhaseReport {
            phase: phase.into(),
            pid: std::process::id(),
            account_revalidated: true,
            cache_policy: "private-http".into(),
            first_source: if phase == "completion-restart" {
                "cached-file"
            } else {
                "remote-https"
            }
            .into(),
            replay_source: cached_source.then(|| "cached-file".into()),
            clear_kept_playing: phase == "completion-clear",
            default_limit_bytes: 2 * 1024 * 1024 * 1024,
        },
    );
}
