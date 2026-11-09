// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::sync::atomic::AtomicBool;

fn pair() -> (Capture, CaptureReader) {
    channel(Cancellation(Arc::new(AtomicBool::new(false))))
}

fn http_pair() -> (Capture, CaptureReader) {
    let pair = channel_http(Cancellation(Arc::new(AtomicBool::new(false))));
    pair.0.confirm_playing();
    pair
}

fn http_headers(capture: &Capture, fields: &[(&str, &str)]) {
    gst::init().unwrap();
    let mut response = gst::Structure::builder("response-headers")
        .field("Content-Type", "audio/mpeg")
        .field("Content-Length", "6")
        .build();
    for (name, value) in fields {
        response.set(*name, *value);
    }
    capture.headers(
        &gst::Structure::builder("http-headers")
            .field("http-status-code", 200u32)
            .field(
                "uri",
                "https://example.invalid/private?token=synthetic-uri-secret",
            )
            .field(
                "request-headers",
                gst::Structure::builder("request-headers")
                    .field("Authorization", "synthetic-auth-secret")
                    .field("Cookie", "synthetic-cookie-secret")
                    .build(),
            )
            .field("response-headers", response)
            .build(),
    );
}

#[test]
fn private_http_metadata_is_available_only_for_the_complete_played_response() {
    let (capture, mut reader) = http_pair();
    http_headers(
        &capture,
        &[
            ("ETag", "\"original-v1\""),
            ("Cache-Control", "private, no-cache, must-revalidate"),
            ("Set-Cookie", "synthetic-set-cookie-secret"),
        ],
    );
    assert!(capture.completed_http_policy().is_none());
    buffer(&capture, 0, b"onetwo");
    // Exact Content-Length already establishes a completed response; a native
    // flush/drop before its source EOS must not discard the received bytes.
    assert!(capture.is_complete());
    assert!(capture.completed_http_policy().is_some());
    capture.eos();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"onetwo");
    let policy = capture.completed_http_policy().unwrap();
    assert_eq!(policy.complete_mp3().unwrap(), 6);
    let text = format!("{policy:?}");
    for forbidden in [
        "synthetic-uri-secret",
        "synthetic-auth-secret",
        "synthetic-cookie-secret",
        "synthetic-set-cookie-secret",
        "original-v1",
    ] {
        assert!(!text.contains(forbidden));
    }
    // A completed source can be released without discarding its proof. Clear
    // and account retirement use the independent cancellation fence instead.
    capture.interrupt();
    assert!(capture.completed_http_policy().is_some());
    capture.0.cancellation.cancel();
    assert!(capture.completed_http_policy().is_none());
}

#[test]
fn private_http_refusals_retire_only_the_optional_capture() {
    for fields in [
        vec![("Cache-Control", "private, no-store")],
        vec![("Vary", "Cookie")],
        vec![("content-length", "6")],
        vec![("Cache-Control", "max-age=not-a-number")],
    ] {
        let (capture, mut reader) = http_pair();
        http_headers(&capture, &fields);
        buffer(&capture, 0, b"onetwo");
        capture.eos();
        assert!(reader.read_to_end(&mut Vec::new()).is_err());
        assert!(capture.completed_http_policy().is_none());
        assert!(capture.http_rejection().is_some());
        // No request or pipeline handle is owned by the capture. Its failure
        // state cannot stop the player or rewrite buffers/events.
        assert_eq!(capture.0.status.load(Ordering::Acquire), ABORTED);
    }
}

#[test]
fn private_http_complete_storage_and_future_reuse_are_distinct_decisions() {
    let (capture, mut reader) = http_pair();
    http_headers(&capture, &[("ETag", "\"original-v1\"")]);
    buffer(&capture, 0, b"one");
    capture.eos();
    assert!(reader.read_to_end(&mut Vec::new()).is_err());
    assert!(capture.completed_http_policy().is_none());

    // Complete cacheable bytes can be retained without a validator. A later
    // fresh response may provide Repr-Digest, but these bytes alone never prove
    // that the next authorized source is unchanged.
    let (capture, mut reader) = http_pair();
    http_headers(&capture, &[]);
    buffer(&capture, 0, b"onetwo");
    capture.eos();
    reader.read_to_end(&mut Vec::new()).unwrap();
    let policy = capture.completed_http_policy().unwrap();
    let stored = policy.record([1; 32], [2; 32], [3; 32], 6).unwrap();
    assert_eq!(
        policy.revalidate(&stored, [1; 32], [2; 32]),
        Err(Reason::ValidatorsMissing)
    );
}

