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
