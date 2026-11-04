// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic protocol and actual-state observation tests; no account or media.

use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread::{self, JoinHandle},
};

const TOKEN: &str = "synthetic-wave-private-token";
const ACCOUNT: &str =
    r#"{"result":{"account":{"uid":1234,"displayName":"synthetic Wave person"}}}"#;

enum Hook {
    ClearAccount(usize),
    Cancel(usize, WaveCancellation),
    Truncate(usize),
    DropHeaders(usize),
}
fn fixture(
    responses: Vec<(u16, String)>,
    hook: Option<Hook>,
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
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => panic!("missing synthetic Wave request"),
                }
            };
            requests.push(read_request(&mut stream));
            match &hook {
                Some(Hook::ClearAccount(i)) if *i == index => invalidator.clear_session(),
                Some(Hook::Cancel(i, c)) if *i == index => c.cancel(),
                _ => {}
            }
            if matches!(&hook, Some(Hook::DropHeaders(i)) if *i == index) {
                continue;
            }
            let declared = body.len()
                + if matches!(&hook, Some(Hook::Truncate(i)) if *i == index) {
                    16
                } else {
                    0
                };
            write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n{body}").unwrap();
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
                .map(|v| v.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(size <= 128 * 1024);
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
        if request.lines().next().unwrap().contains("/feedback ") {
            assert_ne!(json_body(request)["event"]["type"], "radioFinished");
        }
    }
    requests
}
fn token() -> AccessToken {
    AccessToken::for_test(TOKEN, Duration::from_secs(60))
}
fn session_body(batch: &str, ids: &[u64], radio: Option<&str>) -> String {
    let mut value = serde_json::json!({"result":{"batchId":batch,"sequence":ids.iter().map(|id|serde_json::json!({"type":"track","track":{"id":id,"title":{"unusual":"ignored"},"artists":null}})).collect::<Vec<_>>()}});
    if let Some(radio) = radio {
        value["result"]["radioSessionId"] = radio.into();
    }
    value.to_string()
}
fn track_body(ids: &[u64]) -> String {
    serde_json::json!({"result":ids.iter().map(|id|serde_json::json!({"id":id,"title":"canonical synthetic Wave track","artists":[{"name":"synthetic artist"}],"available":true,"durationMs":10000})).collect::<Vec<_>>()}).to_string()
}
fn json_body(request: &str) -> serde_json::Value {
    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap()
}

