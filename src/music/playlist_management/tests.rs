// SPDX-License-Identifier: GPL-3.0-or-later
//! Original synthetic loopback fixtures; never a live account or write.

use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const TOKEN: &str = "synthetic-private-playlist-edit-token";
const TITLE: &str = "synthetic-private-playlist-title";
const ACCOUNT: &str =
    r#"{"result":{"account":{"uid":1234,"displayName":"synthetic-private-person"}}}"#;

fn token() -> AccessToken {
    AccessToken::for_test(TOKEN, Duration::from_secs(3600))
}
fn id() -> PlaylistId {
    PlaylistId {
        owner: "1234".into(),
        kind: "17".into(),
    }
}
fn state(revision: u64, entries: &[(&str, &str)]) -> String {
    json!({"result":{"owner":{"uid":1234},"kind":17,"title":TITLE,
        "trackCount":entries.len(),"revision":revision,"visibility":"private",
        "tracks":entries.iter().map(|(id,album)|json!({"id":id,"albumId":album})).collect::<Vec<_>>()}}).to_string()
}
fn modified_state(revision: u64, entries: &[(&str, &str)], key: &str, value: Value) -> String {
    let mut body: Value = serde_json::from_str(&state(revision, entries)).unwrap();
    body["result"][key] = value;
    body.to_string()
}
fn track(selector: &str, album: &str) -> Track {
    track_from_wire(
        serde_json::from_value(json!({"id":selector,"title":"synthetic-private-track",
        "available":true,"albums":[{"id":album}]}))
        .unwrap(),
    )
    .unwrap()
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
        responses.into_iter().enumerate().map(|(index,(status,body))| {
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream,_)) => break stream,
                Err(error) if error.kind()==std::io::ErrorKind::WouldBlock && Instant::now()<deadline => thread::sleep(Duration::from_millis(5)),
                Err(_) => panic!("missing synthetic playlist-management request"),
            }
        };
        let request = read_request(&mut stream);
        if clear_after == Some(index) { invalidator.clear_session(); }
        // Status zero models an accepted write whose response was lost.
        if status != 0 {
            write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        request
    }).collect()
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
        assert!(bytes.len() < 32 * 1024);
    }
    let header = String::from_utf8(bytes.clone()).unwrap();
    let size = header
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|n| n.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(size < 32 * 1024);
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
        std::io::ErrorKind::WouldBlock,
        "no hidden retries"
    );
    for request in &requests {
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains(&format!("authorization: oauth {TOKEN}\r\n")));
        assert!(!lower.contains("client_secret"));
        assert!(!lower.contains("x-yandex-music-client"));
        assert!(!request.lines().next().unwrap().contains(TOKEN));
    }
    requests
}
fn form(request: &str) -> Vec<(String, String)> {
    let body = request.split("\r\n\r\n").nth(1).unwrap();
    reqwest::Url::parse(&format!("http://localhost/?{body}"))
        .unwrap()
        .query_pairs()
        .map(|(a, b)| (a.into_owned(), b.into_owned()))
        .collect()
}
fn authenticated_snapshot(service: &MusicService, token: &AccessToken) -> EditablePlaylist {
    service.account(token).unwrap();
    service.playlist_edit_state(token, &id()).unwrap()
}

#[test]
fn read_snapshot_preserves_duplicate_composite_server_positions_and_redacts_debug() {
    let before = state(7, &[("42", "9"), ("42:9", "9"), ("42", "8")]);
    let (service, server, listener) = fixture(vec![(200, ACCOUNT.into()), (200, before)], None);
    let token = token();
    let snapshot = authenticated_snapshot(&service, &token);
    assert_eq!(snapshot.revision.get(), 7);
    assert_eq!(
        snapshot
            .entries
            .iter()
            .map(|x| (x.position, x.selector()))
            .collect::<Vec<_>>(),
        vec![(0, "42:9".into()), (1, "42:9".into()), (2, "42:8".into())]
    );
    let visible = format!(
        "{snapshot:?} {:?} {:?}",
        snapshot.entries,
        PlaylistChange::Rename(TITLE.into())
    );
    for secret in [TOKEN, TITLE, "1234", "42:9"] {
        assert!(!visible.contains(secret));
    }
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 2);
    assert!(requests[1].starts_with("GET /users/1234/playlists/17?richTracks=true HTTP/1.1"));
}

