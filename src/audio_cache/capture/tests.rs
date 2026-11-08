// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::sync::atomic::AtomicBool;

fn pair() -> (Capture, CaptureReader) {
    channel(Cancellation(Arc::new(AtomicBool::new(false))))
}

fn http_pair() -> (Capture, CaptureReader) {
    channel_http(Cancellation(Arc::new(AtomicBool::new(false))))
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
    assert!(capture.completed_http_policy().is_none());
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
    let (capture, mut reader) = pair();
    headers(&capture, (SLOTS + 1) * CHUNK, None);
    let bytes = vec![1; CHUNK];
    for index in 0..=SLOTS {
        buffer(&capture, (index * CHUNK) as u64, &bytes);
    }
    assert_eq!(capture.0.status.load(Ordering::Acquire), ABORTED);
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