#[test]
fn modern_session_json_batches_original_provenance_and_feedback_are_bounded() {
    let (service, server, listener) = fixture(
        vec![
            (200, ACCOUNT.into()),
            (
                200,
                session_body(
                    "private-batch-1",
                    &[42, 43, 42],
                    Some("private-radio-session"),
                ),
            ),
            (200, track_body(&[43, 42])),
            (200, r#"{"result":"ok"}"#.into()),
            (200, String::new()),
            (200, session_body("private-batch-2", &[44], None)),
            (200, track_body(&[44])),
            (200, r#"{"result":"ok"}"#.into()),
        ],
        None,
    );
    let token = token();
    service.account(&token).unwrap();
    let cancel = WaveCancellation::default();
    let (session, batch) = service.begin_wave(&token, &cancel).unwrap();
    assert_eq!(
        batch
            .tracks
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        vec!["42", "43", "42"]
    );
    assert_eq!(
        (batch.http_status, batch.lookup_http_status, batch.omitted),
        (200, 200, 0)
    );
    let now = Instant::now();
    let mut coordinator = WaveCoordinator::new(&session);
    assert!(
        coordinator
            .select(1, &batch.tracks[0], &batch.marker(), now)
            .unwrap()
            .is_empty()
    );
    assert!(
        coordinator
            .observe(PlaybackState::Loading, now + Duration::from_secs(1))
            .unwrap()
            .is_empty()
    );
    let started = coordinator
        .observe(PlaybackState::Playing, now + Duration::from_secs(2))
        .unwrap();
    assert_eq!(
        started.iter().map(WaveEvent::kind).collect::<Vec<_>>(),
        vec![WaveEventKind::RadioStarted, WaveEventKind::TrackStarted]
    );
    for event in &started {
        assert_eq!(
            service.wave_feedback(&token, &session, event),
            Ok(WaveFeedbackAck { http_status: 200 })
        );
    }
    let next = service
        .wave_batch(
            &token,
            &session,
            &batch
                .tracks
                .iter()
                .map(|t| t.id.clone())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(next.tracks[0].id, "44");
    let finished = coordinator
        .observe(PlaybackState::Ended, now + Duration::from_secs(5))
        .unwrap();
    assert_eq!(finished[0].heard(), Duration::from_secs(3));
    service
        .wave_feedback(&token, &session, &finished[0])
        .unwrap();
    let requests = complete(server, listener);
    assert_eq!(requests.len(), 8);
    assert!(requests[1].starts_with("POST /rotor/session/new HTTP/1.1"));
    assert!(requests[1].contains("application/json"));
    assert_eq!(
        json_body(&requests[1]),
        serde_json::json!({"seeds":["user:onyourwave"],"includeTracksInResponse":true,"includeWaveModel":true})
    );
    assert!(requests[2].starts_with("POST /tracks HTTP/1.1"));
    assert!(requests[2].ends_with("track-ids=42&track-ids=43"));
    assert!(requests[5].starts_with("POST /rotor/session/private-radio-session/tracks HTTP/1.1"));
    assert_eq!(
        json_body(&requests[5]),
        serde_json::json!({"queue":["42","43","42"]})
    );
    let report = json_body(&requests[7]);
    assert_eq!(report["batchId"], "private-batch-1");
    assert_eq!(report["event"]["type"], "trackFinished");
    assert_eq!(report["event"]["trackId"], "42");
    assert_eq!(report["event"]["totalPlayedSeconds"], 3.0);
    assert_eq!(report["event"]["from"], "unofficial-client-own");
    assert!(
        report["event"]["timestamp"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
    let debug = format!(
        "{session:?} {batch:?} {:?} {:?} {coordinator:?}",
        batch.marker(),
        finished[0]
    );
    for private in [
        "private-radio-session",
        "private-batch-1",
        TOKEN,
        "synthetic artist",
        "1234",
    ] {
        assert!(!debug.contains(private));
    }
}

#[test]
fn batch_missing_records_are_partial_and_unavailable_records_stay_metadata() {
    let mut records: serde_json::Value = serde_json::from_str(&track_body(&[42])).unwrap();
    records["result"][0]["available"] = false.into();
    let (service, server, listener) = fixture(
        vec![
            (200, ACCOUNT.into()),
            (200, session_body("batch-1", &[42, 43], Some("radio-1"))),
            (200, records.to_string()),
        ],
        None,
    );
    let token = token();
    service.account(&token).unwrap();
    let (session, batch) = service
        .begin_wave(&token, &WaveCancellation::default())
        .unwrap();
    assert_eq!(batch.omitted, 1);
    assert_eq!(batch.tracks.len(), 1);
    assert!(!batch.tracks[0].available);
    let mut coordinator = WaveCoordinator::new(&session);
    assert!(matches!(
        coordinator.select(1, &batch.tracks[0], &batch.marker(), Instant::now()),
        Err(WaveError::InvalidObservation)
    ));
    assert_eq!(complete(server, listener).len(), 3);
}

#[test]
fn no_session_write_without_validated_account_or_after_local_cancel() {
    let (service, server, listener) = fixture(vec![], None);
    let token = token();
    let cancelled = WaveCancellation::default();
    cancelled.cancel();
    assert!(matches!(
        service.begin_wave(&token, &cancelled),
        Err(WaveError::Cancelled)
    ));
    assert!(matches!(
        service.begin_wave(&token, &WaveCancellation::default()),
        Err(WaveError::Music(MusicError::Unauthenticated))
    ));
    assert!(complete(server, listener).is_empty());
}

#[test]
fn session_and_normalization_refusals_have_distinct_safe_stages() {
    let (service, server, listener) = fixture(
        vec![
            (200, ACCOUNT.into()),
            (200, session_body("batch-1", &[42], Some("radio-1"))),
            (403, "private canonical lookup refusal".into()),
        ],
        None,
    );
    let token = token();
    service.account(&token).unwrap();
    let cancel = WaveCancellation::default();
    let error = service.begin_wave(&token, &cancel).unwrap_err();
    assert_eq!(error.stage(), Some(WaveStage::Normalization));
    assert_eq!(error.music_error(), Some(MusicError::Forbidden));
    assert_eq!(error.http_status(), Some(403));
    assert!(cancel.is_cancelled());
    assert!(service.validate_session(&token).is_ok());
    assert!(!format!("{error:?} {error}").contains("private"));
    assert_eq!(complete(server, listener).len(), 3);
}

#[test]
fn rejected_or_uncertain_session_write_has_no_identity_fallback_or_retry() {
    for (status, body, expected) in [
        (
            403,
            "private forbidden",
            WaveError::Request {
                stage: WaveStage::Session,
                error: MusicError::Forbidden,
            },
        ),
        (
            401,
            "private rejection",
            WaveError::Request {
                stage: WaveStage::Session,
                error: MusicError::AccessDenied,
            },
        ),
        (
            429,
            "private limit",
            WaveError::Request {
                stage: WaveStage::Session,
                error: MusicError::RateLimited,
            },
        ),
        (500, "private unknown", WaveError::WriteOutcomeUnknown),
        (302, "private redirect", WaveError::WriteOutcomeUnknown),
        (
            201,
            "private unexpected success",
            WaveError::WriteOutcomeUnknown,
        ),
    ] {
        let (service, server, listener) =
            fixture(vec![(200, ACCOUNT.into()), (status, body.into())], None);
        let token = token();
        service.account(&token).unwrap();
        let cancel = WaveCancellation::default();
        let error = service.begin_wave(&token, &cancel).unwrap_err();
        assert_eq!(error.stage(), Some(WaveStage::Session));
        assert_eq!(error.http_status(), Some(status));
        assert_eq!(error.category(), expected.category());
        assert_eq!(error.music_error(), expected.music_error());
        assert!(cancel.is_cancelled());
        assert!(!format!("{error:?} {error}").contains("private"));
        assert_eq!(complete(server, listener).len(), 2);
    }
}

#[test]
fn unsupported_mixed_or_unknown_session_response_does_not_continue() {
    let good = session_body("batch-1", &[42], Some("radio-1"));
    let mut mixed: serde_json::Value = serde_json::from_str(&good).unwrap();
    mixed["error"] = serde_json::json!({"message":"private provider response"});
    let mut unknown: serde_json::Value = serde_json::from_str(&good).unwrap();
    unknown["result"]["unknownSession"] = true.into();
    let mut hostile: serde_json::Value = serde_json::from_str(&good).unwrap();
    hostile["result"]["radioSessionId"] = "../private?token=private".into();
    for (body, expected) in [
        ("private malformed".into(), WaveError::UnsupportedResponse),
        (mixed.to_string(), WaveError::UnsupportedResponse),
        (unknown.to_string(), WaveError::UnknownSession),
        (hostile.to_string(), WaveError::UnsupportedResponse),
        (
            session_body("batch-1", &[42], None),
            WaveError::UnsupportedResponse,
        ),
        (
            session_body("batch-1", &[], Some("radio-1")),
            WaveError::NoTracks,
        ),
        (
            session_body("batch-1", &[42; 51], Some("radio-1")),
            WaveError::UnsupportedResponse,
        ),
    ] {
        let (service, server, listener) = fixture(vec![(200, ACCOUNT.into()), (200, body)], None);
        let token = token();
        service.account(&token).unwrap();
        let cancel = WaveCancellation::default();
        let error = service.begin_wave(&token, &cancel).unwrap_err();
        assert_eq!(error.stage(), Some(WaveStage::Session));
        assert_eq!(error.http_status(), Some(200));
        assert_eq!(error.category(), expected.category());
        assert!(cancel.is_cancelled());
        assert_eq!(complete(server, listener).len(), 2);
    }
}

#[test]
fn cancelling_or_logging_out_during_new_session_prevents_track_lookup() {
    for logout in [false, true] {
        let cancel = WaveCancellation::default();
        let hook = if logout {
            Hook::ClearAccount(1)
        } else {
            Hook::Cancel(1, cancel.clone())
        };
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, session_body("batch-1", &[42], Some("radio-1"))),
            ],
            Some(hook),
        );
        let token = token();
        service.account(&token).unwrap();
        let error = service.begin_wave(&token, &cancel).unwrap_err();
        assert_eq!(
            error,
            if logout {
                WaveError::Music(MusicError::Unauthenticated)
            } else {
                WaveError::Cancelled
            }
        );
        assert_eq!(complete(server, listener).len(), 2);
    }
}

#[test]
fn feedback_ambiguous_outcome_poisoning_prevents_replay() {
    for (status, body) in [
        (500, "private failed"),
        (200, "private malformed"),
        (200, r#"{"error":{"message":"private error"}}"#),
        (200, r#"{"error":{"message":"private error"},"error":null}"#),
        (200, r#"{"error":null,"error":null}"#),
        (200, "[null]"),
    ] {
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, session_body("batch-1", &[42], Some("radio-1"))),
                (200, track_body(&[42])),
                (status, body.into()),
            ],
            None,
        );
        let token = token();
        service.account(&token).unwrap();
        let (session, batch) = service
            .begin_wave(&token, &WaveCancellation::default())
            .unwrap();
        let mut coordinator = WaveCoordinator::new(&session);
        let now = Instant::now();
        coordinator
            .select(1, &batch.tracks[0], &batch.marker(), now)
            .unwrap();
        let reports = coordinator.observe(PlaybackState::Playing, now).unwrap();
        assert_eq!(
            service.wave_feedback(&token, &session, &reports[0]),
            Err(WaveError::Response {
                stage: WaveStage::Feedback,
                cause: WaveResponseCause::WriteOutcomeUnknown,
                http: status,
            })
        );
        assert!(session.is_cancelled());
        assert_eq!(
            service.wave_feedback(&token, &session, &reports[0]),
            Err(WaveError::Cancelled)
        );
        assert_eq!(complete(server, listener).len(), 4);
    }
}

fn pure_session() -> (WaveSession, WaveBatchMarker, Track) {
    let identity = Arc::new(Identity {
        binding: SearchBinding {
            epoch: 1,
            token_key: [0; 32],
            uid: "private-account".into(),
        },
        radio_id: Zeroizing::new("private-radio".into()),
        cancellation: WaveCancellation::default(),
        operation: Mutex::new(()),
    });
    let session = WaveSession(identity.clone());
    let marker = WaveBatchMarker {
        identity,
        batch_id: Zeroizing::new("private-batch".into()),
        tracks: Arc::new(vec!["42".into()]),
    };
    let track = Track {
        id: "42".into(),
        title: "synthetic track".into(),
        artists: vec!["synthetic artist".into()],
        artist_links: Vec::new(),
        album_id: None,
        album_title: None,
        duration_ms: Some(999_000),
        cover: None,
        explicit: false,
        available: true,
    };
    (session, marker, track)
}

#[test]
fn coordinator_uses_actual_playing_time_and_never_position_or_declared_duration() {
    let (session, marker, track) = pure_session();
    let mut c = WaveCoordinator::new(&session);
    let now = Instant::now();
    assert!(c.select(10, &track, &marker, now).unwrap().is_empty());
    assert!(
        c.observe(PlaybackState::Loading, now + Duration::from_secs(5))
            .unwrap()
            .is_empty()
    );
    let events = c
        .observe(PlaybackState::Playing, now + Duration::from_secs(7))
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].kind(), WaveEventKind::TrackStarted);
    assert!(
        c.observe(PlaybackState::Playing, now + Duration::from_secs(9))
            .unwrap()
            .is_empty()
    );
    assert!(
        c.observe(PlaybackState::Paused, now + Duration::from_secs(10))
            .unwrap()
            .is_empty()
    );
    assert!(
        c.observe(PlaybackState::Paused, now + Duration::from_secs(40))
            .unwrap()
            .is_empty()
    );
    assert!(
        c.observe(PlaybackState::Playing, now + Duration::from_secs(45))
            .unwrap()
            .is_empty()
    );
    // A seek can change track position but is deliberately not a coordinator
    // input. Only these five actual playing seconds contribute to the report.
    let finished = c
        .observe(PlaybackState::Ended, now + Duration::from_secs(47))
        .unwrap();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].kind(), WaveEventKind::TrackFinished);
    assert_eq!(finished[0].heard(), Duration::from_secs(5));
    assert!(
        c.observe(PlaybackState::Ended, now + Duration::from_secs(48))
            .unwrap()
            .is_empty()
    );
    let end = c.finish(now + Duration::from_secs(49)).unwrap();
    assert!(end.is_empty(), "natural EOS already completed the track");
    assert!(matches!(
        c.finish(now + Duration::from_secs(50)),
        Err(WaveError::InvalidObservation)
    ));
}

