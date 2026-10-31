// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::{
    cache::{AccountScope, CachePolicy},
    music::{
        Artist,
        cache_codec::{self, MetadataRecord},
    },
};
use ring::rand::{SecureRandom, SystemRandom};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::PathBuf,
    sync::mpsc,
    thread::{self, JoinHandle},
};

const ACCOUNT: &str =
    r#"{"result":{"account":{"uid":1234,"displayName":"Synthetic cache owner"}}}"#;
const FIRST: &str = r#"{"result":[{"id":"42","title":"First synthetic title","available":true,"artists":[{"id":6,"name":"Synthetic artist"}],"albums":[{"id":7,"title":"Synthetic album"}]}]}"#;
const SECOND: &str = r#"{"result":[{"id":"42","title":"Second synthetic title","available":true,"artists":[],"albums":[]}]}"#;
const ALBUM: &str =
    r#"{"result":{"id":"7","title":"Synthetic album","artists":[],"volumes":[[]]}}"#;
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let mut bytes = [0u8; 16];
        SystemRandom::new().fill(&mut bytes).unwrap();
        let suffix: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let path = std::env::temp_dir().join(format!("karin-catalog-cache-{suffix}"));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn root(&self) -> PathBuf {
        self.0.join("persistent")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
enum Reply {
    Json(u16, &'static str),
    Disconnect,
    Delayed(&'static str, mpsc::SyncSender<()>, mpsc::Receiver<()>),
}
fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until =>
            {
                thread::sleep(Duration::from_millis(2))
            }
            Err(_) => panic!("expected bounded synthetic Music request"),
        }
    }
}
fn request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0u8];
    while !bytes.ends_with(b"\r\n\r\n") {
        assert_eq!(stream.read(&mut byte).unwrap(), 1);
        bytes.push(byte[0]);
        assert!(bytes.len() < 32 * 1024);
    }
    let headers = String::from_utf8(bytes).unwrap();
    let length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(length < 32 * 1024);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    headers.lines().next().unwrap().to_owned()
}
type Fixture = (
    MusicService,
    AccessToken,
    JoinHandle<Vec<String>>,
    TcpListener,
);
type Prepared = (
    Temp,
    Cache,
    CachedCatalog,
    MusicService,
    AccessToken,
    JoinHandle<Vec<String>>,
    TcpListener,
);
fn fixture(replies: Vec<Reply>) -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let service = MusicService::for_test(&format!("http://{}/", listener.local_addr().unwrap()));
    let retained = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        replies.into_iter().map(|reply|{let mut stream=accept(&listener);let first=request(&mut stream);let response=match reply{Reply::Json(status,body)=>Some((status,body)),Reply::Disconnect=>None,Reply::Delayed(body,entered,release)=>{entered.send(()).unwrap();release.recv_timeout(Duration::from_secs(4)).unwrap();Some((200,body))}};if let Some((status,body))=response{write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();}first}).collect()
    });
    let token = AccessToken::for_test("synthetic-cache-token", Duration::from_secs(60));
    (service, token, server, retained)
}
fn prepare(replies: Vec<Reply>, policy: CachePolicy) -> Prepared {
    let fixture = fixture(replies);
    fixture.0.account(&fixture.1).unwrap();
    let temp = Temp::new();
    let cache = Cache::new(temp.root(), policy).unwrap();
    cache.bind(Some(fixture.0.cache_scope(&fixture.1).unwrap()));
    let catalog = CachedCatalog::new(cache.clone());
    (
        temp, cache, catalog, fixture.0, fixture.1, fixture.2, fixture.3,
    )
}
fn complete(server: JoinHandle<Vec<String>>, listener: TcpListener) -> Vec<String> {
    let requests = server.join().unwrap();
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "no hidden extra request or N+1 lookup"
    );
    requests
}

