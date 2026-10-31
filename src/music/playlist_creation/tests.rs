// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic loopback-only creation and ambiguity regressions.

use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const TOKEN: &str = "synthetic-private-create-token";
const ACCOUNT: &str = r#"{"result":{"account":{"uid":1234,"displayName":"synthetic person"}}}"#;

fn token() -> AccessToken {
    AccessToken::for_test(TOKEN, Duration::from_secs(60))
}

fn playlist(owner: u64, kind: u64, title: &str, count: u64) -> String {
    serde_json::json!({"result":{"owner":{"uid":owner},"kind":kind,"title":title,"trackCount":count,"visibility":"private"}})
        .to_string()
}

fn fixture(
    responses: Vec<(u16, String)>,
    clear_after: Option<usize>,
) -> (MusicService, JoinHandle<Vec<String>>, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let service = MusicService::for_test(&format!("http://{}/", listener.local_addr().unwrap()));
    let retained = listener.try_clone().unwrap();
    let invalidator = service.clone();
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        for (index, (status, body)) in responses.into_iter().enumerate() {
            let deadline = Instant::now() + Duration::from_secs(4);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => panic!("missing synthetic playlist-creation request"),
                }
            };
            requests.push(read_request(&mut stream));
            if clear_after == Some(index) {
                invalidator.clear_session();
            }
            write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        requests
    });
    (service, server, retained)
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
    let header = String::from_utf8(bytes.clone()).unwrap();
    let size = header
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(size <= 4096);
    let mut body = vec![0; size];
    stream.read_exact(&mut body).unwrap();
    bytes.extend_from_slice(&body);
    String::from_utf8(bytes).unwrap()
}

fn complete(server: JoinHandle<Vec<String>>, listener: TcpListener) -> Vec<String> {
    let requests = server.join().unwrap();
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    for request in &requests {
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
        assert!(!request.contains("client_secret"));
        assert!(!request.lines().next().unwrap().contains(TOKEN));
    }
    requests
}

#[test]
fn create_is_account_bound_form_encoded_and_independently_confirmed() {
    for visibility in [PlaylistVisibility::Private, PlaylistVisibility::Public] {
        let title = "Мой список & друзья + 1";
        let mut body: serde_json::Value =
            serde_json::from_str(&playlist(1234, 17, title, 0)).unwrap();
        body["result"]["visibility"] = visibility.as_str().into();
        let body = body.to_string();
        let (service, server, listener) = fixture(
            vec![(200, ACCOUNT.into()), (200, body.clone()), (200, body)],
            None,
        );
        let token = token();
        service.account(&token).unwrap();
        let created = service
            .create_playlist(&token, &format!("  {title}  "), visibility)
            .unwrap();
        assert_eq!(created.id.owner, "1234");
        assert_eq!(created.id.kind, "17");
        assert_eq!(created.title, title);
        let requests = complete(server, listener);
        assert!(requests[1].starts_with("POST /users/1234/playlists/create HTTP/1.1"));
        assert!(requests[1].contains("application/x-www-form-urlencoded"));
        let form = requests[1].split("\r\n\r\n").nth(1).unwrap();
        let pairs: Vec<_> = reqwest::Url::parse(&format!("http://localhost/?{form}"))
            .unwrap()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("title".into(), title.into()),
                ("visibility".into(), visibility.as_str().into())
            ]
        );
        assert!(requests[2].starts_with("GET /users/1234/playlists/17 HTTP/1.1"));
    }
}

#[test]
fn invalid_title_and_unvalidated_or_expired_session_never_write() {
    let (service, server, listener) = fixture(vec![], None);
    let token = token();
    for title in [
        "",
        "   ",
        "name\nsecret",
        "name\0",
        &"a".repeat(201),
        &"😀".repeat(201),
    ] {
        assert_eq!(
            service.create_playlist(&token, title, PlaylistVisibility::Private),
            Err(InteractionError::Music(MusicError::InvalidInput))
        );
    }
    assert_eq!(
        service.create_playlist(&token, "valid", PlaylistVisibility::Private),
        Err(InteractionError::Music(MusicError::Unauthenticated))
    );
    assert!(complete(server, listener).is_empty());
    let (service, server, listener) = fixture(vec![(200, ACCOUNT.into())], None);
    service.account(&token).unwrap();
    token.invalidate();
    assert_eq!(
        service.create_playlist(&token, "valid", PlaylistVisibility::Private),
        Err(InteractionError::Music(MusicError::Unauthenticated))
    );
    assert_eq!(complete(server, listener).len(), 1);
}

