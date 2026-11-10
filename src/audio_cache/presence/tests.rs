// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual protected records and original MPEG frames; no account or network.

use super::*;
use crate::audio_cache::test_support::{account, structural_mp3};

fn fixture() -> (Arc<Sandbox>, AudioCache, Lease, ResourceKey) {
    let sandbox = Sandbox::create(&std::env::temp_dir()).unwrap();
    let authority = Arc::new(OnlineStreamAuthority::with_storage_permission(Arc::new(
        PrivateHttpCachePolicy,
    )));
    let cache = AudioCache::open_namespace(sandbox.clone(), authority, 1, 100_000).unwrap();
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    let key = ResourceKey::track("101").unwrap();
    (sandbox, cache, lease, key)
}

fn request(cache: &AudioCache, lease: &Lease, key: ResourceKey) -> Request {
    let check = cache.begin_online_check(lease, key).unwrap();
    cache.observe_verified_online(lease, check).unwrap();
    cache.begin(lease, key).unwrap()
}

fn publish(cache: &AudioCache, lease: &Lease, key: ResourceKey) {
    let audio = structural_mp3();
    let policy = http_policy::ResponsePolicy::parse(
        200,
        [
            ("content-type".into(), "audio/mpeg".into()),
            ("content-length".into(), audio.len().to_string()),
            ("etag".into(), "\"original-complete-response\"".into()),
        ],
    )
    .unwrap();
    cache
        .put_reader_http(
            request(cache, lease, key),
            audio.as_slice(),
            |sha, length| {
                policy
                    .record([1; 32], [2; 32], sha, length)
                    .map_err(|_| CacheError::InvalidInput)
            },
        )
        .unwrap();
}

#[test]
fn only_an_actually_started_request_is_downloading_and_cancellation_retires_it() {
    let (_sandbox, cache, lease, key) = fixture();
    let mut job = request(&cache, &lease, key);
    assert_eq!(cache.presence(&lease, &[key]).unwrap(), [Presence::Missing]);
    cache.retain_write(&mut job, || true).unwrap();
    // Preparation/queueing is not proof that a download has started.
    assert_eq!(cache.presence(&lease, &[key]).unwrap(), [Presence::Missing]);
    cache.start_write(&job).unwrap();
    assert_eq!(
        cache.presence(&lease, &[key]).unwrap(),
        [Presence::Downloading]
    );
    job.cancel();
    assert_eq!(cache.presence(&lease, &[key]).unwrap(), [Presence::Missing]);
    drop(job);
    assert_eq!(cache.presence(&lease, &[key]).unwrap(), [Presence::Missing]);
}

#[test]
fn publication_switches_to_stored_without_granting_playback_and_reuses_a_verified_fingerprint() {
    let (_sandbox, cache, lease, key) = fixture();
    publish(&cache, &lease, key);
    assert_eq!(cache.presence(&lease, &[key]).unwrap(), [Presence::Stored]);
    assert!(matches!(
        cache.get(&lease, key),
        Err(CacheError::NeedsOnline)
    ));
    let duplicate_views = vec![key; MAX_QUERY];
    assert_eq!(
        cache.presence(&lease, &duplicate_views).unwrap(),
        vec![Presence::Stored; MAX_QUERY]
    );
    assert_eq!(cache.0.presence.lock().unwrap().verifications, 1);
    assert_eq!(cache.0.presence.lock().unwrap().records.len(), 1);
    assert_eq!(
        cache.presence(&lease, &vec![key; MAX_QUERY + 1]),
        Err(CacheError::Limit)
    );
}

#[test]
fn staged_and_partial_records_never_become_saved() {
    let (_sandbox, cache, lease, key) = fixture();
    let mut job = request(&cache, &lease, key);
    cache.retain_write(&mut job, || true).unwrap();
    cache.start_write(&job).unwrap();
    struct Truncated(bool);
    impl Read for Truncated {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            if self.0 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            self.0 = true;
            output[..4].copy_from_slice(b"part");
            Ok(4)
        }
    }
    assert_eq!(
        cache.put_reader_http_attempt(&job, Truncated(false), |_, _| unreachable!()),
        Err(CacheError::Unavailable)
    );
    assert_eq!(
        cache.presence(&lease, &[key]).unwrap(),
        [Presence::Downloading]
    );
    drop(job);
    assert_eq!(cache.presence(&lease, &[key]).unwrap(), [Presence::Missing]);
    assert_eq!(cache.stats().unwrap().entries, 0);
}

#[test]
fn cold_reopen_recovers_presence_from_validated_files_without_an_online_grant() {
    let (sandbox, cache, lease, key) = fixture();
    publish(&cache, &lease, key);
    assert_eq!(cache.presence(&lease, &[key]).unwrap(), [Presence::Stored]);
    drop(cache);
    let authority = Arc::new(OnlineStreamAuthority::with_storage_permission(Arc::new(
        PrivateHttpCachePolicy,
    )));
    let restarted = AudioCache::open_namespace(sandbox, authority, 1, 100_000).unwrap();
    let fresh = restarted.bind(Some(account(1))).unwrap().unwrap();
    assert_eq!(restarted.0.presence.lock().unwrap().records.len(), 0);
    assert_eq!(
        restarted.presence(&fresh, &[key]).unwrap(),
        [Presence::Stored]
    );
    assert!(restarted.get(&fresh, key).is_err());
}