#[test]
fn missing_revision_partial_sequence_bad_owner_and_conflicting_album_never_become_editable() {
    for (key, value) in [
        ("revision", Value::Null),
        ("tracks", json!([])),
        ("owner", json!({"uid":9999})),
        ("uid", json!(9999)),
        ("tracks", json!([{"id":"42:8","albumId":9}])),
    ] {
        let body = modified_state(7, &[("42", "9")], key, value);
        let (service, server, listener) = fixture(vec![(200, ACCOUNT.into()), (200, body)], None);
        service.account(&token()).unwrap();
        assert!(service.playlist_edit_state(&token(), &id()).is_err());
        assert_eq!(complete(server, listener).len(), 2);
    }
    let body = state(0, &[]);
    let (service, server, listener) = fixture(vec![(200, ACCOUNT.into()), (200, body)], None);
    assert_eq!(
        authenticated_snapshot(&service, &token()).revision.get(),
        0,
        "explicit zero is not a synthesized revision"
    );
    complete(server, listener);
}

#[test]
fn unusable_optional_rich_metadata_never_compacts_or_retargets_server_occurrences() {
    let binding = SearchBinding {
        epoch: 0,
        token_key: [0; 32],
        uid: "1234".into(),
    };
    let mut body: Value =
        serde_json::from_str(&state(7, &[("42", "9"), ("42", "9"), ("99", "8")])).unwrap();
    body["result"]["tracks"][0]["track"] =
        json!({"id":"42","available":{"unsupported":"display"},"title":"private display"});
    body["result"]["tracks"][1]["track"] = json!(["unsupported rich field"]);
    body["result"]["tracks"][2]["track"] = json!({"id":"99","available":true,"title":"valid"});
    let snapshot = decode_state(body.to_string().as_bytes(), &id(), binding.clone()).unwrap();
    assert_eq!(
        snapshot
            .entries
            .iter()
            .map(|entry| entry.position)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(snapshot.entries[0].track.is_none());
    assert!(snapshot.entries[1].track.is_none());
    assert_eq!(snapshot.entries[2].track.as_ref().unwrap().id, "99:8");
    let intended = Intended::new(&snapshot, PlaylistChange::Remove { position: 1 }).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&intended.form[2].1).unwrap(),
        json!([{"op":"delete","from":1,"to":2}])
    );
    for wrong in [
        json!({"id":"different-track","title":"wrong"}),
        json!({"id":{"unsupported":"identity"}}),
    ] {
        body["result"]["tracks"][0]["track"] = wrong;
        assert!(matches!(
            decode_state(body.to_string().as_bytes(), &id(), binding.clone()),
            Err(MusicError::InvalidResponse)
        ));
    }
}

#[test]
fn foreign_unvalidated_expired_and_stale_accounts_never_issue_an_edit_request() {
    let (service, server, listener) =
        fixture(vec![(200, ACCOUNT.into()), (200, state(7, &[]))], None);
    let token = token();
    assert!(matches!(
        service.playlist_edit_state(&token, &id()),
        Err(InteractionError::Music(MusicError::Unauthenticated))
    ));
    let snapshot = authenticated_snapshot(&service, &token);
    let mut foreign = id();
    foreign.owner = "9999".into();
    assert!(!service.owns_playlist(&token, &foreign).unwrap());
    assert!(matches!(
        service.playlist_edit_state(&token, &foreign),
        Err(InteractionError::Music(MusicError::InvalidInput))
    ));
    service.clear_session();
    assert!(matches!(
        service.edit_playlist(&token, &snapshot, PlaylistChange::Delete),
        Err(InteractionError::Music(MusicError::Unauthenticated))
    ));
    assert_eq!(complete(server, listener).len(), 2);
}

