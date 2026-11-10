// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic service/descriptor fixtures. Resolving must never fetch media.

use super::*;
use crate::stream::tests::{
    accepted, check_no_second_request, check_redacted, legacy_body, receive, respond, token,
    validated_service,
};
use serde_json::json;
use std::{net::TcpListener, thread};

const PRIVATE: &str = "synthetic-private-delivery-value";
const XML: &str = "<download-info><host>s268myt.storage.yandex.net</host><path>/synthetic-resource/track.mp3</path><ts>000592c073d58600</ts><s>synthetic-private-delivery-value</s></download-info>";

#[test]
fn descriptor_binding_distinguishes_file_and_encoding_from_temporary_signature_address() {
    let source = |path: &str, timestamp: &str, bitrate: u32| {
        let xml = format!(
            "<download-info><host>s268myt.storage.yandex.net</host><path>{path}</path><ts>{timestamp}</ts><s>synthetic-private-delivery-value</s></download-info>"
        );
        let parts = xml::parse(xml.as_bytes()).unwrap();
        let uri = legacy_uri(&parts).unwrap();
        RemotePlaybackSource::from_resolver(&uri, Instant::now() + SOURCE_FRESHNESS)
            .unwrap()
            .with_http_cache_resource(HttpResourceIdentity::from_validated_descriptor(
                &parts, bitrate,
            ))
    };
    let first = source("/synthetic-resource/track.mp3", "000592c073d58600", 192);
    let renewed = source("/synthetic-resource/track.mp3", "000592c073d58601", 192);
    let other = source("/synthetic-resource/other.mp3", "000592c073d58601", 192);
    let encoding = source("/synthetic-resource/track.mp3", "000592c073d58601", 320);
    assert_ne!(first.http_cache_uri_hash(), renewed.http_cache_uri_hash());
    assert_eq!(
        first.http_cache_resource_hash(),
        renewed.http_cache_resource_hash()
    );
    assert_ne!(
        first.http_cache_resource_hash(),
        other.http_cache_resource_hash()
    );
    assert_ne!(
        first.http_cache_resource_hash(),
        encoding.http_cache_resource_hash()
    );
    let headers = || {
        [
            ("content-type", "audio/mpeg"),
            ("content-length", "3"),
            ("etag", "\"synthetic-version\""),
        ]
        .map(|(name, value)| (name.into(), value.into()))
    };
    let policy = crate::audio_cache::http_policy::ResponsePolicy::parse(200, headers()).unwrap();
    let stored = policy
        .record(
            first.http_cache_uri_hash(),
            first.http_cache_resource_hash(),
            [3; 32],
            3,
        )
        .unwrap();
    assert!(
        policy
            .revalidate(
                &stored,
                renewed.http_cache_uri_hash(),
                renewed.http_cache_resource_hash()
            )
            .is_ok()
    );
    assert_eq!(
        policy.revalidate(
            &stored,
            other.http_cache_uri_hash(),
            other.http_cache_resource_hash()
        ),
        Err(crate::audio_cache::http_policy::Reason::ResourceChanged)
    );
    assert_eq!(
        policy.revalidate(
            &stored,
            encoding.http_cache_uri_hash(),
            encoding.http_cache_resource_hash()
        ),
        Err(crate::audio_cache::http_policy::Reason::ResourceChanged)
    );
    let replacement = crate::audio_cache::http_policy::ResponsePolicy::parse(
        200,
        [
            ("content-type", "audio/mpeg"),
            ("content-length", "3"),
            ("etag", "\"synthetic-replacement\""),
        ]
        .map(|(name, value)| (name.into(), value.into())),
    )
    .unwrap();
    assert_eq!(
        replacement.revalidate(
            &stored,
            renewed.http_cache_uri_hash(),
            renewed.http_cache_resource_hash()
        ),
        Err(crate::audio_cache::http_policy::Reason::RepresentationChanged)
    );
    let visible = format!("{renewed:?}");
    assert!(!visible.contains("synthetic-resource") && !visible.contains("get-mp3"));
}

