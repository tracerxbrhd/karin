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
    ] {
        let (service, server, listener) =
            fixture(vec![(200, ACCOUNT.into()), (status, body.into())], None);
        let token = token();
        service.account(&token).unwrap();
        let cancel = WaveCancellation::default();
        let error = service.begin_wave(&token, &cancel).unwrap_err();
        assert_eq!(error, expected);
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
        assert_eq!(service.begin_wave(&token, &cancel).unwrap_err(), expected);
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
            Err(WaveError::WriteOutcomeUnknown)
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
    assert_eq!(end.len(), 1);
    assert_eq!(end[0].kind(), WaveEventKind::RadioFinished);
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
            Err(WaveError::UnsupportedResponse)
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
