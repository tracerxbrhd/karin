// SPDX-License-Identifier: GPL-3.0-or-later
//! Loopback-only interaction fixtures. No real account or service request.

use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const TOKEN: &str = "synthetic-interaction-private-token";
const ACCOUNT: &str =
    r#"{"result":{"account":{"uid":1234,"displayName":"synthetic-private-person"}}}"#;

fn token() -> AccessToken {
    AccessToken::for_test(TOKEN, Duration::from_secs(60))
}
fn membership(uid: u64, revision: u64, liked: bool) -> String {
    let tracks = if liked {
        serde_json::json!([{"id":42,"albumId":7}])
    } else {
        serde_json::json!([])
    };
    serde_json::json!({"result":{"library":{"uid":uid,"revision":revision,"tracks":tracks}}})
        .to_string()
}

fn fixture(responses: Vec<(u16, String)>) -> (MusicService, JoinHandle<Vec<String>>, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let service = MusicService::for_test(&format!("http://{}/", listener.local_addr().unwrap()));
    let retained = listener.try_clone().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in responses {
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
                    Err(_) => panic!("missing synthetic interaction request"),
                }
            };
            requests.push(read_request(&mut stream));
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
    assert!(size < 1024);
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
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains(&format!("authorization: oauth {TOKEN}\r\n")));
        assert!(!lower.contains("x-yandex-music-client"));
        assert!(!lower.contains("client_secret"));
        assert!(!request.lines().next().unwrap().contains(TOKEN));
    }
    requests
}