fn options(body: &str) -> Vec<DeliveryOption> {
    serde_json::from_str::<Envelope>(body).unwrap().result
}

#[test]
fn legacy_source_uses_the_canonical_checksum_path_without_grant_time_guessing() {
    let parts = xml::parse(XML.as_bytes()).unwrap();
    let uri = legacy_uri(&parts).unwrap();
    // Expected digest was independently checked against Python hashlib using
    // synthetic inputs, not obtained from a live account or a media response.
    assert_eq!(
        uri.as_str(),
        "https://s268myt.storage.yandex.net/get-mp3/766d90d05273a9e150d67231940ffa27/000592c073d58600/synthetic-resource/track.mp3"
    );
    assert_eq!(
        xml::classify(XML.as_bytes()),
        xml::MetadataCategory::LegacySignatureRequired
    );
    let visible = format!("{parts:?}");
    assert!(!visible.contains(PRIVATE) && !visible.contains("s268myt"));
}

#[test]
fn legacy_descriptor_server_labels_are_canonicalized_without_collapsing_resource_identity() {
    // Names deliberately exercise the former naming-layout rejection. They
    // are fixtures within the existing trusted zone, not live CDN assertions.
    let source = |host: &str| {
        let body = XML.replace("s268myt.storage.yandex.net", host);
        let parts = xml::parse(body.as_bytes()).unwrap();
        let uri = legacy_uri(&parts).unwrap();
        RemotePlaybackSource::from_resolver(&uri, Instant::now() + SOURCE_FRESHNESS)
            .unwrap()
            .with_http_cache_resource(HttpResourceIdentity::from_validated_descriptor(&parts, 192))
    };
    let lower = source("s123.storage.yandex.net");
    let upper = source("S123.STORAGE.YANDEX.NET");
    let other = source("server-2026.storage.yandex.net");
    assert_eq!(lower.http_cache_uri_hash(), upper.http_cache_uri_hash());
    assert_eq!(
        lower.http_cache_resource_hash(),
        upper.http_cache_resource_hash()
    );
    assert_ne!(
        lower.http_cache_resource_hash(),
        other.http_cache_resource_hash()
    );
    for item in [lower, upper, other] {
        assert!(!item.is_expired());
        let visible = format!("{item:?}");
        assert!(!visible.contains(PRIVATE) && !visible.contains("storage.yandex.net"));
    }
}

#[test]
fn metadata_policy_accepts_only_the_three_known_exact_https_paths() {
    let probe = DeliveryProbe::new().unwrap();
    for path in ["get-download-info", "download-info", "file-download-info"] {
        let url = format!("https://storage.mds.yandex.net/{path}/synthetic?sign={PRIVATE}");
        assert!(probe.validate_metadata_url(&url).is_ok());
    }
    for url in [
        "https://storage.mds.yandex.net/media/synthetic",
        "https://storage.mds.yandex.net/download-info",
        "https://storage.mds.yandex.net/get-download-info/",
        "https://storage.mds.yandex.net/file-download-info",
        "https://storage.mds.yandex.net/file-download-info/",
        "https://storage.mds.yandex.net/file-download-info-lookalike/synthetic",
        "https://storage.mds.yandex.net/sub/file-download-info/synthetic",
        "https://storage.mds.yandex.net/file-download-info/%2fprivate",
        "https://storage.mds.yandex.net/download-info/%2fprivate",
        "https://storage.mds.yandex.net/download-info/../media/synthetic",
    ] {
        assert_eq!(
            probe.validate_metadata_url(url).unwrap_err(),
            DeliveryCategory::MetadataPathUnverified
        );
    }
    for url in [
        "https://storage.mds.yandex.net.evil.test/download-info/synthetic",
        "https://sub.storage.mds.yandex.net/download-info/synthetic",
        "http://storage.mds.yandex.net/download-info/synthetic",
        "https://storage.mds.yandex.net:444/download-info/synthetic",
        "https://127.0.0.1/download-info/synthetic",
        "https://storage.mds.yandex.net.evil.test/file-download-info/synthetic",
    ] {
        assert_eq!(
            probe.validate_metadata_url(url).unwrap_err(),
            DeliveryCategory::MetadataOriginUnverified
        );
    }
    for url in [
        "https://private@storage.mds.yandex.net/download-info/synthetic",
        "https://@storage.mds.yandex.net/download-info/synthetic",
        "https://:@storage.mds.yandex.net/download-info/synthetic",
        "https://storage.mds.yandex.net:/download-info/synthetic",
        "https://storage.mds.yandex.net/download-info/synthetic#private",
        " https://storage.mds.yandex.net/download-info/synthetic",
        "https://storage.mds.yandex.net\\download-info\\synthetic",
    ] {
        assert_eq!(
            probe.validate_metadata_url(url).unwrap_err(),
            DeliveryCategory::UnsafeResource
        );
    }
}