#[test]
fn fresh_track_hit_avoids_http_and_forced_refresh_replaces_the_typed_record() {
    let (_temp, _cache, catalog, service, token, server, listener) = prepare(
        vec![
            Reply::Json(200, ACCOUNT),
            Reply::Json(200, FIRST),
            Reply::Json(200, SECOND),
        ],
        CachePolicy::default(),
    );
    let first = catalog.track(&service, &token, "42", false).unwrap();
    assert_eq!(first.provenance, Provenance::Network);
    assert_eq!(first.value.title, "First synthetic title");
    let hit = catalog.track(&service, &token, "42", false).unwrap();
    assert_eq!(hit.provenance, Provenance::FreshCache);
    assert!(hit.lease.as_ref().unwrap().is_current());
    assert_eq!(
        hit.value.artist_links,
        vec![Artist {
            id: "6".into(),
            name: "Synthetic artist".into(),
            cover: None
        }]
    );
    let fresh = catalog.track(&service, &token, "42", true).unwrap();
    assert_eq!(fresh.provenance, Provenance::Network);
    assert_eq!(fresh.value.title, "Second synthetic title");
    assert_eq!(
        catalog
            .track(&service, &token, "42", false)
            .unwrap()
            .value
            .title,
        "Second synthetic title"
    );
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 3);
    assert!(requests[1].starts_with("POST /tracks "));
    assert!(requests[2].starts_with("POST /tracks "));
}
#[test]
fn stale_transport_fallback_is_explicit_and_keeps_no_playback_resource() {
    let (_temp, _cache, catalog, service, token, server, listener) = prepare(
        vec![
            Reply::Json(200, ACCOUNT),
            Reply::Json(200, FIRST),
            Reply::Disconnect,
        ],
        CachePolicy {
            metadata_ttl: Duration::ZERO,
            ..CachePolicy::default()
        },
    );
    catalog.track(&service, &token, "42", false).unwrap();
    let saved = catalog.track(&service, &token, "42", false).unwrap();
    assert_eq!(saved.provenance, Provenance::StaleCache);
    assert_eq!(saved.value.title, "First synthetic title");
    assert!(saved.lease.as_ref().unwrap().is_current());
    assert_eq!(complete(server, listener).len(), 3);
}
#[test]
fn refusal_restriction_rate_limit_and_bad_schema_never_fall_back_to_cached_track() {
    for (status, body, expected) in [
        (401, "{}", MusicError::AccessDenied),
        (403, "{}", MusicError::Forbidden),
        (451, "{}", MusicError::Restricted),
        (429, "{}", MusicError::RateLimited),
        (500, "{}", MusicError::Http(500)),
        (200, r#"{"result":null}"#, MusicError::InvalidResponse),
    ] {
        let (_temp, cache, catalog, service, token, server, listener) = prepare(
            vec![
                Reply::Json(200, ACCOUNT),
                Reply::Json(200, FIRST),
                Reply::Json(status, body),
            ],
            CachePolicy::default(),
        );
        catalog.track(&service, &token, "42", false).unwrap();
        assert_eq!(
            catalog.track(&service, &token, "42", true).unwrap_err(),
            expected
        );
        let key = CacheKey::new(EntryKind::Track, "42", 0).unwrap();
        assert!(
            cache.get(&cache.lease().unwrap(), &key).unwrap().is_none(),
            "known refusal cannot leave a fresh hit masking it"
        );
        assert_eq!(complete(server, listener).len(), 3);
    }
}
#[test]
fn mutable_membership_always_contacts_server_and_network_result_wins() {
    const FIRST_LIST: &str =
        r#"{"result":[{"uid":1234,"kind":5,"title":"First playlist","trackCount":0}]}"#;
    const SECOND_LIST: &str =
        r#"{"result":[{"uid":1234,"kind":5,"title":"Updated playlist","trackCount":2}]}"#;
    let (_temp, _cache, catalog, service, token, server, listener) = prepare(
        vec![
            Reply::Json(200, ACCOUNT),
            Reply::Json(200, FIRST_LIST),
            Reply::Json(200, SECOND_LIST),
        ],
        CachePolicy::default(),
    );
    assert_eq!(
        catalog
            .playlists(&service, &token, 0, false)
            .unwrap()
            .value
            .items[0]
            .title,
        "First playlist"
    );
    let next = catalog.playlists(&service, &token, 0, false).unwrap();
    assert_eq!(next.provenance, Provenance::Network);
    assert_eq!(next.value.items[0].title, "Updated playlist");
    assert_eq!(complete(server, listener).len(), 3);
}
#[test]
fn owned_and_liked_playlist_snapshots_use_distinct_record_keys() {
    const OWNED: &str =
        r#"{"result":[{"uid":1234,"kind":5,"title":"Owned playlist","trackCount":0}]}"#;
    const LIKED: &str = r#"{"result":[{"playlist":{"uid":9876,"kind":9,"title":"Liked playlist","trackCount":0}}]}"#;
    let (_temp, _cache, catalog, service, token, server, listener) = prepare(
        vec![
            Reply::Json(200, ACCOUNT),
            Reply::Json(200, OWNED),
            Reply::Json(200, LIKED),
            Reply::Disconnect,
            Reply::Disconnect,
        ],
        CachePolicy::default(),
    );
    catalog.playlists(&service, &token, 0, false).unwrap();
    catalog.liked_playlists(&service, &token, 0, false).unwrap();
    let owned = catalog.playlists(&service, &token, 0, false).unwrap();
    let liked = catalog.liked_playlists(&service, &token, 0, false).unwrap();
    assert_eq!(owned.value.items[0].title, "Owned playlist");
    assert_eq!(liked.value.items[0].title, "Liked playlist");
    assert_eq!(owned.provenance, Provenance::StaleCache);
    assert_eq!(liked.provenance, Provenance::StaleCache);
    assert_eq!(complete(server, listener).len(), 5);
}
#[test]
fn cache_disabled_or_unavailable_preserves_good_network_data_without_plaintext_fallback() {
    let (temp, cache, catalog, service, token, server, listener) = prepare(
        vec![
            Reply::Json(200, ACCOUNT),
            Reply::Json(200, FIRST),
            Reply::Json(200, SECOND),
        ],
        CachePolicy {
            enabled: false,
            ..CachePolicy::default()
        },
    );
    assert!(
        catalog
            .track(&service, &token, "42", false)
            .unwrap()
            .cache_error
            .is_none()
    );
    assert_eq!(
        catalog
            .track(&service, &token, "42", false)
            .unwrap()
            .provenance,
        Provenance::Network
    );
    assert!(!temp.root().exists());
    assert!(cache.lease().is_none());
    assert_eq!(complete(server, listener).len(), 3);
    let (temp, _cache, catalog, service, token, server, listener) = prepare(
        vec![Reply::Json(200, ACCOUNT), Reply::Json(200, FIRST)],
        CachePolicy::default(),
    );
    fs::create_dir(temp.root()).unwrap();
    fs::set_permissions(temp.root(), fs::Permissions::from_mode(0o755)).unwrap();
    let result = catalog.track(&service, &token, "42", false).unwrap();
    assert_eq!(result.provenance, Provenance::Network);
    assert_eq!(result.cache_error, Some(CacheError::Unavailable));
    assert_eq!(result.value.title, "First synthetic title");
    assert_eq!(complete(server, listener).len(), 2);
}
#[test]
fn facade_never_rebinds_or_reads_another_accounts_current_cache() {
    let (_temp, cache, catalog, service, token, server, listener) = prepare(
        vec![Reply::Json(200, ACCOUNT), Reply::Json(200, FIRST)],
        CachePolicy::default(),
    );
    cache.bind(Some(AccountScope::from_account_id("9876").unwrap()));
    let wrong = cache.lease().unwrap();
    let key = CacheKey::new(EntryKind::Track, "42", 0).unwrap();
    let data = MetadataRecord::Track(Track {
        id: "42".into(),
        title: "Other account metadata".into(),
        artists: Vec::new(),
        artist_links: Vec::new(),
        album_id: None,
        album_title: None,
        duration_ms: None,
        cover: None,
        explicit: false,
        available: true,
    });
    cache
        .put(
            &wrong,
            &key,
            &cache_codec::encode(EntryKind::Track, &data).unwrap(),
        )
        .unwrap();
    let result = catalog.track(&service, &token, "42", false).unwrap();
    assert_eq!(result.provenance, Provenance::Network);
    assert_eq!(result.value.title, "First synthetic title");
    assert!(wrong.is_current());
    assert_eq!(
        cache_codec::decode(
            EntryKind::Track,
            &cache.get(&wrong, &key).unwrap().unwrap().bytes
        )
        .unwrap(),
        data
    );
    let another = AccessToken::for_test("synthetic-other-token", Duration::from_secs(60));
    assert_eq!(
        catalog.track(&service, &another, "42", false).unwrap_err(),
        MusicError::Unauthenticated
    );
    assert_eq!(complete(server, listener).len(), 2);
}
#[test]
fn clear_during_network_load_keeps_fresh_reply_but_prevents_repopulation() {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();
    let (_temp, cache, catalog, service, token, server, listener) = prepare(
        vec![
            Reply::Json(200, ACCOUNT),
            Reply::Delayed(FIRST, entered_tx, release_rx),
        ],
        CachePolicy::default(),
    );
    let worker = thread::spawn(move || catalog.track(&service, &token, "42", false));
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    cache.clear().unwrap();
    release_tx.send(()).unwrap();
    let result = worker.join().unwrap().unwrap();
    assert_eq!(result.provenance, Provenance::Network);
    assert!(result.lease.is_none());
    assert_eq!(cache.usage().unwrap().entries, 0);
    assert_eq!(complete(server, listener).len(), 2);
}
#[test]
fn logout_during_load_discards_data_and_never_writes_old_session_cache() {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();
    let (_temp, cache, catalog, service, token, server, listener) = prepare(
        vec![
            Reply::Json(200, ACCOUNT),
            Reply::Delayed(FIRST, entered_tx, release_rx),
        ],
        CachePolicy::default(),
    );
    let other = service.clone();
    let cancelled = token.clone();
    let worker = thread::spawn(move || catalog.track(&service, &token, "42", false));
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    cancelled.invalidate();
    other.clear_session();
    cache.bind(None);
    release_tx.send(()).unwrap();
    assert_eq!(
        worker.join().unwrap().unwrap_err(),
        MusicError::Unauthenticated
    );
    assert_eq!(cache.usage().unwrap().entries, 0);
    assert_eq!(complete(server, listener).len(), 2);
}
#[test]
fn concurrent_stable_misses_share_one_network_load_then_reuse_its_cache_record() {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();
    let (_temp, _cache, catalog, service, token, server, listener) = prepare(
        vec![
            Reply::Json(200, ACCOUNT),
            Reply::Delayed(ALBUM, entered_tx, release_rx),
        ],
        CachePolicy::default(),
    );
    let (second_catalog, second_service, second_token) =
        (catalog.clone(), service.clone(), token.clone());
    let registry = Arc::clone(&catalog.flights);
    let first = thread::spawn(move || catalog.album(&service, &token, "7", false));
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let second =
        thread::spawn(move || second_catalog.album(&second_service, &second_token, "7", false));
    let deadline = Instant::now() + Duration::from_secs(1);
    while registry.state.lock().unwrap().waiting == 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        registry.state.lock().unwrap().waiting,
        1,
        "second request actually waited for the active HTTP load"
    );
    release_tx.send(()).unwrap();
    let a = first.join().unwrap().unwrap();
    let b = second.join().unwrap().unwrap();
    assert_eq!(a.provenance, Provenance::Network);
    assert_eq!(b.provenance, Provenance::FreshCache);
    assert_eq!(a.value, b.value);
    assert_eq!(complete(server, listener).len(), 2);
}