#[test]
fn native_duplicate_response_header_containers_preserve_policy_and_bounds() {
    gst::init().unwrap();
    for use_list in [false, true] {
        for no_store in [false, true] {
            let (capture, mut reader) = http_pair();
            let controls = [
                "private",
                "must-revalidate",
                if no_store { "no-store" } else { "no-cache" },
            ];
            let controls = if use_list {
                gst::glib::SendValue::from_owned(gst::List::new(controls))
            } else {
                gst::glib::SendValue::from_owned(gst::Array::new(controls))
            };
            let mut response = gst::Structure::builder("response-headers")
                .field("Content-Type", "audio/mpeg")
                .field("Content-Length", "6")
                .field("ETag", "\"original-v1\"")
                .field("Vary", gst::Array::new(["Accept-Encoding"]))
                .build();
            response.set_value("Cache-Control", controls);
            capture.headers(
                &gst::Structure::builder("http-headers")
                    .field("http-status-code", 200u32)
                    .field("response-headers", response)
                    .build(),
            );
            buffer(&capture, 0, b"onetwo");
            capture.eos();
            let bytes = reader.read_to_end(&mut Vec::new());
            if no_store {
                assert!(bytes.is_err());
                assert!(capture.completed_http_policy().is_none());
                assert_eq!(capture.http_rejection(), Some(Reason::NoStore));
            } else {
                assert_eq!(bytes.unwrap(), 6);
                let policy = capture.completed_http_policy().unwrap();
                assert!(policy.directives.private);
                assert!(policy.directives.no_cache);
                assert!(policy.directives.must_revalidate);
            }
        }
    }
    // Exact-length fields remain singleton even when native code represents
    // their duplicate values as an array. Container shape cannot hide a
    // conflicting directive or amplify the bounded metadata allocation.
    for (name, value) in [
        ("Content-Length", gst::Array::new(["6", "6"])),
        ("Cache-Control", gst::Array::new(vec!["private"; 65])),
        ("Cache-Control", gst::Array::new(["x".repeat(4097)])),
    ] {
        let (capture, mut reader) = http_pair();
        let mut response = gst::Structure::builder("response-headers")
            .field("Content-Type", "audio/mpeg")
            .field("Content-Length", "6")
            .build();
        response.set(name, value);
        capture.headers(
            &gst::Structure::builder("http-headers")
                .field("http-status-code", 200u32)
                .field("response-headers", response)
                .build(),
        );
        assert!(reader.read_to_end(&mut Vec::new()).is_err());
        assert!(capture.completed_http_policy().is_none());
    }
}

fn headers(capture: &Capture, length: usize, extra: Option<(&str, &str)>) {
    gst::init().unwrap();
    let mut response = gst::Structure::builder("response-headers")
        .field("Content-Type", "audio/mpeg")
        .field("Content-Length", length.to_string())
        .build();
    if let Some((name, value)) = extra {
        response.set(name, value);
    }
    capture.headers(
        &gst::Structure::builder("http-headers")
            .field("http-status-code", 200u32)
            .field("response-headers", response)
            .build(),
    );
}
fn buffer(capture: &Capture, offset: u64, bytes: &[u8]) {
    let mut buffer = gst::Buffer::from_mut_slice(bytes.to_vec());
    buffer.get_mut().unwrap().set_offset(offset);
    capture.buffer(&buffer);
}

#[test]
fn complete_linear_bytes_are_reused_without_another_transport() {
    let (capture, mut reader) = pair();
    headers(&capture, 6, None);
    buffer(&capture, 0, b"one");
    buffer(&capture, 3, b"two");
    capture.eos();
    capture.interrupt(); // Completed source may be released before writer drains.
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"onetwo");
}

#[test]
fn one_supported_response_fits_before_writer_start_even_with_small_source_buffers() {
    let (capture, mut reader) = channel_http(Cancellation(Arc::new(AtomicBool::new(false))));
    headers(&capture, MAX_AUDIO as usize, None);
    let bytes = [0x5a; 4096];
    // The old slot-per-native-buffer queue overflowed long before this finite
    // body finished. No writer, Playing acknowledgment or source EOS is needed
    // to receive the full supported response without delaying the producer.
    for offset in (0..MAX_AUDIO).step_by(bytes.len()) {
        buffer(&capture, offset, &bytes);
    }
    assert_eq!(SLOTS * CHUNK, MAX_AUDIO as usize);
    assert!(capture.is_complete());
    assert!(!capture.has_played());
    assert_eq!(capture.failure(), None);
    assert!(capture.completed_http_policy().is_none());
    capture.event(&gst::event::FlushStart::new());
    capture.interrupt_with(CaptureFailure::PipelineDropped);
    assert!(capture.is_complete());
    capture.confirm_playing();
    let mut total = 0u64;
    let mut output = [0; 8192];
    loop {
        let count = reader.read(&mut output).unwrap();
        if count == 0 {
            break;
        }
        assert!(output[..count].iter().all(|byte| *byte == 0x5a));
        total += count as u64;
    }
    assert_eq!(total, MAX_AUDIO);
    assert!(capture.completed_http_policy().is_some());
    assert_eq!(capture.failure(), None);
}

