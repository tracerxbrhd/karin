// SPDX-License-Identifier: GPL-3.0-or-later
//! Original generated audio and explicit fake grants exercise the production
//! selection/fill/player boundary. These fixtures establish no service rights.

use super::*;
use crate::{
    audio_cache::{CachePolicy, MissingStorageContract, test_support::SyntheticAuthority},
    cache::AccountScope,
    playback::{PlaybackController, PlaybackOutput, PlaybackSnapshot, PlaybackState},
};
use gstreamer::{self as gst, prelude::*};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "karin-cache-bridge-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn media() -> Vec<u8> {
    static MEDIA: OnceLock<Vec<u8>> = OnceLock::new();
    MEDIA.get_or_init(|| {
        gst::init().unwrap();
        let directory = Directory::new();
        let path = directory.0.join("original.mp3");
        let pipeline = gst::parse::launch("audiotestsrc num-buffers=300 samplesperbuffer=441 volume=0.01 ! audioconvert ! audio/x-raw,rate=44100,channels=1 ! lamemp3enc target=bitrate bitrate=128 cbr=true ! xingmux ! filesink name=output").unwrap().downcast::<gst::Pipeline>().unwrap();
        pipeline.by_name("output").unwrap().set_property("location", path.to_str().unwrap());
        pipeline.set_state(gst::State::Playing).unwrap();
        let completed = pipeline.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(8), &[gst::MessageType::Eos, gst::MessageType::Error]);
        pipeline.set_state(gst::State::Null).unwrap();
        assert!(completed.is_some_and(|message| matches!(message.view(), gst::MessageView::Eos(_))));
        fs::read(path).unwrap()
    }).clone()
}

