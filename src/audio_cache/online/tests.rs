// SPDX-License-Identifier: GPL-3.0-or-later
//! Original generated audio and independent fixture permissions exercise the
//! production admission coordinator; no account or service media is used.

use super::*;
use crate::audio_cache::test_support::{SyntheticAudio, SyntheticAuthority, account};
use std::{
    sync::{Condvar, mpsc},
    thread,
};

struct Fixture {
    _sandbox: Arc<Sandbox>,
    permission: Arc<SyntheticAuthority>,
    authority: Arc<OnlineStreamAuthority>,
    cache: AudioCache,
    lease: Lease,
    budget: Arc<crate::cache_budget::CacheBudget>,
}

impl Fixture {
    fn new() -> Self {
        let sandbox = Sandbox::create(&std::env::temp_dir()).unwrap();
        let permission = Arc::new(SyntheticAuthority::default());
        permission.allow(&account(1), &101u64.into());
        permission.allow(&account(1), &102u64.into());
        let authority = Arc::new(OnlineStreamAuthority::with_storage_permission(
            permission.clone(),
        ));
        let budget = crate::cache_budget::CacheBudget::new(
            sandbox.path.join("metadata"),
            sandbox.path.join("audio"),
            true,
            Some(500 * 1024 * 1024),
        )
        .unwrap();
        let cache = AudioCache::open_with_budget(
            sandbox.path.join("audio"),
            authority.clone(),
            CachePolicy::default(),
            budget.clone(),
        )
        .unwrap();
        let lease = cache.bind(Some(account(1))).unwrap().unwrap();
        Self {
            _sandbox: sandbox,
            permission,
            authority,
            cache,
            lease,
            budget,
        }
    }

    fn observe(&self, track: u64) {
        let check = self
            .cache
            .begin_online_check(&self.lease, track.into())
            .unwrap();
        self.cache
            .observe_verified_online(&self.lease, check)
            .unwrap();
    }

    fn put_original(&self) {
        self.cache
            .put(
                self.cache.begin(&self.lease, 101u64).unwrap(),
                &SyntheticAudio::tone(400).unwrap(),
            )
            .unwrap();
    }
}

fn duplicate(check: &OnlineCheck) -> OnlineCheck {
    // Deliberately reproduce a ticket for adversarial replay testing. Normal
    // callers receive an opaque non-Clone, non-Copy one-use value.
    OnlineCheck {
        identity: check.identity,
        sequence: check.sequence,
    }
}

#[test]
fn default_and_unpermitted_online_success_never_create_storage_rights() {
    let f = Fixture::new();
    let identity = f.cache.identity(&f.lease, 101u64.into()).unwrap();
    for authority in [
        OnlineStreamAuthority::default(),
        OnlineStreamAuthority::with_storage_permission(Arc::new(SyntheticAuthority::default())),
    ] {
        let check = authority.begin_online(identity).unwrap();
        assert!(matches!(
            authority.authorize(&identity),
            Err(CacheError::NoGrant)
        ));
        assert!(matches!(
            authority.prepare_online(check),
            Err(CacheError::NoGrant)
        ));
        assert!(matches!(
            authority.authorize(&identity),
            Err(CacheError::NoGrant)
        ));
    }
    assert_eq!(f.cache.stats().unwrap(), Stats::default());
}

#[test]
fn independent_permission_and_online_admission_both_guard_original_audio() {
    let f = Fixture::new();
    assert!(matches!(
        f.cache.get(&f.lease, 101u64),
        Err(CacheError::NoGrant)
    ));
    assert!(matches!(
        f.cache.begin(&f.lease, 101u64),
        Err(CacheError::NoGrant)
    ));
    f.observe(101);
    f.put_original();
    let hit = f.cache.get(&f.lease, 101u64).unwrap().unwrap();
    assert_eq!(
        &*hit.admit().unwrap(),
        &SyntheticAudio::tone(400).unwrap().0
    );
    assert!(matches!(
        f.cache.get(&f.lease, 102u64),
        Err(CacheError::NoGrant)
    ));
    f.permission.deny(&account(1), &101u64.into());
    assert!(matches!(
        f.cache.get(&f.lease, 101u64),
        Err(CacheError::Revoked)
    ));
}