#[test]
fn preroll_writer_drains_complete_bytes_but_waits_for_actual_playing_before_eof() {
    let (capture, mut reader) = channel_http(Cancellation(Arc::new(AtomicBool::new(false))));
    http_headers(&capture, &[("ETag", "\"original-v1\"")]);
    buffer(&capture, 0, b"onetwo");
    assert!(capture.is_complete());
    let (drained, body_drained) = mpsc::channel();
    let (waiting, eof_waiting) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut body = [0; 6];
        reader.read_exact(&mut body).unwrap();
        drained.send(body).unwrap();
        let polls = std::cell::Cell::new(0);
        let result = reader.read_checked(&mut [0; 1], || {
            polls.set(polls.get() + 1);
            if polls.get() == 2 {
                waiting.send(()).unwrap();
            }
            true
        });
        done.send(result).unwrap();
    });
    assert_eq!(
        body_drained.recv_timeout(Duration::from_secs(2)).unwrap(),
        *b"onetwo"
    );
    // A second predicate poll is an observed blocked EOF state, not a sleep
    // used to guess which callback happened first.
    eof_waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(finished.try_recv(), Err(TryRecvError::Empty)));
    assert!(capture.completed_http_policy().is_none());
    capture.confirm_playing();
    assert_eq!(
        finished
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap(),
        0
    );
    worker.join().unwrap();
    assert!(capture.completed_http_policy().is_some());
    assert_eq!(capture.failure(), None);
}

#[test]
fn pipeline_retirement_releases_unplayed_complete_capture_but_preserves_played_body() {
    for played in [false, true] {
        let (capture, mut reader) = channel_http(Cancellation(Arc::new(AtomicBool::new(false))));
        http_headers(&capture, &[("ETag", "\"original-v1\"")]);
        buffer(&capture, 0, b"onetwo");
        assert!(capture.is_complete());
        // The early writer can drain the entire response during preroll. Only
        // its EOF remains gated on a worker-confirmed Playing transition.
        let mut body = [0; 6];
        reader.read_exact(&mut body).unwrap();
        assert_eq!(body, *b"onetwo");
        if played {
            capture.confirm_playing();
        }
        capture.retire_pipeline();
        if played {
            assert!(capture.is_complete());
            assert_eq!(reader.read(&mut [0; 1]).unwrap(), 0);
            assert!(capture.completed_http_policy().is_some());
            assert_eq!(capture.failure(), None);
        } else {
            // No request cancellation or account retirement is needed to
            // release the writer after a decoder/output failure. Late native
            // events cannot revive or publish the abandoned unplayed body.
            assert!(reader.read(&mut [0; 1]).is_err());
            assert_eq!(capture.failure(), Some(CaptureFailure::PipelineDropped));
            capture.confirm_playing();
            capture.eos();
            assert!(!capture.is_complete());
            assert!(capture.completed_http_policy().is_none());
            assert!(reader.read(&mut [0; 1]).is_err());
        }
    }
}

#[test]
fn retirement_observer_finishes_a_queued_count_marker_but_never_revives_aborted_input() {
    let (capture, mut reader) = pair();
    headers(&capture, 6, None);
    buffer(&capture, 0, b"onetwo");
    // Deterministically expose the exact producer instruction boundary:
    // complete bytes are genuinely queued and received==expected, while the
    // final RECEIVING→COMPLETE CAS has not yet been observed.
    capture.0.status.store(RECEIVING, Ordering::Release);
    capture.interrupt_with(CaptureFailure::PipelineDropped);
    assert!(capture.is_complete());
    assert_eq!(capture.failure(), None);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"onetwo");

    // A queued-count observation must never revive a capture with an earlier
    // failure, even if another producer observation arrives after retirement.
    let (capture, reader) = pair();
    headers(&capture, 6, None);
    capture.interrupt_with(CaptureFailure::Seek);
    capture.0.received.store(6, Ordering::Release);
    assert!(!capture.is_complete());
    assert_eq!(capture.failure(), Some(CaptureFailure::Seek));
    drop(reader);
}