#[test]
fn ambiguous_write_is_not_replayed_or_misreported_as_success() {
    for (status, body) in [
        (500, "private failure".into()),
        (302, "private redirect".into()),
        (200, "private malformed response".into()),
        (200, r#"{"result":{"owner":{"uid":1234},"kind":17}}"#.into()),
        (200, playlist(5678, 17, "name", 0)),
        (200, playlist(1234, 17, "different", 0)),
        (200, {
            let mut value: serde_json::Value =
                serde_json::from_str(&playlist(1234, 17, "name", 0)).unwrap();
            value["error"] = serde_json::json!({"message":"private provider failure"});
            value.to_string()
        }),
    ] {
        let (service, server, listener) =
            fixture(vec![(200, ACCOUNT.into()), (status, body)], None);
        let token = token();
        service.account(&token).unwrap();
        assert_eq!(
            service.create_playlist(&token, "name", PlaylistVisibility::Private),
            Err(InteractionError::WriteOutcomeUnknown)
        );
        assert_eq!(complete(server, listener).len(), 2);
    }
}

#[test]
fn rejected_write_preserves_safe_error_without_retry() {
    for (status, expected) in [
        (401, InteractionError::Music(MusicError::AccessDenied)),
        (403, InteractionError::Music(MusicError::Forbidden)),
        (409, InteractionError::Conflict),
        (429, InteractionError::Music(MusicError::RateLimited)),
    ] {
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (status, "synthetic-private-provider-response".into()),
            ],
            None,
        );
        let token = token();
        service.account(&token).unwrap();
        let err = service
            .create_playlist(
                &token,
                "synthetic-private-title",
                PlaylistVisibility::Private,
            )
            .unwrap_err();
        assert_eq!(err, expected);
        let diagnostic = format!("{err:?} {err}");
        for value in [
            TOKEN,
            "1234",
            "synthetic-private-title",
            "synthetic-private-provider-response",
        ] {
            assert!(!diagnostic.contains(value));
        }
        assert_eq!(complete(server, listener).len(), 2);
    }
}

#[test]
fn failed_or_mismatched_confirmation_never_replays_the_creation() {
    for (status, body, expected) in [
        (
            429,
            "private limit".into(),
            InteractionError::ConfirmationUnavailable,
        ),
        (
            200,
            "private invalid".into(),
            InteractionError::ConfirmationUnavailable,
        ),
        (
            200,
            playlist(5678, 17, "name", 0),
            InteractionError::ConfirmationMismatch,
        ),
        (
            200,
            playlist(1234, 18, "name", 0),
            InteractionError::ConfirmationMismatch,
        ),
        (
            200,
            playlist(1234, 17, "other", 0),
            InteractionError::ConfirmationMismatch,
        ),
        (
            401,
            "private rejection".into(),
            InteractionError::Music(MusicError::AccessDenied),
        ),
        (
            403,
            "private permission refusal".into(),
            InteractionError::Music(MusicError::Forbidden),
        ),
        (
            451,
            "private service restriction".into(),
            InteractionError::Music(MusicError::Restricted),
        ),
    ] {
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, playlist(1234, 17, "name", 0)),
                (status, body),
            ],
            None,
        );
        let token = token();
        service.account(&token).unwrap();
        assert_eq!(
            service.create_playlist(&token, "name", PlaylistVisibility::Private),
            Err(expected)
        );
        assert_eq!(complete(server, listener).len(), 3);
    }
}

#[test]
fn logout_during_post_or_confirmation_discards_late_results() {
    for clear_after in [1, 2] {
        let mut responses = vec![(200, ACCOUNT.into()), (200, playlist(1234, 17, "name", 0))];
        if clear_after == 2 {
            responses.push((200, playlist(1234, 17, "name", 0)));
        }
        let (service, server, listener) = fixture(responses, Some(clear_after));
        let token = token();
        service.account(&token).unwrap();
        assert_eq!(
            service.create_playlist(&token, "name", PlaylistVisibility::Private),
            Err(InteractionError::Music(MusicError::Unauthenticated))
        );
        assert_eq!(complete(server, listener).len(), clear_after + 1);
    }
}

#[test]
fn unknown_or_changed_visibility_is_not_reported_as_confirmed_private() {
    for visibility in [serde_json::Value::Null, serde_json::json!("public")] {
        let mut confirmation: serde_json::Value =
            serde_json::from_str(&playlist(1234, 17, "name", 0)).unwrap();
        confirmation["result"]["visibility"] = visibility.clone();
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, playlist(1234, 17, "name", 0)),
                (200, confirmation.to_string()),
            ],
            None,
        );
        let token = token();
        service.account(&token).unwrap();
        let expected = if visibility.is_null() {
            InteractionError::ConfirmationUnavailable
        } else {
            InteractionError::ConfirmationMismatch
        };
        assert_eq!(
            service.create_playlist(&token, "name", PlaylistVisibility::Private),
            Err(expected)
        );
        assert_eq!(complete(server, listener).len(), 3);
    }
}
