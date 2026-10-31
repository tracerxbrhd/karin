// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic loopback Music protocol fixtures. No real account, OAuth or media.

use super::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread::{self, JoinHandle},
    time::Instant,
};

const TOKEN: &str = "synthetic-private-access-token";
const ACCOUNT: &str = r#"{"result":{"account":{"uid":1234,"displayName":"synthetic-private-person","login":"synthetic-private-login"}},"invocationInfo":{"requestId":"synthetic-private-request"}}"#;
const TRACK: &str = r#"{"id":"42","title":"synthetic-private-title","available":true,"durationMs":123000,"artists":[{"id":11,"name":"synthetic-private-artist"}],"albums":[{"id":7,"title":"synthetic-private-album"}]}"#;

fn token() -> AccessToken {
    AccessToken::for_test(TOKEN, Duration::from_secs(3600))
}
fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut headers = Vec::new();
    let mut byte = [0];
    while !headers.ends_with(b"\r\n\r\n") {
        assert_eq!(stream.read(&mut byte).unwrap(), 1);
        headers.push(byte[0]);
        assert!(headers.len() < 32 * 1024);
    }
    let text = String::from_utf8(headers.clone()).unwrap();
    let len = text
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(len < 32 * 1024);
    let mut body = vec![0; len];
    stream.read_exact(&mut body).unwrap();
    headers.extend_from_slice(&body);
    String::from_utf8(headers).unwrap()
}
fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(5))
            }
            Err(error) => panic!("expected local fixture request: {error}"),
        }
    }
}
fn fixture(responses: Vec<(u16, String)>) -> (MusicService, JoinHandle<Vec<String>>, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let retained = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        responses.into_iter().map(|(status,body)| { let mut stream=accept(&listener); let request=read_request(&mut stream); write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap(); request }).collect()
    });
    let service = MusicService {
        client: Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_millis(500))
            .timeout(Duration::from_secs(2))
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .unwrap(),
        origin: Url::parse(&format!("http://{address}/")).unwrap(),
        account: Arc::new(Mutex::new(AccountCache::default())),
    };
    (service, server, retained)
}
fn seed_search_account(service: &MusicService) {
    service.account.lock().unwrap().validated =
        Some((MusicService::token_key(&token()), "1234".into()));
}
fn search_fixture(
    responses: Vec<(u16, String)>,
) -> (MusicService, JoinHandle<Vec<String>>, TcpListener) {
    let fixture = fixture(responses);
    seed_search_account(&fixture.0);
    fixture
}
fn complete(server: JoinHandle<Vec<String>>, listener: TcpListener) -> Vec<String> {
    let requests = server.join().unwrap();
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "no retry or fallback"
    );
    for request in &requests {
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains(&format!("authorization: oauth {TOKEN}\r\n")));
        assert!(!lower.contains("x-yandex-music-client"));
        assert!(!lower.contains("client_secret"));
        assert!(!request.lines().next().unwrap().contains(TOKEN));
    }
    requests
}
fn assert_redacted(value: impl fmt::Debug) {
    let visible = format!("{value:?}");
    for private in [
        TOKEN,
        "1234",
        "synthetic-private-person",
        "synthetic-private-login",
        "synthetic-private-title",
        "synthetic-private-artist",
        "synthetic-private-album",
        "synthetic-private-playlist",
        "synthetic-private-request",
    ] {
        assert!(!visible.contains(private));
    }
}