#[test]
fn selection_requires_explicit_full_mp3_and_rejects_ambiguous_protection() {
    let resolver = StreamResolver::new().unwrap();
    let metadata = "https://storage.mds.yandex.net/download-info/synthetic";
    for (body, expected) in [
        (
            json!({"result":[{"codec":"aac","preview":false,"downloadInfoUrl":metadata}]}),
            DeliveryCategory::UnsupportedCodec,
        ),
        (
            json!({"result":[{"codec":"mp3","preview":true,"downloadInfoUrl":metadata}]}),
            DeliveryCategory::PreviewOnly,
        ),
        (
            json!({"result":[{"codec":"mp3","downloadInfoUrl":metadata}]}),
            DeliveryCategory::PreviewUnverified,
        ),
        (
            json!({"result":[{"codec":"mp3","preview":null,"downloadInfoUrl":metadata}]}),
            DeliveryCategory::PreviewUnverified,
        ),
        (
            json!({"result":[{"codec":"mp3","preview":true,"downloadInfoUrl":metadata},{"codec":"mp3","preview":false,"downloadInfoUrl":metadata}]}),
            DeliveryCategory::UnsupportedResponse,
        ),
    ] {
        assert_eq!(
            resolver.select_mp3(options(&body.to_string())).unwrap_err(),
            expected
        );
    }
    for marker in [
        "key",
        "decryptionKey",
        "decryption_key",
        "encrypted",
        "encryption",
        "drm",
        "protected",
        "protection",
        "licenseUrl",
        "license_url",
    ] {
        let body = json!({"result":[{"codec":"mp3","preview":false,"downloadInfoUrl":metadata,marker:PRIVATE}]}).to_string();
        assert_eq!(
            resolver.select_mp3(options(&body)).unwrap_err(),
            DeliveryCategory::ProtectionRequired
        );
    }
    // No downgrade to a second plain MP3 entry when protection declarations
    // conflict within the MP3 offers.
    let body = json!({"result":[{"codec":"mp3","preview":false,"downloadInfoUrl":metadata,"key":PRIVATE},{"codec":"mp3","preview":false,"downloadInfoUrl":metadata}]}).to_string();
    assert_eq!(
        resolver.select_mp3(options(&body)).unwrap_err(),
        DeliveryCategory::ProtectionRequired
    );
    for (transport, expected) in [
        ("encraw", DeliveryCategory::ProtectionRequired),
        (PRIVATE, DeliveryCategory::UnsupportedResponse),
    ] {
        let body = json!({"result": [
            {"codec":"flac", "transport":transport},
            {"codec":"mp3", "preview":false, "downloadInfoUrl":metadata}
        ]})
        .to_string();
        assert_eq!(resolver.select_mp3(options(&body)).unwrap_err(), expected);
    }
}