#[test]
fn exact_occurrences_skip_once_and_preload_failure_reports_no_fictional_listening() {
    let (session, marker, track) = pure_session();
    let mut c = WaveCoordinator::new(&session);
    let now = Instant::now();
    c.select(1, &track, &marker, now).unwrap();
    c.observe(PlaybackState::Playing, now).unwrap();
    assert!(
        c.select(1, &track, &marker, now + Duration::from_secs(1))
            .unwrap()
            .is_empty()
    );
    let skip = c
        .select(2, &track, &marker, now + Duration::from_secs(2))
        .unwrap();
    assert_eq!(skip.len(), 1);
    assert_eq!(skip[0].kind(), WaveEventKind::Skip);
    assert_eq!(skip[0].heard(), Duration::from_secs(2));
    assert!(
        c.observe(PlaybackState::Loading, now + Duration::from_secs(3))
            .unwrap()
            .is_empty()
    );
    assert!(
        c.observe(PlaybackState::Error, now + Duration::from_secs(4))
            .unwrap()
            .is_empty()
    );
    assert!(
        c.select(3, &track, &marker, now + Duration::from_secs(5))
            .unwrap()
            .is_empty()
    );
    let start = c
        .observe(PlaybackState::Playing, now + Duration::from_secs(6))
        .unwrap();
    assert_eq!(start.len(), 1);
    assert_eq!(start[0].kind(), WaveEventKind::TrackStarted);
    let stopped = c
        .observe(PlaybackState::Stopped, now + Duration::from_secs(7))
        .unwrap();
    assert_eq!(stopped[0].kind(), WaveEventKind::Skip);
    assert_eq!(stopped[0].heard(), Duration::from_secs(1));
    assert!(
        c.observe(PlaybackState::Stopped, now + Duration::from_secs(8))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn foreign_marker_clock_replay_and_cancel_are_rejected_without_late_reports() {
    let (session, marker, track) = pure_session();
    let (_, foreign, _) = pure_session();
    let mut c = WaveCoordinator::new(&session);
    let now = Instant::now();
    assert!(matches!(
        c.select(1, &track, &foreign, now),
        Err(WaveError::InvalidObservation)
    ));
    c.select(1, &track, &marker, now).unwrap();
    c.observe(PlaybackState::Playing, now).unwrap();
    assert!(matches!(
        c.observe(PlaybackState::Paused, now - Duration::from_secs(1)),
        Err(WaveError::InvalidObservation)
    ));
    let events = c
        .observe(PlaybackState::Paused, now + Duration::from_secs(2))
        .unwrap();
    assert!(events.is_empty());
    c.cancel();
    assert!(session.is_cancelled());
    assert!(matches!(
        c.observe(PlaybackState::Ended, now + Duration::from_secs(3)),
        Err(WaveError::Cancelled)
    ));
    assert!(matches!(
        c.finish(now + Duration::from_secs(3)),
        Err(WaveError::Cancelled)
    ));
}

#[test]
fn busy_batch_and_cancelled_session_never_issue_an_extra_post() {
    let (service, server, listener) = fixture(
        vec![
            (200, ACCOUNT.into()),
            (200, session_body("batch-1", &[42], Some("radio-1"))),
            (200, track_body(&[42])),
        ],
        None,
    );
    let token = token();
    service.account(&token).unwrap();
    let (session, _) = service
        .begin_wave(&token, &WaveCancellation::default())
        .unwrap();
    let lock = session.0.operation.lock().unwrap();
    assert!(matches!(
        service.wave_batch(&token, &session, &[]),
        Err(WaveError::Busy)
    ));
    drop(lock);
    assert!(matches!(
        service.wave_batch(&token, &session, &vec!["42".into(); 501]),
        Err(WaveError::Music(MusicError::InvalidInput))
    ));
    let (foreign, marker, track) = pure_session();
    let mut coordinator = WaveCoordinator::new(&foreign);
    let now = Instant::now();
    coordinator.select(1, &track, &marker, now).unwrap();
    let reports = coordinator.observe(PlaybackState::Playing, now).unwrap();
    assert_eq!(
        service.wave_feedback(&token, &session, &reports[0]),
        Err(WaveError::InvalidObservation)
    );
    session.cancel();
    assert!(matches!(
        service.wave_batch(&token, &session, &[]),
        Err(WaveError::Cancelled)
    ));
    assert_eq!(complete(server, listener).len(), 3);
}

#[test]
fn next_batch_cannot_rebind_the_session_or_replay_a_lost_cursor_result() {
    for body in [
        session_body("batch-2", &[43], Some("other-radio")),
        "private malformed response".into(),
    ] {
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, session_body("batch-1", &[42], Some("radio-1"))),
                (200, track_body(&[42])),
                (200, body),
            ],
            None,
        );
        let token = token();
        service.account(&token).unwrap();
        let (session, _) = service
            .begin_wave(&token, &WaveCancellation::default())
            .unwrap();
        assert!(matches!(
            service.wave_batch(&token, &session, &["42".into()]),
            Err(WaveError::Response {
                stage: WaveStage::Tracks,
                cause: WaveResponseCause::Unsupported,
                http: 200,
            })
        ));
        assert!(session.is_cancelled());
        assert!(matches!(
            service.wave_batch(&token, &session, &["42".into()]),
            Err(WaveError::Cancelled)
        ));
        assert_eq!(complete(server, listener).len(), 4);
    }
}