#[test]
fn latest_online_ticket_wins_and_consumed_tickets_cannot_replay() {
    let f = Fixture::new();
    let first = f.cache.begin_online_check(&f.lease, 101u64.into()).unwrap();
    let latest = f.cache.begin_online_check(&f.lease, 102u64.into()).unwrap();
    let replay = duplicate(&latest);
    assert_eq!(
        f.cache.observe_verified_online(&f.lease, first),
        Err(CacheError::Cancelled)
    );
    f.cache.observe_verified_online(&f.lease, latest).unwrap();
    let identity = f.cache.identity(&f.lease, 102u64.into()).unwrap();
    let current = f.authority.authorize(&identity).unwrap();
    assert_eq!(
        f.cache.observe_verified_online(&f.lease, replay),
        Err(CacheError::Cancelled)
    );
    f.authority.check(&current).unwrap();
    assert!(matches!(
        f.cache.begin(&f.lease, 101u64),
        Err(CacheError::NoGrant)
    ));
}

#[test]
fn starting_a_failed_or_cancelled_resolution_retires_the_previous_hit() {
    let f = Fixture::new();
    f.observe(101);
    f.put_original();
    let hit = f.cache.get(&f.lease, 101u64).unwrap().unwrap();
    let _abandoned = f.cache.begin_online_check(&f.lease, 101u64.into()).unwrap();
    assert!(matches!(
        f.cache.get(&f.lease, 101u64),
        Err(CacheError::NoGrant)
    ));
    assert!(matches!(hit.admit(), Err(CacheError::Revoked)));
    // A failed HTTP request never reaches observation.
    assert_eq!(f.cache.stats().unwrap().entries, 1);
    f.observe(101);
    assert!(f.cache.get(&f.lease, 101u64).unwrap().is_some());
}

#[test]
fn denied_permission_consumes_the_ticket_without_retaining_old_admission() {
    let f = Fixture::new();
    f.observe(101);
    f.permission.deny(&account(1), &101u64.into());
    let check = f.cache.begin_online_check(&f.lease, 101u64.into()).unwrap();
    let replay = duplicate(&check);
    assert_eq!(
        f.cache.observe_verified_online(&f.lease, check),
        Err(CacheError::Revoked)
    );
    assert_eq!(
        f.cache.observe_verified_online(&f.lease, replay),
        Err(CacheError::Cancelled)
    );
    assert!(matches!(
        f.cache.get(&f.lease, 101u64),
        Err(CacheError::NoGrant)
    ));
}

#[test]
fn lease_policy_budget_and_clear_changes_fence_pending_observations() {
    for change in 0..4 {
        let f = Fixture::new();
        let before = f.cache.identity(&f.lease, 101u64.into()).unwrap();
        let check = f.cache.begin_online_check(&f.lease, 101u64.into()).unwrap();
        match change {
            0 => {
                f.cache.bind(Some(account(1))).unwrap();
            }
            1 => {
                f.cache.configure(CachePolicy { max_bytes: 1024 }).unwrap();
            }
            2 => {
                f.budget.configure(true, Some(1024 * 1024 * 1024)).unwrap();
            }
            _ => {
                f.cache.clear().unwrap();
            }
        }
        let lease = f.cache.lease().unwrap();
        let after = f.cache.identity(&lease, 101u64.into()).unwrap();
        assert!(before != after, "the ephemeral admission must change");
        assert_eq!(
            before.key(),
            after.key(),
            "local fences must not change the persistent key"
        );
        assert_eq!(
            f.cache.observe_verified_online(&f.lease, check),
            Err(CacheError::Cancelled)
        );
        assert!(matches!(
            f.cache.get(&lease, 101u64),
            Err(CacheError::NoGrant)
        ));
        let check = f.cache.begin_online_check(&lease, 101u64.into()).unwrap();
        f.cache.observe_verified_online(&lease, check).unwrap();
        assert!(f.cache.get(&lease, 101u64).unwrap().is_none());
    }
}