#[test]
fn insert_uses_one_revision_bound_post_then_independent_exact_readback() {
    let before = state(7, &[("42", "9"), ("42", "9")]);
    let after = state(8, &[("42", "9"), ("99", "8"), ("42", "9")]);
    let (service, server, listener) = fixture(
        vec![
            (200, ACCOUNT.into()),
            (200, before.clone()),
            (200, before),
            (200, after.clone()),
            (200, after),
        ],
        None,
    );
    let token = token();
    let snapshot = authenticated_snapshot(&service, &token);
    let PlaylistMutationOutcome::Confirmed(after) = service
        .edit_playlist(
            &token,
            &snapshot,
            PlaylistChange::Insert {
                track: track("99:8", "8"),
                at: 1,
            },
        )
        .unwrap()
    else {
        panic!("expected confirmed insert")
    };
    assert_eq!(
        after
            .entries
            .iter()
            .map(PlaylistOccurrence::selector)
            .collect::<Vec<_>>(),
        ["42:9", "99:8", "42:9"]
    );
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 5);
    assert!(requests[2].starts_with("GET "));
    assert!(requests[3].starts_with("POST /users/1234/playlists/17/change HTTP/1.1"));
    let fields = form(&requests[3]);
    assert_eq!(fields[0], ("kind".into(), "17".into()));
    assert_eq!(fields[1], ("revision".into(), "7".into()));
    assert_eq!(
        serde_json::from_str::<Value>(&fields[2].1).unwrap(),
        json!([{"op":"insert","at":1,"tracks":[{"id":"99","albumId":"8"}]}])
    );
    assert!(requests[4].starts_with("GET "));
}

#[test]
fn remove_targets_exact_duplicate_occurrence_and_move_is_one_atomic_diff_batch() {
    for (change, after, expected) in [
        (
            PlaylistChange::Remove { position: 1 },
            state(8, &[("42", "9"), ("99", "8")]),
            json!([{"op":"delete","from":1,"to":2}]),
        ),
        (
            PlaylistChange::Move { from: 1, to: 2 },
            state(8, &[("42", "9"), ("99", "8"), ("42", "9")]),
            json!([{"op":"delete","from":1,"to":2},{"op":"insert","at":2,"tracks":[{"id":"42","albumId":"9"}]}]),
        ),
        (
            PlaylistChange::Move { from: 2, to: 0 },
            state(8, &[("99", "8"), ("42", "9"), ("42", "9")]),
            json!([{"op":"delete","from":2,"to":3},{"op":"insert","at":0,"tracks":[{"id":"99","albumId":"8"}]}]),
        ),
    ] {
        let before = state(7, &[("42", "9"), ("42", "9"), ("99", "8")]);
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, before.clone()),
                (200, before),
                (200, after.clone()),
                (200, after),
            ],
            None,
        );
        let token = token();
        let snapshot = authenticated_snapshot(&service, &token);
        assert!(matches!(
            service.edit_playlist(&token, &snapshot, change),
            Ok(PlaylistMutationOutcome::Confirmed(_))
        ));
        let requests = complete(server, listener);
        let posts = requests
            .iter()
            .filter(|r| r.starts_with("POST "))
            .collect::<Vec<_>>();
        assert_eq!(posts.len(), 1);
        assert_eq!(
            serde_json::from_str::<Value>(&form(posts[0])[2].1).unwrap(),
            expected
        );
    }
}

#[test]
fn rename_and_visibility_require_acknowledgment_and_server_readback() {
    for (change, key, value, route) in [
        (
            PlaylistChange::Rename("  Новое & имя + 1  ".into()),
            "title",
            json!("Новое & имя + 1"),
            "name",
        ),
        (
            PlaylistChange::Visibility(PlaylistVisibility::Public),
            "visibility",
            json!("public"),
            "visibility",
        ),
    ] {
        let before = state(7, &[("42", "9")]);
        let after = modified_state(7, &[("42", "9")], key, value.clone());
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, before.clone()),
                (200, before),
                (200, after.clone()),
                (200, after),
            ],
            None,
        );
        let token = token();
        let snapshot = authenticated_snapshot(&service, &token);
        assert!(matches!(
            service.edit_playlist(&token, &snapshot, change),
            Ok(PlaylistMutationOutcome::Confirmed(_))
        ));
        let requests = complete(server, listener);
        assert!(
            requests[3].starts_with(&format!("POST /users/1234/playlists/17/{route} HTTP/1.1"))
        );
        assert_eq!(
            form(&requests[3]),
            vec![("value".into(), value.as_str().unwrap().into())]
        );
        assert!(requests[4].starts_with("GET "));
    }
}