#[test]
fn feedback_timestamp_is_utc_bounded_and_handles_leap_years() {
    assert_eq!(
        utc_timestamp(UNIX_EPOCH).unwrap(),
        "1970-01-01T00:00:00.000Z"
    );
    assert_eq!(
        utc_timestamp(UNIX_EPOCH + Duration::from_secs(1_709_251_199) + Duration::from_millis(123))
            .unwrap(),
        "2024-02-29T23:59:59.123Z"
    );
    assert!(utc_timestamp(UNIX_EPOCH - Duration::from_secs(1)).is_err());
    assert!(utc_timestamp(UNIX_EPOCH + Duration::from_secs(253_402_300_800)).is_err());
}

#[test]
fn normalization_keeps_composite_selectors_and_exact_matches_in_provider_order() {
    for reverse in [false, true] {
        let ids = ["42:7", "42", "42:7", "43", "44"];
        let sequence = serde_json::json!({"result":{
            "radioSessionId":"private-radio",
            "batchId":"private-batch",
            "sequence":ids.iter().map(|id|serde_json::json!({"type":"track","track":{"id":id}})).collect::<Vec<_>>()
        }});
        let mut canonical = serde_json::json!({"result":[
            {"id":"42","title":"bare synthetic","available":true},
            {"id":"42:7","title":"exact synthetic","available":true},
            {"id":"43","title":"unavailable synthetic","available":false}
        ]});
        if reverse {
            canonical["result"].as_array_mut().unwrap().reverse();
        }
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, sequence.to_string()),
                (200, canonical.to_string()),
            ],
            None,
        );
        let token = token();
        service.account(&token).unwrap();
        let (session, batch) = service
            .begin_wave(&token, &WaveCancellation::default())
            .unwrap();
        assert_eq!(
            batch
                .tracks
                .iter()
                .map(|track| track.id.as_str())
                .collect::<Vec<_>>(),
            vec!["42:7", "42", "42:7", "43"]
        );
        assert_eq!(batch.tracks[0].title, "exact synthetic");
        assert_eq!(batch.tracks[1].title, "bare synthetic");
        assert_eq!(batch.tracks[2].title, "exact synthetic");
        assert!(!batch.tracks[3].available);
        assert_eq!(batch.omitted, 1);
        let mut coordinator = WaveCoordinator::new(&session);
        coordinator
            .select(1, &batch.tracks[0], &batch.marker(), Instant::now())
            .unwrap();
        let requests = complete(server, listener);
        assert_eq!(requests.len(), 3);
        assert!(requests[2].ends_with("track-ids=42%3A7&track-ids=42&track-ids=43&track-ids=44"));
    }
}

#[test]
fn normalization_refuses_duplicate_and_unsolicited_canonical_records() {
    for canonical in [track_body(&[42, 42]), track_body(&[42, 99])] {
        let (service, server, listener) = fixture(
            vec![
                (200, ACCOUNT.into()),
                (200, session_body("batch-1", &[42, 43], Some("radio-1"))),
                (200, canonical),
            ],
            None,
        );
        let token = token();
        service.account(&token).unwrap();
        let cancellation = WaveCancellation::default();
        let error = service.begin_wave(&token, &cancellation).unwrap_err();
        assert_eq!(error.stage(), Some(WaveStage::Normalization));
        assert_eq!(error.music_error(), Some(MusicError::InvalidResponse));
        assert_eq!(error.http_status(), Some(200));
        assert!(cancellation.is_cancelled());
        assert_eq!(complete(server, listener).len(), 3);
    }
}

#[test]
fn status_only_unknown_and_terminated_responses_stop_before_track_lookup() {
    for (flag, expected) in [
        ("unknownSession", WaveError::UnknownSession),
        ("terminated", WaveError::Terminated),
    ] {
        for next in [false, true] {
            let terminal = serde_json::json!({"result":{flag:true}}).to_string();
            let mut responses = vec![(200, ACCOUNT.into())];
            if next {
                responses.extend([
                    (200, session_body("batch-1", &[42], Some("radio-1"))),
                    (200, track_body(&[42])),
                ]);
            }
            responses.push((200, terminal));
            let (service, server, listener) = fixture(responses, None);
            let token = token();
            service.account(&token).unwrap();
            let cancellation = WaveCancellation::default();
            if next {
                let (session, _) = service.begin_wave(&token, &cancellation).unwrap();
                let error = service.wave_batch(&token, &session, &[]).unwrap_err();
                assert_eq!(error.stage(), Some(WaveStage::Tracks));
                assert_eq!(error.http_status(), Some(200));
                assert_eq!(error.category(), expected.category());
                assert!(session.is_cancelled());
                assert_eq!(
                    service.wave_batch(&token, &session, &[]).unwrap_err(),
                    WaveError::Cancelled
                );
            } else {
                let error = service.begin_wave(&token, &cancellation).unwrap_err();
                assert_eq!(error.stage(), Some(WaveStage::Session));
                assert_eq!(error.http_status(), Some(200));
                assert_eq!(error.category(), expected.category());
            }
            assert!(cancellation.is_cancelled());
            assert_eq!(complete(server, listener).len(), if next { 4 } else { 2 });
        }
    }
}

