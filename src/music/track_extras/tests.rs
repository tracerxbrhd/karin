// SPDX-License-Identifier: GPL-3.0-or-later
//! Original lyric strings and local HTTP only; no account/content fixtures.

use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread::{self, JoinHandle},
};

const TOKEN: &str = "original-synthetic-extras-token";
const ACCOUNT: &str = r#"{"result":{"account":{"uid":1234,"displayName":"Original fixture"}}}"#;

struct Reply {
    status: u16,
    body: String,
    headers: Vec<(&'static str, &'static str)>,
}
impl Reply {
    fn new(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            body: body.into(),
            headers: Vec::new(),
        }
    }
    fn header(mut self, name: &'static str, value: &'static str) -> Self {
        self.headers.push((name, value));
        self
    }
}

fn fixture(
    replies: Vec<Reply>,
    clear_after: Option<usize>,
) -> (MusicService, AccessToken, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let service = MusicService::for_test(&format!("http://{}/", listener.local_addr().unwrap()));
    let invalidator = service.clone();
    let worker = thread::spawn(move || {
        let mut requests = Vec::new();
        for (index, reply) in replies.into_iter().enumerate() {
            let deadline = Instant::now() + Duration::from_secs(4);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => panic!("missing bounded original-extras request"),
                }
            };
            requests.push(read_request(&mut stream));
            if clear_after == Some(index) {
                invalidator.clear_session();
            }
            write!(
                stream,
                "HTTP/1.1 {} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n",
                reply.status,
                reply.body.len()
            )
            .unwrap();
            if !reply
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("Content-Type"))
            {
                write!(stream, "Content-Type: application/json\r\n").unwrap();
            }
            for (name, value) in reply.headers {
                write!(stream, "{name}: {value}\r\n").unwrap();
            }
            write!(stream, "\r\n{}", reply.body).unwrap();
        }
        requests
    });
    let token = AccessToken::for_test(TOKEN, Duration::from_secs(60));
    (service, token, worker)
}

fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
        assert_eq!(stream.read(&mut byte).unwrap(), 1);
        bytes.push(byte[0]);
        assert!(bytes.len() < 16 * 1024);
    }
    let headers = String::from_utf8(bytes.clone()).unwrap();
    let size = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(size <= 16 * 1024);
    let mut body = vec![0; size];
    stream.read_exact(&mut body).unwrap();
    bytes.extend_from_slice(&body);
    String::from_utf8(bytes).unwrap()
}

fn track() -> Track {
    Track {
        id: "1:77".into(),
        title: "Original & 100% track".into(),
        artists: vec!["Original artist".into()],
        artist_links: Vec::new(),
        album_id: Some("77".into()),
        album_title: Some("Fixture / Album".into()),
        duration_ms: Some(120_000),
        cover: None,
        explicit: false,
        available: true,
    }
}

fn normalized(id: u64) -> serde_json::Value {
    serde_json::json!({"id":id,"title":"Original normalized fixture","artists":[{"name":"Original artist"}],"available":true})
}

fn external_record() -> serde_json::Value {
    serde_json::json!({
        "trackName":"Original & 100% track","artistName":"Original artist","duration":120.0,
        "instrumental":false,"plainLyrics":"First original line\nSecond original line",
        "syncedLyrics":"[00:01.200]First original line\n[00:03.000]Second original line"
    })
}

fn authenticated(service: &MusicService, token: &AccessToken) {
    service.account(token).unwrap();
}

fn assert_music_request(request: &str) {
    assert!(
        request
            .to_ascii_lowercase()
            .contains(&format!("authorization: oauth {TOKEN}\r\n"))
    );
    assert!(
        !request
            .to_ascii_lowercase()
            .contains("x-yandex-music-client")
    );
    assert!(!request.lines().next().unwrap().contains(TOKEN));
}

