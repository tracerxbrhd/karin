// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic loopback delivery metadata. No service account or audio content.

use super::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Instant,
};

const PRIVATE: &str = "synthetic-private-delivery-value";

fn token() -> AccessToken {
    AccessToken::for_test(PRIVATE, Duration::from_secs(3600))
}

fn receive(stream: &mut TcpStream) -> String {
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
    String::from_utf8(bytes).unwrap()
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn accepted(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("expected a synthetic local request: {error}"),
        }
    }
}

fn validated_service(token: &AccessToken) -> MusicService {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let service = MusicService::for_test(&format!("http://{}/", listener.local_addr().unwrap()));
    let server = thread::spawn(move || {
        let mut stream = accepted(&listener);
        let _ = receive(&mut stream);
        respond(
            &mut stream,
            200,
            r#"{"result":{"account":{"uid":1234,"displayName":"Synthetic account"}}}"#,
        );
    });
    service.account(token).unwrap();
    server.join().unwrap();
    service
}

fn fixture(
    status: u16,
    body: String,
    delay: Duration,
) -> (DeliveryProbe, thread::JoinHandle<String>, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let retained = listener.try_clone().unwrap();
    let origin = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let mut stream = accepted(&listener);
        let request = receive(&mut stream);
        thread::sleep(delay);
        respond(&mut stream, status, &body);
        request
    });
    (
        DeliveryProbe {
            client: Client::builder()
                .no_proxy()
                .connect_timeout(Duration::from_millis(100))
                .timeout(Duration::from_secs(2))
                .redirect(Policy::none())
                .retry(reqwest::retry::never())
                .build()
                .unwrap(),
            origin,
            metadata_fixture_origin: None,
        },
        server,
        retained,
    )
}

fn check_no_second_request(listener: TcpListener) {
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "no retry, descriptor follow-up or media request"
    );
}

fn legacy_body(url: &str) -> String {
    json!({
        "result":[{"codec":"mp3","bitrateInKbps":192,"downloadInfoUrl":url,"direct":true,"preview":false}],
        "invocationInfo":{"requestId":PRIVATE}
    })
    .to_string()
}

fn check_redacted(report: DeliveryReport) {
    let visible = format!("{report}\n{report:?}");
    for secret in [PRIVATE, "storage.example", "track-private", "phone-private"] {
        assert!(!visible.contains(secret), "only fixed report fields");
    }
    assert!(!visible.contains("https://"));
    assert!(!visible.contains("OAuth "));
}

#[test]
fn metadata_probe_uses_existing_bound_session_and_never_follows_descriptor() {
    let token = token();
    let service = validated_service(&token);
    let descriptor = TcpListener::bind("127.0.0.1:0").unwrap();
    let body = legacy_body(&format!(
        "http://{}/track-private?secret={PRIVATE}",
        descriptor.local_addr().unwrap()
    ));
    let (probe, server, retained) = fixture(200, body, Duration::ZERO);
    let report = probe.probe(&service, &token, "42:7", &DeliveryCancellation::new());
    assert_eq!(report.category, DeliveryCategory::MetadataOriginUnverified);
    assert_eq!(report.http_status, Some(200));
    assert_eq!(report.resource, DeliveryResource::InsecureDescriptor);
    assert!(report.formats.mp3 && report.direct_hint);
    assert!(!report.protection_required);
    assert!(report.to_string().contains("protection=not-established"));
    let request = server.join().unwrap();
    assert!(request.starts_with("GET /tracks/42:7/download-info HTTP/1.1\r\n"));
    let request = request.to_ascii_lowercase();
    assert!(request.contains(&format!("authorization: oauth {PRIVATE}\r\n")));
    for absent in [
        "client_secret",
        "x-yandex-music-client",
        "cookie:",
        "referer:",
        "origin:",
        "sign=",
    ] {
        assert!(!request.contains(absent));
    }
    check_no_second_request(retained);
    check_no_second_request(descriptor);
    check_redacted(report);
}

#[test]
fn session_required_invalid_track_and_pre_cancel_make_no_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let service = MusicService::for_test(&format!("http://{}/", listener.local_addr().unwrap()));
    let probe = DeliveryProbe::new().unwrap();
    let token = token();
    assert_eq!(
        probe
            .probe(&service, &token, "42", &DeliveryCancellation::new())
            .category,
        DeliveryCategory::SessionRequired
    );
    let service = validated_service(&token);
    for invalid in [
        "",
        "42/private",
        "..",
        "42?phone-private",
        "42::7",
        ":42",
        "42:",
    ] {
        let report = probe.probe(&service, &token, invalid, &DeliveryCancellation::new());
        assert_eq!(report.category, DeliveryCategory::InvalidTrack);
        check_redacted(report);
    }
    let cancellation = DeliveryCancellation::new();
    cancellation.cancel();
    assert_eq!(
        probe.probe(&service, &token, "42", &cancellation).category,
        DeliveryCategory::Cancelled
    );
    check_no_second_request(listener);
}

