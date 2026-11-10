// SPDX-License-Identifier: GPL-3.0-or-later
//! Loopback-only transport checks; completed media is validated by AudioCache.
use super::*;
use crate::stream::{
    StreamResolver,
    tests::{accepted, receive},
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

fn setup() -> (TcpListener, StreamResolver, RemotePlaybackSource) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}/", listener.local_addr().unwrap());
    let remote = RemotePlaybackSource::from_test_resolver(
        &format!("{origin}get-mp3/synthetic/private-resource"),
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let resolver = StreamResolver::for_test(&origin, &origin, &origin);
    (listener, resolver, remote)
}
fn token() -> AccessToken {
    AccessToken::for_test("synthetic-token-never-sent", Duration::from_secs(60))
}
fn assert_safe_request(request: &str) {
    let request = request.to_ascii_lowercase();
    assert!(request.starts_with("get /get-mp3/synthetic/private-resource "));
    assert!(request.contains("accept-encoding: identity\r\n"));
    assert!(request.contains("cache-control: no-cache\r\n"));
    assert!(request.contains(concat!(
        "user-agent: karin/",
        env!("CARGO_PKG_VERSION"),
        " playback\r\n"
    )));
    for forbidden in [
        "authorization:",
        "cookie:",
        "referer:",
        "origin:",
        "range:",
        "if-match:",
        "if-unmodified-since:",
        "synthetic-token-never-sent",
    ] {
        assert!(
            !request.contains(forbidden),
            "credential-free unconditional full GET only"
        );
    }
}
#[test]
fn selected_source_get_is_unconditional_and_returns_stream_without_reading_body() {
    let (listener, resolver, remote) = setup();
    let server = thread::spawn(move || {
        let mut stream = accepted(&listener);
        assert_safe_request(&receive(&mut stream));
        // No body is sent. Returning the response must require headers only.
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: 1000000\r\nETag: \"synthetic-version\"\r\nConnection: close\r\n\r\n").unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap_or(0), 0);
    });
    let (response, policy) = resolver
        .download_response(&remote, &token(), &DeliveryCancellation::new())
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(policy.complete_mp3(), Ok(1000000));
    assert!(policy.has_validator());
    drop(response);
    server.join().unwrap();
}
#[test]
fn refusals_and_unsupported_http_responses_retain_only_fixed_reason_and_status() {
    for (status, headers, expected) in [
        (403, "", Reason::AccessDenied),
        (410, "", Reason::UnsupportedStatus),
        (451, "", Reason::ContentRestricted),
        (200, "Cache-Control: no-store\r\n", Reason::NoStore),
        (
            200,
            "Content-Encoding: gzip\r\n",
            Reason::UnsupportedEncoding,
        ),
        (
            206,
            "Content-Range: bytes 1-2/3\r\n",
            Reason::PartialResponse,
        ),
    ] {
        let (listener, resolver, remote) = setup();
        let server = thread::spawn(move || {
            let mut stream = accepted(&listener);
            assert_safe_request(&receive(&mut stream));
            write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: audio/mpeg\r\nContent-Length: 2\r\nETag: \"synthetic-version\"\r\n{headers}Connection: close\r\n\r\n").unwrap();
        });
        let error = resolver
            .download_response(&remote, &token(), &DeliveryCancellation::new())
            .unwrap_err();
        assert_eq!(
            error,
            DownloadError {
                reason: expected,
                status: Some(status)
            }
        );
        let visible = format!("{error:?}");
        for private in [
            "private-resource",
            "synthetic-token",
            "synthetic-version",
            "http://",
            "127.0.0.1",
        ] {
            assert!(!visible.contains(private));
        }
        server.join().unwrap();
    }
}
#[test]
fn cancellation_and_token_expiry_prevent_request_and_redirect_is_not_followed() {
    let (listener, resolver, remote) = setup();
    let cancellation = DeliveryCancellation::new();
    cancellation.cancel();
    for (token, cancellation) in [
        (token(), cancellation),
        (
            AccessToken::for_test("synthetic-expired-token", Duration::ZERO),
            DeliveryCancellation::new(),
        ),
    ] {
        assert_eq!(
            resolver
                .download_response(&remote, &token, &cancellation)
                .unwrap_err(),
            DownloadError {
                reason: Reason::Cancelled,
                status: None
            }
        );
    }
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err());
    listener.set_nonblocking(false).unwrap();
    let second = TcpListener::bind("127.0.0.1:0").unwrap();
    let target = format!("http://{}/untrusted", second.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut stream = accepted(&listener);
        assert_safe_request(&receive(&mut stream));
        write!(stream, "HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    assert_eq!(
        resolver
            .download_response(&remote, &token(), &DeliveryCancellation::new())
            .unwrap_err(),
        DownloadError {
            reason: Reason::UnsupportedStatus,
            status: Some(302)
        }
    );
    server.join().unwrap();
    second.set_nonblocking(true).unwrap();
    assert!(second.accept().is_err());
}
#[test]
fn transport_inherits_the_existing_client_deadline() {
    let (listener, _resolver, remote) = setup();
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(100))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .unwrap();
    let (ready, started) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let mut stream = accepted(&listener);
        assert_safe_request(&receive(&mut stream));
        ready.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(2)).unwrap();
    });
    let worker =
        thread::spawn(move || response(&client, &remote, &token(), &DeliveryCancellation::new()));
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    let result = worker.join().unwrap();
    release.send(()).unwrap();
    server.join().unwrap();
    assert_eq!(
        result.unwrap_err(),
        DownloadError {
            reason: Reason::Timeout,
            status: None
        }
    );
}

#[test]
fn cancellation_during_request_rejects_the_response_before_body_processing() {
    let (listener, resolver, remote) = setup();
    let (started, observed) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let mut stream = accepted(&listener);
        assert_safe_request(&receive(&mut stream));
        started.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(2)).unwrap();
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: 1000000\r\nETag: \"synthetic-version\"\r\nConnection: close\r\n\r\n").unwrap();
    });
    let cancellation = DeliveryCancellation::new();
    let worker_cancellation = cancellation.clone();
    let worker =
        thread::spawn(move || resolver.download_response(&remote, &token(), &worker_cancellation));
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    cancellation.cancel();
    release.send(()).unwrap();
    assert_eq!(
        worker.join().unwrap().unwrap_err(),
        DownloadError {
            reason: Reason::Cancelled,
            status: Some(200)
        }
    );
    server.join().unwrap();
}

#[test]
fn disconnected_server_yields_safe_network_category_without_raw_transport_error() {
    let (listener, resolver, remote) = setup();
    drop(listener);
    let error = resolver
        .download_response(&remote, &token(), &DeliveryCancellation::new())
        .unwrap_err();
    assert_eq!(
        error,
        DownloadError {
            reason: Reason::Network,
            status: None
        }
    );
    assert_eq!(
        format!("{error:?}"),
        "DownloadError { reason: Network, status: None }"
    );
}