#[test]
fn best_eligible_offer_is_selected_without_following_unknown_origins() {
    let resolver = StreamResolver::new().unwrap();
    let body = json!({"result":[
        {"codec":"mp3","preview":false,"bitrateInKbps":128,"downloadInfoUrl":"https://storage.mds.yandex.net/download-info/low"},
        {"codec":"mp3","preview":false,"bitrateInKbps":320,"downloadInfoUrl":"https://storage.mds.yandex.net/get-download-info/high"},
        {"codec":"mp3","preview":true,"bitrateInKbps":320,"downloadInfoUrl":"https://storage.mds.yandex.net/download-info/preview"},
        {"codec":"mp3","preview":false,"bitrateInKbps":999,"downloadInfoUrl":"https://other.example.test/download-info/untrusted"}
    ]}).to_string();
    assert_eq!(
        resolver.select_mp3(options(&body)).unwrap().path(),
        "/get-download-info/high"
    );
}

#[test]
fn optional_and_raw_transport_never_override_protection_or_preview() {
    let resolver = StreamResolver::new().unwrap();
    for transport in [json!(null), json!("raw")] {
        let mut body = json!({"result": [{
            "codec": "mp3",
            "preview": false,
            "downloadInfoUrl": "https://storage.mds.yandex.net/download-info/synthetic",
            "transport": transport
        }]});
        assert!(resolver.select_mp3(options(&body.to_string())).is_ok());
        for marker in [
            "key",
            "decryption_key",
            "protected",
            "protection",
            "license_url",
        ] {
            body["result"][0][marker] = json!(PRIVATE);
            assert_eq!(
                resolver.select_mp3(options(&body.to_string())).unwrap_err(),
                DeliveryCategory::ProtectionRequired
            );
            body["result"][0].as_object_mut().unwrap().remove(marker);
        }
        body["result"][0]["preview"] = json!(true);
        assert_eq!(
            resolver.select_mp3(options(&body.to_string())).unwrap_err(),
            DeliveryCategory::PreviewOnly
        );
    }
}

#[test]
fn invalid_media_origins_and_path_components_never_escape_the_resolver() {
    for host in [
        "private.example.test",
        "storage.yandex.net",
        "a.b.storage.yandex.net",
        "s268myt.storage.yandex.net.evil.test",
        "127.0.0.1",
        "user@storage.yandex.net",
    ] {
        let body = XML.replace("s268myt.storage.yandex.net", host);
        let result = xml::parse(body.as_bytes()).and_then(|parts| {
            legacy_uri(&parts).map_err(|_| xml::MetadataCategory::UnsupportedResponse)
        });
        assert!(result.is_err());
    }
    for path in [
        "/../private",
        "/x/../private",
        "/%2fprivate",
        "/private name",
        "/private?token=x",
        "//other.test/private",
        "/private\\path",
    ] {
        let body = XML.replace("/synthetic-resource/track.mp3", path);
        let result = xml::parse(body.as_bytes()).and_then(|parts| {
            legacy_uri(&parts).map_err(|_| xml::MetadataCategory::UnsupportedResponse)
        });
        assert!(result.is_err());
    }
    for field in [
        "<key>private-key</key>",
        "<encrypted>true</encrypted>",
        "<protected>true</protected>",
        "<decryption_key>private-key</decryption_key>",
        "<license_url>private-license</license_url>",
        "<protection>private-protection</protection>",
    ] {
        let body = XML.replace("</download-info>", &format!("{field}</download-info>"));
        assert!(matches!(
            xml::parse(body.as_bytes()),
            Err(xml::MetadataCategory::ProtectionRequired)
        ));
    }
    let body = XML.replace(
        "</download-info>",
        "<preview>true</preview></download-info>",
    );
    assert!(matches!(
        xml::parse(body.as_bytes()),
        Err(xml::MetadataCategory::PreviewOnly)
    ));
}