struct Http {
    origin: String,
    media_requests: Arc<AtomicUsize>,
    delivery_requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<()>>,
}
impl Http {
    fn new(body: Vec<u8>, status: u16, extra_headers: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        let media_requests = Arc::new(AtomicUsize::new(0));
        let delivery_requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let server = {
            let origin = origin.clone();
            let media_requests = media_requests.clone();
            let delivery_requests = delivery_requests.clone();
            let stop = stop.clone();
            thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(value) => value,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => panic!("synthetic HTTP accept failed"),
                    };
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut request = Vec::new();
                    while !request.ends_with(b"\r\n\r\n") {
                        let mut byte = [0];
                        stream.read_exact(&mut byte).unwrap();
                        request.push(byte[0]);
                        assert!(request.len() < 8192);
                    }
                    let request = String::from_utf8(request).unwrap();
                    let path = request.split_whitespace().nth(1).unwrap();
                    let lower = request.to_ascii_lowercase();
                    let (code, mime, headers, response) = if path == "/account/status" {
                        (200, "application/json", "", br#"{"result":{"account":{"uid":1234,"displayName":"Synthetic account"}}}"#.to_vec())
                    } else if path == "/tracks/123/download-info" {
                        delivery_requests.fetch_add(1, Ordering::AcqRel);
                        (200, "application/json", "", format!(r#"{{"result":[{{"codec":"mp3","preview":false,"downloadInfoUrl":"{origin}download-info/original"}}]}}"#).into_bytes())
                    } else if path == "/download-info/original" {
                        assert!(!lower.contains("authorization:") && !lower.contains("cookie:"));
                        (200, "application/xml", "", b"<download-info><host>s268myt.storage.yandex.net</host><path>/original/track.mp3</path><ts>000592c073d58600</ts><s>synthetic-value</s></download-info>".to_vec())
                    } else if path.starts_with("/get-mp3/") {
                        media_requests.fetch_add(1, Ordering::AcqRel);
                        assert!(!lower.contains("authorization:") && !lower.contains("cookie:"));
                        (status, "audio/mpeg", extra_headers, body.clone())
                    } else {
                        (404, "text/plain", "", Vec::new())
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {code} Fixture\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n",
                        response.len()
                    );
                    let _ = stream.write_all(&response);
                }
            })
        };
        Self {
            origin,
            media_requests,
            delivery_requests,
            stop,
            server: Some(server),
        }
    }
    fn resolver(&self) -> StreamResolver {
        StreamResolver::for_test(&self.origin, &self.origin, &self.origin)
    }
    fn remote(&self) -> RemotePlaybackSource {
        RemotePlaybackSource::from_test_resolver(
            &format!("{}get-mp3/original", self.origin),
            Instant::now() + Duration::from_secs(60),
        )
        .unwrap()
    }
    fn select(&self, cache: &AudioCache, lease: &Lease) -> Selection {
        select(
            self.resolver(),
            &token(),
            &DeliveryCancellation::new(),
            self.remote(),
            "123",
            Some((cache.clone(), lease.clone())),
        )
    }
    fn requested(&self) -> usize {
        self.media_requests.load(Ordering::Acquire)
    }
}
impl Drop for Http {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.server.take().unwrap().join().unwrap();
    }
}
fn token() -> AccessToken {
    AccessToken::for_test("synthetic-cache-token", Duration::from_secs(300))
}
fn scope() -> AccountScope {
    AccountScope::from_account_id("1234").unwrap()
}
fn key() -> ResourceKey {
    ResourceKey::track("123").unwrap()
}
fn store(directory: &Directory) -> (AudioCache, Lease, Arc<SyntheticAuthority>) {
    let authority = Arc::new(SyntheticAuthority::default());
    authority.allow(&scope(), &key());
    let cache = AudioCache::open(
        directory.0.join("audio"),
        authority.clone(),
        CachePolicy::default(),
    )
    .unwrap();
    let lease = cache.bind(Some(scope())).unwrap().unwrap();
    (cache, lease, authority)
}
fn wait(
    player: &PlaybackController,
    predicate: impl Fn(&PlaybackSnapshot) -> bool,
) -> PlaybackSnapshot {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let snapshot = player.snapshot();
        if predicate(&snapshot) {
            return snapshot;
        }
        assert_ne!(
            snapshot.state,
            PlaybackState::Error,
            "fixed native failure: {:?}",
            snapshot.last_error
        );
        assert!(
            Instant::now() < deadline,
            "native state timed out: {:?}",
            snapshot.state
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn production_resolution_fill_hit_restart_and_current_session_admission() {
    let directory = Directory::new();
    let http = Http::new(media(), 200, "");
    let (cache, lease, authority) = store(&directory);
    let service = MusicService::for_test(&http.origin);
    let token = token();
    service.account(&token).unwrap();
    let first = resolve(
        http.resolver(),
        &service,
        &token,
        "123",
        &DeliveryCancellation::new(),
        Some((cache.clone(), lease.clone())),
    )
    .unwrap();
    assert!(!first.source.is_cached());
    assert_eq!(http.requested(), 0);
    first.fill.unwrap().run().unwrap();
    assert_eq!(http.requested(), 1);
    assert_eq!(cache.stats().unwrap().entries, 1);
    let second = resolve(
        http.resolver(),
        &service,
        &token,
        "123",
        &DeliveryCancellation::new(),
        Some((cache.clone(), lease)),
    )
    .unwrap();
    assert!(second.source.is_cached() && second.fill.is_none());
    assert_eq!(
        http.delivery_requests.load(Ordering::Acquire),
        2,
        "fresh streaming admission on each hit"
    );
    assert_eq!(http.requested(), 1);
    drop(second);
    drop(cache);
    let reopened = AudioCache::open(
        directory.0.join("audio"),
        authority.clone(),
        CachePolicy::default(),
    )
    .unwrap();
    let lease = reopened.bind(Some(scope())).unwrap().unwrap();
    assert!(http.select(&reopened, &lease).source.is_cached());
    let other = reopened
        .bind(Some(AccountScope::from_account_id("9999").unwrap()))
        .unwrap()
        .unwrap();
    let selection = resolve(
        http.resolver(),
        &service,
        &token,
        "123",
        &DeliveryCancellation::new(),
        Some((reopened.clone(), other)),
    )
    .unwrap();
    assert!(!selection.source.is_cached() && selection.fill.is_none());
    assert!(
        resolve(
            http.resolver(),
            &service,
            &AccessToken::for_test("expired", Duration::ZERO),
            "123",
            &DeliveryCancellation::new(),
            None
        )
        .is_err()
    );
    assert_eq!(http.requested(), 1);
}

#[test]
fn missing_contract_has_no_media_request_or_storage_write() {
    let directory = Directory::new();
    let http = Http::new(media(), 200, "");
    let cache = AudioCache::open(
        directory.0.join("audio"),
        Arc::new(MissingStorageContract),
        CachePolicy::default(),
    )
    .unwrap();
    let lease = cache.bind(Some(scope())).unwrap().unwrap();
    let selection = http.select(&cache, &lease);
    assert!(!selection.source.is_cached() && selection.fill.is_none());
    assert_eq!(http.requested(), 0);
    assert_eq!(cache.stats().unwrap().entries, 0);
}

#[test]
fn pending_hit_is_fenced_at_final_admission_by_clear_and_revocation() {
    for revoked in [false, true] {
        let directory = Directory::new();
        let http = Http::new(media(), 200, "");
        let (cache, lease, authority) = store(&directory);
        http.select(&cache, &lease).fill.unwrap().run().unwrap();
        let selected = http.select(&cache, &lease);
        assert!(selected.source.is_cached());
        if revoked {
            authority.deny(&scope(), &key());
        } else {
            cache.clear().unwrap();
        }
        assert!(!selected.source.admit_cache_or_online().is_cached());
        assert_eq!(http.requested(), 1);
    }
}

#[test]
fn cancelled_or_revoked_fill_does_not_fetch_or_publish() {
    for cancelled in [false, true] {
        let directory = Directory::new();
        let http = Http::new(media(), 200, "");
        let (cache, lease, authority) = store(&directory);
        let fill = http.select(&cache, &lease).fill.unwrap();
        if cancelled {
            fill.cancellation().cancel();
        } else {
            authority.deny(&scope(), &key());
        }
        assert!(fill.run().is_err());
        assert_eq!(http.requested(), 0);
        assert_eq!(cache.stats().unwrap().entries, 0);
    }
}

#[test]
fn rejected_http_copies_leave_online_source_and_no_record() {
    for (status, headers) in [
        (403, ""),
        (302, "Location: /get-mp3/other\r\n"),
        (200, "Cache-Control: private, no-store\r\n"),
        (200, "Content-Range: bytes 0-5/20\r\n"),
        (200, "Content-Encoding: identity\r\n"),
        (200, "Content-Type: text/html\r\n"),
    ] {
        let directory = Directory::new();
        let http = Http::new(media(), status, headers);
        let (cache, lease, _) = store(&directory);
        let selection = http.select(&cache, &lease);
        assert!(!selection.source.is_cached());
        assert_eq!(selection.fill.unwrap().run(), Err(CacheError::Unavailable));
        assert_eq!(http.requested(), 1, "no redirects or retry");
        assert_eq!(cache.stats().unwrap().entries, 0);
    }
}

#[test]
fn cached_mp3_uses_existing_player_and_survives_active_clear() {
    let directory = Directory::new();
    let http = Http::new(media(), 200, "");
    let (cache, lease, _) = store(&directory);
    http.select(&cache, &lease).fill.unwrap().run().unwrap();
    let selected = http.select(&cache, &lease);
    assert!(selected.source.is_cached());
    assert!(!format!("{:?}", selected.source).contains("original"));
    let player = PlaybackController::new(PlaybackOutput::Fakesink).unwrap();
    player
        .prepare(selected.source.admit_cache_or_online())
        .unwrap();
    wait(&player, |s| s.state == PlaybackState::Ready);
    player.play().unwrap();
    wait(&player, |s| {
        s.state == PlaybackState::Playing && s.playing_time > Duration::from_millis(40)
    });
    cache.clear().unwrap();
    assert_eq!(cache.stats().unwrap().bytes, 0);
    player.pause().unwrap();
    wait(&player, |s| s.state == PlaybackState::Paused);
    player.set_volume(0.2).unwrap();
    wait(&player, |s| (s.volume - 0.2).abs() < 0.001);
    player.seek(Duration::from_secs(1)).unwrap();
    wait(&player, |s| {
        s.position.is_some_and(|p| p >= Duration::from_millis(900))
    });
    player.resume().unwrap();
    wait(&player, |s| s.state == PlaybackState::Ended);
    assert_eq!(
        http.requested(),
        1,
        "active pinned inode, no remote playback"
    );
    player.stop().unwrap();
    wait(&player, |s| s.state == PlaybackState::Stopped);
    player.shutdown();
    wait(&player, |s| !s.worker_running);
}

#[test]
fn pending_native_admission_and_bad_decoder_fall_back_once() {
    for clear_before_worker in [false, true] {
        let directory = Directory::new();
        let http = Http::new(media(), 200, "");
        let (cache, lease, _) = store(&directory);
        let request = cache.begin(&lease, key()).unwrap();
        // A complete but unsupported original body exercises the native guard;
        // no remote manifest URL is permitted to escape through cached bytes.
        cache
            .put_reader(
                request,
                b"#EXTM3U\nhttp://127.0.0.1:1/forbidden\n".as_slice(),
            )
            .unwrap();
        let selected = http.select(&cache, &lease).source.admit_cache_or_online();
        assert!(selected.is_cached());
        if clear_before_worker {
            cache.clear().unwrap();
        }
        let player = PlaybackController::new(PlaybackOutput::Fakesink).unwrap();
        player.prepare(selected).unwrap();
        wait(&player, |s| s.state == PlaybackState::Ready);
        player.play().unwrap();
        wait(&player, |s| s.state == PlaybackState::Playing);
        assert_eq!(http.requested(), 1);
        player.shutdown();
        wait(&player, |s| !s.worker_running);
    }
}

#[test]
fn changed_disk_bytes_and_expired_remote_cannot_be_used_as_a_hit_or_fill() {
    let directory = Directory::new();
    let http = Http::new(media(), 200, "");
    let (cache, lease, _) = store(&directory);
    http.select(&cache, &lease).fill.unwrap().run().unwrap();
    fn corrupt(path: &std::path::Path) -> bool {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if corrupt(&path) {
                    return true;
                }
            } else if path.file_name().is_some_and(|name| name == "media.bin") {
                fs::OpenOptions::new()
                    .write(true)
                    .open(path)
                    .unwrap()
                    .write_all(b"broken")
                    .unwrap();
                return true;
            }
        }
        false
    }
    assert!(corrupt(&directory.0.join("audio")));
    assert!(!http.select(&cache, &lease).source.is_cached());
    cache.clear().unwrap();
    let lease = cache.lease().unwrap();
    let expired = RemotePlaybackSource::from_test_resolver(
        &format!("{}get-mp3/original", http.origin),
        Instant::now() + Duration::from_millis(40),
    )
    .unwrap();
    thread::sleep(Duration::from_millis(60));
    let selected = select(
        http.resolver(),
        &token(),
        &DeliveryCancellation::new(),
        expired,
        "123",
        Some((cache.clone(), lease)),
    );
    assert_eq!(selected.fill.unwrap().run(), Err(CacheError::Cancelled));
    assert_eq!(http.requested(), 1);
    assert_eq!(cache.stats().unwrap().entries, 0);
}