#[test]
fn destructive_cancellation_retires_an_unplayed_complete_writer_waiting_for_eof() {
    let (capture, mut reader) = channel_http(Cancellation(Arc::new(AtomicBool::new(false))));
    http_headers(&capture, &[]);
    buffer(&capture, 0, b"onetwo");
    let (waiting, ready) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        reader.read_exact(&mut [0; 6]).unwrap();
        let polls = std::cell::Cell::new(0);
        let result = reader.read_checked(&mut [0; 1], || {
            polls.set(polls.get() + 1);
            if polls.get() == 2 {
                waiting.send(()).unwrap();
            }
            true
        });
        done.send(result).unwrap();
    });
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    capture.0.cancellation.cancel();
    assert!(
        finished
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .is_err()
    );
    worker.join().unwrap();
    capture.confirm_playing();
    assert!(capture.completed_http_policy().is_none());
}

#[test]
fn repeated_segment_size_updates_are_not_discontinuities_but_remapping_is() {
    gst::init().unwrap();
    let (capture, mut reader) = pair();
    headers(&capture, 6, None);
    let first = gst::FormattedSegment::<gst::format::Bytes>::new();
    capture.event(&gst::event::Segment::new(&first));
    buffer(&capture, 0, b"one");
    let mut known_size = first.clone();
    known_size.set_duration(gst::format::Bytes::from_u64(6));
    known_size.set_stop(gst::format::Bytes::from_u64(6));
    capture.event(&gst::event::Segment::new(&known_size));
    buffer(&capture, 3, b"two");
    capture.event(&gst::event::FlushStart::new());
    capture.interrupt_with(CaptureFailure::PipelineDropped);
    assert_eq!(capture.failure(), None);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"onetwo");

    let (capture, mut reader) = pair();
    headers(&capture, 6, None);
    capture.event(&gst::event::Segment::new(&first));
    buffer(&capture, 0, b"one");
    let mut remapped = first;
    remapped.set_start(gst::format::Bytes::from_u64(3));
    capture.event(&gst::event::Segment::new(&remapped));
    assert!(reader.read_to_end(&mut Vec::new()).is_err());
    assert_eq!(capture.failure(), Some(CaptureFailure::Segment));
}

#[test]
fn first_capture_failure_survives_followup_eos_and_pipeline_drop() {
    gst::init().unwrap();
    let seek = gst::event::Seek::new(
        1.0,
        gst::SeekFlags::FLUSH,
        gst::SeekType::Set,
        gst::format::Bytes::from_u64(1),
        gst::SeekType::None,
        gst::format::Bytes::from_u64(0),
    );
    for (event, expected) in [
        (seek, CaptureFailure::Seek),
        (gst::event::FlushStart::new(), CaptureFailure::Flush),
    ] {
        let (capture, mut reader) = pair();
        headers(&capture, 6, None);
        buffer(&capture, 0, b"one");
        capture.event(&event);
        capture.eos();
        capture.interrupt_with(CaptureFailure::PipelineDropped);
        assert!(reader.read_to_end(&mut Vec::new()).is_err());
        assert_eq!(capture.failure(), Some(expected));
    }
    for (offset, expected) in [
        (u64::MAX, CaptureFailure::OffsetUnknown),
        (1, CaptureFailure::OffsetMismatch),
    ] {
        let (capture, reader) = pair();
        headers(&capture, 6, None);
        buffer(&capture, offset, b"one");
        capture.eos();
        drop(reader);
        assert_eq!(capture.failure(), Some(expected));
    }
}

#[test]
fn incomplete_reordered_or_noncontiguous_bytes_never_complete() {
    for offset in [0, 1, 4] {
        let (capture, mut reader) = pair();
        headers(&capture, 6, None);
        buffer(&capture, offset, b"one");
        capture.eos();
        assert!(reader.read_to_end(&mut Vec::new()).is_err());
    }
}

#[test]
fn redirects_partial_encoded_and_no_store_responses_are_not_copied() {
    for extra in [
        Some(("Cache-Control", "private, no-store")),
        Some(("Content-Encoding", "identity")),
        Some(("Content-Range", "bytes 0-5/6")),
        Some(("Content-Type", "text/html")),
        Some(("Content-Length", "18446744073709551616")),
    ] {
        let (capture, mut reader) = pair();
        headers(&capture, 6, extra);
        buffer(&capture, 0, b"onetwo");
        capture.eos();
        assert!(reader.read_to_end(&mut Vec::new()).is_err());
    }
    for status in [206u32, 302, 403] {
        let (capture, mut reader) = pair();
        capture.headers(
            &gst::Structure::builder("http-headers")
                .field("http-status-code", status)
                .build(),
        );
        assert!(reader.read_to_end(&mut Vec::new()).is_err());
    }
}