#[test]
fn full_resolution_for_all_paths_sends_credentials_only_to_music_and_never_fetches_media() {
    for (prefix, host) in [
        ("get-download-info", "s268myt.storage.yandex.net"),
        ("download-info", "s123.storage.yandex.net"),
        ("file-download-info", "server-2026.storage.yandex.net"),
    ] {
        let token = token();
        let service = validated_service(&token);
        let api = TcpListener::bind("127.0.0.1:0").unwrap();
        let descriptor = TcpListener::bind("127.0.0.1:0").unwrap();
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let api_origin = format!("http://{}/", api.local_addr().unwrap());
        let metadata_origin = format!("http://{}/", descriptor.local_addr().unwrap());
        let media_origin = format!("http://{}/", media.local_addr().unwrap());
        let resolver = StreamResolver::for_test(&api_origin, &metadata_origin, &media_origin);
        let retained_api = api.try_clone().unwrap();
        let retained_descriptor = descriptor.try_clone().unwrap();
        let mut body: serde_json::Value = serde_json::from_str(&legacy_body(&format!(
            "{metadata_origin}{prefix}/synthetic?sign={PRIVATE}"
        )))
        .unwrap();
        // Cover legacy omission, optional null, and the known unprotected raw
        // declaration without changing acquisition or credential boundaries.
        if prefix == "download-info" {
            body["result"][0]["transport"] = json!(null);
        } else if prefix == "file-download-info" {
            body["result"][0]["transport"] = json!("raw");
        }
        let body = body.to_string();
        let xml = XML.replace("s268myt.storage.yandex.net", host);
        let xml = if prefix == "file-download-info" {
            xml.replace(
                "</download-info>",
                "<transport>raw</transport></download-info>",
            )
        } else {
            xml
        };
        let api_server = thread::spawn(move || {
            let mut stream = accepted(&api);
            let request = receive(&mut stream);
            respond(&mut stream, 200, &body);
            request
        });
        let metadata_server = thread::spawn(move || {
            let mut stream = accepted(&descriptor);
            let request = receive(&mut stream);
            respond(&mut stream, 200, &xml);
            request
        });
        let source = resolver
            .resolve(&service, &token, "42:7", &DeliveryCancellation::new())
            .unwrap();
        assert!(!source.is_expired());
        let visible = format!("{source:?} {resolver:?}");
        assert!(
            !visible.contains(PRIVATE) && !visible.contains("http") && !visible.contains("s268")
        );
        let api_request = api_server.join().unwrap().to_ascii_lowercase();
        assert!(api_request.starts_with("get /tracks/42:7/download-info "));
        assert!(api_request.contains(&format!("authorization: oauth {PRIVATE}\r\n")));
        let metadata_request = metadata_server.join().unwrap().to_ascii_lowercase();
        assert!(metadata_request.starts_with(&format!("get /{prefix}/")));
        for absent in [
            "authorization:",
            "cookie:",
            "origin:",
            "referer:",
            "x-yandex-music-client",
        ] {
            assert!(!metadata_request.contains(absent));
        }
        check_no_second_request(retained_api);
        check_no_second_request(retained_descriptor);
        check_no_second_request(media);
    }
}

