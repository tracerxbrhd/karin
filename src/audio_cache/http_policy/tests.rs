// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn response(status: u16, extra: &[(&str, &str)]) -> Result<ResponsePolicy, Reason> {
    let mut headers = vec![
        ("Content-Type".to_owned(), "audio/mpeg".to_owned()),
        ("Content-Length".to_owned(), "4".to_owned()),
    ];
    headers.extend(extra.iter().map(|(a, b)| (a.to_string(), b.to_string())));
    ResponsePolicy::parse(status, headers)
}

fn original() -> Metadata {
    response(200, &[("ETag", "\"original-1\"")])
        .unwrap()
        .record([1; 32], [2; 32], [3; 32], 4)
        .unwrap()
}

#[test]
fn full_response_record_is_bounded_integrity_checked_and_opaque() {
    let record = original();
    let encoded = record.encode().unwrap();
    assert!(encoded.len() < MAX_METADATA);
    assert_eq!(Metadata::decode(&encoded).unwrap(), record);
    let mut corrupt = encoded;
    *corrupt.last_mut().unwrap() ^= 1;
    assert_eq!(Metadata::decode(&corrupt), Err(Reason::Integrity));
    assert_eq!(Metadata::decode(b"bad"), Err(Reason::Integrity));
    assert_eq!(
        Metadata::decode(&vec![0; MAX_METADATA + 1]),
        Err(Reason::Integrity)
    );
    assert!(!format!("{record:?}").contains("original"));
    assert_eq!(record.bytes, 4);
}

#[test]
fn storage_and_reuse_are_separate_without_a_validator() {
    let policy = response(200, &[]).unwrap();
    let record = policy.record([1; 32], [2; 32], [3; 32], 4).unwrap();
    assert_eq!(
        policy.revalidate(&record, [1; 32], [2; 32]),
        Err(Reason::ValidatorsMissing)
    );
    let fresh_digest = format!("sha-256=:{}:", STANDARD.encode([3; 32]));
    let head = response(200, &[("Repr-Digest", &fresh_digest)]).unwrap();
    assert_eq!(head.revalidate(&record, [9; 32], [8; 32]), Ok(()));
}

#[test]
fn no_store_and_malformed_directives_decline_only_caching() {
    for value in ["no-store", "PUBLIC, No-StOrE", "private, no-store=\"any\""] {
        assert_eq!(
            response(200, &[("Cache-Control", value)]).unwrap_err(),
            Reason::NoStore
        );
    }
    for value in [
        "max-age=-1",
        "max-age=abc",
        "max-age=1, max-age=2",
        "private,",
        "no-cache=\"unterminated",
    ] {
        assert_eq!(
            response(200, &[("Cache-Control", value)]).unwrap_err(),
            Reason::HeadersInvalid
        );
    }
}

#[test]
fn private_no_cache_must_revalidate_and_freshness_never_skip_validation() {
    let policy = response(200, &[
        ("Cache-Control", "private, no-cache=\"set-cookie, x-private\", must-revalidate, max-age=\"3600\", s-maxage=7200"),
        ("Age", "30"),
        ("Expires", "0"),
    ]).unwrap();
    assert!(
        policy.directives.no_cache
            && policy.directives.must_revalidate
            && policy.directives.private
    );
    assert_eq!(policy.directives.max_age, Some(3600));
    assert_eq!(policy.directives.s_maxage, Some(7200));
    assert_eq!(policy.directives.age, Some(30));
    assert_eq!(policy.directives.expires_unix, Some(0));
    let record = policy.record([1; 32], [2; 32], [3; 32], 4).unwrap();
    assert_eq!(
        policy.revalidate(&record, [1; 32], [2; 32]),
        Err(Reason::ValidatorsMissing)
    );
    assert!(
        response(200, &[("Pragma", "no-cache")])
            .unwrap()
            .directives
            .no_cache
    );
}

#[test]
fn strong_etag_accepts_same_resource_but_not_unrelated_identical_tags() {
    let policy = response(200, &[("ETag", "\"original-1\"")]).unwrap();
    assert_eq!(policy.revalidate(&original(), [1; 32], [2; 32]), Ok(()));
    // A fresh admitted legacy descriptor may renew its signed URL while naming
    // the exact same file/encoding. Its sealed object binding is independent of
    // the URL, and a fresh strong validator still has to match.
    assert_eq!(policy.revalidate(&original(), [7; 32], [2; 32]), Ok(()));
    assert_eq!(
        policy.revalidate(&original(), [7; 32], [8; 32]),
        Err(Reason::ResourceChanged)
    );
    let changed = response(200, &[("ETag", "\"changed-2\"")]).unwrap();
    assert_eq!(
        changed.revalidate(&original(), [1; 32], [2; 32]),
        Err(Reason::RepresentationChanged)
    );
}