#[test]
fn real_endpoint_contracts_map_to_typed_models_without_transport_identity() {
    let compact = r#"{"id":"42","albumId":7}"#;
    let expanded = format!("{{\"result\":[{TRACK}]}}");
    let (service,server,listener)=search_fixture(vec![
        (200,ACCOUNT.into()),
        (200,format!("{{\"result\":{{\"library\":{{\"tracks\":[{compact}]}}}}}}")),(200,expanded.clone()),
        (200,r#"{"result":[{"owner":{"uid":1234},"kind":8,"title":"synthetic-private-playlist","trackCount":1}]}"#.into()),
        (200,format!("{{\"result\":{{\"tracks\":[{compact}]}}}}")),(200,expanded.clone()),
        (200,format!("{{\"result\":{{\"text\":\"query\",\"tracks\":{{\"total\":2,\"perPage\":1,\"results\":[{TRACK}]}}}}}}")),
        (200,expanded.clone()),
        (200,r#"{"result":{"text":"query","albums":{"total":1,"perPage":20,"results":[{"id":7,"title":"synthetic-private-album","artists":[{"id":11,"name":"synthetic-private-artist"}]}]}}}"#.into()),
        (200,r#"{"result":{"text":"query","artists":{"total":1,"perPage":20,"results":[{"id":11,"name":"synthetic-private-artist"}]}}}"#.into()),
        (200,expanded),
        (200,format!("{{\"result\":{{\"id\":7,\"title\":\"synthetic-private-album\",\"volumes\":[[{TRACK}]]}}}}")),
    ]);
    let token = token();
    let account = service.account(&token).unwrap();
    assert_eq!(account.display_name, "synthetic-private-person");
    assert_redacted(&account);
    let liked = service.liked_tracks(&token, 0).unwrap();
    assert_eq!(liked.items.len(), 1);
    assert_eq!(liked.items[0].id, "42");
    assert!(!liked.has_more);
    assert_redacted(&liked);
    assert_redacted(&liked.items[0]);
    let playlists = service.playlists(&token, 0).unwrap();
    assert_eq!(playlists.items[0].track_count, 1);
    assert_redacted(&playlists.items[0]);
    let tracks = service
        .playlist_tracks(&token, &playlists.items[0].id, 0)
        .unwrap();
    assert_eq!(tracks.items[0].album_id.as_deref(), Some("7"));
    let search = service
        .search(&token, "query & safely encoded", SearchKind::Tracks, 0)
        .unwrap();
    assert!(search.has_more);
    assert_eq!(search.tracks[0].duration_ms, Some(123000));
    assert_redacted(&search);
    let albums = service
        .search(&token, "query", SearchKind::Albums, 0)
        .unwrap();
    assert_eq!(albums.albums[0].id, "7");
    assert!(!albums.has_more);
    let artists = service
        .search(&token, "query", SearchKind::Artists, 0)
        .unwrap();
    assert_eq!(artists.artists[0].id, "11");
    let track = service.track(&token, "42:7").unwrap();
    assert!(track.available);
    assert_eq!(track.artists, vec!["synthetic-private-artist"]);
    let album = service.album(&token, "7").unwrap();
    assert_eq!(album.tracks.len(), 1);
    assert_redacted(&album);
    let requests = complete(server, listener);
    assert!(requests[0].starts_with("GET /account/status HTTP/1.1"));
    assert!(
        requests[1]
            .starts_with("GET /users/1234/likes/tracks?if-modified-since-revision=0 HTTP/1.1")
    );
    assert!(requests[2].starts_with("POST /tracks HTTP/1.1"));
    assert!(requests[2].ends_with("track-ids=42%3A7"));
    assert!(requests[3].starts_with("GET /users/1234/playlists/list HTTP/1.1"));
    assert!(requests[4].starts_with("GET /users/1234/playlists/8?richTracks=true HTTP/1.1"));
    assert!(requests[6].starts_with("GET /search?text=query+%26+safely+encoded&type=track&page=0&nocorrect=true&playlist-in-best=false HTTP/1.1"));
    assert!(requests[7].starts_with("POST /tracks HTTP/1.1"));
    assert!(requests[11].starts_with("GET /albums/7/with-tracks HTTP/1.1"));
    assert_redacted(service);
}

#[test]
fn validated_account_cache_is_token_bound_and_logout_expiry_stop_before_transport() {
    let (service, server, listener) = fixture(vec![
        (200, ACCOUNT.into()),
        (403, "synthetic-private-person".into()),
        (200, ACCOUNT.into()),
    ]);
    let token = token();
    assert_eq!(
        service.liked_tracks(&token, 0),
        Err(MusicError::Unauthenticated)
    );
    service.account(&token).unwrap();
    let other = AccessToken::for_test("another-synthetic-token", Duration::from_secs(20));
    assert_eq!(
        service.playlists(&other, 0),
        Err(MusicError::Unauthenticated)
    );
    assert_eq!(service.account(&token), Err(MusicError::Forbidden));
    assert_eq!(
        service.playlists(&token, 0),
        Err(MusicError::Unauthenticated)
    );
    service.account(&token).unwrap();
    service.clear_session();
    assert_eq!(
        service.playlists(&token, 0),
        Err(MusicError::Unauthenticated)
    );
    let expired = AccessToken::for_test(TOKEN, Duration::ZERO);
    assert_eq!(service.account(&expired), Err(MusicError::Unauthenticated));
    assert_eq!(complete(server, listener).len(), 3);
}

#[test]
fn delayed_validation_cannot_restore_account_after_logout() {
    use std::sync::mpsc;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let service = MusicService::for_test(&format!("http://{}/", listener.local_addr().unwrap()));
    let retained = listener.try_clone().unwrap();
    let (received_tx, received_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        assert!(read_request(&mut stream).starts_with("GET /account/status HTTP/1.1"));
        received_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{ACCOUNT}",
            ACCOUNT.len()
        )
        .unwrap();
    });
    let worker_service = service.clone();
    let worker = thread::spawn(move || worker_service.account(&token()));
    received_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    service.clear_session();
    release_tx.send(()).unwrap();
    assert_eq!(worker.join().unwrap(), Err(MusicError::Unauthenticated));
    server.join().unwrap();
    assert_eq!(
        service.account_id(&token()),
        Err(MusicError::Unauthenticated)
    );
    retained.set_nonblocking(true).unwrap();
    assert_eq!(
        retained.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn older_validation_cannot_overwrite_a_newer_token_account() {
    use std::sync::mpsc;
    const OTHER_ACCOUNT: &str =
        r#"{"result":{"account":{"uid":9876,"displayName":"second-synthetic-private-person"}}}"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let service = MusicService::for_test(&format!("http://{}/", listener.local_addr().unwrap()));
    let retained = listener.try_clone().unwrap();
    let (received_tx, received_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut old_stream = accept(&listener);
        assert!(
            read_request(&mut old_stream).contains(&format!("authorization: OAuth {TOKEN}\r\n"))
        );
        received_tx.send(()).unwrap();
        let mut new_stream = accept(&listener);
        assert!(
            read_request(&mut new_stream)
                .contains("authorization: OAuth second-synthetic-token\r\n")
        );
        write!(
            new_stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{OTHER_ACCOUNT}",
            OTHER_ACCOUNT.len()
        )
        .unwrap();
        drop(new_stream);
        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        write!(
            old_stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{ACCOUNT}",
            ACCOUNT.len()
        )
        .unwrap();
    });
    let old_service = service.clone();
    let old_worker = thread::spawn(move || old_service.account(&token()));
    received_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let new_token = AccessToken::for_test("second-synthetic-token", Duration::from_secs(3600));
    let current = service.account(&new_token).unwrap();
    assert_eq!(current.display_name, "second-synthetic-private-person");
    release_tx.send(()).unwrap();
    assert_eq!(old_worker.join().unwrap(), Err(MusicError::Unauthenticated));
    server.join().unwrap();
    assert_eq!(service.account_id(&new_token).unwrap(), "9876");
    assert_eq!(
        service.account_id(&token()),
        Err(MusicError::Unauthenticated)
    );
    retained.set_nonblocking(true).unwrap();
    assert_eq!(
        retained.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn queued_invalidated_token_cannot_clear_another_validated_session() {
    let (service, server, listener) = fixture(vec![
        (200, ACCOUNT.into()),
        (200, r#"{"result":[]}"#.into()),
    ]);
    let current = token();
    service.account(&current).unwrap();
    let old = AccessToken::for_test("old-synthetic-private-token", Duration::from_secs(3600));
    let queued = old.clone();
    old.invalidate();
    assert_eq!(service.account(&queued), Err(MusicError::Unauthenticated));
    let playlists = service.playlists(&current, 0).unwrap();
    assert!(playlists.items.is_empty());
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 2);
    assert!(requests[1].starts_with("GET /users/1234/playlists/list HTTP/1.1"));
}

#[test]
fn duplicate_playlist_entries_survive_a_deduplicated_track_response() {
    let (service, server, listener) = fixture(vec![
        (200, ACCOUNT.into()),
        (
            200,
            r#"{"result":{"tracks":[{"id":42,"albumId":7},{"id":42,"albumId":7}]}}"#.into(),
        ),
        (200, format!("{{\"result\":[{TRACK}]}}")),
    ]);
    let token = token();
    service.account(&token).unwrap();
    let page = service
        .playlist_tracks(
            &token,
            &PlaylistId {
                owner: "1234".into(),
                kind: "8".into(),
            },
            0,
        )
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0], page.items[1]);
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 3);
    assert!(requests[2].ends_with("track-ids=42%3A7"));
    assert_eq!(requests[2].matches("track-ids=").count(), 1);
}

#[test]
fn service_error_categories_and_envelopes_fail_closed_without_private_debug() {
    for (status, body, error) in [
        (401, "synthetic-private-person", MusicError::AccessDenied),
        (
            403,
            r#"{"error":"missing-required-scopes","message":"synthetic-private-person"}"#,
            MusicError::MissingScopes,
        ),
        (
            403,
            r#"{"error":{"name":"missing-required-scopes"}}"#,
            MusicError::MissingScopes,
        ),
        (
            403,
            r#"{"error":"missing-required-scopes","error":"synthetic-private-person"}"#,
            MusicError::Forbidden,
        ),
        (
            403,
            r#"{"error":{"message":"missing-required-scopes"}}"#,
            MusicError::Forbidden,
        ),
        (
            403,
            r#"{"error":"API access restricted"}"#,
            MusicError::Forbidden,
        ),
        (
            403,
            "<html>synthetic-private-person</html>",
            MusicError::Forbidden,
        ),
        (429, "synthetic-private-person", MusicError::RateLimited),
        (451, "synthetic-private-person", MusicError::Restricted),
        (500, "synthetic-private-person", MusicError::Http(500)),
        (
            200,
            "not JSON synthetic-private-person",
            MusicError::InvalidResponse,
        ),
        (
            200,
            r#"{"result":{},"error":"synthetic-private-person"}"#,
            MusicError::InvalidResponse,
        ),
        (
            200,
            r#"{"result":{"account":{"uid":1234}},"result":{"account":{"uid":1234}}}"#,
            MusicError::InvalidResponse,
        ),
        (
            200,
            r#"{"result":{"account":{"uid":0}}}"#,
            MusicError::Unauthenticated,
        ),
    ] {
        let (service, server, listener) = fixture(vec![(status, body.into())]);
        let got = service.account(&token()).unwrap_err();
        assert_eq!(got, error);
        assert_redacted(got);
        assert_eq!(complete(server, listener).len(), 1);
    }
}

#[test]
fn bounded_local_library_pages_and_provider_search_pages_do_not_invent_paging() {
    let references = (1..=51)
        .map(|id| json!({"id":id,"albumId":7}))
        .collect::<Vec<_>>();
    let expanded = (1..=50)
        .rev()
        .map(|id| json!({"id":id,"title":format!("track{id}")}))
        .collect::<Vec<_>>();
    let (service, server, listener) = search_fixture(vec![
        (200, ACCOUNT.into()),
        (
            200,
            json!({"result":{"library":{"tracks":references}}}).to_string(),
        ),
        (200, json!({"result":expanded}).to_string()),
        (
            200,
            json!({"result":{"library":{"tracks":[{"id":1}]}}}).to_string(),
        ),
        (
            200,
            r#"{"result":{"text":"query","tracks":{"total":3,"perPage":1,"results":[{"id":42}]}}}"#
                .into(),
        ),
        (200, r#"{"result":[{"id":42}]}"#.into()),
        (200, r#"{"result":{"text":"query"}}"#.into()),
    ]);
    let token = token();
    service.account(&token).unwrap();
    let page = service.liked_tracks(&token, 0).unwrap();
    assert!(page.has_more);
    assert_eq!(page.items.len(), 50);
    assert_eq!(page.items[0].id, "1");
    assert_eq!(page.items[49].id, "50");
    let empty = service.liked_tracks(&token, 1).unwrap();
    assert!(empty.items.is_empty());
    assert!(!empty.has_more);
    assert!(
        service
            .search(&token, "query", SearchKind::Tracks, 1)
            .unwrap()
            .has_more
    );
    let empty = service
        .search(&token, "query", SearchKind::Tracks, 0)
        .unwrap();
    assert!(empty.tracks.is_empty());
    assert!(!empty.has_more);
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 7);
    assert_eq!(requests[2].split("track-ids=").count() - 1, 50);
    assert!(!requests[1].contains("page="));
    assert!(requests[4].contains("&page=1&"));
}

#[test]
fn invalid_inputs_missing_records_and_schema_drift_never_create_release_content() {
    let (service, server, listener) = search_fixture(vec![
        (200, r#"{"result":[]}"#.into()),
        (200, r#"{"result":[{"id":43}]}"#.into()),
        (200, r#"{"result":{"id":7}}"#.into()),
        (200, r#"{"result":{}}"#.into()),
    ]);
    let token = token();
    for invalid in [
        "",
        "../account/status",
        "private\nidentifier",
        "https://evil.invalid/",
    ] {
        assert_eq!(
            service.track(&token, invalid),
            Err(MusicError::InvalidInput)
        );
    }
    assert_eq!(
        service.search(&token, "\0private", SearchKind::Tracks, 0),
        Err(MusicError::InvalidInput)
    );
    assert_eq!(
        service.search(&token, "query", SearchKind::Tracks, 10001),
        Err(MusicError::InvalidInput)
    );
    assert_eq!(
        service.track(&token, "42"),
        Err(MusicError::InvalidResponse)
    );
    assert_eq!(
        service.track(&token, "42"),
        Err(MusicError::InvalidResponse)
    );
    assert_eq!(service.album(&token, "7"), Err(MusicError::InvalidResponse));
    assert_eq!(
        service.search(&token, "query", SearchKind::Tracks, 0),
        Err(search_contract(SearchParseStage::Section, 200))
    );
    assert_eq!(complete(server, listener).len(), 4);
}

fn wire_fixture(wire: String, delay: Duration) -> Result<Account, MusicError> {
    wire_request_fixture(wire, delay, |service| service.account(&token()))
}
fn wire_request_fixture<T>(
    wire: String,
    delay: Duration,
    operation: impl FnOnce(&MusicService) -> Result<T, MusicError>,
) -> Result<T, MusicError> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        read_request(&mut stream);
        let (headers, body) = wire.split_once("\r\n\r\n").unwrap();
        write!(stream, "{headers}\r\n\r\n").unwrap();
        thread::sleep(delay);
        if let Err(error) = stream.write_all(body.as_bytes()) {
            assert!(matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ));
        }
    });
    let service = MusicService {
        client: Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(250))
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .unwrap(),
        origin: Url::parse(&format!("http://{address}/")).unwrap(),
        account: Arc::new(Mutex::new(AccountCache::default())),
    };
    seed_search_account(&service);
    let result = operation(&service);
    server.join().unwrap();
    result
}

#[test]
fn bounded_bodies_deadlines_and_failed_reads_never_expose_response_material() {
    for (wire,delay,error) in [
        (format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",MAX_BODY+1),Duration::ZERO,MusicError::ResponseTooLarge),
        (format!("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}","x".repeat(MAX_BODY+1)),Duration::ZERO,MusicError::ResponseTooLarge),
        ("HTTP/1.1 200 OK\r\nContent-Length: 1024\r\nConnection: close\r\n\r\nsynthetic-private-person".into(),Duration::ZERO,MusicError::Network),
        (format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{ACCOUNT}",ACCOUNT.len()),Duration::from_millis(700),MusicError::Timeout),
        (format!("HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",MAX_BODY+1),Duration::ZERO,MusicError::Forbidden),
        ("HTTP/1.1 403 Forbidden\r\nContent-Length: 1024\r\nConnection: close\r\n\r\nsynthetic-private-person".into(),Duration::ZERO,MusicError::Forbidden),
    ] { assert_eq!(wire_fixture(wire,delay),Err(error));assert_redacted(error); }
}

#[test]
fn real_location_header_is_not_followed_and_authorization_is_not_forwarded() {
    let alternate = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = alternate.local_addr().unwrap();
    assert_eq!(
        wire_fixture(
            format!(
                "HTTP/1.1 302 Found\r\nLocation: http://{address}/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ),
            Duration::ZERO
        ),
        Err(MusicError::Http(302))
    );
    alternate.set_nonblocking(true).unwrap();
    assert_eq!(
        alternate.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn search_accepts_provider_sections_without_echo_and_credits_without_artist_ids() {
    // The upstream models permit these fields to be absent. The original
    // adapter rejected an entire valid search page because of either one.
    let (service, server, listener) = search_fixture(vec![
        (200, r#"{"result":{"tracks":{"total":1,"results":[{"id":42,"title":"synthetic-private-title","artists":[{"name":"synthetic-private-artist"},{"id":12}]}]}}}"#.into()),
        (200, r#"{"result":[{"id":42,"title":"synthetic-private-title","artists":[{"name":"synthetic-private-artist"},{"id":12}]}]}"#.into()),
        (200, r#"{"result":{"albums":{"results":[{"id":7,"artists":[{"name":"synthetic-private-artist"}]}],"perPage":20}}}"#.into()),
        (200, r#"{"result":{"artists":{"results":[{"id":"11","name":"synthetic-private-artist"}],"total":1}}}"#.into()),
        (200, r#"{"result":{"tracks":{"total":0}}}"#.into()),
        (200, r#"{"result":{"text":"synthetic query"}}"#.into()),
    ]);
    let token = token();
    let tracks = service
        .search(&token, "Синтетика & Latin", SearchKind::Tracks, 0)
        .unwrap();
    assert_eq!(tracks.tracks[0].artists, ["synthetic-private-artist"]);
    assert!(!tracks.has_more);
    assert_redacted(&tracks);
    let albums = service
        .search(&token, "synthetic", SearchKind::Albums, 0)
        .unwrap();
    assert_eq!(albums.albums[0].artists, ["synthetic-private-artist"]);
    assert!(!albums.has_more);
    let artists = service
        .search(&token, "synthetic", SearchKind::Artists, 0)
        .unwrap();
    assert_eq!(artists.artists[0].id, "11");
    assert!(!artists.has_more);
    for _ in 0..2 {
        let empty = service
            .search(&token, "synthetic", SearchKind::Tracks, 0)
            .unwrap();
        assert!(empty.tracks.is_empty());
        assert!(!empty.has_more);
    }
    let requests = complete(server, listener);
    let target = requests[0]
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap();
    let url = Url::parse(&format!("http://fixture.invalid{target}")).unwrap();
    assert_eq!(
        url.query_pairs().find(|(key, _)| key == "text").unwrap().1,
        "Синтетика & Latin"
    );
}

#[test]
fn search_refuses_oversized_hydration_without_losing_provider_pagination() {
    let tracks = (1..=201).map(|id| json!({"id":id})).collect::<Vec<_>>();
    let (service, server, listener) = search_fixture(vec![
        (
            200,
            json!({"result":{"tracks":{"total":202,"perPage":201,"results":tracks}}}).to_string(),
        ),
        (
            200,
            r#"{"result":{"tracks":{"total":202,"perPage":201,"results":[{"id":202}]}}}"#.into(),
        ),
        (200, r#"{"result":[{"id":202}]}"#.into()),
        (
            200,
            r#"{"result":{"tracks":{"total":202,"perPage":201,"results":[]}}}"#.into(),
        ),
    ]);
    let token = token();
    let first = service
        .search(&token, "synthetic", SearchKind::Tracks, 0)
        .unwrap_err();
    assert_eq!(
        first,
        search_failure(
            MusicError::ResponseTooLarge,
            SearchParseStage::TrackLookup,
            200
        )
    );
    let last = service
        .search(&token, "synthetic", SearchKind::Tracks, 1)
        .unwrap();
    assert_eq!(last.tracks[0].id, "202");
    assert!(!last.has_more);
    let empty = service
        .search(&token, "synthetic", SearchKind::Tracks, 2)
        .unwrap();
    assert!(empty.tracks.is_empty());
    assert!(!empty.has_more);
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 4);
    assert!(requests[1].starts_with("GET /search?"));
    assert!(requests[2].starts_with("POST /tracks "));
}

#[test]
fn relaxed_optional_metadata_still_rejects_unidentified_entities_and_invalid_pages() {
    let bodies = [
        r#"{"result":{"unexpected":"synthetic-private-person"}}"#,
        r#"{"result":{"tracks":{}}}"#,
        r#"{"result":{"artists":{"total":1,"results":[{"name":"synthetic-private-artist"}]}}}"#,
        r#"{"result":{"artists":{"total":1,"results":[{"id":11}]}}}"#,
        r#"{"result":{"tracks":{"total":1,"perPage":0,"results":[{"id":42}]}}}"#,
        r#"{"result":{"tracks":{"total":0,"results":[{"id":42}]}}}"#,
    ];
    let (service, server, listener) =
        search_fixture(bodies.iter().map(|body| (200, (*body).into())).collect());
    for (index, _) in bodies.iter().enumerate() {
        let kind = if matches!(index, 2 | 3) {
            SearchKind::Artists
        } else {
            SearchKind::Tracks
        };
        let error = service.search(&token(), "synthetic", kind, 0).unwrap_err();
        let expected_stage = match index {
            0 => SearchParseStage::Section,
            1 | 4 | 5 => SearchParseStage::Pagination,
            2 | 3 => SearchParseStage::ArtistNormalization,
            _ => unreachable!(),
        };
        assert_eq!(error, search_contract(expected_stage, 200));
        assert_eq!(error.category(), "response-contract");
        assert_eq!(error.http_status(), Some(200));
        assert_redacted(error);
    }
    assert_eq!(complete(server, listener).len(), bodies.len());
}

#[test]
fn rich_playlist_entries_expand_only_missing_tracks_and_keep_duplicate_order() {
    let (service, server, listener) = fixture(vec![
        (
            200,
            format!(
                r#"{{"result":{{"trackCount":4,"tracks":[{{"id":42,"albumId":7,"track":{TRACK}}},{{"id":43}},{{"id":42,"albumId":7,"track":{TRACK}}},{{"id":43}}]}}}}"#
            ),
        ),
        (
            200,
            r#"{"result":[{"id":43,"title":"second synthetic track"}]}"#.into(),
        ),
    ]);
    let id = PlaylistId {
        owner: "1234".into(),
        kind: "8".into(),
    };
    let page = service.playlist_tracks(&token(), &id, 0).unwrap();
    assert_eq!(
        page.items
            .iter()
            .map(|track| track.id.as_str())
            .collect::<Vec<_>>(),
        ["42", "43", "42", "43"]
    );
    assert_eq!(page.items[0].title, "synthetic-private-title");
    assert_eq!(page.items[1].title, "second synthetic track");
    assert!(!page.has_more);
    let requests = complete(server, listener);
    assert!(requests[0].starts_with("GET /users/1234/playlists/8?richTracks=true HTTP/1.1"));
    assert!(requests[1].ends_with("track-ids=43"));
}

#[test]
fn omitted_empty_playlist_opens_but_missing_nonempty_contents_and_mismatched_tracks_fail() {
    let (service, server, listener) = fixture(vec![
        (200, r#"{"result":{"trackCount":0}}"#.into()),
        (200, r#"{"result":{"trackCount":0,"tracks":[]}}"#.into()),
        (200, r#"{"result":{"trackCount":1}}"#.into()),
        (200, r#"{"result":{}}"#.into()),
        (
            200,
            r#"{"result":{"tracks":[{"id":42,"track":{"id":43}}]}}"#.into(),
        ),
    ]);
    let id = PlaylistId {
        owner: "1234".into(),
        kind: "8".into(),
    };
    for _ in 0..2 {
        let page = service.playlist_tracks(&token(), &id, 0).unwrap();
        assert!(page.items.is_empty());
        assert!(!page.has_more);
    }
    for _ in 0..3 {
        assert_eq!(
            service.playlist_tracks(&token(), &id, 0),
            Err(MusicError::InvalidResponse)
        );
    }
    assert_eq!(complete(server, listener).len(), 5);
}

#[test]
fn artist_detail_uses_identified_resource_and_validates_pager_before_navigation() {
    let (service, server, listener) = fixture(vec![
        (
            200,
            format!(
                r#"{{"result":{{"tracks":[{TRACK}],"pager":{{"page":0,"perPage":1,"total":2}}}}}}"#
            ),
        ),
        (
            200,
            r#"{"result":{"tracks":[],"pager":{"page":1,"perPage":1,"total":2}}}"#.into(),
        ),
        (
            200,
            r#"{"result":{"tracks":[],"pager":{"page":0,"perPage":1,"total":2}}}"#.into(),
        ),
    ]);
    let token = token();
    let first = service.artist_tracks(&token, "11", 0).unwrap();
    assert!(first.has_more);
    assert_eq!(first.items[0].id, "42");
    let last = service.artist_tracks(&token, "11", 1).unwrap();
    assert!(!last.has_more);
    assert!(last.items.is_empty());
    assert_eq!(
        service.artist_tracks(&token, "11", 1),
        Err(MusicError::InvalidResponse)
    );
    assert_eq!(
        service.artist_tracks(&token, "11", 10001),
        Err(MusicError::InvalidInput)
    );
    let requests = complete(server, listener);
    assert!(requests[0].starts_with("GET /artists/11/tracks?page=0&page-size=50 HTTP/1.1"));
    assert!(requests[1].starts_with("GET /artists/11/tracks?page=1&page-size=50 HTTP/1.1"));
}

#[test]
fn typed_artwork_is_reduced_from_track_album_artist_and_playlist_metadata() {
    let template = "avatars.yandex.net/get-music-content/synthetic-private-cover/%%";
    let expected = crate::artwork::CoverArt::parse(template).unwrap();
    let track = track_from_wire(
        serde_json::from_value(json!({
            "id":42,"coverUri":template,"explicit":true,"available":false
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(track.cover, Some(expected.clone()));
    assert!(track.explicit);
    assert!(!track.available);
    let album = album_from_wire(
        serde_json::from_value(json!({
            "id":7,"coverUri":template,"volumes":[]
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(album.cover, Some(expected.clone()));
    let artist = artist_from_wire(
        serde_json::from_value(json!({
            "id":11,"name":"synthetic-private-artist","cover":{"uri":template}
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(artist.cover, Some(expected.clone()));
    let playlist = playlist_from_wire(
        serde_json::from_value(json!({
            "owner":{"uid":1234},"kind":8,"trackCount":0,"cover":{"itemsUri":[template]}
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(playlist.cover, Some(expected));
    for visible in [
        format!("{track:?}"),
        format!("{album:?}"),
        format!("{artist:?}"),
        format!("{playlist:?}"),
        format!("{:?}", track.cover),
    ] {
        assert!(!visible.contains("synthetic-private-cover"));
        assert!(!visible.contains("avatars.yandex.net"));
        assert!(!visible.contains("synthetic-private-artist"));
    }
}

#[test]
fn published_cover_shapes_survive_search_and_canonical_track_batch() {
    // Synthetic resources shaped after MarshalX 9ca132e5's Track/Album/Cover
    // fixtures and yamuse 368c746's protocol-relative image normalization.
    // No published resource, account identifier or remote image is requested.
    let track_cover = "avatars.yandex.net/get-music-content/12345678/abcde1234f56789-1/%%";
    let album_cover = "https://avatars.yandex.net/get-music-content/12345678/ff00aa11bb22cc33-1/%%";
    let artist_cover = "//avatars.yandex.net/get-music-content/12345678/artist-cover-1/%%";
    let playlist_cover =
        "avatars.yandex.net/get-music-user-playlist/12345/ABCDEF0123456789/%%?1700000000000";
    let body = json!({"result":{
        "text":"synthetic-private-query",
        "tracks":{"total":2,"perPage":20,"results":[{"id":42},{"id":43}]},
        "albums":{"total":2,"perPage":20,"results":[
            {"id":7,"coverUri":album_cover},
            {"id":8,"coverUri":{"unsupported":"synthetic-private-cover"}}
        ]},
        "artists":{"total":2,"perPage":20,"results":[
            {"id":11,"name":"synthetic-private-artist","cover":{
                "uri":"https://evil.invalid/%%",
                "itemsUri":[false,artist_cover]
            }},
            {"id":12,"name":"synthetic-private-artist","cover":{"itemsUri":false}}
        ]},
        "playlists":{"total":2,"perPage":20,"results":[
            {"owner":{"uid":1234},"kind":8,"cover":{"uri":playlist_cover}},
            {"owner":{"uid":1234},"kind":9,"cover":{
                "itemsUri":["https://avatars.yandex.net/get-music-user-playlist/synthetic/%%?token=synthetic-private-token"]
            }}
        ]}
    }});
    let canonical = json!({"result":[
        {"id":42,"available":true,"coverUri":track_cover},
        {"id":43,"available":true,"coverUri":["synthetic-private-cover"]}
    ]});
    let (service, server, listener) =
        search_fixture(vec![(200, body.to_string()), (200, canonical.to_string())]);
    let result = service
        .search_all(&token(), "synthetic-private-query")
        .unwrap();
    let tracks = result.tracks.as_ref().unwrap();
    let albums = result.albums.as_ref().unwrap();
    let artists = result.artists.as_ref().unwrap();
    let playlists = result.playlists.as_ref().unwrap();
    for (covers, expected) in [
        (
            tracks
                .items
                .iter()
                .map(|item| item.cover.clone())
                .collect::<Vec<_>>(),
            track_cover,
        ),
        (
            albums
                .items
                .iter()
                .map(|item| item.cover.clone())
                .collect::<Vec<_>>(),
            album_cover,
        ),
        (
            artists
                .items
                .iter()
                .map(|item| item.cover.clone())
                .collect::<Vec<_>>(),
            artist_cover,
        ),
        (
            playlists
                .items
                .iter()
                .map(|item| item.cover.clone())
                .collect::<Vec<_>>(),
            playlist_cover,
        ),
    ] {
        assert_eq!(
            covers.len(),
            2,
            "optional artwork must not discard the page"
        );
        assert_eq!(
            covers[0],
            Some(crate::artwork::CoverArt::parse(expected).unwrap())
        );
        assert_eq!(covers[1], None);
    }
    assert_eq!(
        tracks
            .items
            .iter()
            .map(|track| track.id.as_str())
            .collect::<Vec<_>>(),
        ["42", "43"]
    );
    assert!(tracks.items.iter().all(|track| track.available));
    assert_redacted(&result);
    let requests = complete(server, listener);
    assert_eq!(
        requests.len(),
        2,
        "artwork parsing cannot add network requests"
    );
    assert!(requests[1].ends_with("track-ids=42&track-ids=43"));
}

#[test]
fn malformed_optional_artwork_never_discards_valid_track_or_catalog_entities() {
    for invalid in [
        json!(null),
        json!(false),
        json!(12),
        json!({"uri":"synthetic-private-cover"}),
        json!(["synthetic-private-cover"]),
        json!("https://evil.invalid/synthetic-private-cover/%%"),
        json!(
            "https://avatars.yandex.net/get-music-content/synthetic/%%?token=synthetic-private-token"
        ),
        json!("http://avatars.yandex.net/get-music-content/synthetic/%%"),
        json!("https://avatars.yandex.net:443/get-music-content/synthetic/%%"),
        json!("https://avatars.yandex.net/get-music-content/synthetic/%%?1700000000000"),
        json!("https://avatars.yandex.net/get-music-user-playlist/synthetic/%%?"),
        json!(
            "https://avatars.yandex.net/get-music-user-playlist/synthetic/%%?170000000000000000000"
        ),
        json!(
            "https://avatars.yandex.net/get-music-user-playlist/synthetic/%%?1700000000000&token=synthetic-private-token"
        ),
        json!(
            "https://avatars.yandex.net/get-music-user-playlist/synthetic/%%?1700000000000#synthetic-private-fragment"
        ),
    ] {
        let track = track_from_wire(
            serde_json::from_value(json!({
                "id":42,"coverUri":invalid.clone(),"available":true
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(track.cover.is_none());
        assert!(track.available);
        let album = album_from_wire(
            serde_json::from_value(json!({
                "id":7,"coverUri":invalid.clone()
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(album.cover.is_none());
        let artist = artist_from_wire(
            serde_json::from_value(json!({
                "id":11,"name":"synthetic-private-artist","cover":invalid.clone()
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(artist.cover.is_none());
        let playlist = playlist_from_wire(
            serde_json::from_value(json!({
                "owner":{"uid":1234},"kind":8,"trackCount":0,"cover":invalid
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(playlist.cover.is_none());
    }
}

#[test]
fn artwork_fallbacks_choose_first_valid_template_without_widening_image_origin() {
    let direct = "avatars.yandex.net/get-music-content/synthetic/direct/%%";
    let fallback = "avatars.yandex.net/get-music-content/synthetic/fallback/%%";
    let expected_direct = crate::artwork::CoverArt::parse(direct);
    let expected_fallback = crate::artwork::CoverArt::parse(fallback);
    let track = track_from_wire(
        serde_json::from_value(json!({
            "id":42,"coverUri":"https://evil.invalid/%%","albums":[{"id":7,"coverUri":fallback}]
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(track.cover, expected_fallback);
    let track = track_from_wire(
        serde_json::from_value(json!({
            "id":42,"coverUri":direct,"albums":[{"id":7,"coverUri":fallback}]
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(track.cover, expected_direct);
    let artist = artist_from_wire(serde_json::from_value(json!({
        "id":11,"name":"synthetic-private-artist","cover":{"uri":"https://evil.invalid/%%","itemsUri":[null,17,"https://evil.invalid/%%",fallback,direct]}
    })).unwrap()).unwrap();
    assert_eq!(artist.cover, expected_fallback);
    let playlist = playlist_from_wire(serde_json::from_value(json!({
        "owner":{"uid":1234},"kind":8,"trackCount":0,"cover":{"uri":direct,"itemsUri":[fallback]}
    })).unwrap()).unwrap();
    assert_eq!(playlist.cover, expected_direct);
}

#[test]
fn content_marker_uses_only_explicit_boolean_and_preserves_metadata_availability() {
    for (value, expected) in [
        (json!(true), true),
        (json!(false), false),
        (json!(null), false),
        (json!("true"), false),
        (json!(1), false),
    ] {
        let track = track_from_wire(
            serde_json::from_value(json!({
                "id":42,"explicit":value,"contentWarning":"explicit","available":false
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(track.explicit, expected);
        assert!(!track.available);
    }
    let track = track_from_wire(
        serde_json::from_value(json!({
            "id":42,"contentWarning":"explicit","available":true
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(!track.explicit);
    assert!(track.available);
}

#[test]
fn search_embedded_album_identity_is_optional_but_result_identity_remains_strict() {
    // Pinned yamu Album.id is optional even though a Track.id is required.
    // Compact credits describe a real track; they need not be navigable albums.
    // Unrequested result blocks can use different/unsupported schemas without
    // replacing valid requested tracks with a fabricated empty page.
    let body = json!({"result":{
        "tracks":{"total":2,"perPage":20,"results":[
            {"id":42,"title":"synthetic-private-title","available":true,
             "albums":[{"title":"synthetic-private-album","coverUri":"avatars.yandex.net/get-music-content/synthetic/%%"}]},
            {"id":"43","albums":[{"id":null,"title":"synthetic-private-album"}]}
        ]},
        "albums":{"total":"unsupported-unrequested-counter","results":[{"id":null}]},
        "artists":{"results":[{"name":"synthetic-private-artist"}]},
        "best":{"type":"unexpected-kind","result":{"synthetic-private-field":true}}
    }}).to_string();
    let (service, server, listener) = search_fixture(vec![
        (200, body),
        (200, r#"{"result":[{"id":42,"title":"synthetic-private-title","available":true,"albums":[{"title":"synthetic-private-album","coverUri":"avatars.yandex.net/get-music-content/synthetic/%%"}]},{"id":"43","albums":[{"title":"synthetic-private-album"}]}]}"#.into()),
        (
            200,
            r#"{"result":{"albums":{"total":1,"results":[{"title":"synthetic-private-album"}]}}}"#
                .into(),
        ),
        (
            200,
            r#"{"result":[{"id":42,"albums":[{"title":"synthetic-private-album"}]}]}"#.into(),
        ),
    ]);
    let token = token();
    let results = service
        .search(&token, "Синтетика & Latin", SearchKind::Tracks, 0)
        .unwrap();
    assert_eq!(results.http_status, 200);
    assert_eq!(results.tracks.len(), 2);
    assert_eq!(results.tracks[0].id, "42");
    assert_eq!(results.tracks[1].id, "43");
    assert!(results.tracks.iter().all(|track| track.album_id.is_none()));
    assert_eq!(
        results.tracks[0].album_title.as_deref(),
        Some("synthetic-private-album")
    );
    assert!(results.tracks[0].cover.is_some());
    assert!(!results.has_more);
    assert_redacted(&results);
    assert_eq!(
        service.search(&token, "synthetic", SearchKind::Albums, 0),
        Err(search_contract(SearchParseStage::AlbumNormalization, 200))
    );
    let track = service.track(&token, "42").unwrap();
    assert!(track.album_id.is_none());
    assert_eq!(
        track.album_title.as_deref(),
        Some("synthetic-private-album")
    );
    assert_eq!(complete(server, listener).len(), 4);
}

#[test]
fn search_null_and_mixed_display_credits_preserve_valid_tracks_and_detail_opening() {
    // Current MarshalX 9ca132e5 base.py::de_list omits null/non-list credit
    // containers and non-object members. The original 99a177e Vec<ArtistWire>
    // and Vec<AlbumWire> rejected these complete, identified track records.
    let tracks = json!([
        {"id":42,"title":"Synthetic null credits","available":true,
         "artists":null,"albums":null},
        {"id":"43","title":"Synthetic mixed credits","available":true,
         "artists":[null,7,"not an artist",{},
             {"id":null,"name":null,"error":"synthetic-private-message"},
             {"id":"https://sensitive.invalid/private","name":"Synthetic credited artist"},
             {"name":false},{"name":"\u{0000}synthetic-private-person"},
             {"name":"Synthetic second artist"}],
         "albums":[null,false,"not an album",{},
             {"id":null,"title":"Synthetic compact album","artists":null,"volumes":false,
              "coverUri":"avatars.yandex.net/get-music-content/synthetic/%%"}]},
        {"id":44,"title":null,"available":true,"artists":{},"albums":false,
         "durationMs":null},
        {"id":45,"title":false,"available":true,"durationMs":"synthetic-private-value",
         "artists":[{"name":{"unsupported":"synthetic-private-person"}}],
         "albums":[{"id":"invalid/private-id","title":"Synthetic display-only album"}]}
    ]);
    let selected = tracks.as_array().unwrap()[1].clone();
    let (service, server, listener) = search_fixture(vec![
        (
            200,
            json!({"result":{"tracks":{"total":4,"perPage":20,"results":tracks.clone()}}})
                .to_string(),
        ),
        (200, json!({"result":tracks}).to_string()),
        (200, json!({"result":[selected]}).to_string()),
        (
            200,
            json!({"result":{"albums":{"total":1,"results":[{
                "id":7,"title":"Synthetic album result","artists":[null,{"name":false},
                    {"name":"Synthetic album artist"}]
            }]}}})
            .to_string(),
        ),
    ]);
    let token = token();
    let page = service
        .search(&token, "Synthetic & Синтетика", SearchKind::Tracks, 0)
        .unwrap();
    assert_eq!(
        page.tracks
            .iter()
            .map(|track| track.id.as_str())
            .collect::<Vec<_>>(),
        ["42", "43", "44", "45"]
    );
    assert!(!page.has_more);
    assert!(page.tracks[0].artists.is_empty());
    assert!(page.tracks[0].album_id.is_none());
    assert_eq!(
        page.tracks[1].artists,
        ["Synthetic credited artist", "Synthetic second artist"]
    );
    assert!(page.tracks[1].album_id.is_none());
    assert_eq!(
        page.tracks[1].album_title.as_deref(),
        Some("Synthetic compact album")
    );
    assert!(page.tracks[1].cover.is_some());
    assert_eq!(page.tracks[2].title, "Без названия");
    assert_eq!(page.tracks[3].title, "Без названия");
    assert!(page.tracks[3].duration_ms.is_none());
    assert!(page.tracks[3].album_id.is_none());
    assert_eq!(
        page.tracks[3].album_title.as_deref(),
        Some("Synthetic display-only album")
    );
    assert!(page.tracks.iter().all(|track| track.available));
    let detail = service.track(&token, &page.tracks[1].id).unwrap();
    assert_eq!(detail.id, "43");
    assert_eq!(detail.artists, page.tracks[1].artists);
    let albums = service
        .search(&token, "Synthetic", SearchKind::Albums, 0)
        .unwrap();
    assert_eq!(albums.albums[0].id, "7");
    assert_eq!(albums.albums[0].artists, ["Synthetic album artist"]);
    assert_redacted(page);
    assert_redacted(detail);
    assert_eq!(complete(server, listener).len(), 4);
}

#[test]
fn minimal_search_hits_keep_required_identity_strict_and_private() {
    let cases = [
        (
            r#"{"title":"synthetic-private-title","artists":null}"#,
            SearchSchemaShape::IdentityMissing,
        ),
        (
            r#"{"id":null,"artists":null}"#,
            SearchSchemaShape::IdentityMissing,
        ),
        (
            r#"{"id":{},"artists":null}"#,
            SearchSchemaShape::IdentityType,
        ),
        (
            r#"{"id":false,"artists":null}"#,
            SearchSchemaShape::IdentityType,
        ),
        (
            r#"{"id":-42,"artists":null}"#,
            SearchSchemaShape::IdentityInvalid,
        ),
        (
            r#"{"id":"../private","artists":null}"#,
            SearchSchemaShape::IdentityInvalid,
        ),
        (
            r#"{"id":42,"id":43,"artists":null}"#,
            SearchSchemaShape::FieldsUnsupported,
        ),
        (r#"[42]"#, SearchSchemaShape::RecordNotObject),
        (r#"["42:7"]"#, SearchSchemaShape::RecordNotObject),
    ];
    let (service, server, listener) = search_fixture(
        cases
            .iter()
            .map(|(item, _)| {
                (
                    200,
                    format!(r#"{{"result":{{"tracks":{{"total":1,"results":[{item}]}}}}}}"#),
                )
            })
            .collect(),
    );
    for (_, shape) in cases {
        let error = service
            .search(&token(), "synthetic-private-query", SearchKind::Tracks, 0)
            .unwrap_err();
        assert_eq!(
            error,
            search_contract(SearchParseStage::TrackWireShape(shape), 200)
        );
        assert_eq!(error.diagnostic_stage(), Some("track-wire"));
        assert_eq!(error.diagnostic_shape(), Some(shape.as_str()));
        assert_eq!(error.http_status(), Some(200));
        assert_redacted(error);
        assert!(!format!("{error} {error:?}").contains("synthetic-private-query"));
    }
    assert_eq!(complete(server, listener).len(), cases.len());
    for invalid in [json!(null), json!(-7), json!("invalid/private-id")] {
        let album =
            serde_json::from_value::<AlbumWire>(json!({"id":invalid.clone(),"artists":null}));
        assert!(album.is_err() || album_from_wire(album.unwrap()).is_err());
        let artist =
            serde_json::from_value::<ArtistWire>(json!({"id":invalid,"name":"Synthetic artist"}));
        assert!(artist.is_err() || artist_from_wire(artist.unwrap()).is_err());
    }
}

#[test]
fn search_reports_first_failed_layer_with_observed_http_and_fixed_private_safe_errors() {
    let cases = [
        (
            SearchKind::Tracks,
            SearchParseStage::Envelope,
            r#"{"result":"synthetic-private-query"}"#,
        ),
        (
            SearchKind::Tracks,
            SearchParseStage::Envelope,
            r#"{"result":{},"error":{"message":"synthetic-private-person"}}"#,
        ),
        (
            SearchKind::Tracks,
            SearchParseStage::Section,
            r#"{"result":{"tracks":"synthetic-private-title"}}"#,
        ),
        (
            SearchKind::Tracks,
            SearchParseStage::Section,
            r#"{"result":{"albums":{"results":[{"id":7}]}}}"#,
        ),
        (
            SearchKind::Tracks,
            SearchParseStage::Pagination,
            r#"{"result":{"tracks":{"total":"synthetic-private-query","results":[{"id":42}]}}}"#,
        ),
        (
            SearchKind::Tracks,
            SearchParseStage::Pagination,
            r#"{"result":{"tracks":{"total":1,"perPage":0,"results":[{"id":42}]}}}"#,
        ),
        (
            SearchKind::Tracks,
            SearchParseStage::TrackWireShape(SearchSchemaShape::IdentityMissing),
            r#"{"result":{"tracks":{"total":1,"results":[{"title":"synthetic-private-title"}]}}}"#,
        ),
        (
            SearchKind::Tracks,
            SearchParseStage::TrackWireShape(SearchSchemaShape::IdentityInvalid),
            r#"{"result":{"tracks":{"total":1,"results":[{"id":"https://sensitive.invalid/private?token=synthetic-private-query"}]}}}"#,
        ),
        (
            SearchKind::Tracks,
            SearchParseStage::TrackWireShape(SearchSchemaShape::RecordNotObject),
            r#"{"result":{"tracks":{"total":1,"results":["synthetic-private-title"]}}}"#,
        ),
        (
            SearchKind::Albums,
            SearchParseStage::AlbumWire,
            r#"{"result":{"albums":{"total":1,"results":[{"id":-1}]}}}"#,
        ),
        (
            SearchKind::Albums,
            SearchParseStage::AlbumNormalization,
            r#"{"result":{"albums":{"total":1,"results":[{"title":"synthetic-private-album"}]}}}"#,
        ),
        (
            SearchKind::Artists,
            SearchParseStage::ArtistWire,
            r#"{"result":{"artists":{"total":1,"results":[{"id":11,"name":false}]}}}"#,
        ),
        (
            SearchKind::Artists,
            SearchParseStage::ArtistNormalization,
            r#"{"result":{"artists":{"total":1,"results":[{"id":11}]}}}"#,
        ),
    ];
    let (service, server, listener) = search_fixture(
        cases
            .iter()
            .map(|(_, _, body)| (200, (*body).into()))
            .collect(),
    );
    for (kind, stage, _) in cases {
        let error = service
            .search(&token(), "synthetic-private-query", kind, 0)
            .unwrap_err();
        assert_eq!(error, search_contract(stage, 200));
        assert_eq!(error.category(), "response-contract");
        assert_eq!(error.http_status(), Some(200));
        assert_eq!(error.diagnostic_stage(), Some(stage.as_str()));
        assert_redacted(error);
        let visible = format!("{error} {error:?}");
        for private in [
            "synthetic-private-query",
            "sensitive.invalid",
            "durationMs",
            "result.tracks",
            "missing field",
            "invalid type",
        ] {
            assert!(!visible.contains(private));
        }
    }
    assert_eq!(complete(server, listener).len(), cases.len());
}

#[test]
fn staged_search_rejects_duplicate_known_fields_instead_of_losing_them_in_a_json_tree() {
    let cases = [
        (
            SearchParseStage::Envelope,
            r#"{"result":{"text":"a"},"result":{"text":"b"}}"#,
        ),
        (
            SearchParseStage::Envelope,
            r#"{"result":{"tracks":{"total":0},"tracks":{"total":0}}}"#,
        ),
        (
            SearchParseStage::Section,
            r#"{"result":{"tracks":{"total":1,"results":[{"id":42}],"results":[{"id":43}]}}}"#,
        ),
        (
            SearchParseStage::Section,
            r#"{"result":{"tracks":{"total":1,"total":2,"results":[{"id":42}]}}}"#,
        ),
        (
            SearchParseStage::TrackWireShape(SearchSchemaShape::FieldsUnsupported),
            r#"{"result":{"tracks":{"total":1,"results":[{"id":42,"id":43}]}}}"#,
        ),
    ];
    let (service, server, listener) = search_fixture(
        cases
            .iter()
            .map(|(_, body)| (200, (*body).into()))
            .collect(),
    );
    for (stage, _) in cases {
        assert_eq!(
            service.search(&token(), "synthetic", SearchKind::Tracks, 0),
            Err(search_contract(stage, 200))
        );
    }
    assert_eq!(complete(server, listener).len(), cases.len());
}

#[test]
fn search_transport_status_is_observed_and_never_invented_from_a_parse_category() {
    let (service, server, listener) = search_fixture(vec![
        (200, r#"{"result":{"tracks":{"total":0}}}"#.into()),
        (201, r#"{"result":{"tracks":{"total":0}}}"#.into()),
        (401, "synthetic-private-person".into()),
        (403, r#"{"error":{"name":"missing-permissions"}}"#.into()),
    ]);
    let token = token();
    assert_eq!(
        service
            .search(&token, "synthetic", SearchKind::Tracks, 0)
            .unwrap()
            .http_status,
        200
    );
    for expected in [
        MusicError::Http(201),
        MusicError::AccessDenied,
        MusicError::Forbidden,
    ] {
        let error = service
            .search(&token, "synthetic", SearchKind::Tracks, 0)
            .unwrap_err();
        assert_eq!(error, expected);
        assert_eq!(error.http_status(), expected.http_status());
        assert!(error.diagnostic_stage().is_none());
    }
    assert_eq!(complete(server, listener).len(), 4);
}

#[test]
fn search_success_status_survives_body_size_read_and_deadline_failures() {
    for (wire, delay, kind) in [
        (format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", MAX_BODY + 1), Duration::ZERO, SearchFailureKind::ResponseTooLarge),
        (format!("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}", "x".repeat(MAX_BODY + 1)), Duration::ZERO, SearchFailureKind::ResponseTooLarge),
        ("HTTP/1.1 200 OK\r\nContent-Length: 1024\r\nConnection: close\r\n\r\nsynthetic-private-person".into(), Duration::ZERO, SearchFailureKind::Network),
        (format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{ACCOUNT}", ACCOUNT.len()), Duration::from_millis(700), SearchFailureKind::Timeout),
    ] {
        let error = wire_request_fixture(wire, delay, |service| service.search(&token(), "synthetic", SearchKind::Tracks, 0)).unwrap_err();
        assert_eq!(error, MusicError::SearchResponse { stage: SearchParseStage::Envelope, kind, http: 200 });
        assert_redacted(error);
        assert_eq!(error.http_status(), Some(200));
    }
    let (service, server, listener) = search_fixture(vec![]);
    drop(listener);
    server.join().unwrap();
    let error = service
        .search(&token(), "synthetic", SearchKind::Tracks, 0)
        .unwrap_err();
    assert_eq!(error, MusicError::Network);
    assert!(error.http_status().is_none());
    assert!(error.diagnostic_stage().is_none());
}

#[test]
fn search_page_bounds_retain_success_status_without_weakening_request_limits() {
    let tracks = (0..=MAX_SEARCH_PAGE)
        .map(|id| json!({"id":id}))
        .collect::<Vec<_>>();
    let (service, server, listener) = search_fixture(vec![(
        200,
        json!({"result":{"tracks":{"total":tracks.len(),"results":tracks}}}).to_string(),
    )]);
    let error = service
        .search(&token(), "synthetic", SearchKind::Tracks, 0)
        .unwrap_err();
    assert_eq!(
        error,
        MusicError::SearchResponse {
            stage: SearchParseStage::Pagination,
            kind: SearchFailureKind::ResponseTooLarge,
            http: 200
        }
    );
    assert_redacted(error);
    assert_eq!(complete(server, listener).len(), 1);
}

#[test]
fn identity_only_search_ignores_all_display_shapes_and_expands_once_in_original_order() {
    // The first identified hit reproduces track-fields-unsupported in the old
    // shared TrackWire path. Its search metadata is not a canonical track.
    let odd_hit = r#"{"id":42,"available":true,"title":{"private":"synthetic-private-title"},"artists":null,"artists":[{"name":"a","name":"b"}],"albums":[{"id":null,"id":7}],"durationMs":[3],"coverUri":false,"explicit":{"private":true}}"#;
    assert!(serde_json::from_str::<TrackWire>(odd_hit).is_err());
    let raw = RawValue::from_string(odd_hit.into()).unwrap();
    assert_eq!(
        search_track_shape(&raw, 200).diagnostic_shape(),
        Some("track-fields-unsupported")
    );
    let search = format!(
        r#"{{"result":{{"text":"synthetic-private-query","tracks":{{"total":6,"perPage":5,"results":[{odd_hit},{{"id":43,"available":{{"ignored":true}},"title":null,"artists":false}},{{"id":42,"durationMs":"ignored"}},{{"id":44,"albums":[null]}},{{"id":45,"available":true,"explicit":[1]}}]}}}}}}"#
    );
    let normalized = json!({"result":[
        {"id":43,"title":"Normalized second","available":true,"artists":[{"name":"Canonical artist"}]},
        {"id":45,"title":"Unavailable canonical track","available":false},
        null,
        {"id":42,"title":"Normalized first","available":true,"durationMs":123000}
    ]}).to_string();
    let (service, server, listener) = search_fixture(vec![
        (200, search),
        (200, normalized),
        (
            200,
            r#"{"result":[{"id":42,"title":"Normalized first","available":true}]}"#.into(),
        ),
    ]);
    let page = service
        .search(&token(), "synthetic-private-query", SearchKind::Tracks, 0)
        .unwrap();
    assert_eq!(
        page.tracks
            .iter()
            .map(|track| track.id.as_str())
            .collect::<Vec<_>>(),
        ["42", "43", "42", "45"]
    );
    assert_eq!(page.tracks[0].title, "Normalized first");
    assert_eq!(page.tracks[1].artists, ["Canonical artist"]);
    assert_eq!(page.tracks[2].title, "Normalized first");
    assert!(!page.tracks[3].available);
    assert_eq!(page.omitted_tracks, 1);
    assert!(page.has_more);
    assert_eq!(page.http_status, 200);
    let detail = service.track(&token(), &page.tracks[0].id).unwrap();
    assert!(detail.available);
    assert_eq!(detail.title, "Normalized first");
    assert_redacted(&page);
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 3);
    assert!(requests[0].starts_with("GET /search?"));
    assert!(requests[1].starts_with("POST /tracks HTTP/1.1"));
    assert!(requests[1].ends_with("track-ids=42&track-ids=43&track-ids=44&track-ids=45"));
    assert!(requests[2].ends_with("track-ids=42"));
}

#[test]
fn identified_search_section_does_not_require_a_well_shaped_query_echo() {
    let (service,server,listener)=search_fixture(vec![
        (200,r#"{"result":{"text":{"unsupported":"synthetic-private-query"},"tracks":{"total":1,"results":[{"id":42}]}}}"#.into()),
        (200,r#"{"result":[{"id":42,"title":"Canonical track","available":true}]}"#.into()),
    ]);
    let page = service
        .search(&token(), "synthetic", SearchKind::Tracks, 0)
        .unwrap();
    assert_eq!(page.tracks[0].title, "Canonical track");
    assert_eq!(page.omitted_tracks, 0);
    assert_eq!(complete(server, listener).len(), 2);
}

#[test]
fn search_keeps_composite_identity_and_prefers_exact_matches_over_bare_aliases() {
    let hits = json!({"result":{"tracks":{"total":6,"perPage":5,"results":[
        {"id":"42:7"},{"id":"42"},{"id":"42:8"},{"id":"43:9"},{"id":"42:7"}
    ]}}})
    .to_string();
    let normalized=json!({"result":[
        {"id":42,"title":"Canonical bare","available":true,"albums":[{"id":7,"title":"Canonical album"}]},
        {"id":"42:7","title":"Exact version","available":true}
    ]}).to_string();
    let (service, server, listener) = search_fixture(vec![
        (200, hits),
        (200, normalized),
        (
            200,
            r#"{"result":[{"id":42,"title":"Canonical detail","available":true}]}"#.into(),
        ),
    ]);
    let page = service
        .search(&token(), "synthetic", SearchKind::Tracks, 0)
        .unwrap();
    assert_eq!(
        page.tracks
            .iter()
            .map(|track| track.id.as_str())
            .collect::<Vec<_>>(),
        ["42:7", "42", "42:8", "42:7"]
    );
    assert_eq!(page.tracks[0].title, "Exact version");
    assert_eq!(page.tracks[2].title, "Canonical bare");
    assert_eq!(page.tracks[2].album_id.as_deref(), Some("7"));
    assert_eq!(
        page.tracks[2].album_title.as_deref(),
        Some("Canonical album")
    );
    assert_eq!(page.omitted_tracks, 1);
    assert!(page.has_more);
    let detail = service.track(&token(), &page.tracks[0].id).unwrap();
    assert_eq!(detail.id, "42:7");
    assert!(detail.available);
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 3);
    assert!(
        requests[1].ends_with("track-ids=42%3A7&track-ids=42&track-ids=42%3A8&track-ids=43%3A9")
    );
    assert!(requests[2].ends_with("track-ids=42%3A7"));
}

#[test]
fn search_batch_limit_counts_distinct_ids_and_missing_rows_do_not_change_paging() {
    let hits = (1..=48)
        .chain([1, 2])
        .map(|id| json!({"id":id,"title":{"ignored":true}}))
        .collect::<Vec<_>>();
    let (service, server, listener) = search_fixture(vec![
        (
            200,
            json!({"result":{"tracks":{"total":100,"perPage":50,"results":hits}}}).to_string(),
        ),
        (200, r#"{"result":[]}"#.into()),
        (
            200,
            r#"{"result":{"tracks":{"total":100,"perPage":50,"results":[]}}}"#.into(),
        ),
    ]);
    let page = service
        .search(&token(), "synthetic", SearchKind::Tracks, 0)
        .unwrap();
    assert!(page.tracks.is_empty());
    assert_eq!(page.omitted_tracks, 50);
    assert!(page.has_more);
    let empty = service
        .search(&token(), "synthetic", SearchKind::Tracks, 1)
        .unwrap();
    assert!(empty.tracks.is_empty());
    assert_eq!(empty.omitted_tracks, 0);
    assert!(!empty.has_more);
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[1].split("track-ids=").count() - 1, 48);
    assert!(requests[2].starts_with("GET /search?"));
}

#[test]
fn search_refuses_more_than_fifty_positions_even_when_the_id_is_repeated() {
    let hits = vec![json!({"id":42}); 51];
    let (service, server, listener) = search_fixture(vec![(
        200,
        json!({"result":{"tracks":{"total":100,"perPage":51,"results":hits}}}).to_string(),
    )]);
    let error = service
        .search(&token(), "synthetic", SearchKind::Tracks, 0)
        .unwrap_err();
    assert_eq!(
        error,
        search_failure(
            MusicError::ResponseTooLarge,
            SearchParseStage::TrackLookup,
            200
        )
    );
    assert_eq!(complete(server, listener).len(), 1);
}

#[test]
fn search_lookup_rejects_conflicting_or_unsolicited_canonical_identities() {
    for normalized in [
        r#"{"result":[{"id":42,"available":true},{"id":42,"available":false}]}"#,
        r#"{"result":[{"id":42},{"id":99}]}"#,
        r#"{"result":[{"id":"42:8"}]}"#,
        r#"{"result":[{"id":42,"id":43}]}"#,
        r#"{"result":[{"id":42,"available":"synthetic-private-person"}]}"#,
        r#"{"result":[{"title":"synthetic-private-person"}]}"#,
        r#"{"result":[{"id":42}],"error":{"message":"synthetic-private-person"}}"#,
    ] {
        let (service, server, listener) = search_fixture(vec![
            (
                200,
                r#"{"result":{"tracks":{"total":2,"results":[{"id":"42:7"},{"id":43}]}}}"#.into(),
            ),
            (200, normalized.into()),
        ]);
        let error = service
            .search(&token(), "synthetic-private-query", SearchKind::Tracks, 0)
            .unwrap_err();
        assert_eq!(error, search_contract(SearchParseStage::TrackLookup, 200));
        assert_eq!(error.diagnostic_stage(), Some("track-lookup"));
        assert_eq!(error.http_status(), Some(200));
        assert_redacted(error);
        assert_eq!(complete(server, listener).len(), 2);
    }
}

#[test]
fn search_requires_the_cached_account_and_token_before_any_request() {
    let (service, server, listener) = fixture(vec![]);
    assert_eq!(
        service.search(&token(), "synthetic", SearchKind::Tracks, 0),
        Err(MusicError::Unauthenticated)
    );
    assert_eq!(
        service.search_all(&token(), "synthetic"),
        Err(MusicError::Unauthenticated)
    );
    seed_search_account(&service);
    let another = AccessToken::for_test("synthetic-other-token", Duration::from_secs(60));
    assert_eq!(
        service.search(&another, "synthetic", SearchKind::Tracks, 0),
        Err(MusicError::Unauthenticated)
    );
    assert_eq!(
        service.search_all(&another, "synthetic"),
        Err(MusicError::Unauthenticated)
    );
    let expired = token();
    expired.invalidate();
    assert_eq!(
        service.search(&expired, "synthetic", SearchKind::Tracks, 0),
        Err(MusicError::Unauthenticated)
    );
    assert_eq!(
        service.search_all(&expired, "synthetic"),
        Err(MusicError::Unauthenticated)
    );
    assert!(complete(server, listener).is_empty());
}

#[test]
fn search_binding_epoch_survives_neither_logout_nor_same_token_revalidation() {
    for aggregate in [false, true] {
        for invalidate_during_lookup in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let retained = listener.try_clone().unwrap();
            let service = MusicService::for_test(&format!("http://{address}/"));
            seed_search_account(&service);
            let (reached, wait) = std::sync::mpsc::channel();
            let (release, ready) = std::sync::mpsc::channel();
            let server = thread::spawn(move || {
                let mut requests = Vec::new();
                let mut first = accept(&listener);
                requests.push(read_request(&mut first));
                if !invalidate_during_lookup {
                    reached.send(()).unwrap();
                    ready.recv_timeout(Duration::from_secs(4)).unwrap();
                }
                // Cache invalidation may make the client close immediately after
                // headers; a cancelled fixture response need not finish writing.
                let body = r#"{"result":{"tracks":{"total":1,"results":[{"id":42}]}}}"#;
                let _ = write!(
                    first,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                if invalidate_during_lookup {
                    let mut second = accept(&listener);
                    requests.push(read_request(&mut second));
                    reached.send(()).unwrap();
                    ready.recv_timeout(Duration::from_secs(4)).unwrap();
                    let body = r#"{"result":[{"id":42,"title":"synthetic-private-title","available":true}]}"#;
                    let _ = write!(
                        second,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
                requests
            });
            let worker_service = service.clone();
            let worker = thread::spawn(move || {
                if aggregate {
                    worker_service.search_all(&token(), "synthetic").map(|_| ())
                } else {
                    worker_service
                        .search(&token(), "synthetic", SearchKind::Tracks, 0)
                        .map(|_| ())
                }
            });
            wait.recv_timeout(Duration::from_secs(4)).unwrap();
            service.clear_session();
            // Represents successful revalidation with the same credential; an old
            // operation must still fail its captured cache epoch instead of joining
            // the new session merely because token/UID happen to compare equal.
            seed_search_account(&service);
            release.send(()).unwrap();
            assert_eq!(worker.join().unwrap(), Err(MusicError::Unauthenticated));
            let requests = complete(server, retained);
            assert_eq!(requests.len(), if invalidate_during_lookup { 2 } else { 1 });
        }
    }
}

#[test]
fn search_lookup_http_errors_keep_the_observed_status_without_metadata_fallback() {
    for (status, error) in [
        (401, MusicError::AccessDenied),
        (403, MusicError::Forbidden),
        (429, MusicError::RateLimited),
        (451, MusicError::Restricted),
        (500, MusicError::Http(500)),
        (302, MusicError::Http(302)),
    ] {
        let (service,server,listener)=search_fixture(vec![
            (200,r#"{"result":{"tracks":{"total":1,"results":[{"id":42,"title":"must not be fallback"}]}}}"#.into()),
            (status,r#"{"error":{"message":"synthetic-private-person"}}"#.into()),
        ]);
        let actual = service
            .search(&token(), "synthetic-private-query", SearchKind::Tracks, 0)
            .unwrap_err();
        assert_eq!(actual, error);
        assert_eq!(actual.http_status(), Some(status));
        assert_redacted(actual);
        assert_eq!(complete(server, listener).len(), 2);
    }
}

#[test]
fn search_lookup_transport_failure_does_not_reuse_search_http_200() {
    for aggregate in [false, true] {
        for delayed in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let service = MusicService {
                client: Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(1))
                    .redirect(Policy::none())
                    .retry(reqwest::retry::never())
                    .build()
                    .unwrap(),
                origin: Url::parse(&format!("http://{address}/")).unwrap(),
                account: Arc::new(Mutex::new(AccountCache::default())),
            };
            seed_search_account(&service);
            let server = thread::spawn(move || {
                let mut first = accept(&listener);
                let search = read_request(&mut first);
                assert!(search.starts_with("GET /search?"));
                let body = r#"{"result":{"tracks":{"total":1,"results":[{"id":42}]}}}"#;
                write!(
                    first,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                let mut second = accept(&listener);
                let lookup = read_request(&mut second);
                assert!(lookup.starts_with("POST /tracks "));
                if delayed {
                    thread::sleep(Duration::from_millis(1500));
                }
                drop(second);
                listener.set_nonblocking(true).unwrap();
                assert_eq!(
                    listener.accept().unwrap_err().kind(),
                    std::io::ErrorKind::WouldBlock
                );
            });
            let error = if aggregate {
                let result = service.search_all(&token(), "synthetic").unwrap();
                assert_eq!(result.http_status, 200);
                result.tracks.unwrap_err()
            } else {
                service
                    .search(&token(), "synthetic", SearchKind::Tracks, 0)
                    .unwrap_err()
            };
            assert_eq!(
                error,
                if delayed {
                    MusicError::Timeout
                } else {
                    MusicError::Network
                }
            );
            assert_eq!(error.http_status(), None);
            assert_eq!(error.diagnostic_stage(), None);
            assert_redacted(error);
            server.join().unwrap();
        }
    }
}

fn unified_wire() -> serde_json::Value {
    json!({"result":{
        "text":"synthetic-private-query",
        "tracks":{"total":3,"perPage":1,"results":[{"id":42,"artists":{"ignored":"synthetic-private-person"}}]},
        "albums":{"total":1,"perPage":20,"results":[{"id":7,"title":"synthetic-private-album"}]},
        "artists":{"total":2,"perPage":1,"results":[{"id":11,"name":"synthetic-private-artist"}]},
        "playlists":{"total":2,"perPage":1,"results":[{"owner":{"uid":4321},"kind":8,"title":"synthetic-private-playlist","cover":{"uri":"avatars.yandex.net/get-music-content/synthetic/%%"}}]}
    }})
}

#[test]
fn unified_search_uses_one_all_request_and_one_normalized_track_batch() {
    let (service, server, listener) = search_fixture(vec![
        (200, unified_wire().to_string()),
        (200, format!("{{\"result\":[{TRACK}]}}")),
    ]);
    let result = service
        .search_all(&token(), "synthetic-private-query")
        .unwrap();
    assert_eq!(result.http_status, 200);
    let tracks = result.tracks.as_ref().unwrap();
    assert_eq!(tracks.items[0].title, "synthetic-private-title");
    assert!(tracks.items[0].available);
    assert!(tracks.has_more);
    assert_eq!(tracks.omitted, 0);
    let albums = result.albums.as_ref().unwrap();
    assert_eq!(albums.items[0].id, "7");
    assert!(!albums.has_more);
    assert!(result.artists.as_ref().unwrap().has_more);
    let playlists = result.playlists.as_ref().unwrap();
    assert_eq!(playlists.items[0].id.owner, "4321");
    assert_eq!(playlists.items[0].id.kind, "8");
    assert_eq!(playlists.items[0].track_count, None);
    assert!(playlists.items[0].cover.is_some());
    assert!(playlists.has_more);
    assert_redacted(&result);
    assert_redacted(tracks);
    assert_redacted(&playlists.items[0]);
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 2);
    assert!(requests[0].contains("&type=all&page=0&"));
    assert!(requests[1].starts_with("POST /tracks HTTP/1.1"));
    assert!(requests[1].ends_with("track-ids=42"));
}

#[test]
fn unified_sections_fail_independently_and_retry_only_the_requested_category() {
    let cases = [
        (
            SearchKind::Tracks,
            "tracks",
            json!({"total":1,"results":[{"id":{"bad":true}}]}),
            SearchParseStage::TrackWireShape(SearchSchemaShape::IdentityType),
        ),
        (
            SearchKind::Albums,
            "albums",
            json!({"total":1,"results":[{"title":"synthetic-private-album"}]}),
            SearchParseStage::AlbumNormalization,
        ),
        (
            SearchKind::Artists,
            "artists",
            json!({"total":1,"results":[{"id":11,"name":false}]}),
            SearchParseStage::ArtistWire,
        ),
        (
            SearchKind::Playlists,
            "playlists",
            json!({"total":1,"results":[{"owner":{"uid":4321},"title":"synthetic-private-playlist"}]}),
            SearchParseStage::PlaylistWire,
        ),
    ];
    for (kind, key, bad, stage) in cases {
        let mut wire = unified_wire();
        wire["result"][key] = bad;
        let good_section = unified_wire()["result"][key].clone();
        let mut retry = serde_json::Map::new();
        retry.insert(key.into(), good_section);
        let mut responses = vec![(200, wire.to_string())];
        if kind != SearchKind::Tracks {
            responses.push((200, format!("{{\"result\":[{TRACK}]}}")));
        }
        responses.push((200, json!({"result":retry}).to_string()));
        if kind == SearchKind::Tracks {
            responses.push((200, format!("{{\"result\":[{TRACK}]}}")));
        }
        let (service, server, listener) = search_fixture(responses);
        let result = service.search_all(&token(), "synthetic").unwrap();
        match kind {
            SearchKind::Tracks => {
                assert_eq!(
                    result.tracks.as_ref().unwrap_err(),
                    &search_contract(stage, 200)
                );
                assert_eq!(result.albums.as_ref().unwrap().items.len(), 1);
            }
            SearchKind::Albums => assert_eq!(
                result.albums.as_ref().unwrap_err(),
                &search_contract(stage, 200)
            ),
            SearchKind::Artists => assert_eq!(
                result.artists.as_ref().unwrap_err(),
                &search_contract(stage, 200)
            ),
            SearchKind::Playlists => assert_eq!(
                result.playlists.as_ref().unwrap_err(),
                &search_contract(stage, 200)
            ),
        }
        if kind != SearchKind::Tracks {
            assert_eq!(result.tracks.as_ref().unwrap().items.len(), 1);
        }
        if kind != SearchKind::Playlists {
            assert_eq!(result.playlists.as_ref().unwrap().items.len(), 1);
        }
        assert_redacted(&result);
        let retry = service.search(&token(), "synthetic", kind, 0).unwrap();
        match kind {
            SearchKind::Tracks => assert_eq!(retry.tracks.len(), 1),
            SearchKind::Albums => assert_eq!(retry.albums.len(), 1),
            SearchKind::Artists => assert_eq!(retry.artists.len(), 1),
            SearchKind::Playlists => assert_eq!(retry.playlists.len(), 1),
        }
        let requests = complete(server, listener);
        assert_eq!(requests.len(), 3);
        let retry_index = if kind == SearchKind::Tracks { 1 } else { 2 };
        assert!(requests[retry_index].starts_with("GET /search?"));
        assert!(!requests[retry_index].contains("&type=all&"));
    }
}

#[test]
fn unified_track_lookup_refusal_preserves_other_sections_and_does_not_retry() {
    for (status, error) in [
        (403, MusicError::Forbidden),
        (429, MusicError::RateLimited),
        (500, MusicError::Http(500)),
    ] {
        let (service, server, listener) = search_fixture(vec![
            (200, unified_wire().to_string()),
            (
                status,
                r#"{"error":{"message":"synthetic-private-person"}}"#.into(),
            ),
        ]);
        let result = service
            .search_all(&token(), "synthetic-private-query")
            .unwrap();
        assert_eq!(result.tracks, Err(error));
        assert_eq!(result.albums.as_ref().unwrap().items.len(), 1);
        assert_eq!(result.artists.as_ref().unwrap().items.len(), 1);
        assert_eq!(result.playlists.as_ref().unwrap().items.len(), 1);
        assert_redacted(result);
        assert_eq!(complete(server, listener).len(), 2);
    }
    let (service, server, listener) = search_fixture(vec![
        (200, unified_wire().to_string()),
        (401, "synthetic-private-person".into()),
    ]);
    assert_eq!(
        service.search_all(&token(), "synthetic"),
        Err(MusicError::AccessDenied)
    );
    assert_eq!(complete(server, listener).len(), 2);
}

#[test]
fn unified_missing_normalized_tracks_preserve_original_section_pagination() {
    let (service,server,listener)=search_fixture(vec![
        (200,unified_wire().to_string()),(200,r#"{"result":[null]}"#.into()),
        (200,r#"{"result":{"playlists":{"total":2,"perPage":1,"results":[{"uid":4567,"kind":9,"title":"Other real selector","trackCount":null}]}}}"#.into()),
        (200,r#"{"result":{"trackCount":1,"tracks":[{"id":42}]}}"#.into()),
        (200,format!("{{\"result\":[{TRACK}]}}")),
    ]);
    let result = service.search_all(&token(), "synthetic").unwrap();
    let tracks = result.tracks.unwrap();
    assert!(tracks.items.is_empty());
    assert_eq!(tracks.omitted, 1);
    assert!(tracks.has_more);
    let playlists = service
        .search(&token(), "synthetic", SearchKind::Playlists, 1)
        .unwrap();
    assert!(!playlists.has_more);
    assert_eq!(playlists.playlists[0].track_count, None);
    let tracks = service
        .playlist_tracks(&token(), &playlists.playlists[0].id, 0)
        .unwrap();
    assert_eq!(tracks.items.len(), 1);
    assert!(tracks.items[0].available);
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 5);
    assert!(requests[2].contains("&type=playlist&page=1&"));
    assert!(requests[3].starts_with("GET /users/4567/playlists/9?richTracks=true HTTP/1.1"));
    assert!(requests[4].starts_with("POST /tracks HTTP/1.1"));
}

#[test]
fn unified_oversized_track_section_does_not_discard_other_categories() {
    let mut wire = unified_wire();
    wire["result"]["tracks"] =
        json!({"total":100,"perPage":51,"results":vec![json!({"id":42});51]});
    let (service, server, listener) = search_fixture(vec![(200, wire.to_string())]);
    let result = service.search_all(&token(), "synthetic").unwrap();
    assert_eq!(
        result.tracks,
        Err(search_failure(
            MusicError::ResponseTooLarge,
            SearchParseStage::TrackLookup,
            200
        ))
    );
    assert_eq!(result.albums.unwrap().items.len(), 1);
    assert_eq!(result.artists.unwrap().items.len(), 1);
    assert_eq!(result.playlists.unwrap().items.len(), 1);
    assert_eq!(complete(server, listener).len(), 1);
}

#[test]
fn unified_unrecognized_envelopes_fail_but_explicit_empty_results_are_valid() {
    for body in [
        r#"{"result":{}}"#,
        r#"{"result":{"unexpected":"synthetic-private-person"}}"#,
    ] {
        let (service, server, listener) = search_fixture(vec![(200, body.into())]);
        assert_eq!(
            service.search_all(&token(), "synthetic"),
            Err(search_contract(SearchParseStage::Section, 200))
        );
        assert_eq!(complete(server, listener).len(), 1);
    }
    for body in [
        r#"{"result":{"text":"synthetic-private-query"}}"#,
        r#"{"result":{"tracks":{"total":0}}}"#,
    ] {
        let (service, server, listener) = search_fixture(vec![(200, body.into())]);
        let result = service.search_all(&token(), "synthetic").unwrap();
        assert!(result.tracks.unwrap().items.is_empty());
        assert!(result.albums.unwrap().items.is_empty());
        assert!(result.artists.unwrap().items.is_empty());
        assert!(result.playlists.unwrap().items.is_empty());
        assert_eq!(complete(server, listener).len(), 1);
    }
    let (service, server, listener) = search_fixture(vec![(
        200,
        r#"{"result":{"text":"valid"},"error":{"message":"synthetic-private-person"}}"#.into(),
    )]);
    assert_eq!(
        service.search_all(&token(), "synthetic"),
        Err(search_contract(SearchParseStage::Envelope, 200))
    );
    assert_eq!(complete(server, listener).len(), 1);
}

#[test]
fn playlist_search_retains_real_selector_and_tolerates_optional_display_metadata() {
    let body = r#"{"result":{"playlists":{"total":3,"perPage":20,"results":[{"owner":{"uid":4321,"name":{"ignored":true}},"kind":8,"title":false,"trackCount":{"unknown":true},"cover":false},{"uid":4567,"kind":9,"title":"Known empty","trackCount":0},{"uid":4567,"kind":10,"title":"ambiguous one","title":"ambiguous two","trackCount":5}]}}}"#;
    let (service, server, listener) = search_fixture(vec![(200, body.into())]);
    let result = service
        .search(&token(), "synthetic", SearchKind::Playlists, 0)
        .unwrap();
    assert_eq!(result.playlists.len(), 3);
    assert_eq!(result.playlists[0].id.owner, "4321");
    assert_eq!(result.playlists[0].id.kind, "8");
    assert_eq!(result.playlists[0].title, "Без названия");
    assert_eq!(result.playlists[0].track_count, None);
    assert_eq!(result.playlists[1].track_count, Some(0));
    assert_eq!(result.playlists[2].id.kind, "10");
    assert_eq!(result.playlists[2].title, "Без названия");
    assert_eq!(result.playlists[2].track_count, None);
    assert!(!result.has_more);
    assert!(result.tracks.is_empty());
    assert_redacted(result);
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains("&type=playlist&page=0&"));
}

#[test]
fn playlist_search_identity_failure_is_section_local_and_never_uses_personal_metadata() {
    for item in [
        r#"{"owner":{"uid":"invalid/private"},"kind":8}"#,
        r#"{"owner":{"uid":4321},"uid":4567,"kind":8}"#,
        r#"{"owner":[4321],"kind":8}"#,
        r#"{"kind":8,"title":"synthetic-private-person"}"#,
        r#"{"uid":4321,"kind":8,"kind":9}"#,
        r#"[4321,8]"#,
    ] {
        let mut wire = unified_wire();
        wire["result"]["tracks"] = json!({"total":0});
        let base = wire["result"].as_object_mut().unwrap();
        base.remove("playlists");
        let mut body = wire.to_string();
        body.pop();
        body.pop();
        body.push_str(&format!(
            r#", "playlists":{{"total":1,"results":[{item}]}}}}}}"#
        ));
        let (service, server, listener) = search_fixture(vec![(200, body)]);
        let result = service.search_all(&token(), "synthetic").unwrap();
        let error = result.playlists.as_ref().unwrap_err();
        assert!(matches!(
            error.diagnostic_stage(),
            Some("playlist-wire" | "playlist-normalization")
        ));
        assert_redacted(error);
        assert_eq!(result.albums.unwrap().items.len(), 1);
        assert_eq!(result.artists.unwrap().items.len(), 1);
        assert_eq!(complete(server, listener).len(), 1);
    }
}

#[test]
fn canonical_track_retains_only_verified_artist_navigation_without_rejecting_display_credits() {
    let (service, server, listener) = search_fixture(vec![(
        200,
        json!({"result":[{
            "id":"42", "title":"Synthetic linked track", "available":true,
            "artists":[
                {"id":11,"name":"Synthetic verified artist"},
                {"name":"Synthetic name-only credit"},
                {"id":{"unsupported":"display-only"},"name":"Synthetic malformed ID credit"},
                {"id":"../private-selector","name":"Synthetic unsafe ID credit"}
            ]
        }]})
        .to_string(),
    )]);
    let track = service.track(&token(), "42").unwrap();
    assert_eq!(track.artists.len(), 4);
    assert_eq!(track.artist_links.len(), 1);
    assert_eq!(track.artist_links[0].id, "11");
    assert_eq!(track.artist_links[0].name, "Synthetic verified artist");
    assert!(track.available);
    assert_redacted(track);
    let requests = complete(server, listener);
    assert_eq!(
        requests.len(),
        1,
        "artist links reuse canonical metadata, without extra requests"
    );
}