#[test]
fn different_token_cannot_reuse_validated_account() {
    let token = token();
    let service = validated_service(&token);
    let other = AccessToken::for_test("synthetic-other-token", Duration::from_secs(3600));
    let report =
        DeliveryProbe::new()
            .unwrap()
            .probe(&service, &other, "42", &DeliveryCancellation::new());
    assert_eq!(report.category, DeliveryCategory::SessionRequired);
}

#[test]
fn legacy_direct_flag_does_not_prove_a_playable_or_unprotected_source() {
    let report = classify(
        legacy_body("https://storage.example/track-private?token=synthetic-private-delivery-value")
            .as_bytes(),
        200,
    );
    assert_eq!(report.category, DeliveryCategory::LegacyNegotiationRequired);
    assert_eq!(report.resource, DeliveryResource::HttpsDescriptor);
    assert!(report.direct_hint);
    assert!(report.to_string().contains("protection=not-established"));
    check_redacted(report);
}

#[test]
fn protection_values_are_skipped_and_never_used_or_echoed() {
    for field in [
        "key",
        "decryptionKey",
        "decryption_key",
        "drm",
        "encryption",
    ] {
        let body = json!({"result":[{"codec":"flac",field:PRIVATE}]}).to_string();
        let report = classify(body.as_bytes(), 200);
        assert_eq!(report.category, DeliveryCategory::ProtectionRequired);
        assert!(report.protection_required);
        assert!(report.formats.flac);
        check_redacted(report);
    }
}

#[test]
fn empty_unknown_malformed_and_oversized_variants_remain_distinct() {
    assert_eq!(
        classify(br#"{"result":[]}"#, 200).category,
        DeliveryCategory::NoDeliveryOptions
    );
    for body in [
        "{}".to_owned(),
        r#"{"result":{"downloadInfo":{"urls":["https://storage.example/track-private"]}}}"#
            .to_owned(),
        "not-json phone-private".to_owned(),
        json!({"result":vec![json!({"codec":"mp3","downloadInfoUrl":PRIVATE}); MAX_OPTIONS + 1]})
            .to_string(),
    ] {
        let report = classify(body.as_bytes(), 200);
        assert_eq!(report.category, DeliveryCategory::UnsupportedResponse);
        check_redacted(report);
    }
}

#[test]
fn resource_credentials_fragment_custom_scheme_and_unbounded_url_are_not_accepted() {
    for resource in [
        format!("https://phone-private:{PRIVATE}@storage.example/resource"),
        format!("https://storage.example/resource#{PRIVATE}"),
        format!("file:///track-private/{PRIVATE}"),
        format!("data:text/plain,{PRIVATE}"),
        PRIVATE.to_owned(),
        format!("https://storage.example/{}", "x".repeat(MAX_RESOURCE_URL)),
    ] {
        let report = classify(legacy_body(&resource).as_bytes(), 200);
        assert_eq!(report.category, DeliveryCategory::UnsafeResource);
        check_redacted(report);
    }
}

#[test]
fn fixed_http_rejections_ignore_personal_provider_messages_and_do_not_retry() {
    for (status, body, expected) in [
        (401, PRIVATE, DeliveryCategory::AccessDenied),
        (403, PRIVATE, DeliveryCategory::Forbidden),
        (429, PRIVATE, DeliveryCategory::RateLimited),
        (451, PRIVATE, DeliveryCategory::Restricted),
        (500, PRIVATE, DeliveryCategory::HttpRejected),
        (302, PRIVATE, DeliveryCategory::HttpRejected),
    ] {
        let token = token();
        let service = validated_service(&token);
        let (probe, server, retained) = fixture(status, body.into(), Duration::ZERO);
        let report = probe.probe(&service, &token, "42", &DeliveryCancellation::new());
        assert_eq!(report.category, expected);
        assert_eq!(report.http_status, Some(status));
        check_redacted(report);
        server.join().unwrap();
        check_no_second_request(retained);
    }
}

#[test]
fn only_known_exact_forbidden_code_can_report_missing_scopes() {
    for body in [
        json!({"error":{"name":"missing-required-scopes","message":PRIVATE}}).to_string(),
        json!({"error":"missing-required-scopes","message":PRIVATE}).to_string(),
    ] {
        assert_eq!(
            forbidden_category(body.as_bytes()),
            DeliveryCategory::MissingScopes
        );
    }
    for body in [
        json!({"error":{"name":"missing-required-scopes-private","message":PRIVATE}}).to_string(),
        json!({"error":{"name":"unknown","message":"missing-required-scopes"}}).to_string(),
        json!({"error":{"name":PRIVATE},"url":PRIVATE}).to_string(),
        "not-json".to_owned(),
    ] {
        assert_eq!(
            forbidden_category(body.as_bytes()),
            DeliveryCategory::Forbidden
        );
    }
}

#[test]
fn bounded_body_is_reported_without_returning_its_contents() {
    let token = token();
    let service = validated_service(&token);
    let (probe, server, retained) = fixture(200, PRIVATE.repeat(MAX_BODY), Duration::ZERO);
    let report = probe.probe(&service, &token, "42", &DeliveryCancellation::new());
    assert_eq!(report.category, DeliveryCategory::ResponseTooLarge);
    check_redacted(report);
    server.join().unwrap();
    check_no_second_request(retained);
}

#[test]
fn cancellation_and_logout_discard_late_delivery_result() {
    for logout in [false, true] {
        let token = token();
        let service = validated_service(&token);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let retained = listener.try_clone().unwrap();
        let origin = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let probe = DeliveryProbe {
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .redirect(Policy::none())
                .retry(reqwest::retry::never())
                .build()
                .unwrap(),
            origin,
            metadata_fixture_origin: None,
        };
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            let mut stream = accepted(&listener);
            let _ = receive(&mut stream);
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            respond(
                &mut stream,
                200,
                &legacy_body("https://storage.example/track-private"),
            );
        });
        let cancellation = DeliveryCancellation::new();
        let worker_token = token.clone();
        let worker_cancellation = cancellation.clone();
        let worker =
            thread::spawn(move || probe.probe(&service, &worker_token, "42", &worker_cancellation));
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        if logout {
            token.invalidate();
        } else {
            cancellation.cancel();
        }
        release_tx.send(()).unwrap();
        let report = worker.join().unwrap();
        assert_eq!(report.category, DeliveryCategory::Cancelled);
        assert_eq!(report.http_status, None);
        check_redacted(report);
        server.join().unwrap();
        check_no_second_request(retained);
    }
}

