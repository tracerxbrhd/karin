// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::sync::atomic::AtomicBool;

fn pair() -> (Capture, CaptureReader) {
    channel(Cancellation(Arc::new(AtomicBool::new(false))))
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