#[test]
fn a_slow_writer_or_oversized_resource_abandons_copy_without_blocking_player() {
    // Exercise the actual nonblocking overflow branch with a deliberately
    // smaller test queue, rather than exhausting an unsupported response size.
    let (capture, mut reader) =
        create_channel_with_slots(Cancellation(Arc::new(AtomicBool::new(false))), false, 2);
    headers(&capture, 3 * CHUNK, None);
    let bytes = vec![1; CHUNK];
    for index in 0..3 {
        buffer(&capture, (index * CHUNK) as u64, &bytes);
    }
    assert_eq!(capture.0.status.load(Ordering::Acquire), ABORTED);
    assert_eq!(capture.failure(), Some(CaptureFailure::BufferOverflow));
    assert!(reader.read_to_end(&mut Vec::new()).is_err());
    let (capture, mut reader) = pair();
    headers(&capture, MAX_AUDIO as usize + 1, None);
    assert!(reader.read_to_end(&mut Vec::new()).is_err());
}

#[test]
fn cancellation_seek_or_failed_pipeline_cannot_publish_partial_bytes() {
    for cancel in [true, false] {
        let (capture, mut reader) = pair();
        headers(&capture, 6, None);
        buffer(&capture, 0, b"one");
        if cancel {
            capture.0.cancellation.cancel();
        } else {
            capture.interrupt();
        }
        assert!(reader.read_to_end(&mut Vec::new()).is_err());
    }
}

#[test]
fn an_idle_reader_rechecks_the_writers_lease_without_waiting_for_data() {
    let (capture, mut reader) = pair();
    let current = Arc::new(AtomicBool::new(true));
    let (entered, ready) = mpsc::channel();
    let (done, result) = mpsc::channel();
    let worker_current = current.clone();
    let worker = std::thread::spawn(move || {
        let sent = AtomicBool::new(false);
        let read = reader.read_checked(&mut [0; 1], || {
            if !sent.swap(true, Ordering::AcqRel) {
                entered.send(()).unwrap();
            }
            worker_current.load(Ordering::Acquire)
        });
        done.send(read).unwrap();
    });
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    current.store(false, Ordering::Release);
    let received = result.recv_timeout(Duration::from_secs(2));
    capture.0.cancellation.cancel();
    assert!(received.unwrap().is_err());
    worker.join().unwrap();
}

fn range_headers(capture: &Capture, length: usize, range: &str, extra: Option<(&str, &str)>) {
    gst::init().unwrap();
    let mut response = gst::Structure::builder("response-headers")
        .field("Content-Type", "audio/mpeg")
        .field("Content-Length", length.to_string())
        .field("Content-Range", range)
        .build();
    if let Some((name, value)) = extra {
        response.set(name, value);
    }
    capture.headers(
        &gst::Structure::builder("http-headers")
            .field("http-status-code", 206u32)
            .field("response-headers", response)
            .build(),
    );
}

#[test]
fn a_full_206_response_is_one_complete_representation_not_range_assembly() {
    let (capture, mut reader) = pair();
    range_headers(&capture, 6, "bytes 0-5/6", None);
    buffer(&capture, 0, b"one");
    buffer(&capture, 3, b"two");
    capture.eos();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"onetwo");
}

#[test]
fn partial_unknown_duplicate_or_inconsistent_206_responses_are_not_complete() {
    for range in [
        "bytes 1-5/6",
        "bytes 0-4/6",
        "bytes 0-5/7",
        "bytes 0-5/*",
        "bytes */6",
        "bytes 0-5/6, bytes 0-5/6",
        "bytes 0-18446744073709551615/18446744073709551616",
    ] {
        let (capture, mut reader) = pair();
        range_headers(&capture, 6, range, None);
        buffer(&capture, 0, b"onetwo");
        capture.eos();
        assert!(reader.read_to_end(&mut Vec::new()).is_err());
    }
    for extra in [
        ("Content-Encoding", "identity"),
        ("Cache-Control", "no-store"),
        ("content-range", "bytes 0-5/6"),
        ("content-length", "6"),
    ] {
        let (capture, mut reader) = pair();
        range_headers(&capture, 6, "bytes 0-5/6", Some(extra));
        buffer(&capture, 0, b"onetwo");
        capture.eos();
        assert!(reader.read_to_end(&mut Vec::new()).is_err());
    }
    // A 206 has the same strict full-length/EOS and offset checks as a 200.
    let (capture, mut reader) = pair();
    range_headers(&capture, 6, "bytes 0-5/6", None);
    buffer(&capture, 0, b"one");
    capture.eos();
    assert!(reader.read_to_end(&mut Vec::new()).is_err());
}