#[test]
fn delete_requires_positive_ack_404_and_independent_own_list_absence() {
    for ack in [r#"{"result":"ok"}"#, r#"{"result":true}"#] {
        let before = state(7, &[("42", "9")]);
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, before.clone()),
                (200, before),
                (200, ack.into()),
                (404, "{}".into()),
                (200, r#"{"result":[]}"#.into()),
            ],
            None,
        );
        let token = token();
        let snapshot = authenticated_snapshot(&service, &token);
        assert!(matches!(
            service.edit_playlist(&token, &snapshot, PlaylistChange::Delete),
            Ok(PlaylistMutationOutcome::Deleted)
        ));
        let requests = complete(server, listener);
        assert_eq!(requests.len(), 6);
        assert!(requests[3].starts_with("POST /users/1234/playlists/17/delete HTTP/1.1"));
        assert!(form(&requests[3]).is_empty());
        assert!(requests[5].starts_with("GET /users/1234/playlists/list HTTP/1.1"));
    }
}

#[test]
fn fresh_revision_or_sequence_conflict_rejects_without_post() {
    for changed in [state(8, &[("42", "9")]), state(7, &[("99", "9")])] {
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, state(7, &[("42", "9")])),
                (200, changed),
            ],
            None,
        );
        let token = token();
        let snapshot = authenticated_snapshot(&service, &token);
        assert!(matches!(
            service.edit_playlist(&token, &snapshot, PlaylistChange::Remove { position: 0 }),
            Err(InteractionError::Conflict)
        ));
        assert!(
            complete(server, listener)
                .iter()
                .all(|r| r.starts_with("GET "))
        );
    }
}

#[test]
fn explicit_server_conflict_is_not_retried_even_when_readback_changed() {
    let before = state(7, &[("42", "9")]);
    let (service, server, listener) = fixture(
        vec![
            (200, ACCOUNT.into()),
            (200, before.clone()),
            (200, before),
            (409, "{}".into()),
            (200, state(8, &[])),
        ],
        None,
    );
    let token = token();
    let snapshot = authenticated_snapshot(&service, &token);
    assert!(matches!(
        service.edit_playlist(&token, &snapshot, PlaylistChange::Remove { position: 0 }),
        Err(InteractionError::Conflict)
    ));
    assert_eq!(
        complete(server, listener)
            .iter()
            .filter(|r| r.starts_with("POST "))
            .count(),
        1
    );
}

#[test]
fn access_loss_never_confirms_or_replays_a_playlist_change() {
    for (status, expected) in [
        (401, MusicError::AccessDenied),
        (403, MusicError::Forbidden),
        (429, MusicError::RateLimited),
        (451, MusicError::Restricted),
    ] {
        let before = state(7, &[("42", "9")]);
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, before.clone()),
                (200, before.clone()),
                (status, "{}".into()),
                (200, before),
            ],
            None,
        );
        let token = token();
        let snapshot = authenticated_snapshot(&service, &token);
        assert!(
            matches!(service.edit_playlist(&token,&snapshot,PlaylistChange::Remove{position:0}),Err(InteractionError::Music(error)) if error == expected)
        );
        assert_eq!(
            complete(server, listener)
                .iter()
                .filter(|r| r.starts_with("POST "))
                .count(),
            1
        );
    }
}

#[test]
fn delete_without_an_explicit_positive_receipt_stays_unknown_even_when_absent() {
    for ack in [
        json!({"result":false}),
        json!({"result":"unknown"}),
        json!({"result":null}),
    ] {
        let before = state(7, &[("42", "9")]);
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, before.clone()),
                (200, before),
                (200, ack.to_string()),
                (404, "{}".into()),
                (200, r#"{"result":[]}"#.into()),
            ],
            None,
        );
        let token = token();
        let snapshot = authenticated_snapshot(&service, &token);
        assert!(matches!(
            service.edit_playlist(&token, &snapshot, PlaylistChange::Delete),
            Err(InteractionError::WriteOutcomeUnknown)
        ));
        assert_eq!(
            complete(server, listener)
                .iter()
                .filter(|r| r.starts_with("POST "))
                .count(),
            1
        );
    }
}