#[test]
fn ambiguous_api_error_never_reaches_the_descriptor_or_media() {
    for error in [json!(null), json!(PRIVATE), json!({"message": PRIVATE})] {
        let token = token();
        let service = validated_service(&token);
        let api = TcpListener::bind("127.0.0.1:0").unwrap();
        let descriptor = TcpListener::bind("127.0.0.1:0").unwrap();
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let api_origin = format!("http://{}/", api.local_addr().unwrap());
        let metadata_origin = format!("http://{}/", descriptor.local_addr().unwrap());
        let media_origin = format!("http://{}/", media.local_addr().unwrap());
        let resolver = StreamResolver::for_test(&api_origin, &metadata_origin, &media_origin);
        let body = json!({
            "result": [{
                "codec": "mp3",
                "preview": false,
                "downloadInfoUrl": format!("{metadata_origin}download-info/synthetic")
            }],
            "error": error
        })
        .to_string();
        let api_server = thread::spawn(move || {
            let mut stream = accepted(&api);
            receive(&mut stream);
            respond(&mut stream, 200, &body);
        });
        let report = resolver
            .resolve(&service, &token, "42", &DeliveryCancellation::new())
            .unwrap_err();
        assert_eq!(report.category, DeliveryCategory::UnsupportedResponse);
        assert_eq!(report.http_status, Some(200));
        assert_eq!(report.stage, DeliveryStage::Info);
        assert!(!report.metadata_attempted);
        check_redacted(report);
        api_server.join().unwrap();
        check_no_second_request(descriptor);
        check_no_second_request(media);
    }
}

#[test]
fn protected_and_unknown_json_transports_never_fetch_descriptors_or_media() {
    for (transport, expected) in [
        (json!("encraw"), DeliveryCategory::ProtectionRequired),
        (json!("plain"), DeliveryCategory::UnsupportedResponse),
        (json!(PRIVATE), DeliveryCategory::UnsupportedResponse),
        (json!(true), DeliveryCategory::UnsupportedResponse),
    ] {
        let token = token();
        let service = validated_service(&token);
        let api = TcpListener::bind("127.0.0.1:0").unwrap();
        let descriptor = TcpListener::bind("127.0.0.1:0").unwrap();
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let api_origin = format!("http://{}/", api.local_addr().unwrap());
        let metadata_origin = format!("http://{}/", descriptor.local_addr().unwrap());
        let media_origin = format!("http://{}/", media.local_addr().unwrap());
        let resolver = StreamResolver::for_test(&api_origin, &metadata_origin, &media_origin);
        let body = json!({"result": [{
            "codec": "mp3",
            "preview": false,
            "downloadInfoUrl": format!("{metadata_origin}download-info/synthetic"),
            "transport": transport
        }]})
        .to_string();
        let api_server = thread::spawn(move || {
            let mut stream = accepted(&api);
            receive(&mut stream);
            respond(&mut stream, 200, &body);
        });
        let report = resolver
            .resolve(&service, &token, "42", &DeliveryCancellation::new())
            .unwrap_err();
        assert_eq!(report.category, expected);
        assert_eq!(report.http_status, Some(200));
        assert_eq!(report.stage, DeliveryStage::Info);
        assert!(!report.metadata_attempted);
        check_redacted(report);
        api_server.join().unwrap();
        check_no_second_request(descriptor);
        check_no_second_request(media);
    }
}

#[test]
fn protected_and_unknown_xml_transports_never_produce_or_fetch_media() {
    for (transport, expected) in [
        ("encraw", DeliveryCategory::ProtectionRequired),
        ("plain", DeliveryCategory::UnsupportedResponse),
        (PRIVATE, DeliveryCategory::UnsupportedResponse),
    ] {
        let token = token();
        let service = validated_service(&token);
        let api = TcpListener::bind("127.0.0.1:0").unwrap();
        let descriptor = TcpListener::bind("127.0.0.1:0").unwrap();
        let retained_descriptor = descriptor.try_clone().unwrap();
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let api_origin = format!("http://{}/", api.local_addr().unwrap());
        let metadata_origin = format!("http://{}/", descriptor.local_addr().unwrap());
        let media_origin = format!("http://{}/", media.local_addr().unwrap());
        let resolver = StreamResolver::for_test(&api_origin, &metadata_origin, &media_origin);
        let body = legacy_body(&format!("{metadata_origin}download-info/synthetic"));
        let api_server = thread::spawn(move || {
            let mut stream = accepted(&api);
            receive(&mut stream);
            respond(&mut stream, 200, &body);
        });
        let xml = XML.replace(
            "</download-info>",
            &format!("<transport>{transport}</transport></download-info>"),
        );
        let descriptor_server = thread::spawn(move || {
            let mut stream = accepted(&descriptor);
            receive(&mut stream);
            respond(&mut stream, 200, &xml);
        });
        let report = resolver
            .resolve(&service, &token, "42", &DeliveryCancellation::new())
            .unwrap_err();
        assert_eq!(report.category, expected);
        assert_eq!(report.http_status, Some(200));
        assert_eq!(report.metadata_http_status, Some(200));
        assert_eq!(report.stage, DeliveryStage::Resolution);
        check_redacted(report);
        api_server.join().unwrap();
        descriptor_server.join().unwrap();
        check_no_second_request(retained_descriptor);
        check_no_second_request(media);
    }
}