#[test]
fn delegate_revision_changes_require_new_online_and_permission_checks() {
    let f = Fixture::new();
    f.observe(101);
    let identity = f.cache.identity(&f.lease, 101u64.into()).unwrap();
    let prior = f.authority.authorize(&identity).unwrap();
    f.permission.allow(&account(1), &101u64.into());
    assert_eq!(f.authority.check(&prior), Err(CacheError::Cancelled));
    assert!(matches!(
        f.cache.begin(&f.lease, 101u64),
        Err(CacheError::Cancelled)
    ));
    f.observe(101);
    let refreshed = f.authority.authorize(&identity).unwrap();
    f.authority.check(&refreshed).unwrap();
    f.permission.deny(&account(1), &101u64.into());
    assert_eq!(f.authority.check(&refreshed), Err(CacheError::Revoked));
}

struct BlockingPermission {
    permission: SyntheticAuthority,
    entered: mpsc::Sender<()>,
    release: Arc<(Mutex<bool>, Condvar)>,
    blocked: AtomicBool,
}
impl StorageAuthority for BlockingPermission {
    fn authorize(&self, identity: &ResourceIdentity) -> Result<StorageGrant> {
        if identity.track == 101u64.into() && !self.blocked.swap(true, Ordering::AcqRel) {
            self.entered.send(()).unwrap();
            let released = self.release.0.lock().unwrap();
            let (released, _) = self
                .release
                .1
                .wait_timeout_while(released, Duration::from_secs(5), |released| !*released)
                .unwrap();
            assert!(
                *released,
                "bounded fixture permission lookup was not released"
            );
        }
        self.permission.authorize(identity)
    }
    fn check(&self, grant: &StorageGrant) -> Result<()> {
        self.permission.check(grant)
    }
}
struct Release(Arc<(Mutex<bool>, Condvar)>);
impl Drop for Release {
    fn drop(&mut self) {
        *self.0.0.lock().unwrap() = true;
        self.0.1.notify_all();
    }
}

#[test]
fn blocked_old_permission_lookup_does_not_block_bind_or_clobber_new_admission() {
    let sandbox = Sandbox::create(&std::env::temp_dir()).unwrap();
    let (entered, started) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let release_guard = Release(release.clone());
    let permission = Arc::new(BlockingPermission {
        permission: SyntheticAuthority::default(),
        entered,
        release,
        blocked: AtomicBool::new(false),
    });
    permission.permission.allow(&account(1), &101u64.into());
    permission.permission.allow(&account(1), &102u64.into());
    let authority = Arc::new(OnlineStreamAuthority::with_storage_permission(permission));
    let cache = AudioCache::open_namespace(sandbox, authority, 1, 100_000).unwrap();
    let old_lease = cache.bind(Some(account(1))).unwrap().unwrap();
    let check = cache.begin_online_check(&old_lease, 101u64.into()).unwrap();
    let pending = {
        let cache = cache.clone();
        thread::spawn(move || cache.observe_verified_online(&old_lease, check))
    };
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    let (bound, binding) = mpsc::channel();
    let bind_worker = {
        let cache = cache.clone();
        thread::spawn(move || bound.send(cache.bind(Some(account(1)))).unwrap())
    };
    let lease = binding
        .recv_timeout(Duration::from_secs(2))
        .expect("binding must not wait for the permission network lookup")
        .unwrap()
        .unwrap();
    bind_worker.join().unwrap();
    let check = cache.begin_online_check(&lease, 102u64.into()).unwrap();
    cache.observe_verified_online(&lease, check).unwrap();
    let current = cache.begin(&lease, 102u64).unwrap();
    drop(release_guard);
    assert_eq!(pending.join().unwrap(), Err(CacheError::Cancelled));
    cache.check_request(&current).unwrap();
    assert!(matches!(
        cache.begin(&lease, 101u64),
        Err(CacheError::NoGrant)
    ));
}