#[test]
fn bare_or_weak_not_modified_is_not_exact_byte_proof() {
    let strong = ResponsePolicy::parse(304, [("etag".into(), "\"original-1\"".into())]).unwrap();
    assert_eq!(strong.revalidate(&original(), [1; 32], [2; 32]), Ok(()));
    for headers in [vec![], vec![("etag".into(), "W/\"original-1\"".into())]] {
        let policy = ResponsePolicy::parse(304, headers).unwrap();
        assert_eq!(
            policy.revalidate(&original(), [1; 32], [2; 32]),
            Err(Reason::ValidatorsMissing)
        );
    }
    let weak = response(200, &[("ETag", "W/\"original-1\"")]).unwrap();
    assert!(
        weak.record([1; 32], [2; 32], [3; 32], 4)
            .unwrap()
            .etag
            .is_none()
    );
}

#[test]
fn repr_digest_can_prove_a_renewed_url_but_content_digest_cannot() {
    let value = format!("sha-256=:{}:", STANDARD.encode([3; 32]));
    let policy = response(200, &[("Repr-Digest", &value)]).unwrap();
    assert_eq!(policy.revalidate(&original(), [7; 32], [8; 32]), Ok(()));
    assert_eq!(
        policy.record([1; 32], [2; 32], [4; 32], 4),
        Err(Reason::DigestMismatch)
    );
    let wrong = format!("sha-256=:{}:", STANDARD.encode([4; 32]));
    assert_eq!(
        response(200, &[("Repr-Digest", &wrong)])
            .unwrap()
            .revalidate(&original(), [7; 32], [8; 32]),
        Err(Reason::RepresentationChanged)
    );
    assert_eq!(
        response(200, &[("Content-Digest", &value)])
            .unwrap()
            .revalidate(&original(), [7; 32], [8; 32]),
        Err(Reason::ResourceChanged)
    );
    for value in [
        "sha-256=:bad:",
        "sha-256=notbinary",
        "sha-256=:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=:\"bad",
    ] {
        assert!(response(200, &[("Repr-Digest", value)]).is_err());
    }
}

#[test]
fn partial_probe_validates_total_but_cannot_be_published() {
    let policy = ResponsePolicy::parse(
        206,
        [
            ("Content-Type".into(), "audio/mpeg".into()),
            ("Content-Length".into(), "1".into()),
            ("Content-Range".into(), "bytes 0-0/4".into()),
            ("ETag".into(), "\"original-1\"".into()),
        ],
    )
    .unwrap();
    assert_eq!(policy.revalidate(&original(), [1; 32], [2; 32]), Ok(()));
    assert_eq!(policy.complete_mp3(), Err(Reason::PartialResponse));
    assert_eq!(
        policy.record([1; 32], [2; 32], [3; 32], 1),
        Err(Reason::PartialResponse)
    );
    assert_eq!(
        response(206, &[("Content-Range", "bytes 0-3/4")])
            .unwrap()
            .complete_mp3(),
        Ok(4)
    );
    assert_eq!(
        response(206, &[("Content-Range", "bytes 1-3/4")])
            .unwrap()
            .complete_mp3(),
        Err(Reason::PartialResponse)
    );
}

#[test]
fn strong_last_modified_requires_both_dates_and_same_bound_resource() {
    let headers = [
        ("Last-Modified", "Sun, 06 Nov 1994 08:49:37 GMT"),
        ("Date", "Sun, 06 Nov 1994 08:50:37 GMT"),
    ];
    let policy = response(200, &headers).unwrap();
    let record = policy.record([1; 32], [2; 32], [3; 32], 4).unwrap();
    assert_eq!(policy.revalidate(&record, [1; 32], [2; 32]), Ok(()));
    assert_eq!(policy.revalidate(&record, [7; 32], [2; 32]), Ok(()));
    let weak = response(
        200,
        &[
            ("Last-Modified", "Sun, 06 Nov 1994 08:49:37 GMT"),
            ("Date", "Sun, 06 Nov 1994 08:50:36 GMT"),
        ],
    )
    .unwrap();
    assert_eq!(
        weak.revalidate(&record, [1; 32], [2; 32]),
        Err(Reason::ValidatorsMissing)
    );
    let tagged_weak = response(
        200,
        &[headers[0], headers[1], ("ETag", "W/\"semantic-only\"")],
    )
    .unwrap();
    assert_eq!(
        tagged_weak.revalidate(&record, [1; 32], [2; 32]),
        Err(Reason::ValidatorsMissing)
    );
}

