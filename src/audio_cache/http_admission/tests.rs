// SPDX-License-Identifier: GPL-3.0-or-later
//! Production admission and publication boundaries; no service account/media.

use super::*;
use crate::audio_cache::{
    http_policy::ResponsePolicy,
    test_support::{SyntheticAudio, account, structural_mp3},
};

fn fixture() -> (Arc<Sandbox>, AudioCache, Lease, ResourceKey) {
    let sandbox = Sandbox::create(&std::env::temp_dir()).unwrap();
    let authority = Arc::new(OnlineStreamAuthority::with_storage_permission(Arc::new(
        PrivateHttpCachePolicy,
    )));
    let cache = AudioCache::open_namespace(sandbox.clone(), authority, 1, 100_000).unwrap();
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    let track = ResourceKey::track("101").unwrap();
    let check = cache.begin_online_check(&lease, track).unwrap();
    cache.observe_verified_online(&lease, check).unwrap();
    (sandbox, cache, lease, track)
}

fn policy(length: usize) -> ResponsePolicy {
    ResponsePolicy::parse(
        200,
        [
            ("Content-Type".into(), "audio/mpeg".into()),
            ("Content-Length".into(), length.to_string()),
            ("ETag".into(), "\"original-response\"".into()),
            (
                "Cache-Control".into(),
                "private, no-cache, must-revalidate".into(),
            ),
        ],
    )
    .unwrap()
}

#[test]
fn private_http_candidate_cannot_bypass_response_or_revalidation() {
    let (_sandbox, cache, lease, track) = fixture();
    let audio = SyntheticAudio::tone(40).unwrap();
    let request = cache.begin(&lease, track).unwrap();
    assert_eq!(request.permit.basis, AdmissionBasis::PrivateHttpCandidate);
    assert_eq!(
        cache.put_reader(request, audio.0.as_slice()),
        Err(CacheError::HttpPolicyRequired)
    );
    // Even a completed online attempt is only a candidate. Neither the
    // ordinary writer nor get() may supply the missing HTTP policy checks.
    let request = cache.begin(&lease, track).unwrap();
    assert_eq!(
        cache.put_reader_before_commit(request, audio.0.as_slice(), || Ok(())),
        Err(CacheError::HttpPolicyRequired)
    );
    assert!(matches!(
        cache.get(&lease, track),
        Err(CacheError::NeedsOnline)
    ));
    assert_eq!(cache.stats().unwrap(), Stats::default());
    let key = cache.identity(&lease, track).unwrap().key();
    assert!(!cache.path(key).exists());
}

#[test]
fn http_publication_binds_atomic_metadata_and_counts_it_in_quota() {
    let (_sandbox, cache, lease, track) = fixture();
    let audio = structural_mp3();
    let expected_sha = hash(&audio);
    let response = policy(audio.len());
    let identity = cache.identity(&lease, track).unwrap();
    let path = cache.path(identity.key());

    for corrupt_checksum in [true, false] {
        cache
            .put_reader_http(
                cache.begin(&lease, track).unwrap(),
                audio.as_slice(),
                |sha256, bytes| {
                    assert_eq!(sha256, expected_sha);
                    assert_eq!(bytes, audio.len() as u64);
                    // No completed record is visible before the response
                    // policy and exact observed payload are admitted together.
                    assert!(!path.exists());
                    response
                        .record([1; 32], [2; 32], sha256, bytes)
                        .map_err(|_| CacheError::InvalidInput)
                },
            )
            .unwrap();

        let mut names = fs::read_dir(&path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["header", "http", "media.bin"]);
        let metadata = read_http_record(&path).unwrap().unwrap();
        assert_eq!(metadata.sha256, expected_sha);
        assert_eq!(metadata.bytes, audio.len() as u64);
        assert!(metadata.directives.no_cache && metadata.directives.must_revalidate);
        let actual = fs::read_dir(&path)
            .unwrap()
            .map(|entry| entry.unwrap().metadata().unwrap().len())
            .sum::<u64>();
        assert_eq!(
            cache.stats().unwrap(),
            Stats {
                bytes: actual,
                entries: 1,
            }
        );
        assert_eq!(
            actual,
            HEADER as u64 + audio.len() as u64 + metadata.encode().unwrap().len() as u64
        );
        assert!(matches!(
            cache.get(&lease, track),
            Err(CacheError::NeedsOnline)
        ));
        assert!(
            read_record(&path, identity.key(), Some(identity.scope()), None)
                .unwrap()
                .is_some()
        );

        let mut corrupted = metadata.clone();
        corrupted.sha256[0] ^= 1;
        let mut encoded = if corrupt_checksum {
            metadata.encode().unwrap()
        } else {
            // A valid metadata-envelope checksum must not hide a policy
            // sidecar that is bound to different audio bytes.
            corrupted.encode().unwrap()
        };
        if corrupt_checksum {
            encoded[8] ^= 1;
        }
        fs::write(path.join("http"), encoded).unwrap();
        assert!(
            read_record(&path, identity.key(), Some(identity.scope()), None)
                .unwrap()
                .is_none()
        );
        assert!(!path.exists());
        assert_eq!(cache.stats().unwrap(), Stats::default());
    }
}

#[test]
fn private_http_publication_rejects_non_mp3_even_when_headers_and_length_match() {
    for bytes in [
        b"<!doctype html><html>upstream error labeled audio/mpeg</html>".to_vec(),
        b"ID3\x04\0\0\0\0\0\0".to_vec(),
        structural_mp3()[..833].to_vec(),
    ] {
        let (_sandbox, cache, lease, track) = fixture();
        let response = policy(bytes.len());
        assert_eq!(
            cache.put_reader_http(
                cache.begin(&lease, track).unwrap(),
                bytes.as_slice(),
                |sha256, length| {
                    response
                        .record([1; 32], [2; 32], sha256, length)
                        .map_err(|_| CacheError::HttpPolicyRequired)
                }
            ),
            Err(CacheError::InvalidInput)
        );
        assert_eq!(cache.stats().unwrap(), Stats::default());
        let identity = cache.identity(&lease, track).unwrap();
        assert!(!cache.path(identity.key()).exists());
        assert!(
            fs::read_dir(&cache.0.sandbox.path)
                .unwrap()
                .all(|entry| entry.unwrap().file_name() == ".lock")
        );
    }
}