#[test]
fn cancelled_during_permission_lookup_cannot_commit() {
    let sandbox = Sandbox::create(&std::env::temp_dir()).unwrap();
    let (entered, started) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let release_guard = Release(release.clone());
    let permission = Arc::new(BlockingPermission {
        permission: SyntheticAuthority::default(),
        entered,
        release,
        blocked: AtomicBool::new(false),
    });
    permission.permission.allow(&account(1), &101u64.into());
    let authority = Arc::new(OnlineStreamAuthority::with_storage_permission(permission));
    let cache = AudioCache::open_namespace(sandbox, authority, 1, 100_000).unwrap();
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    let check = cache.begin_online_check(&lease, 101u64.into()).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let pending = {
        let cache = cache.clone();
        let lease = lease.clone();
        let cancelled = cancelled.clone();
        thread::spawn(move || {
            cache.observe_verified_online_if(&lease, check, || !cancelled.load(Ordering::Acquire))
        })
    };
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    cancelled.store(true, Ordering::Release);
    drop(release_guard);
    assert_eq!(pending.join().unwrap(), Err(CacheError::Cancelled));
    assert!(matches!(
        cache.get(&lease, 101u64),
        Err(CacheError::NoGrant)
    ));
    assert_eq!(cache.stats().unwrap(), Stats::default());
}

fn current_grant(fixture: &Fixture, track: u64) -> StorageGrant {
    let identity = fixture
        .cache
        .identity(&fixture.lease, track.into())
        .unwrap();
    fixture.authority.authorize(&identity).unwrap()
}

#[test]
fn completed_write_survives_next_admission_but_old_hit_does_not() {
    for next_track in [101, 102] {
        let f = Fixture::new();
        f.observe(101);
        f.put_original();
        let old_hit = f.cache.get(&f.lease, 101u64).unwrap().unwrap();
        let grant = current_grant(&f, 101);
        f.authority.retain_write(&grant).unwrap();
        f.authority.seal_write(&grant).unwrap();

        let next = f
            .cache
            .begin_online_check(&f.lease, next_track.into())
            .unwrap();
        // Repeat One and a different next track both retire the old playback
        // grant, while the already completed write gets only its bounded tail.
        assert_eq!(f.authority.check(&grant), Err(CacheError::Revoked));
        assert!(matches!(old_hit.admit(), Err(CacheError::Revoked)));
        f.authority.check_write(&grant).unwrap();
        f.cache.observe_verified_online(&f.lease, next).unwrap();
        f.authority.check_write(&grant).unwrap();
        f.authority.release_write(&grant);
        assert_eq!(f.authority.check_write(&grant), Err(CacheError::Revoked));
    }
}

#[test]
fn registered_completion_can_be_noticed_after_next_check_without_reviving_playback() {
    let f = Fixture::new();
    f.observe(101);
    let grant = current_grant(&f, 101);
    f.authority.retain_write(&grant).unwrap();
    let _next = f.cache.begin_online_check(&f.lease, 102u64.into()).unwrap();
    assert_eq!(f.authority.check_write(&grant), Err(CacheError::Revoked));
    assert_eq!(f.authority.check(&grant), Err(CacheError::Revoked));
    // The Request owner now observes its actual complete-and-played capture.
    // Registration already existed; this closes the last-buffer/next-check race.
    f.authority.seal_write(&grant).unwrap();
    f.authority.check_write(&grant).unwrap();
    assert_eq!(f.authority.check(&grant), Err(CacheError::Revoked));
    let deadline = f
        .authority
        .state
        .lock()
        .unwrap()
        .writes
        .get(&grant.revision)
        .unwrap()
        .deadline;
    f.authority.seal_write(&grant).unwrap();
    assert_eq!(
        f.authority
            .state
            .lock()
            .unwrap()
            .writes
            .get(&grant.revision)
            .unwrap()
            .deadline,
        deadline,
        "repeated completion checks must not renew finalization"
    );
    f.authority
        .state
        .lock()
        .unwrap()
        .writes
        .get_mut(&grant.revision)
        .unwrap()
        .deadline = Instant::now();
    assert_eq!(f.authority.check_write(&grant), Err(CacheError::Expired));
    assert_eq!(f.authority.seal_write(&grant), Err(CacheError::Revoked));
}