#[test]
fn lost_or_malformed_ack_remains_unknown_with_read_only_reconciliation_never_replay() {
    for response in [
        (0, String::new()),
        (200, "{bad-private-response".into()),
        (500, "{}".into()),
        (200, state(7, &[])),
    ] {
        let before = state(7, &[("42", "9")]);
        let after = state(8, &[]);
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, before.clone()),
                (200, before),
                response,
                (200, after.clone()),
                (200, after),
            ],
            None,
        );
        let token = token();
        let snapshot = authenticated_snapshot(&service, &token);
        assert!(matches!(
            service.edit_playlist(&token, &snapshot, PlaylistChange::Remove { position: 0 }),
            Err(InteractionError::WriteOutcomeUnknown)
        ));
        assert!(
            service
                .reconcile_playlist(&token, &id())
                .unwrap()
                .unwrap()
                .entries
                .is_empty()
        );
        let requests = complete(server, listener);
        assert_eq!(
            requests.iter().filter(|r| r.starts_with("POST ")).count(),
            1
        );
        assert!(requests[4..].iter().all(|r| r.starts_with("GET ")));
    }
}

#[test]
fn mismatched_or_failed_readback_is_never_optimistic_success() {
    for (status, body, conflict) in [
        (200, state(8, &[("42", "9")]), false),
        (200, state(9, &[("99", "9")]), true),
        (503, "{}".into(), false),
    ] {
        let before = state(7, &[("42", "9")]);
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, before.clone()),
                (200, before),
                (200, state(8, &[])),
                (status, body),
            ],
            None,
        );
        let token = token();
        let snapshot = authenticated_snapshot(&service, &token);
        let result =
            service.edit_playlist(&token, &snapshot, PlaylistChange::Remove { position: 0 });
        assert!(matches!(
            (status, conflict, result),
            (503, _, Err(InteractionError::ConfirmationUnavailable))
                | (_, true, Err(InteractionError::Conflict))
                | (_, false, Err(InteractionError::ConfirmationMismatch))
        ));
        assert_eq!(
            complete(server, listener)
                .iter()
                .filter(|r| r.starts_with("POST "))
                .count(),
            1
        );
    }
}

#[test]
fn cancellation_during_preflight_prevents_write_and_late_ack_cannot_repopulate_session() {
    for cancelled in [2, 3] {
        let before = state(7, &[("42", "9")]);
        let mut responses = vec![(200, ACCOUNT.into()), (200, before.clone()), (200, before)];
        if cancelled == 3 {
            responses.push((200, state(8, &[])));
        }
        let (service, server, listener) = fixture(responses, Some(cancelled));
        let token = token();
        let snapshot = authenticated_snapshot(&service, &token);
        assert!(matches!(
            service.edit_playlist(&token, &snapshot, PlaylistChange::Remove { position: 0 }),
            Err(InteractionError::Music(MusicError::Unauthenticated))
        ));
        assert!(matches!(
            service.account_id(&token),
            Err(MusicError::Unauthenticated)
        ));
        assert_eq!(complete(server, listener).len(), cancelled + 1);
    }
}

#[test]
fn invalid_positions_unknown_visibility_and_write_gate_never_post() {
    let before = state(7, &[("42", "9")]);
    let changes = vec![
        PlaylistChange::Move { from: 0, to: 0 },
        PlaylistChange::Move { from: 0, to: 1 },
        PlaylistChange::Remove { position: 1 },
        PlaylistChange::Insert {
            track: track("99", "8"),
            at: 2,
        },
        PlaylistChange::Rename(" \n ".into()),
        PlaylistChange::Rename(TITLE.into()),
        PlaylistChange::Visibility(PlaylistVisibility::Private),
    ];
    let mut responses = vec![(200, ACCOUNT.into()), (200, before.clone())];
    responses.extend(changes.iter().map(|_| (200, before.clone())));
    let (service, server, listener) = fixture(responses, None);
    let token = token();
    let snapshot = authenticated_snapshot(&service, &token);
    for change in changes {
        assert!(service.edit_playlist(&token, &snapshot, change).is_err());
    }
    service.account.lock().unwrap().playlist_write = true;
    assert!(matches!(
        service.edit_playlist(&token, &snapshot, PlaylistChange::Delete),
        Err(InteractionError::Conflict)
    ));
    service.account.lock().unwrap().playlist_write = false;
    assert!(
        complete(server, listener)
            .iter()
            .all(|r| r.starts_with("GET "))
    );
}