#[test]
fn similar_uses_minimal_identity_then_one_batch_and_preserves_order_and_partial_rows() {
    let (service, token, server) = fixture(
        vec![
            Reply::new(200, ACCOUNT),
            Reply::new(
                200,
                serde_json::json!({"result":{"track":{"id":1},"similarTracks":[
                    {"id":3,"title":{"unsupported":true},"artists":null},
                    {"id":2,"albums":42}, {"id":3}, {"id":4}, null, {"id":"../bad"}, {"id":1}
                ]}})
                .to_string(),
            ),
            Reply::new(
                200,
                serde_json::json!({"result":[normalized(2),null,normalized(3)]}).to_string(),
            ),
        ],
        None,
    );
    authenticated(&service, &token);
    let result = service.similar_tracks(&token, "1:77").unwrap();
    assert_eq!(
        result
            .tracks
            .iter()
            .map(|track| track.id.as_str())
            .collect::<Vec<_>>(),
        ["3", "2"]
    );
    assert_eq!(result.omitted, 5);
    assert!(!format!("{result:?}").contains("Original"));
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[1].starts_with("GET /tracks/1/similar "));
    assert!(requests[2].starts_with("POST /tracks "));
    assert!(requests[2].ends_with("track-ids=3&track-ids=2&track-ids=4"));
    for request in requests {
        assert_music_request(&request);
    }
}

#[test]
fn similar_empty_and_wrong_source_and_error_envelopes_never_batch_lookup() {
    for (body, expected) in [
        (r#"{"result":{"track":{"id":1},"similarTracks":[]}}"#, None),
        (
            r#"{"result":{"track":{"id":2},"similarTracks":[{"id":3}]}}"#,
            Some(MusicError::InvalidResponse),
        ),
        (
            r#"{"error":{"private":"not diagnostic"},"result":{"track":{"id":1},"similarTracks":[]}}"#,
            Some(MusicError::InvalidResponse),
        ),
        (
            r#"{"result":{"similarTracks":[]}}"#,
            Some(MusicError::InvalidResponse),
        ),
    ] {
        let (service, token, server) =
            fixture(vec![Reply::new(200, ACCOUNT), Reply::new(200, body)], None);
        authenticated(&service, &token);
        let result = service.similar_tracks(&token, "1");
        match expected {
            Some(error) => assert_eq!(result, Err(error)),
            None => assert!(result.unwrap().tracks.is_empty()),
        }
        assert_eq!(server.join().unwrap().len(), 2);
    }
}

#[test]
fn similar_response_and_lookup_are_bounded_and_not_n_plus_one() {
    let hits: Vec<_> = (2..=61).map(|id| serde_json::json!({"id":id})).collect();
    let tracks: Vec<_> = (2..=51).map(normalized).collect();
    let (service, token, server) = fixture(
        vec![
            Reply::new(200, ACCOUNT),
            Reply::new(
                200,
                serde_json::json!({"result":{"track":{"id":1},"similarTracks":hits}}).to_string(),
            ),
            Reply::new(200, serde_json::json!({"result":tracks}).to_string()),
        ],
        None,
    );
    authenticated(&service, &token);
    let result = service.similar_tracks(&token, "1").unwrap();
    assert_eq!(result.tracks.len(), 50);
    assert_eq!(result.omitted, 10);
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[2].matches("track-ids=").count(), 50);

    let hits: Vec<_> = (2..=202).map(|id| serde_json::json!({"id":id})).collect();
    let (service, token, server) = fixture(
        vec![
            Reply::new(200, ACCOUNT),
            Reply::new(
                200,
                serde_json::json!({"result":{"track":{"id":1},"similarTracks":hits}}).to_string(),
            ),
        ],
        None,
    );
    authenticated(&service, &token);
    assert_eq!(
        service.similar_tracks(&token, "1"),
        Err(MusicError::ResponseTooLarge)
    );
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn all_extras_require_current_music_validation_before_any_lookup() {
    let (service, token, server) = fixture(Vec::new(), None);
    assert_eq!(
        service.similar_tracks(&token, "1"),
        Err(MusicError::Unauthenticated)
    );
    for request in [LyricsRequest::Yandex, LyricsRequest::External] {
        assert_eq!(
            service.lyrics(&token, &track(), request),
            Err(MusicError::Unauthenticated)
        );
    }
    assert!(server.join().unwrap().is_empty());
}

#[test]
fn similar_late_account_retirement_cannot_publish_or_start_batch() {
    let (service, token, server) = fixture(
        vec![
            Reply::new(200, ACCOUNT),
            Reply::new(
                200,
                r#"{"result":{"track":{"id":1},"similarTracks":[{"id":2}]}}"#,
            ),
        ],
        Some(1),
    );
    authenticated(&service, &token);
    assert_eq!(
        service.similar_tracks(&token, "1"),
        Err(MusicError::Unauthenticated)
    );
    assert_eq!(server.join().unwrap().len(), 2);

    let (service, token, server) = fixture(
        vec![
            Reply::new(200, ACCOUNT),
            Reply::new(
                200,
                r#"{"result":{"track":{"id":1},"similarTracks":[{"id":2}]}}"#,
            ),
            Reply::new(
                200,
                serde_json::json!({"result":[normalized(2)]}).to_string(),
            ),
        ],
        Some(2),
    );
    authenticated(&service, &token);
    assert_eq!(
        service.similar_tracks(&token, "1"),
        Err(MusicError::Unauthenticated)
    );
    assert_eq!(server.join().unwrap().len(), 3);
}

#[test]
fn yandex_full_text_requires_explicit_rights_and_never_forwards_to_external_source() {
    let body=serde_json::json!({"result":{"lyrics":{"fullLyrics":"Original complete line\nOriginal second line","lyrics":"Opening excerpt only","hasRights":true}}}).to_string();
    let (service, token, server) =
        fixture(vec![Reply::new(200, ACCOUNT), Reply::new(200, body)], None);
    authenticated(&service, &token);
    let lyrics = service
        .lyrics(&token, &track(), LyricsRequest::Yandex)
        .unwrap()
        .unwrap();
    assert_eq!(lyrics.source, LyricsSource::YandexSupplement);
    assert_eq!(lyrics.plain, "Original complete line\nOriginal second line");
    assert!(lyrics.synchronized.is_empty());
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].starts_with("GET /tracks/1/supplement "));
    for request in requests {
        assert_music_request(&request);
    }
}