#[test]
fn explicit_resource_refusal_revokes_write_and_cannot_reregister_old_revision() {
    let f = Fixture::new();
    f.observe(101);
    let grant = current_grant(&f, 101);
    f.authority.retain_write(&grant).unwrap();
    f.authority.seal_write(&grant).unwrap();
    f.authority.revoke_writes(&grant.identity, grant.revision);
    assert_eq!(f.authority.check_write(&grant), Err(CacheError::Revoked));
    assert_eq!(f.authority.retain_write(&grant), Err(CacheError::Revoked));
    assert_eq!(f.authority.seal_write(&grant), Err(CacheError::Revoked));

    f.observe(101);
    let refreshed = current_grant(&f, 101);
    f.authority.retain_write(&refreshed).unwrap();
    f.authority.seal_write(&refreshed).unwrap();
    f.permission.deny(&account(1), &101u64.into());
    assert_eq!(
        f.authority.check_write(&refreshed),
        Err(CacheError::Revoked)
    );
}

#[test]
fn late_refusal_cannot_revoke_a_newer_same_resource_write() {
    let f = Fixture::new();
    f.observe(101);
    let old = current_grant(&f, 101);
    f.authority.retain_write(&old).unwrap();
    f.authority.seal_write(&old).unwrap();
    f.observe(101);
    let latest = current_grant(&f, 101);
    f.authority.retain_write(&latest).unwrap();
    f.authority.seal_write(&latest).unwrap();
    f.authority.revoke_writes(&old.identity, old.revision);
    f.authority.check_write(&old).unwrap();
    f.authority.check_write(&latest).unwrap();

    let next = f.cache.begin_online_check(&f.lease, 101u64.into()).unwrap();
    f.authority.revoke_writes(&next.identity, next.sequence);
    assert_eq!(f.authority.check_write(&old), Err(CacheError::Revoked));
    assert_eq!(f.authority.check_write(&latest), Err(CacheError::Revoked));
}

#[test]
fn write_registration_is_bounded_and_its_finite_lifetime_cannot_be_renewed() {
    let f = Fixture::new();
    let mut registered = Vec::new();
    for track in 101..105 {
        if track > 102 {
            f.permission.allow(&account(1), &track.into());
        }
        f.observe(track);
        let grant = current_grant(&f, track);
        f.authority.retain_write(&grant).unwrap();
        f.authority.seal_write(&grant).unwrap();
        registered.push(grant);
    }
    f.permission.allow(&account(1), &105u64.into());
    f.observe(105);
    let current = current_grant(&f, 105);
    assert_eq!(f.authority.retain_write(&current), Err(CacheError::Limit));
    f.authority.release_write(&registered[0]);
    f.authority.retain_write(&current).unwrap();
    assert_eq!(
        f.authority.retain_write(&current),
        Err(CacheError::Duplicate)
    );
    f.authority
        .state
        .lock()
        .unwrap()
        .writes
        .get_mut(&current.revision)
        .unwrap()
        .deadline = Instant::now();
    assert_eq!(f.authority.check_write(&current), Err(CacheError::Expired));
    assert_eq!(f.authority.retain_write(&current), Err(CacheError::Revoked));
    f.observe(105);
    f.authority.retain_write(&current_grant(&f, 105)).unwrap();
}

#[test]
fn account_namespace_and_lease_changes_prune_write_pins() {
    for change in 0..3 {
        let f = Fixture::new();
        f.observe(101);
        let grant = current_grant(&f, 101);
        f.authority.retain_write(&grant).unwrap();
        f.authority.seal_write(&grant).unwrap();
        let mut next = grant.identity;
        match change {
            0 => next.account = account(2).audio_cache_key(),
            1 => next.namespace += 1,
            _ => next.admission[0] ^= 1,
        }
        f.authority.begin_online(next).unwrap();
        assert_eq!(f.authority.check_write(&grant), Err(CacheError::Revoked));
        assert_eq!(f.authority.seal_write(&grant), Err(CacheError::Revoked));
    }
}