#[test]
fn network_and_timeout_are_fixed_categories_not_transport_details() {
    let token = token();
    let service = validated_service(&token);
    let (mut probe, server, retained) = fixture(
        200,
        legacy_body("https://storage.example/track-private"),
        Duration::from_millis(100),
    );
    probe.client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(30))
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .unwrap();
    let report = probe.probe(&service, &token, "42", &DeliveryCancellation::new());
    assert_eq!(report.category, DeliveryCategory::Timeout);
    check_redacted(report);
    server.join().unwrap();
    check_no_second_request(retained);
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    probe.origin = Url::parse(&format!("http://{}/", closed.local_addr().unwrap())).unwrap();
    drop(closed);
    let report = probe.probe(&service, &token, "42", &DeliveryCancellation::new());
    assert_eq!(report.category, DeliveryCategory::Network);
    check_redacted(report);
}

const XML: &str = r#"<?xml version="1.0"?><download-info><host>storage.example</host><path>/track-private</path><ts>123456</ts><s>synthetic-private-delivery-value</s></download-info>"#;

#[test]
fn descriptor_origin_and_path_are_exact_and_never_widened_by_provider_input() {
    let probe = DeliveryProbe::new().unwrap();
    assert!(
        probe
            .metadata_url(
                legacy_body(
                    "https://storage.mds.yandex.net/get-download-info/track-private?secret=private"
                )
                .as_bytes()
            )
            .is_some()
    );
    for resource in [
        "http://storage.mds.yandex.net/get-download-info/track-private",
        "https://storage.mds.yandex.net:444/get-download-info/track-private",
        "https://storage.mds.yandex.net.evil.example/get-download-info/track-private",
        "https://sub.storage.mds.yandex.net/get-download-info/track-private",
        "https://storage.mds.yandex.net/media/track-private",
        "https://storage.mds.yandex.net/get-download-info/../media/track-private",
        "https://storage.mds.yandex.net/get-download-info",
        "https://phone-private@storage.mds.yandex.net/get-download-info/track-private",
        "https://storage.mds.yandex.net/get-download-info/track-private#private",
    ] {
        assert!(
            probe
                .metadata_url(legacy_body(resource).as_bytes())
                .is_none()
        );
    }
}