#[test]
fn metadata_redirects_fail_without_replaying_or_contacting_the_media_origin() {
    let token = token();
    let service = validated_service(&token);
    let api = TcpListener::bind("127.0.0.1:0").unwrap();
    let descriptor = TcpListener::bind("127.0.0.1:0").unwrap();
    let media = TcpListener::bind("127.0.0.1:0").unwrap();
    let api_origin = format!("http://{}/", api.local_addr().unwrap());
    let metadata_origin = format!("http://{}/", descriptor.local_addr().unwrap());
    let media_origin = format!("http://{}/", media.local_addr().unwrap());
    let resolver = StreamResolver::for_test(&api_origin, &metadata_origin, &media_origin);
    let target = format!("{metadata_origin}download-info/synthetic");
    let api_server = thread::spawn(move || {
        let mut stream = accepted(&api);
        receive(&mut stream);
        respond(&mut stream, 200, &legacy_body(&target));
    });
    let retained = descriptor.try_clone().unwrap();
    let metadata_server = thread::spawn(move || {
        use std::io::Write;
        let mut stream = accepted(&descriptor);
        receive(&mut stream);
        write!(stream, "HTTP/1.1 302 Redirect\r\nLocation: {media_origin}private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let report = resolver
        .resolve(&service, &token, "42", &DeliveryCancellation::new())
        .unwrap_err();
    assert_eq!(report.category, DeliveryCategory::HttpRejected);
    assert_eq!(report.http_status, Some(200));
    assert_eq!(report.metadata_http_status, Some(302));
    assert_eq!(report.stage, DeliveryStage::Descriptor);
    check_redacted(report);
    api_server.join().unwrap();
    metadata_server.join().unwrap();
    check_no_second_request(retained);
    check_no_second_request(media);
}

#[test]
fn a_slow_descriptor_does_not_reset_offer_freshness_and_errors_stay_redacted() {
    let token = token();
    let service = validated_service(&token);
    let api = TcpListener::bind("127.0.0.1:0").unwrap();
    let descriptor = TcpListener::bind("127.0.0.1:0").unwrap();
    let media = TcpListener::bind("127.0.0.1:0").unwrap();
    let api_origin = format!("http://{}/", api.local_addr().unwrap());
    let metadata_origin = format!("http://{}/", descriptor.local_addr().unwrap());
    let media_origin = format!("http://{}/", media.local_addr().unwrap());
    let resolver = StreamResolver::for_test(&api_origin, &metadata_origin, &media_origin);
    let target = format!("{metadata_origin}download-info/synthetic");
    let api_server = thread::spawn(move || {
        let mut stream = accepted(&api);
        receive(&mut stream);
        respond(&mut stream, 200, &legacy_body(&target));
    });
    let metadata_server = thread::spawn(move || {
        let mut stream = accepted(&descriptor);
        receive(&mut stream);
        thread::sleep(Duration::from_millis(1200));
        respond(&mut stream, 200, XML);
    });
    let report = resolver
        .resolve_until(
            &service,
            &token,
            "42",
            &DeliveryCancellation::new(),
            Instant::now() + Duration::from_millis(1000),
        )
        .unwrap_err();
    assert_eq!(report.category, DeliveryCategory::SourceExpired);
    assert_eq!(report.http_status, Some(200));
    assert_eq!(report.metadata_http_status, Some(200));
    assert_eq!(report.stage, DeliveryStage::Resolution);
    check_redacted(report);
    api_server.join().unwrap();
    metadata_server.join().unwrap();
    check_no_second_request(media);
}

#[test]
fn expired_unbound_and_pre_cancelled_attempts_make_no_offer_or_media_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}/", listener.local_addr().unwrap());
    let resolver = StreamResolver::for_test(&origin, &origin, &origin);
    let unbound = MusicService::for_test(&origin);
    let token = token();
    assert_eq!(
        resolver
            .resolve(&unbound, &token, "42", &DeliveryCancellation::new())
            .unwrap_err()
            .category,
        DeliveryCategory::SessionRequired
    );
    let bound = validated_service(&token);
    let cancellation = DeliveryCancellation::new();
    cancellation.cancel();
    assert_eq!(
        resolver
            .resolve(&bound, &token, "42", &cancellation)
            .unwrap_err()
            .category,
        DeliveryCategory::Cancelled
    );
    let report = resolver
        .resolve_until(
            &bound,
            &token,
            "42",
            &DeliveryCancellation::new(),
            Instant::now() - Duration::from_secs(1),
        )
        .unwrap_err();
    assert_eq!(report.category, DeliveryCategory::SourceExpired);
    assert_eq!(report.stage, DeliveryStage::Resolution);
    assert_eq!(report.http_status, None);
    check_redacted(report);
    check_no_second_request(listener);
}

#[test]
fn cancel_logout_and_cache_invalidation_during_descriptor_never_return_a_source() {
    for mode in 0..3 {
        let token = token();
        let service = validated_service(&token);
        let api = TcpListener::bind("127.0.0.1:0").unwrap();
        let descriptor = TcpListener::bind("127.0.0.1:0").unwrap();
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let api_origin = format!("http://{}/", api.local_addr().unwrap());
        let metadata_origin = format!("http://{}/", descriptor.local_addr().unwrap());
        let media_origin = format!("http://{}/", media.local_addr().unwrap());
        let resolver = StreamResolver::for_test(&api_origin, &metadata_origin, &media_origin);
        let target = format!("{metadata_origin}download-info/synthetic");
        let api_server = thread::spawn(move || {
            let mut stream = accepted(&api);
            receive(&mut stream);
            respond(&mut stream, 200, &legacy_body(&target));
        });
        let (started, wait_started) = std::sync::mpsc::channel();
        let (release, wait_release) = std::sync::mpsc::channel();
        let metadata_server = thread::spawn(move || {
            let mut stream = accepted(&descriptor);
            receive(&mut stream);
            started.send(()).unwrap();
            wait_release.recv_timeout(Duration::from_secs(3)).unwrap();
            respond(&mut stream, 200, XML);
        });
        let cancellation = DeliveryCancellation::new();
        let worker_cancel = cancellation.clone();
        let worker_token = token.clone();
        let worker_service = service.clone();
        let worker = thread::spawn(move || {
            resolver.resolve(&worker_service, &worker_token, "42", &worker_cancel)
        });
        wait_started.recv_timeout(Duration::from_secs(3)).unwrap();
        match mode {
            0 => cancellation.cancel(),
            1 => token.invalidate(),
            _ => service.clear_session(),
        }
        release.send(()).unwrap();
        let report = worker.join().unwrap().unwrap_err();
        assert_eq!(
            report.category,
            if mode == 2 {
                DeliveryCategory::SessionRequired
            } else {
                DeliveryCategory::Cancelled
            }
        );
        check_redacted(report);
        api_server.join().unwrap();
        metadata_server.join().unwrap();
        check_no_second_request(media);
    }
}