#[test]
fn missing_refused_and_excerpt_only_yandex_lyrics_are_not_full_text() {
    for body in [
        r#"{"result":{"lyrics":null}}"#,
        r#"{"result":{"lyrics":{"fullLyrics":"not permitted","hasRights":false}}}"#,
        r#"{"result":{"lyrics":{"fullLyrics":"unknown permission"}}}"#,
        r#"{"result":{"lyrics":{"lyrics":"Only opening excerpt","hasRights":true}}}"#,
        r#"{"result":null}"#,
    ] {
        let (service, token, server) =
            fixture(vec![Reply::new(200, ACCOUNT), Reply::new(200, body)], None);
        authenticated(&service, &token);
        assert_eq!(
            service.lyrics(&token, &track(), LyricsRequest::Yandex),
            Ok(None)
        );
        assert_eq!(server.join().unwrap().len(), 2);
    }
    let (service, token, server) = fixture(
        vec![Reply::new(200, ACCOUNT), Reply::new(404, "not found")],
        None,
    );
    authenticated(&service, &token);
    assert_eq!(
        service.lyrics(&token, &track(), LyricsRequest::Yandex),
        Ok(None)
    );
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn public_external_lrc_uses_only_disclosed_metadata_no_authorization_cookies_or_ids() {
    let (service, token, server) = fixture(
        vec![
            Reply::new(200, ACCOUNT),
            Reply::new(200, external_record().to_string()),
        ],
        None,
    );
    authenticated(&service, &token);
    let lyrics = service
        .lyrics(&token, &track(), LyricsRequest::External)
        .unwrap()
        .unwrap();
    assert_eq!(lyrics.source, LyricsSource::Lrclib);
    assert_eq!(lyrics.active_line(1500), Some(0));
    assert_eq!(lyrics.active_line(4000), Some(1));
    assert_eq!(lyrics.active_line(0), None);
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 2);
    let external = requests[1].to_ascii_lowercase();
    for forbidden in [
        "authorization:",
        "cookie:",
        "track-id",
        "account",
        "client_secret",
        TOKEN,
        "x-yandex-music-client",
    ] {
        assert!(
            !external.contains(forbidden),
            "fixed forbidden header/category"
        );
    }
    assert!(requests[1].contains(EXTERNAL_USER_AGENT));
    let path = requests[1]
        .lines()
        .next()
        .unwrap()
        .split_ascii_whitespace()
        .nth(1)
        .unwrap();
    let url = Url::parse(&format!("http://127.0.0.1{path}")).unwrap();
    assert_eq!(url.path(), "/external-lyrics");
    let pairs: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
    assert_eq!(pairs.len(), 4);
    assert_eq!(pairs["track_name"], "Original & 100% track");
    assert_eq!(pairs["artist_name"], "Original artist");
    assert_eq!(pairs["album_name"], "Fixture / Album");
    assert_eq!(pairs["duration"], "120.000");
    assert!(!format!("{lyrics:?}").contains("First original"));
}

