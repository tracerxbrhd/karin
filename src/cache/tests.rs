// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::music::{
    Track,
    cache_codec::{self, MetadataRecord},
};
use std::{
    os::unix::fs::symlink,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let mut random = [0u8; 16];
        SystemRandom::new().fill(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!("karin-cache-test-{}", hex(&random)));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn root(&self) -> PathBuf {
        self.0.join("persistent")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct TestClock(AtomicU64);
impl TestClock {
    fn set(&self, value: u64) {
        self.0.store(value, Ordering::Release);
    }
}
impl Clock for TestClock {
    fn seconds(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
}
fn setup(policy: CachePolicy) -> (Temp, Arc<TestClock>, Cache, CacheLease) {
    let temp = Temp::new();
    let clock = Arc::new(TestClock(AtomicU64::new(100)));
    let cache = Cache::with_clock(temp.root(), policy, clock.clone()).unwrap();
    cache.bind(Some(AccountScope::from_account_id("1234").unwrap()));
    let lease = cache.lease().unwrap();
    (temp, clock, cache, lease)
}
fn key(number: u32) -> CacheKey {
    CacheKey::new(EntryKind::Artwork, &format!("synthetic-{number}"), 0).unwrap()
}
fn raster(marker: u8) -> Vec<u8> {
    let mut bytes = b"KARINRGB".to_vec();
    for value in [2u32, 2, 8] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.push(1);
    bytes.extend_from_slice(&[marker; 16]);
    bytes
}
fn track_record() -> Vec<u8> {
    cache_codec::encode(
        EntryKind::Track,
        &MetadataRecord::Track(Track {
            id: "42".into(),
            title: "Synthetic metadata".into(),
            artists: vec!["Synthetic artist".into()],
            artist_links: Vec::new(),
            album_id: None,
            album_title: None,
            duration_ms: Some(1200),
            cover: None,
            explicit: false,
            available: true,
        }),
    )
    .unwrap()
}

#[test]
fn cache_survives_restart_and_reports_hit_miss_stale_and_clock_rollback() {
    let (temp, clock, cache, lease) = setup(CachePolicy {
        artwork_ttl: Duration::from_secs(10),
        ..CachePolicy::default()
    });
    assert!(cache.get(&lease, &key(1)).unwrap().is_none());
    assert!(!temp.root().exists(), "miss creates no cache directory");
    cache.put(&lease, &key(1), &raster(40)).unwrap();
    let entry = cache.get(&lease, &key(1)).unwrap().unwrap();
    assert_eq!(entry.freshness, Freshness::Fresh);
    assert_eq!(entry.bytes, raster(40));
    drop(lease);
    drop(cache);
    let cache = Cache::with_clock(temp.root(), CachePolicy::default(), clock.clone()).unwrap();
    cache.bind(Some(AccountScope::from_account_id("1234").unwrap()));
    let lease = cache.lease().unwrap();
    clock.set(109);
    assert_eq!(
        cache.get(&lease, &key(1)).unwrap().unwrap().freshness,
        Freshness::Fresh
    );
    clock.set(110);
    let stale = cache.get(&lease, &key(1)).unwrap().unwrap();
    assert_eq!(stale.freshness, Freshness::Stale);
    assert_eq!(stale.age, Duration::from_secs(10));
    clock.set(90);
    assert_eq!(
        cache.get(&lease, &key(1)).unwrap().unwrap().freshness,
        Freshness::Stale,
        "clock rollback must not invent freshness"
    );
}
#[test]
fn account_binding_is_private_stable_and_isolates_identical_keys() {
    let (_temp, _clock, cache, first) = setup(CachePolicy::default());
    cache.put(&first, &key(1), &raster(1)).unwrap();
    cache.bind(Some(AccountScope::from_account_id("5678").unwrap()));
    let second = cache.lease().unwrap();
    assert_eq!(
        cache.get(&first, &key(1)).unwrap_err(),
        CacheError::Cancelled
    );
    assert!(cache.get(&second, &key(1)).unwrap().is_none());
    cache.put(&second, &key(1), &raster(2)).unwrap();
    cache.bind(Some(AccountScope::from_account_id("0001234").unwrap()));
    let first_again = cache.lease().unwrap();
    assert_eq!(
        cache.get(&first_again, &key(1)).unwrap().unwrap().bytes,
        raster(1)
    );
    cache.bind(None);
    assert!(cache.lease().is_none());
    assert!(!first_again.is_current());
    for invalid in [
        "",
        "0",
        "../1234",
        "https://invalid.example",
        "account@example.invalid",
    ] {
        assert!(AccountScope::from_account_id(invalid).is_err());
    }
    let rendered = format!(
        "{cache:?} {first:?} {:?} {:?}",
        AccountScope::from_account_id("1234").unwrap(),
        CacheKey::new(EntryKind::Track, "private-track-42", 0).unwrap()
    );
    for value in ["1234", "5678", "private-track-42"] {
        assert!(!rendered.contains(value));
    }
}
#[test]
fn disabled_caching_never_reads_creates_or_repopulates_persistent_entries() {
    let (temp, _clock, cache, lease) = setup(CachePolicy::default());
    cache.put(&lease, &key(1), &raster(1)).unwrap();
    let before = cache.usage().unwrap();
    cache
        .configure(CachePolicy {
            enabled: false,
            ..CachePolicy::default()
        })
        .unwrap();
    assert!(cache.lease().is_none());
    assert_eq!(
        cache.get(&lease, &key(1)).unwrap_err(),
        CacheError::Disabled
    );
    assert_eq!(
        cache.put(&lease, &key(2), &raster(2)).unwrap_err(),
        CacheError::Disabled
    );
    assert_eq!(
        cache.usage().unwrap(),
        before,
        "valid stored records remain until explicit clear"
    );
    cache.configure(CachePolicy::default()).unwrap();
    let next = cache.lease().unwrap();
    assert_eq!(cache.get(&next, &key(1)).unwrap().unwrap().bytes, raster(1));
    let absent = temp.0.join("absent");
    let disabled = Cache::new(
        absent.clone(),
        CachePolicy {
            enabled: false,
            ..CachePolicy::default()
        },
    )
    .unwrap();
    disabled.bind(Some(AccountScope::from_account_id("1").unwrap()));
    assert!(disabled.lease().is_none());
    assert!(!absent.exists());
}
#[test]
fn lru_touch_and_lowered_budget_evict_old_entries_using_real_file_sizes() {
    let (_temp, clock, cache, lease) = setup(CachePolicy::default());
    cache.put(&lease, &key(1), &raster(1)).unwrap();
    let one = cache.usage().unwrap().bytes;
    let policy = CachePolicy {
        max_bytes: one * 2,
        ..CachePolicy::default()
    };
    cache.configure(policy).unwrap();
    let lease = cache.lease().unwrap();
    clock.set(101);
    cache.put(&lease, &key(2), &raster(2)).unwrap();
    clock.set(102);
    cache.get(&lease, &key(1)).unwrap().unwrap();
    clock.set(103);
    cache.put(&lease, &key(3), &raster(3)).unwrap();
    assert!(cache.get(&lease, &key(2)).unwrap().is_none());
    assert!(cache.get(&lease, &key(1)).unwrap().is_some());
    assert!(cache.get(&lease, &key(3)).unwrap().is_some());
    assert_eq!(
        cache.usage().unwrap(),
        CacheStats {
            bytes: one * 2,
            entries: 2
        }
    );
    cache
        .configure(CachePolicy {
            max_bytes: one,
            ..policy
        })
        .unwrap();
    let stats = cache.trim().unwrap();
    assert_eq!(stats.entries, 1);
    assert!(stats.bytes <= one);
}
#[test]
fn oversized_or_nonmetadata_payload_never_leaves_a_persistent_file() {
    let (temp, _clock, cache, lease) = setup(CachePolicy::default());
    for bytes in [
        b"ID3synthetic-audio".as_slice(),
        b"{\"access_token\":\"synthetic-do-not-save\"}".as_slice(),
        b"https://storage.yandex.net/get-mp3/synthetic".as_slice(),
    ] {
        assert_eq!(
            cache.put(&lease, &key(1), bytes).unwrap_err(),
            CacheError::InvalidData
        );
    }
    assert_eq!(
        cache
            .put(&lease, &key(1), &vec![1; MAX_PAYLOAD_BYTES + 1])
            .unwrap_err(),
        CacheError::TooLarge
    );
    assert!(!temp.root().exists());
    for selector in [
        "../42",
        "https://invalid.example",
        "42?token=synthetic",
        "42#secret",
        "user@host",
        "42/7",
    ] {
        assert!(CacheKey::new(EntryKind::Track, selector, 0).is_err());
    }
    assert!(CacheKey::new(EntryKind::Album, "42", 1).is_err());
    assert!(CacheKey::new(EntryKind::LikedTracks, "liked", 10_001).is_err());
    assert!(Cache::new(PathBuf::from("relative"), CachePolicy::default()).is_err());
    assert!(
        Cache::new(
            temp.root(),
            CachePolicy {
                max_bytes: 0,
                ..CachePolicy::default()
            }
        )
        .is_err()
    );
}
#[test]
fn malformed_or_truncated_entries_and_interrupted_temp_files_recover_as_misses() {
    let (_temp, _clock, cache, lease) = setup(CachePolicy::default());
    cache.put(&lease, &key(1), &raster(1)).unwrap();
    let path = cache.path(&lease, &key(1), false).unwrap().unwrap();
    let original = fs::read(&path).unwrap();
    for corruption in [
        original[..12].to_vec(),
        {
            let mut value = original.clone();
            *value.last_mut().unwrap() ^= 1;
            value
        },
        {
            let mut value = original.clone();
            value[8] = 255;
            value
        },
    ] {
        fs::write(&path, corruption).unwrap();
        assert!(cache.get(&lease, &key(1)).unwrap().is_none());
        assert!(!path.exists());
        cache.put(&lease, &key(1), &raster(1)).unwrap();
    }
    let interrupted = path.parent().unwrap().join(".tmp-interrupted");
    fs::write(&interrupted, b"truncated synthetic metadata").unwrap();
    cache.trim().unwrap();
    assert!(!interrupted.exists());
    assert_eq!(
        cache.get(&lease, &key(1)).unwrap().unwrap().bytes,
        raster(1)
    );
}
#[test]
fn swapped_key_and_account_records_cannot_cross_namespaces() {
    let (_temp, _clock, cache, lease) = setup(CachePolicy::default());
    cache.put(&lease, &key(1), &raster(1)).unwrap();
    cache.put(&lease, &key(2), &raster(2)).unwrap();
    let first = cache.path(&lease, &key(1), false).unwrap().unwrap();
    let second = cache.path(&lease, &key(2), false).unwrap().unwrap();
    let original = fs::read(&first).unwrap();
    fs::write(&second, &original).unwrap();
    assert!(cache.get(&lease, &key(2)).unwrap().is_none());
    cache.bind(Some(AccountScope::from_account_id("9999").unwrap()));
    let other = cache.lease().unwrap();
    cache.put(&other, &key(1), &raster(3)).unwrap();
    let path = cache.path(&other, &key(1), false).unwrap().unwrap();
    fs::write(&path, &original).unwrap();
    assert!(cache.get(&other, &key(1)).unwrap().is_none());
}
#[test]
fn symlinks_hardlinks_and_insecure_private_roots_are_refused_without_following() {
    let (temp, _clock, cache, lease) = setup(CachePolicy::default());
    let outside = temp.0.join("outside");
    fs::write(&outside, b"synthetic-external-data").unwrap();
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o600)).unwrap();
    let path = cache.path(&lease, &key(1), true).unwrap().unwrap();
    symlink(&outside, &path).unwrap();
    assert!(cache.get(&lease, &key(1)).unwrap().is_none());
    assert_eq!(fs::read(&outside).unwrap(), b"synthetic-external-data");
    fs::hard_link(&outside, &path).unwrap();
    assert!(cache.get(&lease, &key(1)).unwrap().is_none());
    assert_eq!(fs::read(&outside).unwrap(), b"synthetic-external-data");
    let insecure = temp.0.join("insecure");
    fs::create_dir(&insecure).unwrap();
    fs::set_permissions(&insecure, fs::Permissions::from_mode(0o755)).unwrap();
    let other = Cache::new(insecure, CachePolicy::default()).unwrap();
    assert_eq!(other.usage().unwrap_err(), CacheError::Unavailable);
    let link = temp.0.join("link");
    symlink(temp.root(), &link).unwrap();
    let other = Cache::new(link, CachePolicy::default()).unwrap();
    assert_eq!(other.clear().unwrap_err(), CacheError::Unavailable);
    assert!(outside.exists());
}
#[test]
fn entry_and_directory_permissions_are_private_and_payload_never_contains_the_account_id() {
    let (_temp, _clock, cache, lease) = setup(CachePolicy::default());
    cache.put(&lease, &key(1), &raster(1)).unwrap();
    let path = cache.path(&lease, &key(1), false).unwrap().unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(!path.components().any(|part| part.as_os_str() == "1234"));
    let raw = fs::read(path).unwrap();
    assert!(!String::from_utf8_lossy(&raw).contains("1234"));
}
#[test]
fn concurrent_clear_blocks_new_leases_and_late_download_cannot_repopulate() {
    let (_temp, _clock, cache, lease) = setup(CachePolicy::default());
    cache.put(&lease, &key(1), &raster(1)).unwrap();
    let io = cache.0.io.lock().unwrap();
    let clearing = cache.clone();
    let done = thread::spawn(move || clearing.clear());
    let deadline = Instant::now() + Duration::from_secs(2);
    while lease.is_current() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    assert!(!lease.is_current());
    assert!(
        cache.lease().is_none(),
        "operations cannot start in the clearing generation"
    );
    drop(io);
    assert_eq!(done.join().unwrap().unwrap(), CacheStats::default());
    assert_eq!(
        cache.put(&lease, &key(2), &raster(2)).unwrap_err(),
        CacheError::Cancelled,
        "a download captured before clear cannot persist afterwards"
    );
    let next = cache.lease().unwrap();
    assert_ne!(next, lease);
    assert!(cache.get(&next, &key(1)).unwrap().is_none());
    cache.put(&next, &key(2), &raster(2)).unwrap();
    assert_eq!(cache.usage().unwrap().entries, 1);
}
#[test]
fn background_response_retired_by_disable_or_account_change_never_writes() {
    let (_temp, _clock, cache, lease) = setup(CachePolicy::default());
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let producer = cache.clone();
    let worker = thread::spawn(move || {
        ready_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        producer.put(&lease, &key(1), &raster(7))
    });
    ready_rx.recv().unwrap();
    cache
        .configure(CachePolicy {
            enabled: false,
            ..CachePolicy::default()
        })
        .unwrap();
    cache.bind(Some(AccountScope::from_account_id("9876").unwrap()));
    release_tx.send(()).unwrap();
    assert_eq!(worker.join().unwrap().unwrap_err(), CacheError::Disabled);
    assert_eq!(cache.usage().unwrap(), CacheStats::default());
}
#[test]
fn metadata_invalidation_after_library_writes_preserves_artwork_and_retires_old_downloads() {
    let (_temp, _clock, cache, lease) = setup(CachePolicy::default());
    let track_key = CacheKey::new(EntryKind::Track, "42", 0).unwrap();
    cache.put(&lease, &track_key, &track_record()).unwrap();
    cache.put(&lease, &key(1), &raster(1)).unwrap();
    assert_eq!(cache.usage().unwrap().entries, 2);
    let remaining = cache.invalidate_metadata().unwrap();
    assert_eq!(remaining.entries, 1);
    assert!(!lease.is_current());
    let next = cache.lease().unwrap();
    assert!(cache.get(&next, &track_key).unwrap().is_none());
    assert_eq!(cache.get(&next, &key(1)).unwrap().unwrap().bytes, raster(1));
    assert_eq!(
        cache.put(&lease, &track_key, &track_record()).unwrap_err(),
        CacheError::Cancelled
    );
}
#[test]
fn short_server_ttl_is_honored_and_noncacheable_refetch_can_remove_stale_cover() {
    let (_temp, clock, cache, lease) = setup(CachePolicy {
        artwork_ttl: Duration::from_secs(20),
        ..CachePolicy::default()
    });
    cache
        .put_with_ttl(&lease, &key(1), &raster(1), Duration::from_secs(3))
        .unwrap();
    clock.set(103);
    assert_eq!(
        cache.get(&lease, &key(1)).unwrap().unwrap().freshness,
        Freshness::Stale
    );
    cache.remove(&lease, &key(1)).unwrap();
    assert!(cache.get(&lease, &key(1)).unwrap().is_none());
    cache
        .put_with_ttl(&lease, &key(1), &raster(1), Duration::from_secs(1000))
        .unwrap();
    clock.set(123);
    assert_eq!(
        cache.get(&lease, &key(1)).unwrap().unwrap().freshness,
        Freshness::Stale,
        "server TTL cannot extend app policy"
    );
}
#[test]
fn concurrent_disk_admission_is_bounded_and_retry_uses_existing_cached_value() {
    let (_temp, _clock, cache, lease) = setup(CachePolicy::default());
    cache.put(&lease, &key(1), &raster(1)).unwrap();
    let io = cache.0.io.lock().unwrap();
    let reader = cache.clone();
    let token = lease.clone();
    let started = Instant::now();
    let worker = thread::spawn(move || reader.get(&token, &key(1)));
    assert_eq!(worker.join().unwrap().unwrap_err(), CacheError::Busy);
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(io);
    assert_eq!(
        cache.get(&lease, &key(1)).unwrap().unwrap().bytes,
        raster(1)
    );
}
#[test]
fn clear_works_while_disabled_and_leaves_noncache_application_files_untouched() {
    let (temp, _clock, cache, lease) = setup(CachePolicy::default());
    cache.put(&lease, &key(1), &raster(1)).unwrap();
    let unrelated = temp.0.join("settings.json");
    fs::write(&unrelated, b"synthetic preference").unwrap();
    cache
        .configure(CachePolicy {
            enabled: false,
            ..CachePolicy::default()
        })
        .unwrap();
    assert_eq!(cache.clear().unwrap(), CacheStats::default());
    assert_eq!(fs::read(&unrelated).unwrap(), b"synthetic preference");
    assert!(cache.lease().is_none());
}

#[test]
fn fifo_cache_record_is_rejected_without_waiting_for_a_writer() {
    let (_temp, _clock, cache, lease) = setup(CachePolicy::default());
    let path = cache.path(&lease, &key(1), true).unwrap().unwrap();
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &path,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    let reader = cache.clone();
    let (tx, rx) = mpsc::sync_channel(1);
    let started = Instant::now();
    let worker = thread::spawn(move || tx.send(reader.get(&lease, &key(1))).unwrap());
    let prompt = match rx.recv_timeout(Duration::from_millis(400)) {
        Ok(result) => {
            assert!(result.unwrap().is_none());
            true
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // Make the old blocking implementation fail without hanging the
            // complete test process: an RDWR nonblocking opener releases it.
            let unblock = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
                .open(&path)
                .unwrap();
            let _ = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            drop(unblock);
            false
        }
        Err(error) => panic!("synthetic cache reader disappeared: {error}"),
    };
    worker.join().unwrap();
    assert!(
        prompt,
        "a cache FIFO waited for a writer before its type check"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn nonregular_or_oversized_cache_records_never_become_a_payload() {
    let (temp, _clock, cache, lease) = setup(CachePolicy::default());
    let socket_path = cache.path(&lease, &key(1), true).unwrap().unwrap();
    // Unix socket names have a much shorter limit than ordinary cache-file
    // paths. Bind a short name, then move that nonregular inode into the cache.
    let short_socket = temp.0.join("socket");
    let _socket = std::os::unix::net::UnixListener::bind(&short_socket).unwrap();
    fs::rename(&short_socket, &socket_path).unwrap();
    assert!(cache.get(&lease, &key(1)).unwrap().is_none());
    let directory_path = cache.path(&lease, &key(2), true).unwrap().unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory_path)
        .unwrap();
    assert!(cache.get(&lease, &key(2)).unwrap().is_none());
    cache.trim().unwrap();
    assert!(!directory_path.exists());
    cache.put(&lease, &key(3), &raster(1)).unwrap();
    let file_path = cache.path(&lease, &key(3), false).unwrap().unwrap();
    fs::write(&file_path, vec![0; HEADER_BYTES + MAX_PAYLOAD_BYTES + 1]).unwrap();
    assert!(cache.get(&lease, &key(3)).unwrap().is_none());
    assert!(!file_path.exists());
    assert!(temp.root().exists());
}

#[test]
fn a_symlink_to_an_otherwise_valid_record_is_not_read() {
    let (temp, _clock, cache, lease) = setup(CachePolicy::default());
    cache.put(&lease, &key(1), &raster(13)).unwrap();
    let path = cache.path(&lease, &key(1), false).unwrap().unwrap();
    let outside = temp.0.join("valid-external-record");
    let original = fs::read(&path).unwrap();
    fs::write(&outside, &original).unwrap();
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o600)).unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&outside, &path).unwrap();
    assert!(cache.get(&lease, &key(1)).unwrap().is_none());
    assert_eq!(fs::read(outside).unwrap(), original);
}