#[test]
fn absent_visibility_is_unknown_and_incomplete_or_oversized_positions_are_refused() {
    let binding = SearchBinding {
        epoch: 0,
        token_key: [0; 32],
        uid: "1234".into(),
    };
    let mut body: Value = serde_json::from_str(&state(7, &[("42", "9")])).unwrap();
    body["result"].as_object_mut().unwrap().remove("visibility");
    let snapshot = decode_state(body.to_string().as_bytes(), &id(), binding.clone()).unwrap();
    assert_eq!(snapshot.visibility, None);
    assert!(matches!(
        Intended::new(
            &snapshot,
            PlaylistChange::Visibility(PlaylistVisibility::Public)
        ),
        Err(InteractionError::ConfirmationUnavailable)
    ));
    for count in [2, 2001] {
        body["result"]["trackCount"] = json!(count);
        assert!(decode_state(body.to_string().as_bytes(), &id(), binding.clone()).is_err());
    }
}

#[test]
fn deletion_is_not_proven_by_missing_or_forbidden_detail_alone() {
    for (detail_status,list_status,list) in [(404,200,json!({"result":[serde_json::from_str::<Value>(&state(7,&[])).unwrap()["result"].clone()]}).to_string()),(404,503,"{}".into()),(403,200,String::new())] {
        let mut responses=vec![(200,ACCOUNT.into()),(detail_status,"{}".into())];
        if detail_status==404 { responses.push((list_status,list)); }
        let(service,server,listener)=fixture(responses,None); service.account(&token()).unwrap();
        assert!(service.reconcile_playlist(&token(),&id()).is_err());
        assert!(complete(server,listener).iter().all(|r|r.starts_with("GET ")));
    }
}

#[test]
fn own_picker_is_one_coherent_bounded_read_and_never_includes_foreign_playlists() {
    let owned = serde_json::from_str::<Value>(&state(7, &[])).unwrap()["result"].clone();
    let mut foreign = owned.clone();
    foreign["owner"]["uid"] = json!(9999);
    let list = json!({"result":[owned.clone(),foreign]}).to_string();
    let (service, server, listener) = fixture(vec![(200, ACCOUNT.into()), (200, list)], None);
    service.account(&token()).unwrap();
    let choices = service.playlist_edit_choices(&token()).unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].id, id());
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 2);
    assert!(requests[1].starts_with("GET /users/1234/playlists/list HTTP/1.1"));

    for list in [
        json!({"result":[owned.clone(),owned.clone()]}),
        json!({"result":[{"owner":{"uid":1234},"kind":"../invalid"}]}),
        json!({"result":vec![owned;2001]}),
    ] {
        let (service, server, listener) =
            fixture(vec![(200, ACCOUNT.into()), (200, list.to_string())], None);
        service.account(&token()).unwrap();
        assert!(service.playlist_edit_choices(&token()).is_err());
        assert_eq!(complete(server, listener).len(), 2);
    }
}

#[test]
fn create_shares_edit_gate_and_cannot_overlap_an_inflight_playlist_change() {
    let (service, server, listener) = fixture(vec![(200, ACCOUNT.into())], None);
    service.account(&token()).unwrap();
    service.account.lock().unwrap().playlist_write = true;
    assert!(matches!(
        service.create_playlist(&token(), "valid title", PlaylistVisibility::Private),
        Err(InteractionError::Conflict)
    ));
    service.account.lock().unwrap().playlist_write = false;
    assert_eq!(complete(server, listener).len(), 1);
}