#[test]
fn malformed_dates_unicode_and_calendar_values_fail_closed_without_panics() {
    assert_eq!(http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Ok(0));
    for date in [
        "Sun, 06 Nov 1994 08:49:37 UTC",
        "Sun, 31 Feb 1994 08:49:37 GMT",
        "Sun, 06 Nov 1994 25:49:37 GMT",
        "Mon, 06 Nov 1994 08:49:37 GMT",
        "Sün, 06 Nov 1994 08:49:37 GMT",
        "0",
    ] {
        assert!(http_date(date).is_err(), "{date}");
    }
}

#[test]
fn untracked_variants_encoding_duplicates_and_sensitive_validators_are_rejected() {
    for value in ["*", "Accept-Language", "Cookie", "Origin, Accept-Language"] {
        assert_eq!(
            response(200, &[("Vary", value)]).unwrap_err(),
            Reason::VaryUnsupported
        );
    }
    assert_eq!(
        response(200, &[("Content-Encoding", "gzip")]).unwrap_err(),
        Reason::UnsupportedEncoding
    );
    assert_eq!(
        response(200, &[("content-length", "4")]).unwrap_err(),
        Reason::HeadersInvalid
    );
    for value in [
        "\"https://example.invalid/private?token=secret\"",
        "\"line\r\nsecret\"",
        "\"non ascii 😀\"",
        "unquoted",
        "\"\"",
    ] {
        assert!(response(200, &[("ETag", value)]).is_err());
    }
    assert_eq!(response(403, &[]).unwrap_err(), Reason::AccessDenied);
    let huge = "x".repeat(4097);
    assert_eq!(
        response(200, &[("X-Ignored", &huge)]).unwrap_err(),
        Reason::HeadersInvalid
    );
}

#[test]
fn vary_accept_encoding_tracks_the_shared_identity_request_profile() {
    let policy = response(
        200,
        &[
            ("ETag", "\"original-1\""),
            ("Vary", "Accept-Encoding, ACCEPT-ENCODING"),
            ("Content-Encoding", "identity"),
        ],
    )
    .unwrap();
    assert!(policy.directives.vary_accept_encoding);
    let record = policy.record([1; 32], [2; 32], [3; 32], 4).unwrap();
    let restored = Metadata::decode(&record.encode().unwrap()).unwrap();
    assert!(restored.directives.vary_accept_encoding);
    // Both observations select the explicitly requested identity encoding,
    // including when Vary starts/stops being repeated by the server.
    assert_eq!(policy.revalidate(&original(), [1; 32], [2; 32]), Ok(()));
    assert_eq!(
        response(200, &[("ETag", "\"original-1\"")])
            .unwrap()
            .revalidate(&restored, [1; 32], [2; 32]),
        Ok(())
    );
    for encoding in ["gzip", "br", "identity, gzip"] {
        assert_eq!(
            response(
                200,
                &[("Vary", "Accept-Encoding"), ("Content-Encoding", encoding)]
            )
            .unwrap_err(),
            Reason::UnsupportedEncoding
        );
    }
    for vary in [
        "Accept-Encoding, Origin",
        "Accept-Encoding, *",
        "Accept-Encoding,",
    ] {
        assert_eq!(
            response(200, &[("Vary", vary)]).unwrap_err(),
            Reason::VaryUnsupported
        );
    }
}

#[test]
fn unknown_headers_and_secrets_are_never_retained_in_metadata() {
    let policy = response(
        200,
        &[
            ("ETag", "\"opaque-1\""),
            ("Set-Cookie", "synthetic-private-cookie"),
            ("X-Debug-Private", "synthetic-private-account"),
        ],
    )
    .unwrap();
    let encoded = policy
        .record([1; 32], [2; 32], [3; 32], 4)
        .unwrap()
        .encode()
        .unwrap();
    let text = String::from_utf8_lossy(&encoded);
    assert!(!text.contains("synthetic-private"));
    assert!(!format!("{policy:?}").contains("opaque-1"));
    for reason in [
        Reason::NoStore,
        Reason::ValidatorsMissing,
        Reason::RepresentationChanged,
    ] {
        assert!(
            reason
                .as_str()
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'-')
        );
    }
}

#[test]
fn reusable_download_validator_requires_strong_identity_or_full_representation_digest() {
    assert!(!response(200, &[]).unwrap().has_validator());
    assert!(
        !response(200, &[("ETag", "W/\"weak\"")])
            .unwrap()
            .has_validator()
    );
    assert!(
        response(200, &[("ETag", "\"strong\"")])
            .unwrap()
            .has_validator()
    );
    let modified = "Tue, 01 Dec 2020 09:00:00 GMT";
    assert!(
        response(
            200,
            &[
                ("Last-Modified", modified),
                ("Date", "Tue, 01 Dec 2020 09:01:00 GMT")
            ]
        )
        .unwrap()
        .has_validator()
    );
    assert!(
        !response(
            200,
            &[
                ("Last-Modified", modified),
                ("Date", "Tue, 01 Dec 2020 09:00:59 GMT")
            ]
        )
        .unwrap()
        .has_validator()
    );
    let digest = format!("sha-256=:{}:", STANDARD.encode([1; 32]));
    assert!(
        response(200, &[("Repr-Digest", &digest)])
            .unwrap()
            .has_validator()
    );
}

