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
fn private_no_cache_requires_validation_even_with_a_freshness_lifetime() {
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

fn application() -> ApplicationIdentity {
    ApplicationIdentity {
        logical: [10; 32],
        variant: [11; 32],
        schema: 2,
    }
}

fn selected(mut metadata: Metadata) -> Metadata {
    let identity = application();
    metadata.binding = Some(ResourceBinding {
        schema: identity.schema,
        logical: Some(identity.logical),
        host: [12; 32],
        path: [13; 32],
        variant: identity.variant,
    });
    metadata
}

fn observed(extra: &[(&str, &str)], elapsed: Duration, receipt: u64) -> Metadata {
    selected(
        response(200, extra)
            .unwrap()
            .observed_at(elapsed, receipt)
            .record([1; 32], [2; 32], [3; 32], 4)
            .unwrap(),
    )
}

#[test]
fn fresh_private_response_reuses_current_application_selection_without_any_validator() {
    let policy = response(200, &[("Cache-Control", "private, max-age=3600")])
        .unwrap()
        .observed_at(Duration::from_millis(20), 1000);
    assert!(!policy.has_validator());
    assert!(policy.reusable());
    let metadata = selected(policy.record([1; 32], [2; 32], [3; 32], 4).unwrap());
    assert_eq!(
        metadata.fresh_for(application(), 1010),
        Ok(FreshReuse {
            kind: Freshness::Explicit,
            remaining: Duration::from_secs(3589),
        })
    );
    let reopened = Metadata::decode(&metadata.encode().unwrap()).unwrap();
    assert_eq!(
        reopened.fresh_for(application(), 4600),
        Err(Reason::FreshnessExpired)
    );
    assert!(!reopened.needs_initial_refresh());
    // Physical placement is deliberately absent from application admission.
    let relocated = Metadata {
        binding: metadata.binding.map(|before| ResourceBinding {
            host: [90; 32],
            path: [91; 32],
            ..before
        }),
        ..metadata.clone()
    };
    assert_eq!(
        relocated.fresh_for(application(), 1010),
        metadata.fresh_for(application(), 1010)
    );
    for changed in [
        ApplicationIdentity {
            logical: [99; 32],
            ..application()
        },
        ApplicationIdentity {
            variant: [99; 32],
            ..application()
        },
    ] {
        assert_eq!(
            metadata.fresh_for(changed, 1010),
            Err(Reason::ApplicationChanged)
        );
    }
    assert_eq!(
        metadata.fresh_for(
            ApplicationIdentity {
                schema: 1,
                ..application()
            },
            1010
        ),
        Err(Reason::ApplicationUnverified)
    );
}

#[test]
fn no_cache_is_always_revalidated_but_must_revalidate_allows_only_fresh_reuse() {
    let no_cache = observed(
        &[("Cache-Control", "no-cache, max-age=3600")],
        Duration::ZERO,
        1000,
    );
    assert_eq!(
        no_cache.fresh_for(application(), 1001),
        Err(Reason::FreshnessNoCache)
    );
    assert!(
        !response(200, &[("Cache-Control", "no-cache, max-age=3600")])
            .unwrap()
            .observed_at(Duration::ZERO, 1000)
            .reusable()
    );
    let must = observed(
        &[("Cache-Control", "must-revalidate, max-age=60")],
        Duration::ZERO,
        1000,
    );
    assert!(must.fresh_for(application(), 1059).is_ok());
    assert_eq!(
        must.fresh_for(application(), 1060),
        Err(Reason::FreshnessExpired)
    );
    let shared_only = observed(&[("Cache-Control", "s-maxage=3600")], Duration::ZERO, 1000);
    assert_eq!(
        shared_only.fresh_for(application(), 1001),
        Err(Reason::FreshnessMissing)
    );
}

#[test]
fn age_receipt_request_delay_restart_and_clock_uncertainty_bound_reuse() {
    let metadata = observed(
        &[("Cache-Control", "max-age=60"), ("Age", "20")],
        Duration::from_millis(1201),
        1000,
    );
    assert_eq!(metadata.observed.unwrap().corrected_initial_age, 22);
    assert_eq!(
        metadata.fresh_for(application(), 1010).unwrap().remaining,
        Duration::from_secs(28)
    );
    assert_eq!(
        metadata.fresh_for(application(), 1038),
        Err(Reason::FreshnessExpired)
    );
    assert_eq!(
        metadata.fresh_for(application(), 999),
        Err(Reason::ClockUncertain)
    );
    let overflow = observed(
        &[
            ("Cache-Control", "max-age=60"),
            ("Age", "18446744073709551615"),
        ],
        Duration::from_secs(1),
        1000,
    );
    assert_eq!(
        overflow.fresh_for(application(), 1000),
        Err(Reason::FreshnessMissing)
    );
    let legacy = Metadata {
        observed: None,
        ..metadata
    };
    assert!(legacy.needs_initial_refresh());
    assert_eq!(
        legacy.fresh_for(application(), 1001),
        Err(Reason::FreshnessMissing)
    );
}

#[test]
fn expires_max_age_precedence_apparent_age_and_future_dates_are_conservative() {
    let date = "Mon, 01 Jan 2024 00:00:00 GMT";
    let now = http_date(date).unwrap();
    let metadata = observed(
        &[("Date", date), ("Expires", "Mon, 01 Jan 2024 00:10:00 GMT")],
        Duration::ZERO,
        now + 100,
    );
    assert_eq!(metadata.observed.unwrap().corrected_initial_age, 100);
    assert_eq!(
        metadata
            .fresh_for(application(), now + 200)
            .unwrap()
            .remaining,
        Duration::from_secs(400)
    );
    let maximum = observed(
        &[
            ("Date", date),
            ("Expires", "Mon, 01 Jan 2024 00:10:00 GMT"),
            ("Cache-Control", "max-age=30"),
        ],
        Duration::ZERO,
        now,
    );
    assert_eq!(
        maximum.fresh_for(application(), now + 30),
        Err(Reason::FreshnessExpired)
    );
    let future = observed(
        &[("Date", date), ("Cache-Control", "max-age=60")],
        Duration::ZERO,
        now - 1,
    );
    assert_eq!(
        future.fresh_for(application(), now),
        Err(Reason::FreshnessMissing)
    );
}

#[test]
fn heuristic_is_ten_percent_capped_and_never_overrides_explicit_staleness() {
    let date = "Mon, 01 Jan 2024 00:00:00 GMT";
    let now = http_date(date).unwrap();
    let headers = [
        ("Date", date),
        ("Last-Modified", "Sun, 31 Dec 2023 23:50:00 GMT"),
    ];
    let metadata = observed(&headers, Duration::ZERO, now);
    assert_eq!(
        metadata.fresh_for(application(), now).unwrap(),
        FreshReuse {
            kind: Freshness::Heuristic,
            remaining: Duration::from_secs(60),
        }
    );
    assert_eq!(
        metadata.fresh_for(application(), now + 60),
        Err(Reason::FreshnessExpired)
    );
    let old = observed(
        &[
            ("Date", date),
            ("Last-Modified", "Sat, 30 Dec 2023 00:00:00 GMT"),
        ],
        Duration::ZERO,
        now,
    );
    assert_eq!(
        old.fresh_for(application(), now).unwrap().remaining,
        Duration::from_secs(3600)
    );
    for explicit in [("Cache-Control", "max-age=0"), ("Expires", "0")] {
        let mut headers = headers.to_vec();
        headers.push(explicit);
        assert_eq!(
            observed(&headers, Duration::ZERO, now).fresh_for(application(), now),
            Err(Reason::FreshnessExpired)
        );
    }
    let incomplete_status = Metadata {
        observed: metadata.observed.map(|before| Observation {
            response_status: 206,
            ..before
        }),
        ..metadata
    };
    assert_eq!(
        incomplete_status.fresh_for(application(), now),
        Err(Reason::FreshnessMissing)
    );
}

#[test]
fn legacy_schema_is_not_a_proved_encoding_change_and_refresh_is_one_time() {
    let mut metadata = observed(&[("Cache-Control", "max-age=60")], Duration::ZERO, 1000);
    metadata.binding.as_mut().unwrap().schema = 1;
    assert!(metadata.needs_initial_refresh());
    assert_eq!(
        metadata.fresh_for(application(), 1001),
        Err(Reason::ApplicationUnverified)
    );
    metadata.binding.as_mut().unwrap().schema = 2;
    assert!(!metadata.needs_initial_refresh());
    assert!(metadata.fresh_for(application(), 1001).is_ok());
    let encoded = metadata.encode().unwrap();
    assert!(!Metadata::decode(&encoded).unwrap().needs_initial_refresh());
}

#[test]
fn not_modified_uses_new_date_for_inherited_expires_and_missing_date_cannot_reset_age() {
    let date = "Mon, 01 Jan 2024 00:00:00 GMT";
    let now = http_date(date).unwrap();
    let stored = observed(
        &[
            ("Date", date),
            ("Last-Modified", "Sun, 31 Dec 2023 23:50:00 GMT"),
            ("Expires", "Mon, 01 Jan 2024 00:01:00 GMT"),
            ("ETag", "\"same\""),
        ],
        Duration::ZERO,
        now,
    );
    let headers = [
        ("Date", "Mon, 01 Jan 2024 00:02:00 GMT"),
        ("ETag", "\"same\""),
    ];
    let policy = response(304, &headers)
        .unwrap()
        .observed_at(Duration::ZERO, now + 120);
    let fresh = same(policy.revalidate_scoped(
        &stored,
        stored.uri_hash,
        stored.resource_hash,
        stored.binding,
        true,
    ));
    assert_eq!(fresh.date_unix, Some(now + 120));
    assert_eq!(
        fresh.fresh_for(application(), now + 120),
        Err(Reason::FreshnessExpired)
    );
    let no_date = response(304, &[("ETag", "\"same\"")])
        .unwrap()
        .observed_at(Duration::ZERO, now + 120);
    let fresh = same(no_date.revalidate_scoped(
        &stored,
        stored.uri_hash,
        stored.resource_hash,
        stored.binding,
        true,
    ));
    assert_eq!(fresh.observed.unwrap().corrected_initial_age, 120);
    assert_eq!(
        fresh.fresh_for(application(), now + 120),
        Err(Reason::FreshnessExpired)
    );
}

#[test]
fn not_modified_replaces_present_cache_control_instead_of_reviving_old_max_age() {
    let date = "Mon, 01 Jan 2024 00:00:00 GMT";
    let now = http_date(date).unwrap();
    let stored = observed(
        &[
            ("Date", date),
            ("Cache-Control", "max-age=3600"),
            ("ETag", "\"same\""),
        ],
        Duration::ZERO,
        now,
    );
    let policy = response(
        304,
        &[
            ("Date", "Mon, 01 Jan 2024 00:02:00 GMT"),
            ("ETag", "\"same\""),
            ("Cache-Control", "private"),
            ("Expires", "0"),
        ],
    )
    .unwrap()
    .observed_at(Duration::ZERO, now + 120);
    let fresh = same(policy.revalidate_scoped(
        &stored,
        stored.uri_hash,
        stored.resource_hash,
        stored.binding,
        true,
    ));
    assert_eq!(fresh.directives.max_age, None);
    assert_eq!(
        fresh.fresh_for(application(), now + 120),
        Err(Reason::FreshnessExpired)
    );
    let omitted = response(
        304,
        &[
            ("Date", "Mon, 01 Jan 2024 00:02:00 GMT"),
            ("ETag", "\"same\""),
        ],
    )
    .unwrap()
    .observed_at(Duration::ZERO, now + 120);
    let fresh = same(omitted.revalidate_scoped(
        &stored,
        stored.uri_hash,
        stored.resource_hash,
        stored.binding,
        true,
    ));
    assert_eq!(fresh.directives.max_age, Some(3600));
    assert!(fresh.fresh_for(application(), now + 120).is_ok());
}

#[test]
fn expired_replica_refresh_needs_actual_fresh_policy_not_just_new_opaque_validators() {
    let stored = observed(
        &[("Cache-Control", "max-age=60"), ("ETag", "\"old-replica\"")],
        Duration::ZERO,
        1000,
    );
    assert_eq!(
        stored.fresh_for(application(), 1060),
        Err(Reason::FreshnessExpired)
    );
    for (control, refresh) in [
        ("max-age=3600", true),
        ("no-cache, max-age=3600", false),
        ("max-age=0", false),
        ("private", false),
    ] {
        let current = response(
            200,
            &[("Cache-Control", control), ("ETag", "\"new-replica\"")],
        )
        .unwrap()
        .observed_at(Duration::ZERO, 1060);
        let validation = current
            .revalidate_scoped(&stored, [9; 32], [8; 32], stored.binding, false)
            .unwrap();
        assert_eq!(
            validation,
            if refresh {
                Validation::RefreshFresh(Reason::ValidatorsIncomparable)
            } else {
                Validation::Unproven(Reason::ValidatorsIncomparable)
            }
        );
    }
}

#[test]
fn strong_etag_accepts_same_resource_but_not_unrelated_identical_tags() {
    let policy = response(200, &[("ETag", "\"original-1\"")]).unwrap();
    assert_eq!(policy.revalidate(&original(), [1; 32], [2; 32]), Ok(()));
    // Even the same physical descriptor does not prove that opaque validators
    // share a namespace at another signed request URI.
    assert_eq!(
        policy.revalidate(&original(), [7; 32], [2; 32]),
        Err(Reason::ResourceChanged)
    );
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
fn strong_last_modified_requires_both_dates_and_the_exact_request_uri() {
    let headers = [
        ("Last-Modified", "Sun, 06 Nov 1994 08:49:37 GMT"),
        ("Date", "Sun, 06 Nov 1994 08:50:37 GMT"),
    ];
    let policy = response(200, &headers).unwrap();
    let record = policy.record([1; 32], [2; 32], [3; 32], 4).unwrap();
    assert_eq!(policy.revalidate(&record, [1; 32], [2; 32]), Ok(()));
    // A matching date at another signed URI is not a common validator scope,
    // even when the physical locator hash remains unchanged.
    assert_eq!(
        policy.revalidate(&record, [7; 32], [2; 32]),
        Err(Reason::ResourceChanged)
    );
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

fn same(result: Result<Validation, Reason>) -> Metadata {
    match result.unwrap() {
        Validation::Same(metadata) => *metadata,
        other => panic!("expected proven same representation, got {other:?}"),
    }
}

#[test]
fn exact_request_scope_survives_restart_but_logical_object_does_not_scope_replicas() {
    let first = remote_object("s123.storage.yandex.net", "/physical-a", "object", 192);
    let moved = remote_object(
        "server-2026.storage.yandex.net",
        "/regional/physical-b",
        "object",
        192,
    );
    let policy = response(200, &[("ETag", "\"object-version\"")]).unwrap();
    let metadata = policy.record_source(&first, [3; 32], 4).unwrap();
    let restored = Metadata::decode(&metadata.encode().unwrap()).unwrap();
    let validated = same(policy.revalidate_source(&restored, &first));
    assert_eq!(validated, restored);
    assert!(!restored.validator_scope(&moved));
    for etag in ["\"object-version\"", "\"independent-replica-tag\""] {
        assert_eq!(
            response(200, &[("ETag", etag)])
                .unwrap()
                .revalidate_source(&restored, &moved),
            Ok(Validation::Unproven(Reason::ValidatorsIncomparable))
        );
    }
    assert_eq!(
        response(200, &[("ETag", "\"new-version\"")])
            .unwrap()
            .revalidate_source(&restored, &first),
        Ok(Validation::Different(Reason::ScopedValidatorChanged))
    );
    assert!(!format!("{restored:?}").contains("physical"));
}

#[test]
fn logical_object_mismatch_cannot_fall_back_to_exact_uri_or_equal_tag() {
    let first = remote_object("s123.storage.yandex.net", "/same-physical", "object-a", 192);
    let other = remote_object("s123.storage.yandex.net", "/same-physical", "object-b", 192);
    let policy = response(200, &[("ETag", "\"coincidence\"")]).unwrap();
    let metadata = policy.record_source(&first, [3; 32], 4).unwrap();
    assert_eq!(first.http_cache_uri_hash(), other.http_cache_uri_hash());
    assert!(!metadata.validator_scope(&other));
    assert_eq!(
        policy.revalidate_source(&metadata, &other),
        Ok(Validation::Unproven(Reason::ValidatorsIncomparable))
    );
    let full = format!("sha-256=:{}:", STANDARD.encode([3; 32]));
    let refreshed = same(
        response(
            200,
            &[("Repr-Digest", &full), ("ETag", "\"current-scope\"")],
        )
        .unwrap()
        .revalidate_source(&metadata, &other),
    );
    assert_eq!(refreshed.binding, other.http_cache_binding());
    assert_eq!(refreshed.etag.as_deref(), Some("\"current-scope\""));
}

#[test]
fn signed_uri_change_requires_full_digest_even_with_same_physical_locator() {
    let first = remote_object("s123.storage.yandex.net", "/same-physical", "object", 192);
    let renewed = crate::playback::RemotePlaybackSource::from_resolver(
        "https://s123.storage.yandex.net/get-mp3/current/0002/resource",
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )
    .unwrap()
    .with_http_cache_resource(crate::stream::HttpResourceIdentity::for_test_object(
        "s123.storage.yandex.net",
        "/same-physical",
        192,
        "0002",
        "https://storage.mds.yandex.net/download-info/group/object",
    ));
    let policy = response(200, &[("ETag", "\"same-tag\"")]).unwrap();
    let metadata = policy.record_source(&first, [3; 32], 4).unwrap();
    assert_ne!(metadata.uri_hash, renewed.http_cache_uri_hash());
    assert_eq!(metadata.resource_hash, renewed.http_cache_resource_hash());
    assert!(!metadata.validator_scope(&renewed));
    assert_eq!(
        policy.revalidate_source(&metadata, &renewed),
        Ok(Validation::Unproven(Reason::ValidatorsIncomparable))
    );
    let full = format!("sha-256=:{}:", STANDARD.encode([3; 32]));
    assert_eq!(
        same(
            response(200, &[("Repr-Digest", &full)])
                .unwrap()
                .revalidate_source(&metadata, &renewed)
        )
        .sha256,
        [3; 32]
    );
}

#[test]
fn independent_replica_last_modified_is_not_a_confirmed_version_change() {
    let first = remote_object("s123.storage.yandex.net", "/physical-a", "object", 192);
    let moved = remote_object(
        "server-2026.storage.yandex.net",
        "/physical-b",
        "object",
        192,
    );
    let old = response(
        200,
        &[
            ("Last-Modified", "Tue, 01 Dec 2020 09:00:00 GMT"),
            ("Date", "Tue, 01 Dec 2020 09:02:00 GMT"),
        ],
    )
    .unwrap();
    let metadata = old.record_source(&first, [3; 32], 4).unwrap();
    let fresh = response(
        200,
        &[
            ("Last-Modified", "Wed, 02 Dec 2020 09:00:00 GMT"),
            ("Date", "Wed, 02 Dec 2020 09:02:00 GMT"),
        ],
    )
    .unwrap();
    assert_eq!(
        fresh.revalidate_source(&metadata, &moved),
        Ok(Validation::Unproven(Reason::ValidatorsIncomparable))
    );
    assert_eq!(
        fresh.revalidate_source(&metadata, &first),
        Ok(Validation::Different(Reason::ScopedValidatorChanged))
    );
}

#[test]
fn full_size_digest_and_encoding_changes_have_distinct_positive_evidence() {
    let first = remote_object("s123.storage.yandex.net", "/physical-a", "object", 192);
    let moved = remote_object(
        "server-2026.storage.yandex.net",
        "/physical-b",
        "object",
        192,
    );
    let policy = response(200, &[("ETag", "\"old-tag\"")]).unwrap();
    let metadata = policy.record_source(&first, [3; 32], 4).unwrap();
    let changed_size = ResponsePolicy::parse(
        200,
        [
            ("content-type".into(), "audio/mpeg".into()),
            ("content-length".into(), "5".into()),
        ],
    )
    .unwrap();
    assert_eq!(
        changed_size.revalidate_source(&metadata, &moved),
        Ok(Validation::Different(Reason::LengthChanged))
    );
    let other_digest = format!("sha-256=:{}:", STANDARD.encode([4; 32]));
    assert_eq!(
        response(200, &[("Repr-Digest", &other_digest)])
            .unwrap()
            .revalidate_source(&metadata, &moved),
        Ok(Validation::Different(Reason::ContentChanged))
    );
    let other_encoding = remote_object("s123.storage.yandex.net", "/physical-a", "object", 320);
    assert_eq!(
        policy.revalidate_source(&metadata, &other_encoding),
        Ok(Validation::Different(Reason::EncodingChanged))
    );
}

#[test]
fn old_binding_migrates_only_after_proof_and_never_transplants_replica_validators() {
    let first = remote_object("s123.storage.yandex.net", "/physical-a", "object", 192);
    let moved = remote_object(
        "server-2026.storage.yandex.net",
        "/physical-b",
        "object",
        192,
    );
    let policy = response(200, &[("ETag", "\"old-scope\"")]).unwrap();
    let metadata = policy
        .record(
            first.http_cache_uri_hash(),
            first.http_cache_resource_hash(),
            [3; 32],
            4,
        )
        .unwrap();
    let restored = Metadata::decode(&metadata.encode().unwrap()).unwrap();
    assert_eq!(restored.binding, None);
    assert_eq!(
        same(policy.revalidate_source(&restored, &first)).binding,
        first.http_cache_binding()
    );
    assert_eq!(
        policy.revalidate_source(&restored, &moved),
        Ok(Validation::Unproven(Reason::ValidatorsIncomparable))
    );
    let full = format!("sha-256=:{}:", STANDARD.encode([3; 32]));
    let digest_only = same(
        response(200, &[("Repr-Digest", &full)])
            .unwrap()
            .revalidate_source(&restored, &moved),
    );
    assert_eq!(digest_only.binding, moved.http_cache_binding());
    assert_eq!(digest_only.etag, None);
    assert_eq!(digest_only.last_modified, None);
    assert_eq!(digest_only.uri_hash, moved.http_cache_uri_hash());
    assert_eq!(digest_only.resource_hash, moved.http_cache_resource_hash());
    assert_eq!(
        Metadata::decode(&digest_only.encode().unwrap()).unwrap(),
        digest_only
    );
}

#[test]
fn unbound_or_unsolicited_304_cannot_bootstrap_identity_even_with_a_digest() {
    let first = remote_object("s123.storage.yandex.net", "/physical-a", "object", 192);
    let moved = remote_object(
        "server-2026.storage.yandex.net",
        "/physical-b",
        "object",
        192,
    );
    let full = format!("sha-256=:{}:", STANDARD.encode([3; 32]));
    let response = ResponsePolicy::parse(
        304,
        [
            ("repr-digest".into(), full),
            ("etag".into(), "\"old-scope\"".into()),
            ("content-length".into(), "4".into()),
        ],
    )
    .unwrap();
    let stored = original();
    assert_eq!(
        response.revalidate_source(&stored, &moved),
        Ok(Validation::Unproven(Reason::UnsupportedValidator))
    );
    let no_condition = Metadata {
        uri_hash: first.http_cache_uri_hash(),
        resource_hash: first.http_cache_resource_hash(),
        binding: first.http_cache_binding(),
        etag: None,
        last_modified: None,
        modified_unix: None,
        ..stored
    };
    assert_eq!(
        response.revalidate_source(&no_condition, &first),
        Ok(Validation::Unproven(Reason::UnsupportedValidator))
    );
}

#[test]
fn contradictory_not_modified_responses_are_not_version_change_evidence() {
    let remote = remote_object("s123.storage.yandex.net", "/physical-a", "object", 192);
    let metadata = response(200, &[("ETag", "\"stored\"")])
        .unwrap()
        .record_source(&remote, [3; 32], 4)
        .unwrap();
    let digest = format!("sha-256=:{}:", STANDARD.encode([4; 32]));
    for extra in [
        vec![("etag", "\"wrong\""), ("content-length", "4")],
        vec![("etag", "\"stored\""), ("content-length", "5")],
        vec![
            ("etag", "\"stored\""),
            ("content-length", "4"),
            ("repr-digest", digest.as_str()),
        ],
    ] {
        let policy =
            ResponsePolicy::parse(304, extra.into_iter().map(|(a, b)| (a.into(), b.into())))
                .unwrap();
        assert_eq!(
            policy.revalidate_source(&metadata, &remote),
            Ok(Validation::Unproven(Reason::UnsupportedValidator))
        );
    }
    let weak = ResponsePolicy::parse(304, [("etag".into(), "W/\"stored\"".into())]).unwrap();
    assert_eq!(
        weak.revalidate_source(&metadata, &remote),
        Ok(Validation::Unproven(Reason::ValidatorsMissing))
    );
}

#[test]
fn qualified_last_modified_304_requires_the_actual_matching_conditional_kind() {
    let remote = remote_object("s123.storage.yandex.net", "/physical-a", "object", 192);
    let modified = "Tue, 01 Dec 2020 09:00:00 GMT";
    let date = "Tue, 01 Dec 2020 09:02:00 GMT";
    let policy = response(200, &[("Last-Modified", modified), ("Date", date)]).unwrap();
    let metadata = policy.record_source(&remote, [3; 32], 4).unwrap();
    let not_modified = ResponsePolicy::parse(
        304,
        [
            ("last-modified".into(), modified.into()),
            ("date".into(), date.into()),
        ],
    )
    .unwrap();
    assert_eq!(
        same(not_modified.revalidate_source(&metadata, &remote))
            .last_modified
            .as_deref(),
        Some(modified)
    );
    let strong_etag_stored = Metadata {
        etag: Some("\"conditional-etag\"".into()),
        ..metadata
    };
    // That request carries If-None-Match, so an LM-only304 cannot substitute
    // another validator kind for its missing strong ETag echo.
    assert_eq!(
        not_modified.revalidate_source(&strong_etag_stored, &remote),
        Ok(Validation::Unproven(Reason::ValidatorsMissing))
    );
}