#[test]
fn external_not_found_instrumental_and_mismatched_metadata_are_distinct() {
    let mut mismatch = external_record();
    mismatch["trackName"] = "Unrelated fixture".into();
    let mut duration = external_record();
    duration["duration"] = 180.into();
    for body in [mismatch, duration] {
        let (service, token, server) = fixture(
            vec![Reply::new(200, ACCOUNT), Reply::new(200, body.to_string())],
            None,
        );
        authenticated(&service, &token);
        assert_eq!(
            service.lyrics(&token, &track(), LyricsRequest::External),
            Err(MusicError::InvalidResponse)
        );
        assert_eq!(server.join().unwrap().len(), 2);
    }
    let mut instrumental = external_record();
    instrumental["instrumental"] = true.into();
    for reply in [
        Reply::new(404, "not found"),
        Reply::new(200, instrumental.to_string()),
    ] {
        let (service, token, server) = fixture(vec![Reply::new(200, ACCOUNT), reply], None);
        authenticated(&service, &token);
        assert_eq!(
            service.lyrics(&token, &track(), LyricsRequest::External),
            Ok(None)
        );
        assert_eq!(server.join().unwrap().len(), 2);
    }
}

#[test]
fn malformed_synchronization_preserves_real_plain_text_without_fake_karaoke() {
    let mut body = external_record();
    body["syncedLyrics"] = "[00:99]Malformed original".into();
    let (service, token, server) = fixture(
        vec![Reply::new(200, ACCOUNT), Reply::new(200, body.to_string())],
        None,
    );
    authenticated(&service, &token);
    let result = service
        .lyrics(&token, &track(), LyricsRequest::External)
        .unwrap()
        .unwrap();
    assert_eq!(result.plain, "First original line\nSecond original line");
    assert!(result.synchronized.is_empty());
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn external_throttle_and_retry_after_are_shared_across_clones_without_replay() {
    let (service, token, server) = fixture(
        vec![
            Reply::new(200, ACCOUNT),
            Reply::new(429, "not logged").header("Retry-After", "600"),
        ],
        None,
    );
    authenticated(&service, &token);
    let before = Instant::now();
    assert_eq!(
        service.lyrics(&token, &track(), LyricsRequest::External),
        Err(MusicError::RateLimited)
    );
    let clone = service.clone();
    assert_eq!(
        clone.lyrics(&token, &track(), LyricsRequest::External),
        Err(MusicError::RateLimited)
    );
    assert!(
        service
            .external_lyrics
            .lock()
            .unwrap()
            .next_allowed
            .unwrap()
            >= before + Duration::from_secs(600)
    );
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn external_single_flight_gate_returns_without_duplicate_network_work() {
    let (service, token, server) = fixture(vec![Reply::new(200, ACCOUNT)], None);
    authenticated(&service, &token);
    let _guard = service.external_lyrics.lock().unwrap();
    assert_eq!(
        service.lyrics(&token, &track(), LyricsRequest::External),
        Err(MusicError::RateLimited)
    );
    assert_eq!(server.join().unwrap().len(), 1);
}

#[test]
fn external_redirects_wrong_mime_and_late_logout_cannot_publish() {
    for (reply, expected) in [
        (
            Reply::new(302, "redirect").header("Location", "https://untrusted.invalid/private"),
            MusicError::Http(302),
        ),
        (
            Reply::new(200, "<html>not lyrics</html>").header("Content-Type", "text/html"),
            MusicError::InvalidResponse,
        ),
    ] {
        let (service, token, server) = fixture(vec![Reply::new(200, ACCOUNT), reply], None);
        authenticated(&service, &token);
        assert_eq!(
            service.lyrics(&token, &track(), LyricsRequest::External),
            Err(expected)
        );
        assert_eq!(server.join().unwrap().len(), 2);
    }
    let (service, token, server) = fixture(
        vec![
            Reply::new(200, ACCOUNT),
            Reply::new(200, external_record().to_string()),
        ],
        Some(1),
    );
    authenticated(&service, &token);
    assert_eq!(
        service.lyrics(&token, &track(), LyricsRequest::External),
        Err(MusicError::Unauthenticated)
    );
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn yandex_errors_and_transport_failure_remain_safe_fixed_categories() {
    for (status, error) in [
        (401, MusicError::AccessDenied),
        (403, MusicError::Forbidden),
        (429, MusicError::RateLimited),
        (451, MusicError::Restricted),
        (500, MusicError::Http(500)),
    ] {
        let (service, token, server) = fixture(
            vec![
                Reply::new(200, ACCOUNT),
                Reply::new(status, r#"{"error":{"secret":"private lyric URL and ID"}}"#),
            ],
            None,
        );
        authenticated(&service, &token);
        let actual = service
            .lyrics(&token, &track(), LyricsRequest::Yandex)
            .unwrap_err();
        assert_eq!(actual, error);
        assert!(!format!("{actual:?} {actual}").contains("private"));
        assert_eq!(server.join().unwrap().len(), 2);
    }
    let (service, token, server) = fixture(vec![Reply::new(200, ACCOUNT)], None);
    authenticated(&service, &token);
    assert_eq!(server.join().unwrap().len(), 1); // listener is now closed
    assert_eq!(
        service.similar_tracks(&token, "1"),
        Err(MusicError::Network)
    );
}

#[test]
fn oversized_and_control_bearing_text_are_rejected_without_logging_contents() {
    for (text, expected) in [
        ("original\0invalid".to_owned(), MusicError::InvalidResponse),
        (
            "o".repeat(crate::lyrics::MAX_LYRICS_BYTES + 1),
            MusicError::ResponseTooLarge,
        ),
    ] {
        let body = serde_json::json!({"result":{"lyrics":{"fullLyrics":text,"hasRights":true}}})
            .to_string();
        let (service, token, server) =
            fixture(vec![Reply::new(200, ACCOUNT), Reply::new(200, body)], None);
        authenticated(&service, &token);
        assert_eq!(
            service.lyrics(&token, &track(), LyricsRequest::Yandex),
            Err(expected)
        );
        assert_eq!(server.join().unwrap().len(), 2);
    }
}

#[test]
fn retry_after_delta_and_http_date_deadlines_are_not_shortened() {
    assert_eq!(retry_after("600", UNIX_EPOCH), Some(600));
    assert_eq!(
        retry_after("Thu, 01 Jan 1970 00:01:00 GMT", UNIX_EPOCH),
        Some(60)
    );
    assert_eq!(
        retry_after(
            "Thu, 01 Jan 1970 00:01:00 GMT",
            UNIX_EPOCH + Duration::from_millis(500)
        ),
        Some(60)
    );
    assert_eq!(
        retry_after(
            "Thu, 01 Jan 1970 00:01:00 GMT",
            UNIX_EPOCH + Duration::from_secs(90)
        ),
        Some(0)
    );
    assert_eq!(
        retry_after("Tue, 29 Feb 2000 00:00:00 GMT", UNIX_EPOCH),
        Some(951_782_400)
    );
    assert_eq!(
        retry_after("Thu, 29 Feb 2001 00:00:00 GMT", UNIX_EPOCH),
        None
    );
    assert_eq!(retry_after("1 secret second", UNIX_EPOCH), None);
}
