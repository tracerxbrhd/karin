// SPDX-License-Identifier: GPL-3.0-or-later
//! Original counted HTTP fixtures; no real library or service credentials.
use super::*;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    thread::{self, JoinHandle},
};

pub(crate) struct Fixture {
    stop: Arc<AtomicBool>,
    pub(crate) changed: Arc<AtomicBool>,
    pub(crate) membership: Arc<AtomicUsize>,
    pub(crate) batches: Arc<AtomicUsize>,
    pub(crate) hold_after: Arc<AtomicUsize>,
    pub(crate) held: Arc<AtomicBool>,
    pub(crate) released: Arc<AtomicBool>,
    pub(crate) service: MusicService,
    pub(crate) origin: String,
    pub(crate) offers: Arc<AtomicUsize>,
    worker: Option<JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn selector(position: usize) -> String {
    if position == 2_401 {
        "100042".into()
    } else {
        (100_000 + position).to_string()
    }
}
fn request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut header = Vec::new();
    let mut byte = [0];
    while !header.ends_with(b"\r\n\r\n") {
        assert_eq!(stream.read(&mut byte).unwrap(), 1);
        header.push(byte[0]);
        assert!(header.len() <= 16 * 1024);
    }
    let header = String::from_utf8(header).unwrap();
    assert!(
        header
            .to_ascii_lowercase()
            .contains("authorization: oauth synthetic-library-index-token")
    );
    assert!(!header.to_ascii_lowercase().contains("cookie:"));
    let length = header
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(length <= 16 * 1024);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    (header, body)
}
pub(crate) fn fixture(count: usize, malformed: bool) -> Fixture {
    make_fixture(count, malformed, false)
}
pub(crate) fn fixture_native(count: usize) -> Fixture {
    make_fixture(count, false, true)
}
fn make_fixture(count: usize, malformed: bool, native: bool) -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}/", listener.local_addr().unwrap());
    let service = if native {
        MusicService::for_native_input_test(&origin)
    } else {
        MusicService::for_test(&origin)
    };
    let stop = Arc::new(AtomicBool::new(false));
    let changed = Arc::new(AtomicBool::new(false));
    let membership = Arc::new(AtomicUsize::new(0));
    let batches = Arc::new(AtomicUsize::new(0));
    let hold_after = Arc::new(AtomicUsize::new(usize::MAX));
    let held = Arc::new(AtomicBool::new(false));
    let released = Arc::new(AtomicBool::new(false));
    let offers = Arc::new(AtomicUsize::new(0));
    let worker = {
        let stop = stop.clone();
        let changed = changed.clone();
        let membership = membership.clone();
        let batches = batches.clone();
        let hold_after = hold_after.clone();
        let held = held.clone();
        let released = released.clone();
        let offers = offers.clone();
        thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("synthetic index listener: {error}"),
                };
                let (header, body) = request(&mut stream);
                let path = header
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .split('?')
                    .next()
                    .unwrap();
                let reply = match path {
                    "/account/status" => json!({"result":{"account":{"uid":1234,"displayName":"Synthetic index account"}}}),
                    "/users/1234/dislikes/tracks" => json!({"result":{"library":{"uid":1234,"revision":1,"tracks":[]}}}),
                    "/users/1234/likes/tracks" | "/users/1234/playlists/7" => {
                        membership.fetch_add(1, Ordering::AcqRel);
                        let references = (0..count).map(|position| {
                            if malformed && position == 0 { json!({"id":"https://invalid.example/private"}) }
                            else {
                                let mut reference = json!({"id":selector(position),"title":{"unsupported":"display"},"artists":true,
                                    "timestamp":format!("2024-01-{:02}T00:00:00Z", position % 28 + 1)});
                                if !native { reference["track"] = json!({"title":false}); }
                                reference
                            }
                        }).collect::<Vec<_>>();
                        let revision = if changed.load(Ordering::Acquire) { 2 } else { 1 };
                        let manifest = json!({"uid":1234,"owner":{"uid":1234},"kind":7,"revision":revision,"trackCount":count,"tracks":references});
                        if path.ends_with("/likes/tracks") { json!({"result":{"library":manifest}}) }
                        else { json!({"result":manifest}) }
                    }
                    "/tracks" => {
                        batches.fetch_add(1, Ordering::AcqRel);
                        let form = Url::parse(&format!("http://127.0.0.1/?{}", String::from_utf8(body).unwrap())).unwrap();
                        let ids = form.query_pairs().filter(|(name, _)| name == "track-ids").map(|(_, value)| value.into_owned()).collect::<Vec<_>>();
                        assert!(!ids.is_empty() && ids.len() <= PAGE_SIZE);
                        assert_eq!(ids.iter().collect::<std::collections::BTreeSet<_>>().len(), ids.len());
                        if ids.iter().any(|id| id.parse::<usize>().is_ok_and(|id| id >= hold_after.load(Ordering::Acquire))) {
                            held.store(true, Ordering::Release);
                            let deadline = std::time::Instant::now() + Duration::from_secs(12);
                            while !released.load(Ordering::Acquire) && !stop.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
                                thread::sleep(Duration::from_millis(2));
                            }
                            assert!(released.load(Ordering::Acquire) || stop.load(Ordering::Acquire), "synthetic index response was not released");
                        }
                        json!({"result":ids.iter().rev().map(|id| {
                            if id == "100077" { Value::Null } else {
                                json!({"id":id,"title":format!("Original {id}"),"available":true,"artists":[{"id":11,"name":"Original artist"}],"albums":[{"id":7,"title":"Original collection"}]})
                            }
                        }).collect::<Vec<_>>()})
                    }
                    path if path.starts_with("/tracks/") && path.ends_with("/download-info") => {
                        offers.fetch_add(1, Ordering::AcqRel);
                        json!({"result":[]})
                    }
                    path if header.starts_with("GET ")
                        && path.strip_prefix("/tracks/")
                            .and_then(|tail| tail.strip_suffix("/similar"))
                            .is_some_and(|id| !id.is_empty() && id.len() <= 20
                                && id.bytes().all(|byte| byte.is_ascii_digit())) => {
                        let id = path.strip_prefix("/tracks/").unwrap().strip_suffix("/similar").unwrap();
                        json!({"result":{"track":{"id":id},"similarTracks":[]}})
                    }
                    _ => panic!("unexpected synthetic index route"),
                }.to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
            }
        })
    };
    Fixture {
        stop,
        changed,
        membership,
        batches,
        hold_after,
        held,
        released,
        service,
        origin,
        offers,
        worker: Some(worker),
    }
}
pub(crate) fn token() -> AccessToken {
    AccessToken::for_test("synthetic-library-index-token", Duration::from_secs(3600))
}