#[test]
fn cancellation_and_account_retirement_during_encoding_prevent_dispatch() {
    struct RetireDuringEncode<'a> {
        service: &'a MusicService,
        cancellation: &'a WaveCancellation,
        logout: bool,
    }
    impl Serialize for RetireDuringEncode<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            if self.logout {
                self.service.clear_session();
            } else {
                self.cancellation.cancel();
            }
            serde_json::json!({"seeds":["user:onyourwave"]}).serialize(serializer)
        }
    }
    for logout in [false, true] {
        let (service, server, listener) = fixture(vec![(200, ACCOUNT.into())], None);
        let token = token();
        service.account(&token).unwrap();
        let binding = service.search_binding(&token).unwrap();
        let cancellation = WaveCancellation::default();
        let error = service
            .wave_post(
                &token,
                &cancellation,
                &binding,
                WaveStage::Session,
                &["rotor", "session", "new"],
                &RetireDuringEncode {
                    service: &service,
                    cancellation: &cancellation,
                    logout,
                },
            )
            .unwrap_err();
        assert_eq!(
            error,
            if logout {
                WaveError::Music(MusicError::Unauthenticated)
            } else {
                WaveError::Cancelled
            }
        );
        assert_eq!(complete(server, listener).len(), 1);
    }
}

#[test]
fn measured_clock_excludes_callback_delays_pause_and_completion_wall_time() {
    let (session, marker, track) = pure_session();
    let mut c = WaveCoordinator::new(&session);
    let now = Instant::now();
    c.select(1, &track, &marker, now).unwrap();
    assert!(
        c.observe_measured(
            PlaybackState::Loading,
            Duration::ZERO,
            now + Duration::from_secs(100)
        )
        .unwrap()
        .is_empty()
    );
    let start = c
        .observe_measured(
            PlaybackState::Playing,
            Duration::from_millis(10),
            now + Duration::from_secs(101),
        )
        .unwrap();
    assert_eq!(
        start.iter().map(WaveEvent::kind).collect::<Vec<_>>(),
        vec![WaveEventKind::RadioStarted, WaveEventKind::TrackStarted]
    );
    for (state, millis, seconds) in [
        (PlaybackState::Playing, 250, 1_000),
        (PlaybackState::Paused, 500, 2_000),
        (PlaybackState::Paused, 500, 3_000),
        (PlaybackState::Playing, 600, 5_000),
    ] {
        assert!(
            c.observe_measured(
                state,
                Duration::from_millis(millis),
                now + Duration::from_secs(seconds)
            )
            .unwrap()
            .is_empty()
        );
    }
    let finished = c
        .observe_measured(
            PlaybackState::Ended,
            Duration::from_millis(700),
            now + Duration::from_secs(6_000),
        )
        .unwrap();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].kind(), WaveEventKind::TrackFinished);
    assert_eq!(finished[0].heard(), Duration::from_millis(700));
    assert!(
        c.observe_measured(
            PlaybackState::Ended,
            Duration::from_millis(700),
            now + Duration::from_secs(7_000)
        )
        .unwrap()
        .is_empty()
    );

    // A completed old cycle stays idempotent. An intentional new source cycle
    // resets the absolute worker counter even for the same metadata/marker.
    assert!(
        c.select(1, &track, &marker, now + Duration::from_secs(8_000))
            .unwrap()
            .is_empty()
    );
    assert!(
        c.observe_measured(
            PlaybackState::Playing,
            Duration::from_millis(700),
            now + Duration::from_secs(8_001)
        )
        .unwrap()
        .is_empty()
    );
    c.select(2, &track, &marker, now + Duration::from_secs(9_000))
        .unwrap();
    let restarted = c
        .observe_measured(
            PlaybackState::Playing,
            Duration::ZERO,
            now + Duration::from_secs(9_001),
        )
        .unwrap();
    assert_eq!(restarted.len(), 1);
    assert_eq!(restarted[0].kind(), WaveEventKind::TrackStarted);
    c.observe_measured(
        PlaybackState::Paused,
        Duration::from_millis(25),
        now + Duration::from_secs(9_002),
    )
    .unwrap();
    let closed = c.finish(now + Duration::from_secs(50_000)).unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].kind(), WaveEventKind::Skip);
    assert_eq!(closed[0].heard(), Duration::from_millis(25));
}

#[test]
fn positive_worker_clock_recovers_a_missed_playing_snapshot_without_faking_zero_time() {
    for state in [
        PlaybackState::Ended,
        PlaybackState::Paused,
        PlaybackState::Error,
    ] {
        let (session, marker, track) = pure_session();
        let mut c = WaveCoordinator::new(&session);
        let now = Instant::now();
        c.select(1, &track, &marker, now).unwrap();
        let events = c
            .observe_measured(
                state,
                Duration::from_millis(35),
                now + Duration::from_secs(30),
            )
            .unwrap();
        let mut expected = vec![WaveEventKind::RadioStarted, WaveEventKind::TrackStarted];
        if state != PlaybackState::Paused {
            expected.push(if state == PlaybackState::Ended {
                WaveEventKind::TrackFinished
            } else {
                WaveEventKind::Skip
            });
            assert_eq!(events.last().unwrap().heard(), Duration::from_millis(35));
        }
        assert_eq!(
            events.iter().map(WaveEvent::kind).collect::<Vec<_>>(),
            expected
        );
        if state == PlaybackState::Paused {
            let events = c
                .observe_measured(
                    PlaybackState::Ended,
                    Duration::from_millis(35),
                    now + Duration::from_secs(60),
                )
                .unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].kind(), WaveEventKind::TrackFinished);
            assert_eq!(events[0].heard(), Duration::from_millis(35));
        }
    }
    for state in [
        PlaybackState::Loading,
        PlaybackState::Ready,
        PlaybackState::Paused,
        PlaybackState::Ended,
        PlaybackState::Error,
    ] {
        let (session, marker, track) = pure_session();
        let mut c = WaveCoordinator::new(&session);
        let now = Instant::now();
        c.select(1, &track, &marker, now).unwrap();
        assert!(
            c.observe_measured(state, Duration::ZERO, now + Duration::from_secs(30))
                .unwrap()
                .is_empty()
        );
        assert!(c.finish(now + Duration::from_secs(60)).unwrap().is_empty());
    }
}

#[test]
fn measured_clock_rejects_reset_backwards_time_and_mixed_observation_modes() {
    let (session, marker, track) = pure_session();
    let mut c = WaveCoordinator::new(&session);
    let now = Instant::now();
    c.select(1, &track, &marker, now).unwrap();
    c.observe_measured(PlaybackState::Playing, Duration::from_millis(100), now)
        .unwrap();
    for (total, time) in [
        (Duration::ZERO, now + Duration::from_secs(1)),
        (Duration::from_millis(100), now - Duration::from_millis(1)),
        (
            MAX_HEARD + Duration::from_millis(1),
            now + Duration::from_secs(1),
        ),
    ] {
        assert!(matches!(
            c.observe_measured(PlaybackState::Paused, total, time),
            Err(WaveError::InvalidObservation)
        ));
    }
    assert!(matches!(
        c.observe(PlaybackState::Paused, now + Duration::from_secs(10)),
        Err(WaveError::InvalidObservation)
    ));
    let finished = c
        .observe_measured(
            PlaybackState::Ended,
            Duration::from_millis(150),
            now + Duration::from_secs(20),
        )
        .unwrap();
    assert_eq!(finished[0].heard(), Duration::from_millis(150));

    c.select(2, &track, &marker, now + Duration::from_secs(21))
        .unwrap();
    c.observe(PlaybackState::Loading, now + Duration::from_secs(22))
        .unwrap();
    assert!(matches!(
        c.observe_measured(
            PlaybackState::Playing,
            Duration::ZERO,
            now + Duration::from_secs(23)
        ),
        Err(WaveError::InvalidObservation)
    ));
    assert_eq!(
        c.observe(PlaybackState::Playing, now + Duration::from_secs(23))
            .unwrap()[0]
            .kind(),
        WaveEventKind::TrackStarted
    );
}