#[test]
fn too_many_native_response_fields_abandon_capture() {
    gst::init().unwrap();
    let (capture, mut reader) = pair();
    let mut response = gst::Structure::builder("response-headers")
        .field("Content-Type", "audio/mpeg")
        .field("Content-Length", "6")
        .build();
    for index in 0..63 {
        response.set(format!("X-Synthetic-{index}"), "ignored");
    }
    capture.headers(
        &gst::Structure::builder("http-headers")
            .field("http-status-code", 200u32)
            .field("response-headers", response)
            .build(),
    );
    assert!(reader.read_to_end(&mut Vec::new()).is_err());
}

#[test]
fn native_eos_proof_is_separate_from_http_completion_and_survives_teardown() {
    let (capture, mut reader) = http_pair();
    http_headers(&capture, &[("ETag", "\"original-v1\"")]);
    buffer(&capture, 0, b"one");
    assert!(capture.can_wait_for_completion());
    assert!(!capture.can_complete_in_background());
    capture.eos();
    assert!(!capture.is_complete());
    assert_eq!(capture.failure(), Some(CaptureFailure::IncompleteEos));
    // Source EOS alone, or an application's guessed Ended state, is not proof
    // that the existing player actually completed its audible pipeline.
    assert!(!capture.has_pipeline_eos());
    assert!(!capture.can_complete_in_background());
    capture.confirm_pipeline_eos();
    assert!(capture.can_complete_in_background());
    capture.retire_pipeline();
    assert!(capture.has_pipeline_eos());
    assert!(capture.can_complete_in_background());
    assert_eq!(capture.failure(), Some(CaptureFailure::IncompleteEos));
    assert!(reader.read_to_end(&mut Vec::new()).is_err());
    assert!(capture.completed_http_policy().is_none());
}

#[test]
fn explicit_seek_and_failed_pipeline_cannot_admit_background_completion() {
    for user_seek in [false, true] {
        let (capture, _reader) = http_pair();
        http_headers(&capture, &[]);
        buffer(&capture, 0, b"one");
        // A demuxer's earlier native flush must not hide a later explicit user
        // seek. The first capture cause and the user's cancellation are distinct.
        capture.event(&gst::event::FlushStart::new());
        assert_eq!(capture.failure(), Some(CaptureFailure::Flush));
        assert!(capture.can_wait_for_completion());
        if user_seek {
            capture.user_seek();
            capture.confirm_pipeline_eos();
        }
        capture.retire_pipeline();
        assert_eq!(capture.failure(), Some(CaptureFailure::Flush));
        assert!(!capture.can_wait_for_completion());
        assert!(!capture.can_complete_in_background());
    }
    let (capture, _reader) = http_pair();
    http_headers(&capture, &[]);
    buffer(&capture, 0, b"one");
    // A native parser discontinuity discards the prefix. After real playback
    // EOS it may attempt an independently checked whole response, not splice.
    capture.event(&gst::event::FlushStart::new());
    capture.confirm_pipeline_eos();
    capture.retire_pipeline();
    assert!(capture.can_complete_in_background());
    capture.0.cancellation.cancel();
    assert!(!capture.can_wait_for_completion());
    assert!(!capture.can_complete_in_background());
}

#[test]
fn malformed_and_refused_http_policy_never_become_background_storage_permission() {
    for fields in [
        vec![("Cache-Control", "private, no-store")],
        vec![
            ("Content-Type", "not-a-supported-type"),
            ("Cache-Control", "no-store"),
        ],
        vec![("Content-Length", "invalid")],
    ] {
        let (capture, _reader) = http_pair();
        http_headers(&capture, &fields);
        capture.confirm_pipeline_eos();
        assert!(capture.http_rejection().is_some());
        assert!(!capture.can_wait_for_completion());
        assert!(!capture.can_complete_in_background());
    }
    for (status, expected) in [
        (403u32, Reason::AccessDenied),
        (451, Reason::ContentRestricted),
    ] {
        let (capture, _reader) = http_pair();
        capture.headers(
            &gst::Structure::builder("http-headers")
                .field("http-status-code", status)
                .build(),
        );
        capture.confirm_pipeline_eos();
        assert_eq!(capture.http_rejection(), Some(expected));
        assert!(!capture.can_wait_for_completion());
    }
    let (capture, _reader) = http_pair();
    range_headers(&capture, 3, "bytes 0-2/6", None);
    assert_eq!(capture.http_rejection(), Some(Reason::PartialResponse));
    assert!(capture.can_wait_for_completion());
    // The later response's explicit refusal must not be hidden by an earlier
    // partial-response classification or a parser's first capture failure.
    http_headers(&capture, &[("Cache-Control", "no-store")]);
    capture.confirm_pipeline_eos();
    assert_eq!(capture.http_rejection(), Some(Reason::NoStore));
    assert!(!capture.can_complete_in_background());

    let (capture, _reader) = http_pair();
    range_headers(&capture, 3, "bytes 0-2/6", None);
    assert_eq!(capture.http_rejection(), Some(Reason::PartialResponse));
    let response = gst::Structure::builder("response-headers")
        .field("Content-Type", "audio/mpeg")
        .field("Content-Length", "6")
        .field("Cache-Control", 123u32)
        .build();
    capture.headers(
        &gst::Structure::builder("http-headers")
            .field("http-status-code", 200u32)
            .field("response-headers", response)
            .build(),
    );
    assert_eq!(capture.http_rejection(), Some(Reason::HeadersInvalid));
    capture.confirm_pipeline_eos();
    assert!(!capture.can_wait_for_completion());
    assert!(!capture.can_complete_in_background());
}

