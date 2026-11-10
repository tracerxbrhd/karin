// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{
    os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "karin-budget-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn budget(&self, limit: Option<u64>) -> Arc<CacheBudget> {
        CacheBudget::new(self.0.join("persistent"), self.0.join("audio"), true, limit).unwrap()
    }
    fn meta(&self, id: u8, art: bool, size: usize) -> PathBuf {
        let dir = self.0.join("persistent/v1").join("0".repeat(64));
        fs::create_dir_all(&dir).unwrap();
        for p in [
            self.0.join("persistent"),
            self.0.join("persistent/v1"),
            dir.clone(),
        ] {
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let p = dir.join(format!("{id:064x}.entry"));
        let mut data = vec![0; size];
        data[..8].copy_from_slice(b"KARNC3A\0");
        data[8] = if art { 7 } else { 1 };
        fs::write(&p, data).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        p
    }
    fn audio(&self, id: u8, size: usize) -> PathBuf {
        let base = self.0.join("audio");
        fs::create_dir_all(&base).unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
        let p = base.join(format!("{id:064x}"));
        fs::DirBuilder::new().mode(0o700).create(&p).unwrap();
        for (name, n) in [("header", 112), ("media.bin", size)] {
            let f = p.join(name);
            fs::write(&f, vec![0; n]).unwrap();
            fs::set_permissions(f, fs::Permissions::from_mode(0o600)).unwrap();
        }
        p
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn configuration_is_memory_only_and_defaults_do_not_create_files() {
    let f = Fixture::new();
    let b = f.budget(Some(2 * 1024 * MIB));
    b.configure(false, None).unwrap();
    assert!(!b.enabled());
    assert!(!f.0.join("persistent").exists());
    assert_eq!(b.usage().unwrap().bytes, 0);
}
#[test]
fn one_budget_evicts_across_actual_categories_and_keeps_small_metadata() {
    let f = Fixture::new();
    let b = f.budget(Some(400));
    let meta = f.meta(1, false, 100);
    let audio = f.audio(2, 180);
    let art = f.meta(3, true, 80);
    assert_eq!(b.usage().unwrap().bytes, 472);
    b.trim().unwrap();
    assert!(b.usage().unwrap().bytes <= 400);
    assert!(!meta.exists() || !audio.exists() || !art.exists());
}
#[test]
fn reservations_account_concurrent_writers_and_release_on_cancel() {
    let f = Fixture::new();
    let b = f.budget(Some(500));
    let p = f.meta(1, false, 40);
    let q = f.meta(2, false, 40);
    let a = b.reserve(Category::Metadata, &p, 250).unwrap();
    assert!(matches!(
        b.reserve(Category::Artwork, &q, 250),
        Err(BudgetError::Limit)
    ));
    drop(a);
    let next = b.reserve(Category::Artwork, &q, 250).unwrap();
    drop(next);
}
#[test]
fn sidecar_update_protects_its_existing_audio_during_admission_and_later_eviction() {
    let f = Fixture::new();
    let b = f.budget(Some(500));
    let target = f.audio(1, 100);
    let victim = f.audio(2, 100);
    for (path, age) in [(&target, 1), (&victim, 2)] {
        File::open(path)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(age)),
            )
            .unwrap();
    }
    let ticket = b.reserve_update(Category::Audio, &target, 80).unwrap();
    assert!(
        target.exists(),
        "the update must not evict its oldest target"
    );
    assert!(!victim.exists(), "ordinary other records remain evictable");
    assert_eq!(fs::read(target.join("media.bin")).unwrap(), vec![0; 100]);
    // A smaller policy fences the ticket but does not let maintenance remove
    // its target while the sidecar owner is still unwinding its operation.
    b.configure(true, Some(1)).unwrap();
    assert!(matches!(b.trim(), Err(BudgetError::Limit)));
    assert!(target.exists());
    assert!(matches!(ticket.finish(80), Err(BudgetError::Cancelled)));
    assert_eq!(b.trim().unwrap().bytes, 0);
    assert!(!target.exists());
}
#[test]
fn sidecar_update_reserves_only_temporary_bytes_and_reports_actual_replaced_size() {
    let f = Fixture::new();
    let b = f.budget(Some(400));
    let target = f.audio(1, 100);
    let http = target.join("http");
    fs::write(&http, vec![1; 20]).unwrap();
    fs::set_permissions(&http, fs::Permissions::from_mode(0o600)).unwrap();
    let original_media = fs::metadata(target.join("media.bin")).unwrap();
    assert_eq!(b.usage().unwrap().bytes, 232);
    let ticket = b.reserve_update(Category::Audio, &target, 40).unwrap();
    let staging = f.0.join("audio").join(format!(".part-{}", "a".repeat(32)));
    fs::DirBuilder::new().mode(0o700).create(&staging).unwrap();
    let next = staging.join("http");
    fs::write(&next, vec![2; 40]).unwrap();
    fs::set_permissions(&next, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(b.usage().unwrap().bytes, 272);
    fs::rename(next, &http).unwrap();
    fs::remove_dir(staging).unwrap();
    ticket.finish(40).unwrap();
    assert_eq!(b.usage().unwrap().bytes, 252);
    assert_eq!(b.usage().unwrap().entries, 1);
    assert_eq!(fs::read(http).unwrap(), vec![2; 40]);
    let current_media = fs::metadata(target.join("media.bin")).unwrap();
    assert_eq!(original_media.ino(), current_media.ino());
    assert_eq!(original_media.dev(), current_media.dev());
}
#[test]
fn sidecar_update_holds_shared_allowance_without_sacrificing_the_candidate() {
    let f = Fixture::new();
    let b = f.budget(Some(500));
    let target = f.audio(1, 100);
    let future = f.0.join("audio").join("2".repeat(64));
    let ticket = b.reserve_update(Category::Audio, &target, 80).unwrap();
    assert!(matches!(
        b.reserve(Category::Audio, &future, 300),
        Err(BudgetError::Limit)
    ));
    assert!(target.exists());
    drop(ticket);
    let other = b.reserve(Category::Audio, &future, 300).unwrap();
    assert!(!target.exists());
    drop(other);
    let target = f.audio(3, 100);
    b.configure(true, Some(300)).unwrap();
    assert!(matches!(
        b.reserve_update(Category::Audio, &target, 80),
        Err(BudgetError::Limit)
    ));
    assert!(
        target.exists(),
        "a refused update must retain the candidate"
    );
}
#[test]
fn sidecar_update_rejects_missing_unsafe_and_busy_targets_without_waiting() {
    let f = Fixture::new();
    let b = f.budget(None);
    let target = f.audio(1, 100);
    let missing = f.0.join("audio").join("2".repeat(64));
    assert!(matches!(
        b.reserve_update(Category::Audio, &missing, 40),
        Err(BudgetError::Unavailable)
    ));
    for (category, size) in [
        (Category::Metadata, 40),
        (Category::Audio, 0),
        (Category::Audio, 8193),
    ] {
        assert!(matches!(
            b.reserve_update(category, &target, size),
            Err(BudgetError::InvalidInput)
        ));
    }
    let outside = f.0.join("keep");
    fs::DirBuilder::new().mode(0o700).create(&outside).unwrap();
    fs::write(outside.join("personal"), b"keep").unwrap();
    let linked = f.0.join("audio").join("3".repeat(64));
    symlink(&outside, &linked).unwrap();
    assert!(matches!(
        b.reserve_update(Category::Audio, &linked, 40),
        Err(BudgetError::Unavailable)
    ));
    let io = b.io.lock().unwrap();
    assert!(matches!(
        b.reserve_update(Category::Audio, &target, 40),
        Err(BudgetError::Unavailable)
    ));
    assert!(target.exists());
    assert_eq!(fs::read(outside.join("personal")).unwrap(), b"keep");
    drop(io);
    assert!(b.reserve_update(Category::Audio, &target, 40).is_ok());
    b.configure(false, None).unwrap();
    assert!(matches!(
        b.reserve_update(Category::Audio, &target, 40),
        Err(BudgetError::Disabled)
    ));
    b.configure(true, None).unwrap();
    *b.available.lock().unwrap() = Some((64 * MIB + 32, 1000 * MIB));
    assert!(matches!(
        b.reserve_update(Category::Audio, &target, 40),
        Err(BudgetError::Limit)
    ));
    assert!(target.exists());
}
#[test]
fn changing_policy_fences_late_publication_and_disable_is_immediate() {
    let f = Fixture::new();
    let b = f.budget(Some(500));
    let p = f.meta(1, false, 40);
    let a = b.reserve(Category::Metadata, &p, 100).unwrap();
    b.configure(false, Some(500)).unwrap();
    assert!(matches!(a.finish(80), Err(BudgetError::Cancelled)));
    assert!(matches!(
        b.reserve(Category::Metadata, &p, 100),
        Err(BudgetError::Disabled)
    ));
    b.configure(true, Some(500)).unwrap();
    assert!(b.reserve(Category::Metadata, &p, 100).is_ok());
}
#[test]
fn unlimited_has_no_fake_quota_but_always_checks_disk_headroom() {
    let f = Fixture::new();
    let b = f.budget(None);
    let p = f.meta(1, false, 40);
    *b.available.lock().unwrap() = Some((70 * MIB, 1000 * MIB));
    assert!(matches!(
        b.reserve(Category::Metadata, &p, 10 * MIB),
        Err(BudgetError::Limit)
    ));
    *b.available.lock().unwrap() = Some((500 * MIB, 1000 * MIB));
    assert!(b.reserve(Category::Metadata, &p, 10 * MIB).is_ok());
    assert!(b.trim().is_ok());
}
#[test]
fn pending_audio_leaves_room_for_small_metadata() {
    let f = Fixture::new();
    let b = f.budget(Some(1024));
    let p = f.audio(1, 1);
    fs::remove_file(p.join("header")).unwrap();
    fs::remove_file(p.join("media.bin")).unwrap();
    fs::remove_dir(&p).unwrap();
    assert!(matches!(
        b.reserve(Category::Audio, &p, 1000),
        Err(BudgetError::Limit)
    ));
    let a = b.reserve(Category::Audio, &p, 800).unwrap();
    let q = f.meta(2, false, 10);
    assert!(b.reserve(Category::Metadata, &q, 80).is_ok());
    drop(a);
}
#[test]
fn restart_reconciles_existing_bytes_and_lru_touches_are_shared() {
    let f = Fixture::new();
    let old = f.audio(1, 100);
    let new = f.meta(2, false, 100);
    let b = f.budget(None);
    b.touch(&old).unwrap();
    let bytes = b.usage().unwrap().bytes;
    drop(b);
    let b = f.budget(Some(220));
    assert_eq!(b.usage().unwrap().bytes, bytes);
    b.trim().unwrap();
    assert!(!new.exists());
    assert!(old.exists());
}
#[test]
fn only_known_private_namespaces_are_counted_or_deleted() {
    let f = Fixture::new();
    let outside = f.0.join("keep");
    fs::write(&outside, b"personal").unwrap();
    let b = f.budget(Some(10));
    fs::create_dir(f.0.join("persistent")).unwrap();
    fs::set_permissions(f.0.join("persistent"), fs::Permissions::from_mode(0o700)).unwrap();
    symlink(&f.0, f.0.join("persistent/v1")).unwrap();
    assert_eq!(b.trim().unwrap().bytes, 0);
    assert_eq!(fs::read(&outside).unwrap(), b"personal");
    assert!(matches!(
        b.reserve(Category::Metadata, &outside, 10),
        Err(BudgetError::InvalidInput)
    ));
}
#[test]
fn staging_usage_is_actual_and_never_deleted_by_eviction() {
    let f = Fixture::new();
    let b = f.budget(Some(200));
    let p = f.meta(1, false, 100);
    let staging = p.parent().unwrap().join(format!(".tmp-{}", "a".repeat(32)));
    fs::write(&staging, [0; 40]).unwrap();
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(b.usage().unwrap().bytes, 140);
    b.trim().unwrap();
    assert!(staging.exists());
}
#[test]
fn malformed_or_overlapping_roots_and_zero_limits_are_rejected() {
    let f = Fixture::new();
    assert!(CacheBudget::new(f.0.clone(), f.0.join("audio"), true, None).is_err());
    assert!(CacheBudget::new(PathBuf::from("relative"), f.0.clone(), true, None).is_err());
    assert!(f.budget(None).configure(true, Some(0)).is_err());
}

#[test]
fn audio_can_reclaim_old_metadata_above_the_protected_floor() {
    let f = Fixture::new();
    let b = f.budget(Some(1000));
    for id in 1..=8 {
        f.meta(id, false, 100);
    }
    let path = f.0.join("audio").join("f".repeat(64));
    let ticket = b.reserve(Category::Audio, &path, 700).unwrap();
    let usage = b.usage().unwrap();
    assert!(usage.bytes <= 300);
    assert!(usage.bytes >= 125);
    drop(ticket);
}
#[test]
fn corrupt_recognized_records_still_count_towards_the_budget() {
    let f = Fixture::new();
    let b = f.budget(None);
    let path = f.meta(1, false, 100);
    fs::write(&path, [0; 100]).unwrap();
    let audio = f.audio(2, 100);
    fs::remove_file(audio.join("header")).unwrap();
    assert_eq!(b.usage().unwrap().bytes, 200);
    b.configure(true, Some(50)).unwrap();
    b.trim().unwrap();
    assert_eq!(b.usage().unwrap().bytes, 0);
}

struct AdmittedAudio {
    cache: crate::audio_cache::AudioCache,
    budget: Arc<CacheBudget>,
    lease: crate::audio_cache::Lease,
    key: crate::audio_cache::ResourceKey,
    original: Vec<u8>,
    record: PathBuf,
    _files: Fixture,
}

impl AdmittedAudio {
    fn new() -> Self {
        use crate::audio_cache::{
            AudioCache, CachePolicy, ResourceKey, test_support::SyntheticAuthority,
        };
        let files = Fixture::new();
        let budget = files.budget(None);
        let authority = Arc::new(SyntheticAuthority::default());
        let account = crate::cache::AccountScope::from_account_id("1234").unwrap();
        let key = ResourceKey::track("42").unwrap();
        authority.allow(&account, &key);
        let cache = AudioCache::open_with_budget(
            files.0.join("audio"),
            authority,
            CachePolicy::default(),
            budget.clone(),
        )
        .unwrap();
        let lease = cache.bind(Some(account)).unwrap().unwrap();
        // Original eight-sample PCM WAV: no network content or service rights.
        let mut original = b"RIFF\x34\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0".to_vec();
        original.extend_from_slice(&8_000u32.to_le_bytes());
        original.extend_from_slice(&16_000u32.to_le_bytes());
        original.extend_from_slice(b"\x02\0\x10\0data\x10\0\0\0");
        for _ in 0..4 {
            original.extend_from_slice(&512i16.to_le_bytes());
            original.extend_from_slice(&(-512i16).to_le_bytes());
        }
        cache
            .put_reader(cache.begin(&lease, key).unwrap(), original.as_slice())
            .unwrap();
        let record = fs::read_dir(files.0.join("audio"))
            .unwrap()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .find(|path| path.is_dir())
            .unwrap();
        Self {
            cache,
            budget,
            lease,
            key,
            original,
            record,
            _files: files,
        }
    }
}

#[test]
fn admitted_audio_hit_does_not_wait_behind_shared_budget_bookkeeping() {
    use std::{sync::mpsc, thread, time::Duration};
    let f = AdmittedAudio::new();
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
    File::open(&f.record)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(old))
        .unwrap();
    let coordinator = f.budget.io.lock().unwrap();
    let (sent, received) = mpsc::channel();
    let cache = f.cache.clone();
    let lease = f.lease.clone();
    let key = f.key;
    let worker = thread::spawn(move || {
        sent.send(cache.get(&lease, key)).unwrap();
    });
    let early = received.recv_timeout(Duration::from_secs(2));
    let did_not_wait = early.is_ok();
    drop(coordinator);
    let hit = early
        .unwrap_or_else(|_| received.recv_timeout(Duration::from_secs(2)).unwrap())
        .unwrap()
        .unwrap();
    worker.join().unwrap();
    assert!(
        did_not_wait,
        "playback lookup queued behind global LRU work"
    );
    let mut file = hit.admit_file().unwrap().as_ref().try_clone().unwrap();
    let mut actual = Vec::new();
    file.read_to_end(&mut actual).unwrap();
    assert_eq!(actual, f.original);
    assert_eq!(fs::metadata(&f.record).unwrap().modified().unwrap(), old);
    // Bookkeeping is still shared and a later uncontended access updates it.
    assert!(f.budget.try_touch(&f.record).unwrap());
    assert!(fs::metadata(&f.record).unwrap().modified().unwrap() > old);
}

#[test]
fn bookkeeping_failure_does_not_discard_an_admitted_integrity_checked_audio_hit() {
    let f = AdmittedAudio::new();
    let budget = f.budget.clone();
    assert!(
        std::thread::spawn(move || {
            let _coordinator = budget.io.lock().unwrap();
            panic!("synthetic LRU bookkeeping failure");
        })
        .join()
        .is_err()
    );
    assert!(matches!(
        f.budget.try_touch(&f.record),
        Err(BudgetError::Unavailable)
    ));
    let hit = f.cache.get(&f.lease, f.key).unwrap().unwrap();
    let mut file = hit.admit_file().unwrap().as_ref().try_clone().unwrap();
    let mut actual = Vec::new();
    file.read_to_end(&mut actual).unwrap();
    assert_eq!(actual, f.original);
}
