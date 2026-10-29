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
        "synthetic-private-request",
    ] {
        assert!(!visible.contains(private));
    }
}

#[test]
fn real_endpoint_contracts_map_to_typed_models_without_transport_identity() {
    let compact = r#"{"id":"42","albumId":7}"#;
    let expanded = format!("{{\"result\":[{TRACK}]}}");
    let (service,server,listener)=fixture(vec![
        (200,ACCOUNT.into()),
        (200,format!("{{\"result\":{{\"library\":{{\"tracks\":[{compact}]}}}}}}")),(200,expanded.clone()),
        (200,r#"{"result":[{"owner":{"uid":1234},"kind":8,"title":"synthetic-private-playlist","trackCount":1}]}"#.into()),
        (200,format!("{{\"result\":{{\"tracks\":[{compact}]}}}}")),(200,expanded.clone()),
        (200,format!("{{\"result\":{{\"text\":\"query\",\"tracks\":{{\"total\":2,\"perPage\":1,\"results\":[{TRACK}]}}}}}}")),
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
    assert!(requests[10].starts_with("GET /albums/7/with-tracks HTTP/1.1"));
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
    let (service, server, listener) = fixture(vec![
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
    assert_eq!(requests.len(), 6);
    assert_eq!(requests[2].split("track-ids=").count() - 1, 50);
    assert!(!requests[1].contains("page="));
    assert!(requests[4].contains("&page=1&"));
}

#[test]
fn invalid_inputs_missing_records_and_schema_drift_never_create_release_content() {
    let (service, server, listener) = fixture(vec![
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
        Err(MusicError::InvalidResponse)
    );
    assert_eq!(complete(server, listener).len(), 4);
}

fn wire_fixture(wire: String, delay: Duration) -> Result<Account, MusicError> {
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
    let result = service.account(&token());
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
    let (service, server, listener) = fixture(vec![
        (200, r#"{"result":{"tracks":{"total":1,"results":[{"id":42,"title":"synthetic-private-title","artists":[{"name":"synthetic-private-artist"},{"id":12}]}]}}}"#.into()),
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
fn search_respects_reported_large_pages_and_stops_on_empty_without_inventing_a_catalog() {
    let tracks = (1..=201).map(|id| json!({"id":id})).collect::<Vec<_>>();
    let (service, server, listener) = fixture(vec![
        (
            200,
            json!({"result":{"tracks":{"total":202,"perPage":201,"results":tracks}}}).to_string(),
        ),
        (
            200,
            r#"{"result":{"tracks":{"total":202,"perPage":201,"results":[{"id":202}]}}}"#.into(),
        ),
        (
            200,
            r#"{"result":{"tracks":{"total":202,"perPage":201,"results":[]}}}"#.into(),
        ),
    ]);
    let token = token();
    let first = service
        .search(&token, "synthetic", SearchKind::Tracks, 0)
        .unwrap();
    assert_eq!(first.tracks.len(), 201);
    assert!(first.has_more);
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
    assert_eq!(complete(server, listener).len(), 3);
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
        r#"{"result":{"tracks":{"total":1,"results":[{"id":42,"artists":[{"name":"\u0000synthetic-private-person"}]}]}}}"#,
    ];
    let (service, server, listener) =
        fixture(bodies.iter().map(|body| (200, (*body).into())).collect());
    for (index, _) in bodies.iter().enumerate() {
        let kind = if matches!(index, 2 | 3) {
            SearchKind::Artists
        } else {
            SearchKind::Tracks
        };
        let error = service.search(&token(), "synthetic", kind, 0).unwrap_err();
        assert_eq!(error, MusicError::InvalidResponse);
        assert_eq!(error.category(), "response-contract");
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
