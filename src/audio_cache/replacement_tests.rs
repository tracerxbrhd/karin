// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete-record replacement and late publication rollback, without services.
use super::*;
use std::os::unix::fs::FileExt;

type PublicationAction = Box<dyn FnOnce() -> Result<()> + Send>;
struct Watch {
    target: PathBuf,
    expected: Vec<u8>,
    action: Option<PublicationAction>,
}
#[derive(Default)]
struct PublicationAuthority {
    watch: Mutex<Option<Watch>>,
    saw_new_record: AtomicBool,
}
impl PublicationAuthority {
    fn watch(&self, path: PathBuf, bytes: &[u8], action: PublicationAction) {
        *self.watch.lock().unwrap() = Some(Watch {
            target: path,
            expected: bytes.to_vec(),
            action: Some(action),
        });
    }
}
impl StorageAuthority for PublicationAuthority {
    fn authorize(&self, identity: &ResourceIdentity) -> Result<StorageGrant> {
        Ok(StorageGrant {
            identity: *identity,
            revision: 1,
            content: None,
            basis: AdmissionBasis::IndependentLicense,
        })
    }
    fn check(&self, _: &StorageGrant) -> Result<()> {
        // Only inject after the new complete record is visible at its real key.
        // This exercises the final publication fence, not a pre-copy failure.
        let action = {
            let mut watched = self.watch.lock().unwrap();
            watched.as_mut().and_then(|watch| {
                if fs::read(&watch.target).ok().as_deref() == Some(watch.expected.as_slice()) {
                    self.saw_new_record.store(true, Ordering::Release);
                    watch.action.take()
                } else {
                    None
                }
            })
        };
        action.map_or(Ok(()), |action| action())
    }
}

fn fixture(
    shared: bool,
) -> (
    Arc<Sandbox>,
    Arc<PublicationAuthority>,
    AudioCache,
    Lease,
    Option<Arc<crate::cache_budget::CacheBudget>>,
) {
    let root = Sandbox::create(&std::env::temp_dir()).unwrap();
    let authority = Arc::new(PublicationAuthority::default());
    let budget = shared.then(|| {
        crate::cache_budget::CacheBudget::new(
            root.path.join("metadata"),
            root.path.join("audio"),
            true,
            Some(128 * 1024 * 1024),
        )
        .unwrap()
    });
    let cache = if let Some(budget) = &budget {
        AudioCache::open_with_budget(
            root.path.join("audio"),
            authority.clone(),
            CachePolicy::default(),
            budget.clone(),
        )
        .unwrap()
    } else {
        AudioCache::open_namespace(root.clone(), authority.clone(), 1, 100_000).unwrap()
    };
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    (root, authority, cache, lease, budget)
}
fn pinned_bytes(file: &File) -> Vec<u8> {
    let mut bytes = vec![0; file.metadata().unwrap().len() as usize];
    file.read_exact_at(&mut bytes, 0).unwrap();
    bytes
}
fn record_path(cache: &AudioCache, lease: &Lease) -> PathBuf {
    cache.path(cache.identity(lease, ResourceKey::from(101)).unwrap().key())
}
fn save(cache: &AudioCache, lease: &Lease, bytes: &[u8]) -> Result<()> {
    cache.put_reader(cache.begin(lease, 101)?, bytes)
}

#[test]
fn late_publication_failure_restores_original_complete_record_and_inode() {
    for failure in [CacheError::Cancelled, CacheError::Unavailable] {
        let (_root, authority, cache, lease, _) = fixture(false);
        let original = b"original permitted recording";
        let replacement = b"different complete permitted recording";
        save(&cache, &lease, original).unwrap();
        let path = record_path(&cache, &lease);
        let before = fs::metadata(path.join("media.bin")).unwrap();
        let header = fs::read(path.join("header")).unwrap();
        authority.watch(
            path.join("media.bin"),
            replacement,
            Box::new(move || Err(failure)),
        );
        assert_eq!(save(&cache, &lease, replacement), Err(failure));
        assert!(authority.saw_new_record.load(Ordering::Acquire));
        let hit = cache.get(&lease, 101).unwrap().unwrap();
        let restored = hit.admit_file().unwrap();
        assert_eq!(pinned_bytes(&restored), original);
        assert_eq!(restored.metadata().unwrap().ino(), before.ino());
        assert_eq!(fs::read(path.join("header")).unwrap(), header);
        assert_eq!(cache.stats().unwrap().entries, 1);
        assert_eq!(fs::read_dir(&cache.0.sandbox.path).unwrap().count(), 2);
    }
}

#[test]
fn successful_replacement_preserves_open_original_and_survives_reopen() {
    let (root, authority, cache, lease, _) = fixture(false);
    let original = b"original permitted recording";
    let replacement = b"new complete permitted recording";
    save(&cache, &lease, original).unwrap();
    let original_file = cache
        .get(&lease, 101)
        .unwrap()
        .unwrap()
        .admit_file()
        .unwrap();
    let original_inode = original_file.metadata().unwrap().ino();
    authority.watch(
        record_path(&cache, &lease).join("media.bin"),
        replacement,
        Box::new(|| Ok(())),
    );
    save(&cache, &lease, replacement).unwrap();
    assert!(authority.saw_new_record.load(Ordering::Acquire));
    let current = cache
        .get(&lease, 101)
        .unwrap()
        .unwrap()
        .admit_file()
        .unwrap();
    assert_eq!(pinned_bytes(&current), replacement);
    assert_ne!(current.metadata().unwrap().ino(), original_inode);
    assert_eq!(pinned_bytes(&original_file), original);
    assert_eq!(
        cache.stats().unwrap().bytes,
        (HEADER + replacement.len()) as u64
    );
    assert_eq!(fs::read_dir(&root.path).unwrap().count(), 2);
    drop(cache);
    let cache = AudioCache::open_namespace(root, authority, 1, 100_000).unwrap();
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    let reopened = cache
        .get(&lease, 101)
        .unwrap()
        .unwrap()
        .admit_file()
        .unwrap();
    assert_eq!(pinned_bytes(&reopened), replacement);
}

#[test]
fn late_account_change_and_shared_disable_restore_old_data_without_admitting_it() {
    for disable in [false, true] {
        let (_root, authority, cache, lease, budget) = fixture(disable);
        let original = b"original permitted recording";
        let replacement = b"new complete permitted recording";
        save(&cache, &lease, original).unwrap();
        let path = record_path(&cache, &lease);
        let owner = Arc::downgrade(&cache.0);
        let changing_budget = budget.clone();
        authority.watch(
            path.join("media.bin"),
            replacement,
            Box::new(move || {
                if let Some(budget) = changing_budget {
                    budget.configure(false, Some(128 * 1024 * 1024)).unwrap();
                    Ok(())
                } else {
                    AudioCache(owner.upgrade().unwrap()).bind(Some(account(2)))?;
                    Err(CacheError::Cancelled)
                }
            }),
        );
        assert_eq!(
            save(&cache, &lease, replacement),
            Err(CacheError::Cancelled)
        );
        assert!(authority.saw_new_record.load(Ordering::Acquire));
        assert_eq!(fs::read(path.join("media.bin")).unwrap(), original);
        assert!(
            cache.get(&lease, 101).is_err(),
            "old admission stays fenced"
        );
        if let Some(budget) = budget {
            budget.configure(true, Some(128 * 1024 * 1024)).unwrap();
        }
        let renewed = cache.bind(Some(account(1))).unwrap().unwrap();
        let hit = cache.get(&renewed, 101).unwrap().unwrap();
        assert_eq!(pinned_bytes(&hit.admit_file().unwrap()), original);
    }
}