#[test]
fn optional_rfc3339_dates_are_validated_and_normalized_without_guessing_order() {
    assert_eq!(added_timestamp("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(added_timestamp("1970-01-01T03:00:00+03:00"), Some(0));
    assert_eq!(
        added_timestamp("2024-02-29T23:59:59.123456789Z"),
        Some(1_709_251_199_123)
    );
    for invalid in [
        "2023-02-29T00:00:00Z",
        "2024-01-01T-1:00:00Z",
        "2024-+1-01T00:00:00Z",
        "2024-01-01T24:00:00Z",
        "2024-01-01T00:00:00",
        "2024-01-01T00:00:00Z/private",
    ] {
        assert_eq!(added_timestamp(invalid), None);
    }
}

#[test]
fn full_collection_over_2000_uses_one_manifest_batched_canonical_metadata_and_exact_positions() {
    let server = fixture(2_403, false);
    let token = token();
    server.service.account(&token).unwrap();
    let snapshot = server
        .service
        .collection_snapshot(&token, &CollectionSource::Liked)
        .unwrap();
    assert_eq!(snapshot.len(), 2_403);
    let mut cursor = 0;
    let mut tracks = Vec::new();
    while cursor < snapshot.len() {
        let batch = server
            .service
            .collection_index_batch(&token, &snapshot, cursor)
            .unwrap();
        assert!(batch.next_position > cursor && batch.next_position <= cursor + 50);
        tracks.extend(batch.tracks);
        cursor = batch.next_position;
    }
    assert_eq!(server.membership.load(Ordering::Acquire), 1);
    assert_eq!(server.batches.load(Ordering::Acquire), 49);
    assert_eq!(tracks.len(), 2_402);
    assert!(!tracks.iter().any(|(position, _)| *position == 77));
    assert_eq!(tracks.last().unwrap().0, 2_402);
    let duplicate = tracks
        .iter()
        .filter(|(_, track)| track.id == "100042")
        .map(|(position, _)| *position)
        .collect::<Vec<_>>();
    assert_eq!(duplicate, [42, 2_401]);
    let refreshed = server
        .service
        .collection_snapshot(&token, &CollectionSource::Liked)
        .unwrap();
    assert!(snapshot.same_membership(&refreshed));
    server.changed.store(true, Ordering::Release);
    assert!(
        !snapshot.same_membership(
            &server
                .service
                .collection_snapshot(&token, &CollectionSource::Liked)
                .unwrap()
        )
    );
    assert!(!format!("{snapshot:?}").contains("100042"));
}

#[test]
fn playlist_snapshot_ignores_display_garbage_but_checks_ids_binding_and_size() {
    let server = fixture(2_101, false);
    let token = token();
    server.service.account(&token).unwrap();
    let source = CollectionSource::Playlist(PlaylistId {
        owner: "1234".into(),
        kind: "7".into(),
    });
    let snapshot = server.service.collection_snapshot(&token, &source).unwrap();
    let batch = server
        .service
        .collection_index_batch(&token, &snapshot, 2_100)
        .unwrap();
    assert_eq!(batch.tracks[0].0, 2_100);
    assert_eq!(batch.next_position, 2_101);
    let before = server.batches.load(Ordering::Acquire);
    server.service.clear_session();
    assert_eq!(
        server
            .service
            .collection_index_batch(&token, &snapshot, 0)
            .unwrap_err(),
        MusicError::Unauthenticated
    );
    assert_eq!(server.batches.load(Ordering::Acquire), before);
    let malformed = fixture(1, true);
    malformed.service.account(&token).unwrap();
    assert_eq!(
        malformed
            .service
            .collection_snapshot(&token, &CollectionSource::Liked)
            .unwrap_err(),
        MusicError::InvalidResponse
    );
    let oversized = fixture(MAX_INDEX_POSITIONS + 1, false);
    oversized.service.account(&token).unwrap();
    assert_eq!(
        oversized
            .service
            .collection_snapshot(&token, &CollectionSource::Liked)
            .unwrap_err(),
        MusicError::ResponseTooLarge
    );
}