#[test]
fn diagnostic_accessors_keep_fixed_categories_and_do_not_invent_http_status() {
    let error = WaveError::Request {
        stage: WaveStage::Feedback,
        error: MusicError::Forbidden,
    };
    assert_eq!(error.stage().map(WaveStage::as_str), Some("feedback"));
    assert_eq!(error.category(), MusicError::Forbidden.category());
    assert_eq!(error.http_status(), Some(403));
    assert_eq!(WaveError::WriteOutcomeUnknown.http_status(), None);
    assert_eq!(WaveError::UnsupportedResponse.http_status(), None);
    assert_eq!(WaveError::Terminated.category(), "session-terminated");
    let safe = format!(
        "{} {} {:?}",
        error.stage().unwrap().as_str(),
        error.category(),
        error.http_status()
    );
    for private in [
        TOKEN,
        "private-radio",
        "private-batch",
        "private-account",
        "synthetic artist",
    ] {
        assert!(!safe.contains(private));
    }
}

#[test]
fn empty_session_and_empty_canonical_lookup_keep_the_actual_failed_stage_and_http() {
    for canonical in [false, true] {
        let mut replies = vec![
            (200, ACCOUNT.into()),
            (
                200,
                session_body(
                    "private-batch",
                    if canonical { &[42] } else { &[] },
                    Some("private-radio"),
                ),
            ),
        ];
        if canonical {
            replies.push((200, r#"{"result":[null]}"#.into()));
        }
        let (service, server, listener) = fixture(replies, None);
        let token = token();
        service.account(&token).unwrap();
        let cancellation = WaveCancellation::default();
        let error = service.begin_wave(&token, &cancellation).unwrap_err();
        assert_eq!(
            error,
            WaveError::Response {
                stage: if canonical {
                    WaveStage::Normalization
                } else {
                    WaveStage::Session
                },
                cause: WaveResponseCause::NoTracks,
                http: 200,
            }
        );
        assert_eq!(error.category(), "no-tracks");
        assert_eq!(error.http_status(), Some(200));
        assert!(cancellation.is_cancelled());
        for private in [TOKEN, "private-batch", "private-radio", "private-account"] {
            assert!(!format!("{error:?} {error}").contains(private));
        }
        assert_eq!(
            complete(server, listener).len(),
            if canonical { 3 } else { 2 }
        );
    }
}

#[test]
fn failed_body_after_headers_keeps_http_and_poisoned_write_is_never_replayed() {
    for feedback in [false, true] {
        let mut responses = vec![(200, ACCOUNT.into())];
        if feedback {
            responses.extend([
                (200, session_body("batch-1", &[42], Some("radio-1"))),
                (200, track_body(&[42])),
            ]);
        }
        responses.push((200, "{}".into()));
        let (service, server, listener) = fixture(
            responses,
            Some(Hook::Truncate(if feedback { 3 } else { 1 })),
        );
        let token = token();
        service.account(&token).unwrap();
        let cancellation = WaveCancellation::default();
        let error = if feedback {
            let (session, batch) = service.begin_wave(&token, &cancellation).unwrap();
            let now = Instant::now();
            let mut c = WaveCoordinator::new(&session);
            c.select(1, &batch.tracks[0], &batch.marker(), now).unwrap();
            let event = c.observe(PlaybackState::Playing, now).unwrap().remove(0);
            let error = service.wave_feedback(&token, &session, &event).unwrap_err();
            assert_eq!(
                service.wave_feedback(&token, &session, &event),
                Err(WaveError::Cancelled)
            );
            error
        } else {
            service.begin_wave(&token, &cancellation).unwrap_err()
        };
        assert_eq!(
            error,
            WaveError::Response {
                stage: if feedback {
                    WaveStage::Feedback
                } else {
                    WaveStage::Session
                },
                cause: WaveResponseCause::WriteOutcomeUnknown,
                http: 200,
            }
        );
        assert!(cancellation.is_cancelled());
        assert_eq!(
            complete(server, listener).len(),
            if feedback { 4 } else { 2 }
        );
    }
}

#[test]
fn canonical_lookup_failure_before_headers_does_not_inherit_session_http_200() {
    let (service, server, listener) = fixture(
        vec![
            (200, ACCOUNT.into()),
            (200, session_body("batch-1", &[42], Some("radio-1"))),
            (200, String::new()),
        ],
        Some(Hook::DropHeaders(2)),
    );
    let token = token();
    service.account(&token).unwrap();
    let cancellation = WaveCancellation::default();
    let error = service.begin_wave(&token, &cancellation).unwrap_err();
    assert_eq!(error.stage(), Some(WaveStage::Normalization));
    assert_eq!(error.music_error(), Some(MusicError::Network));
    assert_eq!(error.http_status(), None);
    assert!(cancellation.is_cancelled());
    assert_eq!(complete(server, listener).len(), 3);
}

// This server sequence models a rejection at any remaining supported event.
// HTTP400 must remain an error even after removing the rejected session-end
// event identified by the owner's separate real-service acceptance.
fn feedback_sequence_with_optional_rejection(rejected: Option<WaveEventKind>) {
    let rejected_request = rejected.map(|kind| match kind {
        WaveEventKind::RadioStarted => 3,
        WaveEventKind::TrackStarted => 16, // First start from the next batch.
        WaveEventKind::Skip => 15,         // Fifth heard occurrence, original batch.
        WaveEventKind::TrackFinished => 17,
    });
    let mut responses = vec![
        (200, ACCOUNT.into()),
        (
            200,
            session_body(
                "private-original-batch",
                &[42, 43, 44, 45, 46],
                Some("private-radio"),
            ),
        ),
        (200, track_body(&[42, 43, 44, 45, 46])),
        (200, r#"{"result":"ok"}"#.into()), // radioStarted
        (200, r#"{"result":"ok"}"#.into()), // first trackStarted
        (200, r#"{"result":"ok"}"#.into()), // first skip
        (200, r#"{"result":"ok"}"#.into()), // second trackStarted
        (
            200,
            session_body("private-next-batch", &[47, 48, 49, 50, 51], None),
        ),
        (200, track_body(&[47, 48, 49, 50, 51])),
    ];
    responses.extend(std::iter::repeat_n((200, r#"{"result":"ok"}"#.into()), 9));
    if let Some(index) = rejected_request {
        responses[index] = (
            400,
            serde_json::json!({"error":{"message":format!(
                "private-response-sentinel {TOKEN} private-radio private-original-batch private-next-batch https://fixture.invalid/private?secret=synthetic"
            )}}).to_string(),
        );
        responses.truncate(index + 1);
    }
    let (service, server, listener) = fixture(responses, None);
    let token = token();
    service.account(&token).unwrap();
    let cancellation = WaveCancellation::default();
    let (session, first) = service.begin_wave(&token, &cancellation).unwrap();
    let mut coordinator = WaveCoordinator::new(&session);
    let now = Instant::now();
    let mut failed_event = None;
    let mut accepted = Vec::new();
    let mut send = |events: Vec<WaveEvent>| {
        for event in events {
            match service.wave_feedback(&token, &session, &event) {
                Ok(ack) => {
                    assert_eq!(ack.http_status, 200);
                    accepted.push(event.kind());
                }
                Err(error) => {
                    assert_eq!(Some(event.kind()), rejected);
                    assert_eq!(error.stage(), Some(WaveStage::Feedback));
                    assert_eq!(error.http_status(), Some(400));
                    assert_eq!(error.category(), "http-rejected");
                    assert_eq!(error.music_error(), Some(MusicError::Http(400)));
                    let safe = format!("{error:?} {error}");
                    for private in [
                        TOKEN,
                        "private-response-sentinel",
                        "private-radio",
                        "private-original-batch",
                        "private-next-batch",
                        "https://fixture.invalid",
                        "synthetic Wave person",
                    ] {
                        assert!(
                            !safe.contains(private),
                            "fixed errors cannot echo wire fields"
                        );
                    }
                    failed_event = Some(event);
                    return false;
                }
            }
        }
        true
    };
    'sequence: {
        let mut next = None;
        for (index, track) in first.tracks.iter().enumerate() {
            let at = now + Duration::from_secs(index as u64 * 2);
            let terminal = coordinator
                .select(index as u64 + 1, track, &first.marker(), at)
                .unwrap();
            if !send(terminal) {
                break 'sequence;
            }
            if !send(
                coordinator
                    .observe_measured(PlaybackState::Playing, Duration::ZERO, at)
                    .unwrap(),
            ) {
                break 'sequence;
            }
            assert!(
                coordinator
                    .observe_measured(
                        PlaybackState::Paused,
                        Duration::from_millis(1250),
                        at + Duration::from_millis(1250),
                    )
                    .unwrap()
                    .is_empty()
            );
            if index == 1 {
                // As in the reported accepted prefix, a later batch is admitted
                // while feedback still belongs to original-batch occurrences.
                next = Some(
                    service
                        .wave_batch(
                            &token,
                            &session,
                            &first
                                .tracks
                                .iter()
                                .map(|track| track.id.clone())
                                .collect::<Vec<_>>(),
                        )
                        .unwrap(),
                );
            }
        }
        let next = next.expect("the next batch was admitted before the fifth skip");
        if !send(
            coordinator
                .select(
                    6,
                    &next.tracks[0],
                    &next.marker(),
                    now + Duration::from_secs(10),
                )
                .unwrap(),
        ) {
            break 'sequence;
        }
        if !send(
            coordinator
                .observe_measured(
                    PlaybackState::Playing,
                    Duration::ZERO,
                    now + Duration::from_secs(10),
                )
                .unwrap(),
        ) {
            break 'sequence;
        }
        if !send(
            coordinator
                .observe_measured(
                    PlaybackState::Ended,
                    Duration::from_millis(2250),
                    now + Duration::from_secs(13),
                )
                .unwrap(),
        ) {
            break 'sequence;
        }
        assert!(
            coordinator
                .observe_measured(
                    PlaybackState::Ended,
                    Duration::from_millis(2250),
                    now + Duration::from_secs(14)
                )
                .unwrap()
                .is_empty(),
            "EOS cannot emit another terminal report"
        );
        assert!(
            coordinator
                .finish(now + Duration::from_secs(15))
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            coordinator.finish(now + Duration::from_secs(16)),
            Err(WaveError::InvalidObservation)
        ));
    }
    if let Some(event) = failed_event {
        assert!(session.is_cancelled());
        assert!(cancellation.is_cancelled());
        assert_eq!(
            service.wave_feedback(&token, &session, &event),
            Err(WaveError::Cancelled)
        );
        assert_eq!(
            service.wave_batch(&token, &session, &[]).unwrap_err(),
            WaveError::Cancelled
        );
        assert_eq!(
            coordinator
                .finish(now + Duration::from_secs(17))
                .unwrap_err(),
            WaveError::Cancelled
        );
        assert_eq!(
            service.begin_wave(&token, &cancellation).unwrap_err(),
            WaveError::Cancelled
        );
    } else {
        assert!(rejected.is_none());
        assert!(!session.is_cancelled());
        assert_eq!(
            accepted
                .iter()
                .filter(|kind| **kind == WaveEventKind::RadioStarted)
                .count(),
            1
        );
        assert_eq!(
            accepted
                .iter()
                .filter(|kind| **kind == WaveEventKind::TrackStarted)
                .count(),
            6
        );
        assert_eq!(
            accepted
                .iter()
                .filter(|kind| **kind == WaveEventKind::Skip)
                .count(),
            5
        );
        assert_eq!(
            accepted
                .iter()
                .filter(|kind| **kind == WaveEventKind::TrackFinished)
                .count(),
            1
        );
        coordinator.cancel();
    }
    let requests = complete(server, listener);
    assert_eq!(
        requests.len(),
        rejected_request.map_or(18, |index| index + 1)
    );
    if let Some(index) = rejected_request {
        let request = json_body(&requests[index]);
        assert_eq!(request["event"]["type"], rejected.unwrap().as_str());
        assert_eq!(
            request["batchId"],
            if index < 16 {
                "private-original-batch"
            } else {
                "private-next-batch"
            }
        );
        if index == 16 {
            assert_eq!(request["event"]["trackId"], "47");
            assert_eq!(request["event"]["totalPlayedSeconds"], 0.0);
        }
    } else {
        assert_eq!(
            json_body(&requests[15])["batchId"],
            "private-original-batch"
        );
        assert_eq!(
            json_body(&requests[15])["event"]["totalPlayedSeconds"],
            1.25
        );
        assert_eq!(json_body(&requests[16])["batchId"], "private-next-batch");
        assert_eq!(
            json_body(&requests[17])["event"]["totalPlayedSeconds"],
            2.25
        );
    }
}

#[test]
fn feedback_accepts_the_complete_cross_batch_sequence_without_duplicate_terminals() {
    feedback_sequence_with_optional_rejection(None);
}

#[test]
fn feedback_400_for_each_event_is_precise_redacted_and_never_replayed() {
    for kind in [
        WaveEventKind::RadioStarted,
        WaveEventKind::TrackStarted,
        WaveEventKind::Skip,
        WaveEventKind::TrackFinished,
    ] {
        feedback_sequence_with_optional_rejection(Some(kind));
    }
}

#[test]
fn local_retirement_drains_only_track_feedback_then_allows_a_fresh_session() {
    // Source replacement and MPRIS Stop share this coordinator contract. The
    // native controller fixture separately exercises their actual entrypoints.
    for (snapshot, heard) in [
        (None, false),
        (Some(PlaybackState::Loading), false),
        (Some(PlaybackState::Ready), false),
        (Some(PlaybackState::Paused), false),
        (Some(PlaybackState::Ended), false),
        (Some(PlaybackState::Error), false),
        (Some(PlaybackState::Playing), true),
        (Some(PlaybackState::Paused), true),
        (Some(PlaybackState::Ended), true),
        (Some(PlaybackState::Stopped), true),
    ] {
        let accepted = (200, r#"{"result":"ok"}"#.into());
        let mut responses = vec![
            (200, ACCOUNT.into()),
            (
                200,
                session_body("original-batch", &[42], Some("old-radio")),
            ),
            (200, track_body(&[42])),
        ];
        if heard {
            responses.extend([accepted.clone(), accepted.clone()]);
            responses.push((200, session_body("prefetched-batch", &[43], None)));
            responses.push((200, track_body(&[43])));
            responses.push(accepted.clone());
        }
        responses.push((200, session_body("fresh-batch", &[44], Some("new-radio"))));
        responses.push((200, track_body(&[44])));
        responses.extend([accepted.clone(), accepted.clone(), accepted]);
        let expected_requests = responses.len();
        let (service, server, listener) = fixture(responses, None);
        let token = token();
        service.account(&token).unwrap();
        let cancellation = WaveCancellation::default();
        let (session, batch) = service.begin_wave(&token, &cancellation).unwrap();
        let mut coordinator = WaveCoordinator::new(&session);
        let now = Instant::now();
        let mut reports = Vec::new();
        if let Some(snapshot) = snapshot {
            coordinator
                .select(1, &batch.tracks[0], &batch.marker(), now)
                .unwrap();
            if heard {
                let started = coordinator
                    .observe_measured(PlaybackState::Playing, Duration::ZERO, now)
                    .unwrap();
                assert_eq!(
                    started.iter().map(WaveEvent::kind).collect::<Vec<_>>(),
                    [WaveEventKind::RadioStarted, WaveEventKind::TrackStarted]
                );
                for event in &started {
                    service.wave_feedback(&token, &session, event).unwrap();
                }
                reports.extend(started);
                // Fetching a new batch cannot replace the original marker of
                // an interrupted or naturally completed current occurrence.
                let next = service
                    .wave_batch(&token, &session, &["42".into()])
                    .unwrap();
                assert_eq!(next.marker.batch_id.as_str(), "prefetched-batch");
            }
            let observed = coordinator
                .observe_measured(
                    snapshot,
                    if heard {
                        Duration::from_millis(1250)
                    } else {
                        Duration::ZERO
                    },
                    now + Duration::from_secs(100),
                )
                .unwrap();
            for event in &observed {
                service.wave_feedback(&token, &session, event).unwrap();
            }
            reports.extend(observed);
        }
        let terminal = coordinator
            .finish(now + Duration::from_secs(10_000))
            .unwrap();
        let interrupted = heard
            && matches!(
                snapshot,
                Some(PlaybackState::Playing | PlaybackState::Paused)
            );
        assert_eq!(
            terminal.len(),
            usize::from(interrupted),
            "{snapshot:?}, heard={heard}"
        );
        for event in &terminal {
            service.wave_feedback(&token, &session, event).unwrap();
        }
        reports.extend(terminal);
        if heard {
            assert_eq!(reports.len(), 3);
            let terminal = reports.last().unwrap();
            assert_eq!(
                terminal.kind(),
                if snapshot == Some(PlaybackState::Ended) {
                    WaveEventKind::TrackFinished
                } else {
                    WaveEventKind::Skip
                }
            );
            assert_eq!(terminal.track_id.as_deref(), Some("42"));
            assert_eq!(terminal.marker.batch_id.as_str(), "original-batch");
            assert_eq!(
                terminal.heard(),
                Duration::from_millis(1250),
                "pause, completion delay and declared duration cannot inflate actual Playing"
            );
        } else {
            assert!(
                reports.is_empty(),
                "unheard/never-selected retirement cannot fabricate feedback"
            );
        }
        assert!(
            !session.is_cancelled(),
            "the original identity must survive until its final skip drains"
        );
        assert!(matches!(
            coordinator.finish(now + Duration::from_secs(10_001)),
            Err(WaveError::InvalidObservation)
        ));
        assert!(matches!(
            coordinator.observe_measured(
                PlaybackState::Playing,
                Duration::from_millis(1250),
                now + Duration::from_secs(10_002)
            ),
            Err(WaveError::InvalidObservation)
        ));
        coordinator.cancel();
        assert!(session.is_cancelled());
        assert!(matches!(
            coordinator.finish(now + Duration::from_secs(10_003)),
            Err(WaveError::Cancelled)
        ));
        assert!(matches!(
            service.wave_batch(&token, &session, &[]),
            Err(WaveError::Cancelled)
        ));
        if let Some(event) = reports.last() {
            assert_eq!(
                service.wave_feedback(&token, &session, event),
                Err(WaveError::Cancelled)
            );
        }
        assert!(matches!(
            service.begin_wave(&token, &cancellation),
            Err(WaveError::Cancelled)
        ));

        // A deliberate later start has a distinct one-shot local identity.
        // Retiring the old identity neither recreates it nor prevents this.
        let fresh_cancellation = WaveCancellation::default();
        let (fresh, batch) = service.begin_wave(&token, &fresh_cancellation).unwrap();
        assert!(!Arc::ptr_eq(&session.0, &fresh.0));
        let mut fresh_coordinator = WaveCoordinator::new(&fresh);
        let at = now + Duration::from_secs(20_000);
        fresh_coordinator
            .select(1, &batch.tracks[0], &batch.marker(), at)
            .unwrap();
        let started = fresh_coordinator
            .observe_measured(PlaybackState::Playing, Duration::from_millis(25), at)
            .unwrap();
        for event in started {
            service.wave_feedback(&token, &fresh, &event).unwrap();
        }
        let terminal = fresh_coordinator
            .finish(at + Duration::from_secs(1))
            .unwrap();
        assert_eq!(terminal.len(), 1);
        assert_eq!(terminal[0].kind(), WaveEventKind::Skip);
        assert_eq!(terminal[0].heard(), Duration::from_millis(25));
        service.wave_feedback(&token, &fresh, &terminal[0]).unwrap();
        fresh_coordinator.cancel();
        let requests = complete(server, listener);
        assert_eq!(requests.len(), expected_requests);
        let feedback = requests
            .iter()
            .filter(|request| request.lines().next().unwrap().contains("/feedback "))
            .map(|request| json_body(request))
            .collect::<Vec<_>>();
        assert_eq!(feedback.len(), if heard { 6 } else { 3 });
        assert!(feedback.iter().all(|body| matches!(
            body["event"]["type"].as_str(),
            Some("radioStarted" | "trackStarted" | "trackFinished" | "skip")
        )));
        if heard {
            assert_eq!(feedback[2]["batchId"], "original-batch");
            assert_eq!(feedback[2]["event"]["totalPlayedSeconds"], 1.25);
        }
    }
}