#[test]
fn bounded_descriptor_read_has_no_credentials_and_stops_before_media_or_signing() {
    let token = token();
    let service = validated_service(&token);
    let descriptor = TcpListener::bind("127.0.0.1:0").unwrap();
    let metadata_origin =
        Url::parse(&format!("http://{}/", descriptor.local_addr().unwrap())).unwrap();
    let retained_descriptor = descriptor.try_clone().unwrap();
    let metadata_server = thread::spawn(move || {
        let mut stream = accepted(&descriptor);
        let request = receive(&mut stream);
        respond(&mut stream, 200, XML);
        request
    });
    let body = legacy_body(
        metadata_origin
            .join("get-download-info/track-private?private=synthetic-private-delivery-value")
            .unwrap()
            .as_str(),
    );
    let (mut probe, api_server, retained_api) = fixture(200, body, Duration::ZERO);
    probe.metadata_fixture_origin = Some(metadata_origin);
    let report = probe.probe(&service, &token, "42", &DeliveryCancellation::new());
    assert_eq!(report.category, DeliveryCategory::LegacySignatureRequired);
    assert_eq!(report.http_status, Some(200));
    assert_eq!(report.metadata_http_status, Some(200));
    assert!(report.metadata_attempted);
    assert!(!report.protection_required, "a signature is not itself DRM");
    assert!(report.to_string().contains("stage=delivery-descriptor"));
    check_redacted(report);
    let metadata_request = metadata_server.join().unwrap().to_ascii_lowercase();
    for absent in [
        "authorization:",
        "cookie:",
        "x-yandex-music-client",
        "referer:",
        "origin:",
    ] {
        assert!(!metadata_request.contains(absent));
    }
    assert!(metadata_request.starts_with("get /get-download-info/"));
    api_server.join().unwrap();
    check_no_second_request(retained_api);
    check_no_second_request(retained_descriptor);
}

#[test]
fn descriptor_rejection_oversize_and_invalid_xml_keep_separate_http_results() {
    for (status, body, expected) in [
        (403, PRIVATE.to_owned(), DeliveryCategory::Forbidden),
        (302, PRIVATE.to_owned(), DeliveryCategory::HttpRejected),
        (
            200,
            PRIVATE.to_owned(),
            DeliveryCategory::UnsupportedResponse,
        ),
        (
            200,
            "x".repeat(MAX_METADATA_BODY + 1),
            DeliveryCategory::ResponseTooLarge,
        ),
        (
            200,
            format!("<download-info><key>{PRIVATE}</key></download-info>"),
            DeliveryCategory::ProtectionRequired,
        ),
    ] {
        let token = token();
        let service = validated_service(&token);
        let descriptor = TcpListener::bind("127.0.0.1:0").unwrap();
        let metadata_origin =
            Url::parse(&format!("http://{}/", descriptor.local_addr().unwrap())).unwrap();
        let retained_descriptor = descriptor.try_clone().unwrap();
        let metadata_server = thread::spawn(move || {
            let mut stream = accepted(&descriptor);
            let _ = receive(&mut stream);
            respond(&mut stream, status, &body);
        });
        let (mut probe, api_server, retained_api) = fixture(
            200,
            legacy_body(
                metadata_origin
                    .join("get-download-info/track-private")
                    .unwrap()
                    .as_str(),
            ),
            Duration::ZERO,
        );
        probe.metadata_fixture_origin = Some(metadata_origin);
        let report = probe.probe(&service, &token, "42", &DeliveryCancellation::new());
        assert_eq!(report.category, expected);
        assert_eq!(report.http_status, Some(200));
        assert_eq!(report.metadata_http_status, Some(status));
        check_redacted(report);
        api_server.join().unwrap();
        metadata_server.join().unwrap();
        check_no_second_request(retained_api);
        check_no_second_request(retained_descriptor);
    }
}

#[test]
fn cancellation_during_descriptor_read_never_returns_late_resolution() {
    let token = token();
    let service = validated_service(&token);
    let descriptor = TcpListener::bind("127.0.0.1:0").unwrap();
    let metadata_origin =
        Url::parse(&format!("http://{}/", descriptor.local_addr().unwrap())).unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let metadata_server = thread::spawn(move || {
        let mut stream = accepted(&descriptor);
        let _ = receive(&mut stream);
        started_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        respond(&mut stream, 200, XML);
    });
    let (mut probe, api_server, retained_api) = fixture(
        200,
        legacy_body(
            metadata_origin
                .join("get-download-info/track-private")
                .unwrap()
                .as_str(),
        ),
        Duration::ZERO,
    );
    probe.metadata_fixture_origin = Some(metadata_origin);
    let cancellation = DeliveryCancellation::new();
    let worker_cancellation = cancellation.clone();
    let worker = thread::spawn(move || probe.probe(&service, &token, "42", &worker_cancellation));
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    cancellation.cancel();
    release_tx.send(()).unwrap();
    let report = worker.join().unwrap();
    assert_eq!(report.category, DeliveryCategory::Cancelled);
    assert_eq!(report.http_status, None);
    assert_eq!(report.metadata_http_status, None);
    api_server.join().unwrap();
    metadata_server.join().unwrap();
    check_no_second_request(retained_api);
}