#[test]
fn simultaneous_clone_intents_issue_only_one_write_and_release_the_gate() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let service = MusicService::for_test(&format!("http://{}/", listener.local_addr().unwrap()));
    let retained = listener.try_clone().unwrap();
    listener.set_nonblocking(true).unwrap();
    let token = token();
    service.account.lock().unwrap().validated =
        Some((MusicService::token_key(&token), "1234".into()));
    let snapshot = decode_state(
        state(7, &[("42", "9")]).as_bytes(),
        &id(),
        service.search_binding(&token).unwrap(),
    )
    .unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        for (index, body) in [state(7, &[("42", "9")]), state(8, &[]), state(8, &[])]
            .into_iter()
            .enumerate()
        {
            let deadline = Instant::now() + Duration::from_secs(4);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => panic!("missing concurrent edit fixture request"),
                }
            };
            requests.push(read_request(&mut stream));
            if index == 0 {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            }
            write!(stream,"HTTP/1.1 200 Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        requests
    });
    let worker_service = service.clone();
    let worker_token = token.clone();
    let worker_snapshot = snapshot.clone();
    let worker = thread::spawn(move || {
        worker_service.edit_playlist(
            &worker_token,
            &worker_snapshot,
            PlaylistChange::Remove { position: 0 },
        )
    });
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(matches!(
        service.edit_playlist(&token, &snapshot, PlaylistChange::Delete),
        Err(InteractionError::Conflict)
    ));
    assert!(matches!(
        service.create_playlist(&token, "other intent", PlaylistVisibility::Private),
        Err(InteractionError::Conflict)
    ));
    release_tx.send(()).unwrap();
    assert!(matches!(
        worker.join().unwrap(),
        Ok(PlaylistMutationOutcome::Confirmed(_))
    ));
    assert!(!service.account.lock().unwrap().playlist_write);
    assert_eq!(
        complete(server, retained)
            .iter()
            .filter(|request| request.starts_with("POST "))
            .count(),
        1
    );
}

#[test]
fn saved_queue_normalization_batches_preserves_duplicates_and_omits_only_missing() {
    let mut requested = (1..=51).map(|id| id.to_string()).collect::<Vec<_>>();
    requested.push("1:7".into());
    requested.push("1".into());
    let first=json!({"result":(1..=50).rev().filter(|id|*id!=3).map(|id|json!({"id":id,"title":"fixture","available":id!=4})).collect::<Vec<_>>()}).to_string();
    let second =
        json!({"result":[{"id":51,"available":true},{"id":"1:7","available":true}]}).to_string();
    let (service, server, listener) = fixture(
        vec![(200, ACCOUNT.into()), (200, first), (200, second)],
        None,
    );
    service.account(&token()).unwrap();
    let tracks = service.restore_tracks(&token(), &requested).unwrap();
    assert_eq!(
        tracks.iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
        requested
            .iter()
            .filter(|id| *id != "3")
            .cloned()
            .collect::<Vec<_>>()
    );
    assert!(!tracks.iter().find(|t| t.id == "4").unwrap().available);
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 3);
    assert!(
        requests[1..]
            .iter()
            .all(|r| r.starts_with("POST /tracks HTTP/1.1"))
    );
    assert_eq!(form(&requests[1]).len(), 50);
    assert_eq!(form(&requests[2]).len(), 2);
}

#[test]
fn saved_queue_batch_bound_and_logout_fence_prevent_late_results_and_more_requests() {
    let (service, server, listener) = fixture(
        vec![
            (200, ACCOUNT.into()),
            (200, r#"{"result":[{"id":1}]}"#.into()),
        ],
        Some(1),
    );
    service.account(&token()).unwrap();
    assert!(matches!(
        service.restore_tracks(&token(), &vec!["1".into(); 501]),
        Err(MusicError::InvalidInput)
    ));
    let ids = (1..=51).map(|id| id.to_string()).collect::<Vec<_>>();
    assert!(matches!(
        service.restore_tracks(&token(), &ids),
        Err(MusicError::Unauthenticated)
    ));
    assert_eq!(complete(server, listener).len(), 2);
}