#[test]
fn capture_observations_distinguish_probe_headers_progress_and_native_eos_safely() {
    gst::init().unwrap();
    let (capture, _reader) = http_pair();
    let initial = capture.observations();
    assert!(initial.contains(&CaptureObservation::Probe(false)));
    assert!(initial.contains(&CaptureObservation::Headers(false)));
    assert!(initial.contains(&CaptureObservation::Started(false)));
    assert!(initial.contains(&CaptureObservation::Progress(CompletionBand::Unknown)));
    let source = gst::ElementFactory::make("souphttpsrc").build().unwrap();
    capture.attach(&source);
    http_headers(&capture, &[("ETag", "\"original-v1\"")]);
    let mut first = gst::Buffer::from_mut_slice(b"one".to_vec());
    first.get_mut().unwrap().set_offset(0);
    first
        .get_mut()
        .unwrap()
        .set_flags(gst::BufferFlags::DISCONT);
    capture.buffer(&first);
    let observed = capture.observations();
    assert!(observed.contains(&CaptureObservation::Probe(true)));
    assert!(observed.contains(&CaptureObservation::Headers(true)));
    assert!(observed.contains(&CaptureObservation::Http(HttpObservation::Complete200)));
    assert!(observed.contains(&CaptureObservation::Length(true)));
    assert!(observed.contains(&CaptureObservation::Started(true)));
    assert!(observed.contains(&CaptureObservation::Progress(
        CompletionBand::HalfToThreeQuarters
    )));
    assert!(observed.contains(&CaptureObservation::Discontinuity(true)));
    assert_eq!(capture.failure(), None); // Initial DISCONT is a normal source flag.
    capture.eos();
    assert!(
        capture
            .observations()
            .contains(&CaptureObservation::SourceEos(true))
    );
    assert!(
        capture
            .observations()
            .contains(&CaptureObservation::PlaybackEos(false))
    );
    capture.confirm_pipeline_eos();
    assert!(
        capture
            .observations()
            .contains(&CaptureObservation::PlaybackEos(true))
    );
    for observation in capture.observations() {
        let value = observation.as_str();
        assert!(
            value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        );
        assert!(!value.contains("synthetic"));
        assert!(!value.contains("original-v1"));
        assert!(!value.contains("http://"));
        assert!(!value.contains("https://"));
    }
    let (wrong, _reader) = http_pair();
    wrong.attach(&gst::ElementFactory::make("fakesrc").build().unwrap());
    assert!(
        wrong
            .observations()
            .contains(&CaptureObservation::Probe(false))
    );
    assert_eq!(wrong.failure(), Some(CaptureFailure::SourceAttachment));
}

#[test]
fn completeness_is_reported_in_fixed_bands_not_exact_resource_sizes() {
    for (bytes, band) in [
        (0, CompletionBand::Zero),
        (1, CompletionBand::UnderQuarter),
        (24, CompletionBand::UnderQuarter),
        (25, CompletionBand::QuarterToHalf),
        (49, CompletionBand::QuarterToHalf),
        (50, CompletionBand::HalfToThreeQuarters),
        (74, CompletionBand::HalfToThreeQuarters),
        (75, CompletionBand::ThreeQuartersToFull),
        (99, CompletionBand::ThreeQuartersToFull),
        (100, CompletionBand::Complete),
    ] {
        let (capture, _reader) = http_pair();
        headers(&capture, 100, None);
        if bytes > 0 {
            buffer(&capture, 0, &vec![1; bytes]);
        }
        assert!(
            capture
                .observations()
                .contains(&CaptureObservation::Progress(band))
        );
    }
}