fn remote_object(
    host: &str,
    path: &str,
    object: &str,
    bitrate: u32,
) -> crate::playback::RemotePlaybackSource {
    use std::time::{Duration, Instant};
    crate::playback::RemotePlaybackSource::from_resolver(
        "https://s123.storage.yandex.net/get-mp3/synthetic/0001/resource",
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap()
    .with_http_cache_resource(crate::stream::HttpResourceIdentity::for_test_object(
        host,
        path,
        bitrate,
        "0001",
        &format!("https://storage.mds.yandex.net/download-info/group/{object}"),
    ))
}

#[test]
fn scoped_object_metadata_survives_restart_and_authenticates_relocated_http_version() {
    let first = remote_object(
        "s123.storage.yandex.net",
        "/physical-a-offset-1-size-4",
        "object",
        192,
    );
    let moved = remote_object(
        "server-2026.storage.yandex.net",
        "/regional/physical-b-offset-2-size-4",
        "object",
        192,
    );
    let policy = response(200, &[("ETag", "\"object-version\"")]).unwrap();
    let metadata = policy.record_source(&first, [3; 32], 4).unwrap();
    let restored = Metadata::decode(&metadata.encode().unwrap()).unwrap();
    assert_ne!(first.http_cache_uri_hash(), moved.http_cache_uri_hash());
    assert_ne!(
        first.http_cache_resource_hash(),
        moved.http_cache_resource_hash()
    );
    assert_eq!(policy.revalidate_source(&restored, &moved), Ok(()));
    assert_eq!(
        response(200, &[("ETag", "\"new-version\"")])
            .unwrap()
            .revalidate_source(&restored, &moved),
        Err(Reason::RepresentationChanged)
    );
    assert!(!format!("{restored:?}").contains("physical"));
}

#[test]
fn logical_object_mismatch_cannot_fall_back_to_matching_physical_location_or_etag() {
    let first = remote_object(
        "s123.storage.yandex.net",
        "/same-physical-offset-1-size-4",
        "object-a",
        192,
    );
    let other = remote_object(
        "s123.storage.yandex.net",
        "/same-physical-offset-1-size-4",
        "object-b",
        192,
    );
    let policy = response(200, &[("ETag", "\"coincidence\"")]).unwrap();
    let metadata = policy.record_source(&first, [3; 32], 4).unwrap();
    assert_eq!(first.http_cache_uri_hash(), other.http_cache_uri_hash());
    assert_eq!(
        first.http_cache_resource_hash(),
        other.http_cache_resource_hash()
    );
    assert!(!metadata.validator_scope(&other));
    assert_eq!(
        policy.revalidate_source(&metadata, &other),
        Err(Reason::IdentityUnverified)
    );
    let full = format!("sha-256=:{}:", STANDARD.encode([3; 32]));
    assert_eq!(
        response(200, &[("Repr-Digest", &full)])
            .unwrap()
            .revalidate_source(&metadata, &other),
        Ok(())
    );
}

#[test]
fn old_record_without_binding_decodes_and_can_use_only_its_existing_scope_or_full_digest() {
    let first = remote_object(
        "s123.storage.yandex.net",
        "/physical-a-offset-1-size-4",
        "object",
        192,
    );
    let moved = remote_object(
        "server-2026.storage.yandex.net",
        "/physical-b-offset-2-size-4",
        "object",
        192,
    );
    let policy = response(200, &[("ETag", "\"version\"")]).unwrap();
    let metadata = policy
        .record(
            first.http_cache_uri_hash(),
            first.http_cache_resource_hash(),
            [3; 32],
            4,
        )
        .unwrap();
    let encoded = metadata.encode().unwrap();
    assert!(!String::from_utf8_lossy(&encoded).contains("binding"));
    let restored = Metadata::decode(&encoded).unwrap();
    assert_eq!(policy.revalidate_source(&restored, &first), Ok(()));
    assert_eq!(
        policy.revalidate_source(&restored, &moved),
        Err(Reason::IdentityUnverified)
    );
    let full = format!("sha-256=:{}:", STANDARD.encode([3; 32]));
    assert_eq!(
        response(200, &[("Repr-Digest", &full)])
            .unwrap()
            .revalidate_source(&restored, &moved),
        Ok(())
    );
}