#[test]
fn like_and_unlike_are_account_bound_and_read_after_write_confirmed() {
    let (service, server, listener) = fixture(vec![
        (200, ACCOUNT.into()),
        (200, membership(1234, 3, false)),
        (200, r#"{"result":{"revision":4}}"#.into()),
        (200, membership(1234, 4, true)),
        (200, r#"{"result":{"revision":5}}"#.into()),
        (200, membership(1234, 5, false)),
    ]);
    let token = token();
    service.account(&token).unwrap();
    assert!(!service.is_track_liked(&token, "42:7").unwrap());
    assert_eq!(
        service.set_track_liked(&token, "42:7", true),
        Ok(LikeState { liked: true })
    );
    assert_eq!(
        service.set_track_liked(&token, "42:7", false),
        Ok(LikeState { liked: false })
    );
    let requests = complete(server, listener);
    assert!(requests[2].starts_with("POST /users/1234/likes/tracks/add-multiple HTTP/1.1"));
    assert!(requests[2].ends_with("track-ids=42%3A7"));
    assert!(requests[4].starts_with("POST /users/1234/likes/tracks/remove HTTP/1.1"));
    for index in [1, 3, 5] {
        assert!(
            requests[index]
                .starts_with("GET /users/1234/likes/tracks?if-modified-since-revision=0 HTTP/1.1")
        );
    }
}

#[test]
fn write_conflict_is_visible_without_retry_or_confirmation_request() {
    let (service, server, listener) = fixture(vec![
        (200, ACCOUNT.into()),
        (
            409,
            r#"{"error":{"message":"synthetic-private-person"}}"#.into(),
        ),
    ]);
    let token = token();
    service.account(&token).unwrap();
    assert_eq!(
        service.set_track_liked(&token, "42", true),
        Err(InteractionError::Conflict)
    );
    assert_eq!(complete(server, listener).len(), 2);
}

#[test]
fn confirmation_refuses_wrong_account_old_revision_and_wrong_membership() {
    for response in [
        membership(5678, 4, true),
        membership(1234, 3, true),
        membership(1234, 4, false),
    ] {
        let (service, server, listener) = fixture(vec![
            (200, ACCOUNT.into()),
            (200, r#"{"result":{"revision":4}}"#.into()),
            (200, response.clone()),
        ]);
        let token = token();
        service.account(&token).unwrap();
        let expected = if response == membership(5678, 4, true) {
            InteractionError::ConfirmationUnavailable
        } else {
            InteractionError::ConfirmationMismatch
        };
        assert_eq!(service.set_track_liked(&token, "42", true), Err(expected));
        assert_eq!(complete(server, listener).len(), 3);
    }
}

#[test]
fn failed_confirmation_never_claims_write_did_not_happen_or_retries() {
    for (status, body) in [
        (429, "private-response"),
        (200, r#"{"result":{"library":null}}"#),
        (200, "unsupported-response"),
    ] {
        let (service, server, listener) = fixture(vec![
            (200, ACCOUNT.into()),
            (200, r#"{"result":{"revision":4}}"#.into()),
            (status, body.into()),
        ]);
        let token = token();
        service.account(&token).unwrap();
        assert_eq!(
            service.set_track_liked(&token, "42", true),
            Err(InteractionError::ConfirmationUnavailable)
        );
        assert_eq!(complete(server, listener).len(), 3);
    }
}

#[test]
fn membership_requires_account_and_valid_library_identity() {
    let (service, server, listener) = fixture(vec![
        (200, ACCOUNT.into()),
        (200, membership(5678, 1, true)),
    ]);
    let token = token();
    assert_eq!(
        service.is_track_liked(&token, "42"),
        Err(InteractionError::Music(MusicError::Unauthenticated))
    );
    assert_eq!(
        service.set_track_liked(&token, "42", true),
        Err(InteractionError::Music(MusicError::Unauthenticated))
    );
    service.account(&token).unwrap();
    assert_eq!(
        service.is_track_liked(&token, "42"),
        Err(InteractionError::Music(MusicError::InvalidResponse))
    );
    service.clear_session();
    assert_eq!(
        service.set_track_liked(&token, "42", true),
        Err(InteractionError::Music(MusicError::Unauthenticated))
    );
    assert_eq!(complete(server, listener).len(), 2);
}

#[test]
fn invalid_track_id_never_reaches_transport_and_errors_are_redacted() {
    let (service, server, listener) = fixture(vec![]);
    let token = token();
    for id in ["", "../private", "private?token=secret", "private\nfield"] {
        let error = service.set_track_liked(&token, id, true).unwrap_err();
        assert_eq!(error, InteractionError::Music(MusicError::InvalidInput));
        assert!(!format!("{error:?} {error}").contains("private"));
    }
    assert!(complete(server, listener).is_empty());
}

#[test]
fn liked_playlist_wrapper_preserves_real_owner_and_redacts_debug() {
    let body = r#"{"result":[{"timestamp":"synthetic-private-time","playlist":{"owner":{"uid":5678},"kind":9,"title":"synthetic-private-playlist","trackCount":2}}]}"#;
    let (service, server, listener) = fixture(vec![(200, ACCOUNT.into()), (200, body.into())]);
    let token = token();
    service.account(&token).unwrap();
    let page = service.liked_playlists(&token, 0).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id.owner, "5678");
    assert_eq!(page.items[0].id.kind, "9");
    assert!(!page.has_more);
    for sensitive in [
        TOKEN,
        "5678",
        "synthetic-private-playlist",
        "synthetic-private-time",
    ] {
        assert!(!format!("{page:?} {:?}", page.items[0]).contains(sensitive));
    }
    let requests = complete(server, listener);
    assert!(requests[1].starts_with("GET /users/1234/likes/playlists HTTP/1.1"));
}

#[test]
fn liked_playlists_empty_and_paging_use_bounded_local_pages() {
    let entries=(0..51).map(|index|format!(r#"{{"playlist":{{"uid":5678,"kind":{index},"title":"Synthetic fixture","trackCount":0}}}}"#)).collect::<Vec<_>>().join(",");
    let populated = format!(r#"{{"result":[{entries}]}}"#);
    let (service, server, listener) = fixture(vec![
        (200, ACCOUNT.into()),
        (200, populated.clone()),
        (200, populated),
        (200, r#"{"result":[]}"#.into()),
    ]);
    let token = token();
    service.account(&token).unwrap();
    let first = service.liked_playlists(&token, 0).unwrap();
    assert_eq!(first.items.len(), 50);
    assert!(first.has_more);
    let second = service.liked_playlists(&token, 1).unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(!second.has_more);
    let empty = service.liked_playlists(&token, 0).unwrap();
    assert!(empty.items.is_empty());
    assert!(!empty.has_more);
    assert_eq!(complete(server, listener).len(), 4);
}

#[test]
fn uncertain_write_never_claims_success_or_replays() {
    for (status, body) in [
        (200, "unsupported-schema"),
        (204, ""),
        (302, "synthetic-private-redirect"),
        (503, "synthetic-private-body"),
    ] {
        let (service, server, listener) =
            fixture(vec![(200, ACCOUNT.into()), (status, body.into())]);
        let token = token();
        service.account(&token).unwrap();
        assert_eq!(
            service.set_track_liked(&token, "42", true),
            Err(InteractionError::WriteOutcomeUnknown)
        );
        assert_eq!(complete(server, listener).len(), 2);
    }
}

#[test]
fn fixed_interaction_errors_do_not_retain_provider_values() {
    let private = "synthetic-private-body-token-account";
    let (service, server, listener) = fixture(vec![(200, ACCOUNT.into()), (403, private.into())]);
    let token = token();
    service.account(&token).unwrap();
    let error = service.set_track_liked(&token, "42", true).unwrap_err();
    assert_eq!(error, InteractionError::Music(MusicError::Forbidden));
    let diagnostic = format!("{error:?} {error}");
    for sensitive in [private, TOKEN, "1234", "42"] {
        assert!(!diagnostic.contains(sensitive));
    }
    assert_eq!(complete(server, listener).len(), 2);
}

#[test]
fn accepted_write_then_unauthorized_confirmation_preserves_session_rejection() {
    let private_body = "synthetic-private-confirmation-response-account-token";
    let (service, server, listener) = fixture(vec![
        (200, ACCOUNT.into()),
        (200, r#"{"result":{"revision":4}}"#.into()),
        (401, private_body.into()),
    ]);
    let token = token();
    service.account(&token).unwrap();
    let error = service.set_track_liked(&token, "42", true).unwrap_err();
    assert_eq!(error, InteractionError::Music(MusicError::AccessDenied));
    let diagnostic = format!("{error:?} {error}");
    for sensitive in [
        private_body,
        TOKEN,
        "1234",
        "42",
        "synthetic-private-person",
    ] {
        assert!(!diagnostic.contains(sensitive));
    }
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 3, "one write, one confirmation, no retry");
    assert!(requests[1].starts_with("POST /users/1234/likes/tracks/add-multiple HTTP/1.1"));
    assert!(
        requests[2]
            .starts_with("GET /users/1234/likes/tracks?if-modified-since-revision=0 HTTP/1.1")
    );
}

#[test]
fn confirmation_retains_only_session_rejection_and_other_outcomes_stay_ambiguous() {
    for error in [MusicError::AccessDenied, MusicError::Unauthenticated] {
        assert_eq!(confirmation_error(error), InteractionError::Music(error));
    }
    for error in [
        MusicError::Timeout,
        MusicError::Network,
        MusicError::InvalidResponse,
        MusicError::Forbidden,
        MusicError::Http(503),
    ] {
        assert_eq!(
            confirmation_error(error),
            InteractionError::ConfirmationUnavailable
        );
    }
}

#[test]
fn library_revision_matches_unsigned_integer_contract_without_guessing_missing_values() {
    for value in [0, 4, u64::MAX] {
        let revision: RevisionWire =
            serde_json::from_value(serde_json::json!({"revision":value})).unwrap();
        assert_eq!(revision.revision, value);
    }
    for value in [
        serde_json::json!({}),
        serde_json::json!({"revision":"4"}),
        serde_json::json!({"revision":-1}),
        serde_json::json!({"revision":4.5}),
    ] {
        assert!(serde_json::from_value::<RevisionWire>(value).is_err());
    }
}

#[test]
fn reactions_read_distinct_account_libraries_without_guessing_unknown_or_conflicting_state() {
    for (liked, disliked) in [(false, false), (true, false), (false, true), (true, true)] {
        let (service, server, listener) = fixture(vec![
            (200, ACCOUNT.into()),
            (200, membership(1234, 7, liked)),
            (200, membership(1234, 9, disliked)),
        ]);
        let token = token();
        service.account(&token).unwrap();
        let result = service.track_reactions(&token, "42:7");
        if liked && disliked {
            assert_eq!(result, Err(MusicError::InvalidResponse.into()));
        } else {
            assert_eq!(result, Ok(TrackReactions { liked, disliked }));
        }
        let requests = complete(server, listener);
        assert_eq!(requests.len(), 3);
        assert!(
            requests[1]
                .starts_with("GET /users/1234/likes/tracks?if-modified-since-revision=0 HTTP/1.1")
        );
        assert!(
            requests[2].starts_with(
                "GET /users/1234/dislikes/tracks?if_modified_since_revision=0 HTTP/1.1"
            )
        );
        assert!(requests.iter().all(|request| request.starts_with("GET ")));
    }
}

#[test]
fn dislike_and_undislike_are_distinct_from_unlike_and_confirm_both_marks() {
    let (service, server, listener) = fixture(vec![
        (200, ACCOUNT.into()),
        (200, membership(1234, 3, true)),
        (200, membership(1234, 2, false)),
        (200, r#"{"result":{"revision":4}}"#.into()),
        (200, membership(1234, 4, false)),
        (200, membership(1234, 4, true)),
        (200, r#"{"result":{"revision":5}}"#.into()),
        (200, membership(1234, 4, false)),
        (200, membership(1234, 5, false)),
    ]);
    let token = token();
    service.account(&token).unwrap();
    assert_eq!(
        service.track_reactions(&token, "42:7"),
        Ok(TrackReactions {
            liked: true,
            disliked: false
        })
    );
    assert_eq!(
        service.set_track_disliked(&token, "42:7", true),
        Ok(TrackReactions {
            liked: false,
            disliked: true
        })
    );
    assert_eq!(
        service.set_track_disliked(&token, "42:7", false),
        Ok(TrackReactions {
            liked: false,
            disliked: false
        })
    );
    let requests = complete(server, listener);
    assert!(requests[3].starts_with("POST /users/1234/dislikes/tracks/add-multiple HTTP/1.1"));
    assert!(requests[6].starts_with("POST /users/1234/dislikes/tracks/remove HTTP/1.1"));
    assert!(requests[3].ends_with("track-ids=42%3A7"));
    assert!(requests[6].ends_with("track-ids=42%3A7"));
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST "))
            .count(),
        2
    );
    assert!(
        requests
            .iter()
            .all(|request| !request.contains("/rotor/") && !request.contains("/download-info"))
    );
}

#[test]
fn adding_like_confirms_server_removed_dislike_and_unlike_does_not_invent_dislike() {
    let (service, server, listener) = fixture(vec![
        (200, ACCOUNT.into()),
        (200, r#"{"result":{"revision":7}}"#.into()),
        (200, membership(1234, 7, true)),
        (200, membership(1234, 6, false)),
        (200, r#"{"result":{"revision":8}}"#.into()),
        (200, membership(1234, 8, false)),
        (200, membership(1234, 6, false)),
    ]);
    let token = token();
    service.account(&token).unwrap();
    assert_eq!(
        service.set_track_reaction(&token, "42", TrackReaction::Like, true),
        Ok(TrackReactions {
            liked: true,
            disliked: false
        })
    );
    assert_eq!(
        service.set_track_reaction(&token, "42", TrackReaction::Like, false),
        Ok(TrackReactions {
            liked: false,
            disliked: false
        })
    );
    let requests = complete(server, listener);
    assert!(requests[1].starts_with("POST /users/1234/likes/tracks/add-multiple HTTP/1.1"));
    assert!(requests[4].starts_with("POST /users/1234/likes/tracks/remove HTTP/1.1"));
}

#[test]
fn reaction_confirmation_requires_requested_state_revision_and_opposite_mark_cleared() {
    for (reaction, enabled, like_revision, liked, dislike_revision, disliked) in [
        (TrackReaction::Dislike, true, 8, true, 8, true),
        (TrackReaction::Like, true, 8, true, 8, true),
        (TrackReaction::Dislike, true, 8, false, 7, true),
        (TrackReaction::Like, true, 7, true, 8, false),
        (TrackReaction::Dislike, true, 8, false, 8, false),
        (TrackReaction::Dislike, false, 8, false, 8, true),
        (TrackReaction::Like, false, 8, true, 8, false),
    ] {
        let (service, server, listener) = fixture(vec![
            (200, ACCOUNT.into()),
            (200, r#"{"result":{"revision":8}}"#.into()),
            (200, membership(1234, like_revision, liked)),
            (200, membership(1234, dislike_revision, disliked)),
        ]);
        let token = token();
        service.account(&token).unwrap();
        assert_eq!(
            service.set_track_reaction(&token, "42", reaction, enabled),
            Err(InteractionError::ConfirmationMismatch)
        );
        let requests = complete(server, listener);
        assert_eq!(requests.len(), 4);
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.starts_with("POST "))
                .count(),
            1
        );
    }
}

#[test]
fn removing_dislike_returns_observed_like_without_inventing_a_restore() {
    let (service, server, listener) = fixture(vec![
        (200, ACCOUNT.into()),
        (200, r#"{"result":{"revision":8}}"#.into()),
        (200, membership(1234, 9, true)),
        (200, membership(1234, 8, false)),
    ]);
    let token = token();
    service.account(&token).unwrap();
    assert_eq!(
        service.set_track_disliked(&token, "42", false),
        Ok(TrackReactions {
            liked: true,
            disliked: false
        })
    );
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST "))
            .count(),
        1
    );
    assert!(requests[1].starts_with("POST /users/1234/dislikes/tracks/remove HTTP/1.1"));
}

#[test]
fn dislike_explicit_rejections_do_not_confirm_or_retry_the_write() {
    for (status, error) in [
        (400, InteractionError::Music(MusicError::Http(400))),
        (401, InteractionError::Music(MusicError::AccessDenied)),
        (403, InteractionError::Music(MusicError::Forbidden)),
        (409, InteractionError::Conflict),
        (429, InteractionError::Music(MusicError::RateLimited)),
        (451, InteractionError::Music(MusicError::Restricted)),
    ] {
        let (service, server, listener) = fixture(vec![
            (200, ACCOUNT.into()),
            (status, "synthetic-private-dislike-body".into()),
        ]);
        let token = token();
        service.account(&token).unwrap();
        assert_eq!(service.set_track_disliked(&token, "42", true), Err(error));
        assert_eq!(complete(server, listener).len(), 2);
        assert!(!format!("{error} {error:?}").contains("synthetic-private"));
    }
}

#[test]
fn uncertain_dislike_acknowledgment_never_replays_or_guesses_membership() {
    for (status, body) in [
        (200, "private unsupported response"),
        (200, r#"{"result":{}}"#),
        (200, r#"{"result":{"revision":"8"}}"#),
        (200, r#"{"result":{"revision":-1}}"#),
        (204, ""),
        (302, "private redirect"),
        (503, "private server body"),
    ] {
        let (service, server, listener) =
            fixture(vec![(200, ACCOUNT.into()), (status, body.into())]);
        let token = token();
        service.account(&token).unwrap();
        assert_eq!(
            service.set_track_disliked(&token, "42", true),
            Err(InteractionError::WriteOutcomeUnknown)
        );
        assert_eq!(complete(server, listener).len(), 2);
    }
}

#[test]
fn reaction_confirmation_keeps_authorization_rejections_and_never_uses_cache() {
    for (status, body, expected) in [
        (
            401,
            "private",
            InteractionError::Music(MusicError::AccessDenied),
        ),
        (403, "private", InteractionError::ConfirmationUnavailable),
        (429, "private", InteractionError::ConfirmationUnavailable),
        (
            200,
            r#"{"result":{"library":null}}"#,
            InteractionError::ConfirmationUnavailable,
        ),
    ] {
        let (service, server, listener) = fixture(vec![
            (200, ACCOUNT.into()),
            (200, r#"{"result":{"revision":8}}"#.into()),
            (200, membership(1234, 8, false)),
            (status, body.into()),
        ]);
        let token = token();
        service.account(&token).unwrap();
        assert_eq!(
            service.set_track_disliked(&token, "42", true),
            Err(expected)
        );
        assert_eq!(complete(server, listener).len(), 4);
    }
}

#[test]
fn reaction_reads_reject_missing_foreign_or_oversized_library_without_claiming_false() {
    let oversized = serde_json::json!({"result":{"library":{"uid":1234,"revision":8,"tracks":vec![serde_json::json!({"id":42}); MAX_COLLECTION+1]}}}).to_string();
    for body in [
        r#"{"result":{"library":null}}"#.into(),
        membership(5678, 8, false),
        oversized,
    ] {
        let expected = if body.len() > 100_000 {
            MusicError::ResponseTooLarge
        } else {
            MusicError::InvalidResponse
        };
        let (service, server, listener) = fixture(vec![
            (200, ACCOUNT.into()),
            (200, membership(1234, 8, false)),
            (200, body),
        ]);
        let token = token();
        service.account(&token).unwrap();
        assert_eq!(service.track_reactions(&token, "42"), Err(expected.into()));
        assert_eq!(complete(server, listener).len(), 3);
    }
}

#[test]
fn reactions_validate_identity_and_music_account_before_network() {
    let (service, server, listener) = fixture(vec![]);
    let token = token();
    for id in ["", "../private", "private?token=secret", "private\nfield"] {
        assert_eq!(
            service.track_reactions(&token, id),
            Err(MusicError::InvalidInput.into())
        );
        assert_eq!(
            service.set_track_disliked(&token, id, true),
            Err(MusicError::InvalidInput.into())
        );
    }
    assert_eq!(
        service.track_reactions(&token, "42"),
        Err(MusicError::Unauthenticated.into())
    );
    assert_eq!(
        service.set_track_disliked(&token, "42", true),
        Err(MusicError::Unauthenticated.into())
    );
    assert!(complete(server, listener).is_empty());
}

fn next_request(listener: &TcpListener) -> (TcpStream, String) {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let request = read_request(&mut stream);
                return (stream, request);
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(5))
            }
            Err(_) => panic!("missing bounded reaction fixture request"),
        }
    }
}
fn send_response(stream: &mut TcpStream, body: &str) {
    write!(stream, "HTTP/1.1 200 Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
}

#[test]
fn reaction_late_reads_and_write_acks_cannot_cross_logout_same_account_revalidation() {
    for write in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let service =
            MusicService::for_test(&format!("http://{}/", listener.local_addr().unwrap()));
        let retained = listener.try_clone().unwrap();
        let (started, wait_started) = std::sync::mpsc::channel();
        let (release, wait_release) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            let (mut account, request) = next_request(&listener);
            requests.push(request);
            send_response(&mut account, ACCOUNT);
            drop(account);
            let (mut pending, request) = next_request(&listener);
            requests.push(request);
            started.send(()).unwrap();
            let (mut revalidation, request) = next_request(&listener);
            requests.push(request);
            send_response(&mut revalidation, ACCOUNT);
            drop(revalidation);
            wait_release.recv_timeout(Duration::from_secs(4)).unwrap();
            let body = if write {
                r#"{"result":{"revision":8}}"#.into()
            } else {
                membership(1234, 8, false)
            };
            send_response(&mut pending, &body);
            requests
        });
        let token = token();
        service.account(&token).unwrap();
        let worker_service = service.clone();
        let worker_token = token.clone();
        let worker = thread::spawn(move || {
            if write {
                worker_service.set_track_disliked(&worker_token, "42", true)
            } else {
                worker_service.track_reactions(&worker_token, "42")
            }
        });
        wait_started.recv_timeout(Duration::from_secs(4)).unwrap();
        service.clear_session();
        service.account(&token).unwrap();
        release.send(()).unwrap();
        assert_eq!(
            worker.join().unwrap(),
            Err(MusicError::Unauthenticated.into())
        );
        let requests = complete(server, retained);
        assert_eq!(
            requests.len(),
            3,
            "no readback or retry under the replacement epoch"
        );
        assert!(requests[2].starts_with("GET /account/status HTTP/1.1"));
    }
}

#[test]
fn lost_dislike_write_response_is_ambiguous_without_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let service = MusicService::for_test(&format!("http://{}/", listener.local_addr().unwrap()));
    let retained = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        let (mut account, first) = next_request(&listener);
        send_response(&mut account, ACCOUNT);
        drop(account);
        let (pending, second) = next_request(&listener);
        drop(pending);
        vec![first, second]
    });
    let token = token();
    service.account(&token).unwrap();
    assert_eq!(
        service.set_track_disliked(&token, "42", true),
        Err(InteractionError::WriteOutcomeUnknown)
    );
    assert_eq!(complete(server, retained).len(), 2);
}