#[test]
fn corruption_or_deletion_invalidates_the_projection_and_does_not_touch_other_tracks() {
    let (_sandbox, cache, lease, first) = fixture();
    let second = ResourceKey::track("102").unwrap();
    publish(&cache, &lease, first);
    publish(&cache, &lease, second);
    assert_eq!(
        cache.presence(&lease, &[first, second]).unwrap(),
        [Presence::Stored, Presence::Stored]
    );
    let path = cache.path(cache.identity(&lease, first).unwrap().key());
    let mut file = OpenOptions::new()
        .write(true)
        .open(path.join("media.bin"))
        .unwrap();
    file.write_all(b"bad").unwrap();
    file.sync_all().unwrap();
    assert_eq!(
        cache.presence(&lease, &[first, second]).unwrap(),
        [Presence::Missing, Presence::Stored]
    );
    // Status inspection never deletes paths behind a publication/selection.
    assert!(path.exists());
    remove_record(&path).unwrap();
    assert_eq!(
        cache.presence(&lease, &[first, second]).unwrap(),
        [Presence::Missing, Presence::Stored]
    );
}

#[test]
fn account_and_clear_fences_cannot_reuse_an_old_presence_result() {
    let (_sandbox, cache, lease, key) = fixture();
    publish(&cache, &lease, key);
    assert_eq!(cache.presence(&lease, &[key]).unwrap(), [Presence::Stored]);
    let other = cache.bind(Some(account(2))).unwrap().unwrap();
    assert!(!cache.presence_current(&lease));
    assert_eq!(cache.presence(&lease, &[key]), Err(CacheError::Cancelled));
    assert_eq!(cache.presence(&other, &[key]).unwrap(), [Presence::Missing]);
    let same = cache.bind(Some(account(1))).unwrap().unwrap();
    assert_eq!(cache.presence(&same, &[key]).unwrap(), [Presence::Stored]);
    cache.fence().unwrap();
    assert!(!cache.presence_current(&same));
    cache.clear().unwrap();
    let after_clear = cache.lease().unwrap();
    assert_eq!(
        cache.presence(&after_clear, &[key]).unwrap(),
        [Presence::Missing]
    );
}

#[test]
fn shared_budget_eviction_and_disable_are_visible_without_a_second_store_index() {
    let sandbox = Sandbox::create(&std::env::temp_dir()).unwrap();
    let metadata = Sandbox::create(&std::env::temp_dir()).unwrap();
    let budget = crate::cache_budget::CacheBudget::new(
        metadata.path.clone(),
        sandbox.path.clone(),
        true,
        None,
    )
    .unwrap();
    let authority = Arc::new(OnlineStreamAuthority::with_storage_permission(Arc::new(
        PrivateHttpCachePolicy,
    )));
    let cache = AudioCache::open_namespace_with_budget(
        sandbox,
        authority,
        1,
        100_000,
        Some(budget.clone()),
    )
    .unwrap();
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    let key = ResourceKey::track("101").unwrap();
    publish(&cache, &lease, key);
    assert_eq!(cache.presence(&lease, &[key]).unwrap(), [Presence::Stored]);
    budget.configure(false, None).unwrap();
    assert!(!cache.presence_current(&lease));
    assert_eq!(
        cache.presence(&cache.lease().unwrap(), &[key]),
        Err(CacheError::Cancelled)
    );
    budget.configure(true, Some(1)).unwrap();
    let current = cache.lease().unwrap();
    assert_eq!(
        cache.presence(&current, &[key]).unwrap(),
        [Presence::Stored]
    );
    // Eviction belongs to CacheBudget, not to this AudioCache observer.
    budget.trim().unwrap();
    assert_eq!(
        cache.presence(&current, &[key]).unwrap(),
        [Presence::Missing]
    );
    // An actual new save after eviction must restore the same projection;
    // the removed inode's memo and the old budget lease cannot mask it.
    budget
        .configure(true, Some(2 * 1024 * 1024 * 1024))
        .unwrap();
    let repaired = cache.lease().unwrap();
    publish(&cache, &repaired, key);
    assert_eq!(
        cache.presence(&repaired, &[key]).unwrap(),
        [Presence::Stored]
    );
}

#[test]
fn a_busy_atomic_publication_gate_never_creates_false_saved_status() {
    let (_sandbox, cache, lease, key) = fixture();
    publish(&cache, &lease, key);
    let gate = cache.io().unwrap();
    assert_eq!(cache.presence(&lease, &[key]), Err(CacheError::Busy));
    assert_eq!(cache.0.presence.lock().unwrap().records.len(), 0);
    drop(gate);
    assert_eq!(cache.presence(&lease, &[key]).unwrap(), [Presence::Stored]);
}