#[test]
fn whole_private_http_206_remains_complete_while_partial_206_never_publishes() {
    let (capture, mut reader) = http_pair();
    range_headers(
        &capture,
        6,
        "bytes 0-5/6",
        Some(("ETag", "\"original-v1\"")),
    );
    buffer(&capture, 0, b"onetwo");
    capture.eos();
    assert!(capture.is_complete());
    assert_eq!(reader.read_to_end(&mut Vec::new()).unwrap(), 6);
    assert_eq!(
        capture
            .response_http_policy()
            .unwrap()
            .complete_mp3()
            .unwrap(),
        6
    );
    assert!(capture.completed_http_policy().is_some());
    assert!(!capture.can_complete_in_background());
    assert!(
        capture
            .observations()
            .contains(&CaptureObservation::Http(HttpObservation::Partial206))
    );

    let (partial, mut reader) = http_pair();
    range_headers(
        &partial,
        3,
        "bytes 0-2/6",
        Some(("ETag", "\"original-v1\"")),
    );
    let original_policy = partial.response_http_policy().unwrap();
    assert_eq!(original_policy.etag.as_deref(), Some("\"original-v1\""));
    assert_eq!(original_policy.representation_length(), Some(6));
    assert_eq!(original_policy.complete_mp3(), Err(Reason::PartialResponse));
    assert!(
        partial
            .observations()
            .contains(&CaptureObservation::Length(true))
    );
    assert!(
        partial
            .observations()
            .contains(&CaptureObservation::Progress(CompletionBand::Zero))
    );
    buffer(&partial, 0, b"one");
    partial.eos();
    assert!(reader.read_to_end(&mut Vec::new()).is_err());
    assert!(!partial.is_complete());
    assert!(partial.completed_http_policy().is_none());
}

#[test]
fn late_native_header_policy_fences_a_sealed_background_write_before_publication() {
    use crate::audio_cache::{
        AudioCache, CacheError, OnlineStreamAuthority, PrivateHttpCachePolicy, ResourceKey,
        Sandbox, account,
    };

    for no_store in [false, true] {
        let sandbox = Sandbox::create(&std::env::temp_dir()).unwrap();
        let authority = Arc::new(OnlineStreamAuthority::with_storage_permission(Arc::new(
            PrivateHttpCachePolicy,
        )));
        let cache = AudioCache::open_namespace(sandbox.clone(), authority, 1, 100_000).unwrap();
        let lease = cache.bind(Some(account(1))).unwrap().unwrap();
        let key = ResourceKey::track("101").unwrap();
        let check = cache.begin_online_check(&lease, key).unwrap();
        cache.observe_verified_online(&lease, check).unwrap();
        let mut request = cache.begin(&lease, key).unwrap();
        let (capture, _reader) = channel_http(request.cancellation());
        cache
            .retain_capture(&mut request, capture.clone(), || true)
            .unwrap();
        http_headers(&capture, &[("ETag", "\"original-v1\"")]);
        buffer(&capture, 0, b"one");
        capture.confirm_playing();
        capture.eos();
        capture.confirm_pipeline_eos();
        let completion = cache.completion_permit(&request).unwrap();
        completion.allow().unwrap();
        completion.current().unwrap();
        let policy = capture.response_http_policy().unwrap();

        // A freshly fetched whole body reaches metadata finalization. Inject a
        // genuine bounded native header structure at that exact checkpoint;
        // the old sealed ticket must not waive a later refusal/malformed policy.
        let result = cache.put_reader_http_attempt(&request, b"onetwo".as_slice(), |sha, bytes| {
            if no_store {
                http_headers(&capture, &[("Cache-Control", "no-store")]);
            } else {
                let headers = gst::Structure::builder("response-headers")
                    .field("Content-Type", "audio/mpeg")
                    .field("Content-Length", "6")
                    .field("Cache-Control", 123u32)
                    .build();
                capture.headers(
                    &gst::Structure::builder("http-headers")
                        .field("http-status-code", 200u32)
                        .field("response-headers", headers)
                        .build(),
                );
            }
            assert_eq!(completion.current(), Err(CacheError::HttpPolicyRequired));
            policy
                .record([1; 32], [2; 32], sha, bytes)
                .map_err(|_| CacheError::HttpPolicyRequired)
        });
        assert_eq!(result, Err(CacheError::HttpPolicyRequired));
        assert_eq!(cache.stats().unwrap().entries, 0);
        assert_eq!(std::fs::read_dir(&sandbox.path).unwrap().count(), 1);
    }
}
