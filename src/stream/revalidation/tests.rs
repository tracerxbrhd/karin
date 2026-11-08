// SPDX-License-Identifier: GPL-3.0-or-later
//! Local header-only responses; no Music account, recording or CDN is used.

use super::*;
use crate::stream::StreamResolver;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Instant,
};

fn setup() -> (TcpListener, StreamResolver, RemotePlaybackSource, Metadata) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}/", listener.local_addr().unwrap());
    let source = RemotePlaybackSource::from_test_resolver(
        &format!("{origin}get-mp3/synthetic/resource"),
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let policy = ResponsePolicy::parse(
        200,
        [
            ("content-type", "audio/mpeg"),
            ("content-length", "3"),
            ("etag", "\"synthetic-version-one\""),
            ("cache-control", "private,no-cache,must-revalidate"),
        ]
        .map(|(name, value)| (name.into(), value.into())),
    )
    .unwrap();
    let mut sha = [0; 32];
    sha.copy_from_slice(ring::digest::digest(&ring::digest::SHA256, b"abc").as_ref());
    let metadata = policy
        .record(
            source.http_cache_uri_hash(),
            source.http_cache_resource_hash(),
            sha,
            3,
        )
        .unwrap();
    (
        listener,
        StreamResolver::for_test(&origin, &origin, &origin),
        source,
        metadata,
    )
}

fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
        assert!(bytes.len() < 8192);
    }
    let request = String::from_utf8(bytes).unwrap().to_ascii_lowercase();
    for forbidden in ["authorization:", "cookie:", "origin:", "referer:"] {
        assert!(!request.contains(forbidden));
    }
    assert!(request.contains("accept-encoding: identity\r\n"));
    assert!(request.contains("cache-control: no-cache\r\n"));
    assert!(request.contains(concat!(
        "user-agent: karin/",
        env!("CARGO_PKG_VERSION"),
        " playback\r\n"
    )));
    request
}

fn token() -> AccessToken {
    AccessToken::for_test("synthetic-token-never-sent", Duration::from_secs(60))
}

#[test]
fn head_revalidation_sends_only_same_resource_condition_and_no_credentials() {
    let (listener, resolver, source, metadata) = setup();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert!(request.starts_with("head /get-mp3/synthetic/resource http/1.1\r\n"));
        assert!(request.contains("if-none-match: \"synthetic-version-one\"\r\n"));
        assert!(!request.contains("range:"));
        stream
            .write_all(b"HTTP/1.1 304 Not Modified\r\nETag: \"synthetic-version-one\"\r\nCache-Control: private,no-cache\r\nContent-Length: 3\r\nConnection: close\r\n\r\n")
            .unwrap();
    });
    assert_eq!(
        resolver.revalidate_http(&source, &metadata, &token(), &DeliveryCancellation::new()),
        Ok(())
    );
    server.join().unwrap();
}

#[test]
fn unsupported_head_can_use_only_a_one_byte_partial_response() {
    let (listener, resolver, source, metadata) = setup();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert!(read_request(&mut stream).starts_with("head "));
        stream
            .write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
        drop(stream);
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert!(request.starts_with("get "));
        assert!(request.contains("range: bytes=0-0\r\n"));
        assert!(request.contains("if-none-match: \"synthetic-version-one\"\r\n"));
        stream
            .write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Type: audio/mpeg\r\nContent-Length: 1\r\nContent-Range: bytes 0-0/3\r\nETag: \"synthetic-version-one\"\r\nConnection: close\r\n\r\na")
            .unwrap();
    });
    assert_eq!(
        resolver.revalidate_http(&source, &metadata, &token(), &DeliveryCancellation::new()),
        Ok(())
    );
    server.join().unwrap();
}

#[test]
fn range_ignored_response_is_dropped_without_reading_its_body() {
    let (listener, resolver, source, metadata) = setup();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert!(read_request(&mut stream).starts_with("head "));
        stream
            .write_all(
                b"HTTP/1.1 501 Not Implemented\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        drop(stream);
        let (mut stream, _) = listener.accept().unwrap();
        assert!(read_request(&mut stream).contains("range: bytes=0-0\r\n"));
        // Deliberately do not supply the advertised body. A body-reading
        // implementation would wait for the probe timeout instead of returning
        // the specific range-unsupported classification.
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: 64000000\r\nETag: \"synthetic-version-one\"\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap_or(0), 0);
    });
    assert_eq!(
        resolver.revalidate_http(&source, &metadata, &token(), &DeliveryCancellation::new()),
        Err(Reason::RangeUnsupported)
    );
    server.join().unwrap();
}

#[test]
fn cancelled_probe_sends_no_request_and_redirects_are_never_followed() {
    let (listener, resolver, source, metadata) = setup();
    let cancellation = DeliveryCancellation::new();
    cancellation.cancel();
    assert_eq!(
        resolver.revalidate_http(&source, &metadata, &token(), &cancellation),
        Err(Reason::Cancelled)
    );
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err());
    listener.set_nonblocking(false).unwrap();
    let second = TcpListener::bind("127.0.0.1:0").unwrap();
    second.set_nonblocking(true).unwrap();
    let destination = format!("http://{}/untrusted", second.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert!(read_request(&mut stream).starts_with("head "));
        write!(stream, "HTTP/1.1 302 Found\r\nLocation: {destination}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    assert_eq!(
        resolver.revalidate_http(&source, &metadata, &token(), &DeliveryCancellation::new()),
        Err(Reason::UnsupportedStatus)
    );
    server.join().unwrap();
    assert!(second.accept().is_err());
}

#[test]
fn new_resource_uses_a_full_representation_digest_without_forwarding_old_validator() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let (listener, resolver, _, metadata) = setup();
    let address = listener.local_addr().unwrap();
    let source = RemotePlaybackSource::from_test_resolver(
        &format!("http://{address}/get-mp3/new/synthetic-file"),
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert!(request.starts_with("head /get-mp3/new/synthetic-file "));
        assert!(!request.contains("if-none-match:") && !request.contains("if-modified-since:"));
        let sha = ring::digest::digest(&ring::digest::SHA256, b"abc");
        let digest = STANDARD.encode(sha.as_ref());
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: 3\r\nRepr-Digest: sha-256=:{digest}:\r\nConnection: close\r\n\r\n").unwrap();
    });
    assert_eq!(
        resolver.revalidate_http(&source, &metadata, &token(), &DeliveryCancellation::new()),
        Ok(())
    );
    server.join().unwrap();
}

#[test]
fn cancellation_during_request_retires_an_otherwise_matching_response() {
    let (listener, resolver, source, metadata) = setup();
    let (started, observed) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert!(read_request(&mut stream).starts_with("head "));
        started.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(2)).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: 3\r\nETag: \"synthetic-version-one\"\r\nConnection: close\r\n\r\n")
            .unwrap();
    });
    let cancellation = DeliveryCancellation::new();
    let worker_cancellation = cancellation.clone();
    let worker = thread::spawn(move || {
        resolver.revalidate_http(&source, &metadata, &token(), &worker_cancellation)
    });
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    cancellation.cancel();
    release.send(()).unwrap();
    assert_eq!(worker.join().unwrap(), Err(Reason::Cancelled));
    server.join().unwrap();
}
