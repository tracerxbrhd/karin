// SPDX-License-Identifier: GPL-3.0-or-later
//! Original generated PCM/MP3 fixtures; no account or service resource is used.
use super::*;
use crate::playback::{PlaybackController, PlaybackOutput, PlaybackSource, PlaybackState};
use std::{process::Command, sync::mpsc, thread, time::Instant};

const FIRST: Identity = Identity {
    account: 1,
    device: 11,
    track: 101,
};
const SECOND: Identity = Identity {
    account: 1,
    device: 11,
    track: 102,
};
fn fixture(budget: u64) -> (Arc<Sandbox>, Arc<SyntheticAuthority>, AudioCache, Lease) {
    let sandbox = Sandbox::create(&std::env::temp_dir()).unwrap();
    let authority = Arc::new(SyntheticAuthority::default());
    authority.online.store(true, Ordering::Release);
    authority.grant(FIRST, 100, true);
    authority.grant(SECOND, 100, true);
    let cache = test_open(sandbox.clone(), authority.clone(), FIRST.device, budget).unwrap();
    let lease = cache.bind(Some(account(FIRST.account))).unwrap().unwrap();
    (sandbox, authority, cache, lease)
}
fn put(cache: &AudioCache, lease: &Lease, track: u64, samples: u32) {
    cache
        .put(
            cache.begin(lease, track).unwrap(),
            &SyntheticAudio::tone(samples).unwrap(),
        )
        .unwrap();
}
fn sample(cache: &AudioCache, lease: &Lease, track: u64) -> Arc<[u8]> {
    cache.get(lease, track).unwrap().unwrap().admit().unwrap()
}

#[test]
fn miss_hit_integrity_and_opaque_private_storage() {
    let (sandbox, _, cache, lease) = fixture(100_000);
    assert!(cache.get(&lease, FIRST.track).unwrap().is_none());
    let audio = SyntheticAudio::tone(4_000).unwrap();
    cache
        .put(cache.begin(&lease, FIRST.track).unwrap(), &audio)
        .unwrap();
    assert_eq!(&*sample(&cache, &lease, FIRST.track), &audio.0);
    let stats = cache.stats().unwrap();
    assert_eq!(stats.entries, 1);
    assert_eq!(stats.bytes, (HEADER + audio.0.len()) as u64);
    for entry in fs::read_dir(&sandbox.path).unwrap().flatten() {
        let name = entry.file_name();
        let name = name.to_str().unwrap();
        assert!(name == ".lock" || name.len() == 64 && valid_hex(name));
        assert_eq!(
            fs::symlink_metadata(entry.path()).unwrap().mode() & 0o777,
            if name == ".lock" { 0o600 } else { 0o700 }
        );
    }
    let hit = cache.get(&lease, FIRST.track).unwrap().unwrap();
    assert_eq!(format!("{hit:?}"), "AudioCacheHit([opaque])");
}

#[test]
fn retrying_a_failed_http_download_starts_a_clean_same_request_transaction() {
    struct InterruptedPrefix {
        sent: bool,
    }
    impl Read for InterruptedPrefix {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            if self.sent {
                return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
            }
            self.sent = true;
            let prefix = b"original-incomplete-prefix";
            bytes[..prefix.len()].copy_from_slice(prefix);
            Ok(prefix.len())
        }
    }
    let sandbox = Sandbox::create(&std::env::temp_dir()).unwrap();
    let authority = Arc::new(OnlineStreamAuthority::with_storage_permission(Arc::new(
        PrivateHttpCachePolicy,
    )));
    let cache = AudioCache::open_namespace(sandbox.clone(), authority, 1, 100_000).unwrap();
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    let track = ResourceKey::track("101").unwrap();
    let check = cache.begin_online_check(&lease, track).unwrap();
    cache.observe_verified_online(&lease, check).unwrap();
    let request = cache.begin(&lease, track).unwrap();
    let metadata_called = AtomicBool::new(false);
    assert_eq!(
        cache.put_reader_http_attempt(&request, InterruptedPrefix { sent: false }, |_, _| {
            metadata_called.store(true, Ordering::Release);
            Err(CacheError::HttpPolicyRequired)
        }),
        Err(CacheError::Unavailable)
    );
    assert!(!metadata_called.load(Ordering::Acquire));
    assert_eq!(cache.stats().unwrap().entries, 0);
    assert_eq!(fs::read_dir(&sandbox.path).unwrap().count(), 1);
    cache.check_request(&request).unwrap();

    let original = test_support::structural_mp3();
    let response = http_policy::ResponsePolicy::parse(
        200,
        [
            ("Content-Type".into(), "audio/mpeg".into()),
            ("Content-Length".into(), original.len().to_string()),
            ("ETag".into(), "\"original-clean-full-body\"".into()),
        ],
    )
    .unwrap();
    cache
        .put_reader_http_attempt(&request, original.as_slice(), |sha256, bytes| {
            assert_eq!(sha256, hash(&original));
            assert_eq!(bytes, original.len() as u64);
            response
                .record([1; 32], [2; 32], sha256, bytes)
                .map_err(|_| CacheError::HttpPolicyRequired)
        })
        .unwrap();
    let identity = cache.identity(&lease, track).unwrap();
    let record = cache.path(identity.key());
    assert_eq!(fs::read(record.join("media.bin")).unwrap(), original);
    let metadata = read_http_record(&record).unwrap().unwrap();
    assert_eq!(metadata.sha256, hash(&original));
    assert_eq!(metadata.bytes, original.len() as u64);
    assert_eq!(cache.stats().unwrap().entries, 1);
    // The same request still occupies its bounded slot until its owner drops;
    // neither a failed prefix nor a clean retry can create another live ticket.
    assert!(matches!(
        cache.begin(&lease, track),
        Err(CacheError::Duplicate)
    ));
    drop(request);
    assert!(cache.begin(&lease, track).is_ok());
}

#[test]
fn corrupted_truncated_and_swapped_records_become_misses() {
    let (_, _, cache, lease) = fixture(100_000);
    for failure in 0..4 {
        put(&cache, &lease, FIRST.track, 400);
        let path = cache.path(FIRST.key());
        match failure {
            0 => fs::write(path.join("media.bin"), b"partial").unwrap(),
            1 => fs::write(path.join("header"), b"partial").unwrap(),
            2 => {
                let mut bytes = fs::read(path.join("media.bin")).unwrap();
                bytes[50] ^= 1;
                fs::write(path.join("media.bin"), bytes).unwrap();
            }
            _ => {
                let mut header = fs::read(path.join("header")).unwrap();
                header[40] ^= 1;
                fs::write(path.join("header"), header).unwrap();
            }
        }
        assert!(cache.get(&lease, FIRST.track).unwrap().is_none());
        assert_eq!(cache.stats().unwrap(), Stats::default());
    }
}

#[test]
fn partial_crash_directories_are_never_hits_and_recovery_removes_them() {
    let (sandbox, authority, cache, _) = fixture(100_000);
    let partial = sandbox.path.join(format!(".part-{}", "a".repeat(32)));
    fs::DirBuilder::new().mode(0o700).create(&partial).unwrap();
    write_private(&partial.join("media.bin"), b"incomplete synthetic record").unwrap();
    drop(cache);
    let cache = test_open(sandbox.clone(), authority, 11, 100_000).unwrap();
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    assert!(!partial.exists());
    assert!(cache.get(&lease, FIRST.track).unwrap().is_none());
    assert_eq!(cache.stats().unwrap(), Stats::default());
}

#[test]
fn duplicate_request_cancellation_and_bounded_admission() {
    let (_, authority, cache, lease) = fixture(100_000);
    let first = cache.begin(&lease, FIRST.track).unwrap();
    assert!(matches!(
        cache.begin(&lease, FIRST.track),
        Err(CacheError::Duplicate)
    ));
    let mut requests = vec![first];
    for track in 2..=4 {
        let identity = Identity { track, ..FIRST };
        authority.grant(identity, 100, true);
        requests.push(cache.begin(&lease, track).unwrap());
    }
    authority.grant(Identity { track: 5, ..FIRST }, 100, true);
    assert!(matches!(cache.begin(&lease, 5), Err(CacheError::Limit)));
    let first = requests.remove(0);
    first.cancel();
    assert_eq!(
        cache.put(first, &SyntheticAudio::tone(400).unwrap()),
        Err(CacheError::Cancelled)
    );
    assert!(cache.get(&lease, FIRST.track).unwrap().is_none());
    drop(requests);
    put(&cache, &lease, FIRST.track, 400);
}

#[test]
fn precommit_cancel_write_failure_and_account_change_publish_no_partial_entry() {
    for outcome in [CacheError::Cancelled, CacheError::Unavailable] {
        let (sandbox, _, cache, lease) = fixture(100_000);
        let request = cache.begin(&lease, FIRST.track).unwrap();
        let cancellation = request.cancel.clone();
        let result =
            cache.put_before_commit(request, &SyntheticAudio::tone(2_000).unwrap(), || {
                if outcome == CacheError::Cancelled {
                    cancellation.store(true, Ordering::Release);
                    Ok(())
                } else {
                    Err(CacheError::Unavailable)
                }
            });
        assert_eq!(result, Err(outcome));
        assert_eq!(cache.stats().unwrap(), Stats::default());
        assert_eq!(
            fs::read_dir(&sandbox.path).unwrap().count(),
            1,
            "only private lock survives a partial write"
        );
    }
    let (_, _, cache, lease) = fixture(100_000);
    let request = cache.begin(&lease, FIRST.track).unwrap();
    assert_eq!(
        cache.put_before_commit(request, &SyntheticAudio::tone(2_000).unwrap(), || {
            cache.bind(Some(account(2)))?;
            Ok(())
        }),
        Err(CacheError::Cancelled)
    );
    assert_eq!(cache.stats().unwrap(), Stats::default());
}

#[test]
fn byte_budget_and_actual_lru_survive_reopen() {
    let one = (HEADER + SyntheticAudio::tone(400).unwrap().0.len()) as u64;
    let (sandbox, authority, cache, lease) = fixture(one * 2);
    put(&cache, &lease, FIRST.track, 400);
    put(&cache, &lease, SECOND.track, 400);
    sample(&cache, &lease, FIRST.track); // FIRST is newer than SECOND now.
    drop(cache);
    let cache = test_open(sandbox, authority.clone(), 11, one * 2).unwrap();
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    let third = Identity {
        track: 103,
        ..FIRST
    };
    authority.grant(third, 100, true);
    put(&cache, &lease, third.track, 400);
    assert!(cache.get(&lease, SECOND.track).unwrap().is_none());
    assert!(cache.get(&lease, FIRST.track).unwrap().is_some());
    assert!(cache.get(&lease, third.track).unwrap().is_some());
    assert_eq!(
        cache.stats().unwrap(),
        Stats {
            bytes: one * 2,
            entries: 2
        }
    );
    assert_eq!(
        cache.put(
            cache.begin(&lease, FIRST.track).unwrap(),
            &SyntheticAudio::tone(20_000).unwrap()
        ),
        Err(CacheError::Limit)
    );
    assert!(
        cache.get(&lease, FIRST.track).unwrap().is_some(),
        "oversized request must not evict a good entry"
    );
}

#[test]
fn expiry_revocation_renewal_and_required_online_checks_are_independent_of_presence() {
    let (_, authority, cache, lease) = fixture(100_000);
    put(&cache, &lease, FIRST.track, 400);
    let hit = cache.get(&lease, FIRST.track).unwrap().unwrap();
    authority.now.store(100, Ordering::Release);
    assert_eq!(hit.admit().unwrap_err(), CacheError::Expired);
    assert!(matches!(
        cache.get(&lease, FIRST.track),
        Err(CacheError::Expired)
    ));
    assert!(!cache.path(FIRST.key()).exists());
    authority.grant(FIRST, 200, true);
    put(&cache, &lease, FIRST.track, 400);
    let hit = cache.get(&lease, FIRST.track).unwrap().unwrap();
    authority.grant(FIRST, 300, true);
    assert_eq!(
        hit.admit().unwrap_err(),
        CacheError::Cancelled,
        "renewal cannot reuse an older capability"
    );
    assert!(cache.get(&lease, FIRST.track).unwrap().is_some());
    authority.revoke(FIRST);
    assert!(matches!(
        cache.get(&lease, FIRST.track),
        Err(CacheError::Revoked)
    ));
    assert!(!cache.path(FIRST.key()).exists());
    authority.grant(FIRST, 300, false);
    put(&cache, &lease, FIRST.track, 400);
    authority.online.store(false, Ordering::Release);
    assert!(matches!(
        cache.get(&lease, FIRST.track),
        Err(CacheError::NeedsOnline)
    ));
    authority.grant(FIRST, 300, true);
    authority.online.store(true, Ordering::Release);
    put(&cache, &lease, FIRST.track, 400);
    authority.online.store(false, Ordering::Release);
    assert!(
        cache.get(&lease, FIRST.track).unwrap().is_some(),
        "explicit synthetic offline grant only"
    );
    authority
        .rights
        .lock()
        .unwrap()
        .get_mut(&FIRST.key())
        .unwrap()
        .online_check_required = true;
    assert!(matches!(
        cache.get(&lease, FIRST.track),
        Err(CacheError::NeedsOnline)
    ));
}

#[test]
fn wrong_account_and_device_cannot_reuse_data_and_logout_fences_old_handles() {
    let (sandbox, authority, cache, first) = fixture(100_000);
    put(&cache, &first, FIRST.track, 400);
    let hit = cache.get(&first, FIRST.track).unwrap().unwrap();
    let second_id = Identity {
        account: 2,
        ..FIRST
    };
    authority.grant(second_id, 100, true);
    let second = cache.bind(Some(account(2))).unwrap().unwrap();
    assert!(cache.get(&second, FIRST.track).unwrap().is_none());
    assert!(matches!(
        cache.get(&first, FIRST.track),
        Err(CacheError::Cancelled)
    ));
    assert_eq!(hit.admit().unwrap_err(), CacheError::Cancelled);
    drop(cache);
    let other = test_open(sandbox, authority.clone(), 12, 100_000).unwrap();
    let other_lease = other.bind(Some(account(1))).unwrap().unwrap();
    assert!(matches!(
        other.get(&other_lease, FIRST.track),
        Err(CacheError::NoGrant)
    ));
    authority.grant(
        Identity {
            device: 12,
            ..FIRST
        },
        100,
        true,
    );
    assert!(other.get(&other_lease, FIRST.track).unwrap().is_none());
    other.logout().unwrap();
    assert_eq!(other.stats().unwrap(), Stats::default());
    assert!(matches!(
        other.begin(&second, FIRST.track),
        Err(CacheError::Cancelled)
    ));
}

#[test]
fn clear_during_write_fences_requests_and_requires_successful_cleanup_retry() {
    let (sandbox, _, cache, lease) = fixture(100_000);
    put(&cache, &lease, SECOND.track, 400);
    let hit = cache.get(&lease, SECOND.track).unwrap().unwrap();
    let request = cache.begin(&lease, FIRST.track).unwrap();
    let writer_cache = cache.clone();
    let (arrived, ready) = mpsc::sync_channel(0);
    let (resume, wait) = mpsc::sync_channel(0);
    let writer = thread::spawn(move || {
        writer_cache.put_before_commit(request, &SyntheticAudio::tone(800).unwrap(), || {
            arrived.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(())
        })
    });
    ready.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(cache.clear(), Err(CacheError::Busy));
    assert_eq!(hit.admit().unwrap_err(), CacheError::Cancelled);
    resume.send(()).unwrap();
    assert_eq!(writer.join().unwrap(), Err(CacheError::Cancelled));
    assert_eq!(cache.clear().unwrap(), Stats::default());
    let fresh = cache.bind(Some(account(1))).unwrap().unwrap();
    assert!(cache.get(&fresh, SECOND.track).unwrap().is_none());
    assert_eq!(fs::read_dir(&sandbox.path).unwrap().count(), 1);
}

#[test]
fn clear_retries_observed_io_contention_without_holding_the_admission_mutex() {
    let (_, _, cache, lease) = fixture(100_000);
    put(&cache, &lease, FIRST.track, 400);
    let hit = cache.get(&lease, FIRST.track).unwrap().unwrap();
    let io = cache.io().unwrap();
    let worker = cache.clone();
    let (observed, waiting) = mpsc::sync_channel(0);
    let clearing = thread::spawn(move || {
        let mut observed = Some(observed);
        worker.clear_observing_contention(
            || {},
            || {
                if let Some(observed) = observed.take() {
                    observed.send(()).unwrap();
                }
            },
        )
    });
    // The observer fires only after a real WouldBlock, not merely thread start.
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let state = cache
        .0
        .state
        .try_lock()
        .expect("clear must not hold GTK admission state while waiting");
    assert!(state.clearing);
    drop(state);
    assert_eq!(hit.admit().unwrap_err(), CacheError::Cancelled);
    drop(io);
    assert_eq!(clearing.join().unwrap(), Ok(Stats::default()));
    assert_eq!(cache.stats().unwrap(), Stats::default());
    let fresh = cache.bind(Some(account(FIRST.account))).unwrap().unwrap();
    assert!(cache.get(&fresh, FIRST.track).unwrap().is_none());
}

#[test]
fn concurrent_reads_and_writes_remain_atomic_and_duplicate_work_is_bounded() {
    let (_, _, cache, lease) = fixture(100_000);
    put(&cache, &lease, SECOND.track, 4_000);
    let first = cache.begin(&lease, FIRST.track).unwrap();
    let writer_cache = cache.clone();
    let (arrived, ready) = mpsc::sync_channel(0);
    let (resume, wait) = mpsc::sync_channel(0);
    let writer = thread::spawn(move || {
        writer_cache.put_before_commit(first, &SyntheticAudio::tone(4_000).unwrap(), || {
            arrived.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(())
        })
    });
    ready.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(matches!(
        cache.begin(&lease, FIRST.track),
        Err(CacheError::Duplicate)
    ));
    assert!(matches!(
        cache.get(&lease, SECOND.track),
        Err(CacheError::Busy)
    ));
    resume.send(()).unwrap();
    writer.join().unwrap().unwrap();
    for track in [FIRST.track, SECOND.track] {
        assert_eq!(
            &*sample(&cache, &lease, track),
            &SyntheticAudio::tone(4_000).unwrap().0
        );
    }
}

#[test]
fn directory_ownership_symlinks_unavailable_storage_and_injected_write_failure_fail_closed() {
    let (sandbox, authority, cache, lease) = fixture(100_000);
    let other = Sandbox::create(&std::env::temp_dir()).unwrap();
    std::os::unix::fs::symlink(&other.path, cache.path(FIRST.key())).unwrap();
    assert!(matches!(
        cache.get(&lease, FIRST.track),
        Err(CacheError::Unavailable)
    ));
    fs::remove_file(cache.path(FIRST.key())).unwrap();
    let request = cache.begin(&lease, FIRST.track).unwrap();
    assert_eq!(
        cache.put_before_commit(request, &SyntheticAudio::tone(400).unwrap(), || Err(
            CacheError::Unavailable
        )),
        Err(CacheError::Unavailable)
    );
    assert_eq!(cache.stats().unwrap(), Stats::default());
    fs::set_permissions(&sandbox.path, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        test_open(sandbox.clone(), authority, 11, 100_000),
        Err(CacheError::Unavailable)
    ));
    fs::set_permissions(&sandbox.path, fs::Permissions::from_mode(0o700)).unwrap();
    let moved = sandbox.path.with_extension("temporarily-unavailable");
    fs::rename(&sandbox.path, &moved).unwrap();
    assert!(matches!(cache.stats(), Err(CacheError::Unavailable)));
    fs::rename(moved, &sandbox.path).unwrap();
    assert_eq!(
        cache.stats().unwrap(),
        Stats::default(),
        "failed admission releases the I/O mutex while the owner lock remains held"
    );
}

#[test]
fn one_namespace_owner_and_retired_tickets_survive_clear_and_reopen() {
    let (sandbox, authority, cache, lease) = fixture(100_000);
    put(&cache, &lease, FIRST.track, 400);
    let old_hit = cache.get(&lease, FIRST.track).unwrap().unwrap();
    let old_request = cache.begin(&lease, SECOND.track).unwrap();
    assert!(matches!(
        test_open(sandbox.clone(), authority.clone(), 11, 100_000),
        Err(CacheError::Busy)
    ));
    cache.clear().unwrap();
    assert_eq!(old_hit.admit().unwrap_err(), CacheError::Cancelled);
    drop(cache);
    let reopened = test_open(sandbox, authority, 11, 100_000).unwrap();
    let current = reopened.bind(Some(account(1))).unwrap().unwrap();
    assert_eq!(
        reopened.put(old_request, &SyntheticAudio::tone(400).unwrap()),
        Err(CacheError::Cancelled)
    );
    assert!(reopened.get(&current, SECOND.track).unwrap().is_none());
    assert_eq!(old_hit.admit().unwrap_err(), CacheError::Cancelled);
}

#[test]
fn namespace_owner_release_is_independent_of_inherited_file_description() {
    let (sandbox, authority, cache, lease) = fixture(100_000);
    put(&cache, &lease, FIRST.track, 400);
    let old_hit = cache.get(&lease, FIRST.track).unwrap().unwrap();
    let old_request = cache.begin(&lease, SECOND.track).unwrap();
    // dup and fork both retain the same Linux open file description. A child
    // between fork and exec can retain this reference even with CLOEXEC set.
    // A duplicate makes that scheduling window deterministic without forking
    // a multithreaded Rust test process or changing the production FD flags.
    let inherited_description = cache.0._lock_file.try_clone().unwrap();
    let peer = cache.clone();
    drop(cache);
    assert!(matches!(
        test_open(sandbox.clone(), authority.clone(), 11, 100_000),
        Err(CacheError::Busy)
    ));
    drop(peer);
    assert!(
        lease.owner.upgrade().is_none(),
        "tickets retain only Weak owners"
    );
    assert_eq!(old_hit.admit().unwrap_err(), CacheError::Cancelled);
    let reopened = test_open(sandbox.clone(), authority.clone(), 11, 100_000).unwrap();
    assert_eq!(
        reopened.put(old_request, &SyntheticAudio::tone(400).unwrap()),
        Err(CacheError::Cancelled)
    );
    drop(inherited_description);
    assert!(
        matches!(
            test_open(sandbox, authority, 11, 100_000),
            Err(CacheError::Busy)
        ),
        "closing an old inherited descriptor cannot unlock the new owner"
    );
}

#[test]
fn active_owner_recovers_orphan_bytes_before_quota_or_stats() {
    let one = (HEADER + SyntheticAudio::tone(400).unwrap().0.len()) as u64;
    let (sandbox, _, cache, lease) = fixture(one);
    let orphan = sandbox.path.join(format!(".part-{}", "b".repeat(32)));
    fs::DirBuilder::new().mode(0o700).create(&orphan).unwrap();
    write_private(
        &orphan.join("media.bin"),
        &SyntheticAudio::tone(400).unwrap().0,
    )
    .unwrap();
    assert_eq!(cache.stats().unwrap(), Stats::default());
    assert!(!orphan.exists());
    put(&cache, &lease, FIRST.track, 400);
    assert_eq!(
        cache.stats().unwrap(),
        Stats {
            bytes: one,
            entries: 1
        }
    );
}

#[test]
fn synthetic_flatpak_xdg_namespace_never_touches_host_profile() {
    let parent = Sandbox::create(&std::env::temp_dir()).unwrap();
    let fake_flatpak_cache = parent
        .path
        .join(".var/app/io.github.tracerxbrhd.Karin/cache");
    fs::create_dir_all(&fake_flatpak_cache).unwrap();
    let sandbox = Sandbox::create(&fake_flatpak_cache).unwrap();
    let authority = Arc::new(SyntheticAuthority::default());
    authority.grant(FIRST, 100, true);
    let cache = test_open(sandbox.clone(), authority, 11, 100_000).unwrap();
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    put(&cache, &lease, FIRST.track, 400);
    assert!(sandbox.path.starts_with(&fake_flatpak_cache));
    assert_eq!(cache.stats().unwrap().entries, 1);
    cache.logout().unwrap();
    assert_eq!(cache.stats().unwrap(), Stats::default());
    // This namespace simulation is deliberately not an installed Flatpak claim.
}

#[test]
fn actual_process_restart_requires_new_authority_and_recovers_generated_audio() {
    if let Some(path) = std::env::var_os("KARIN_SYNTHETIC_CACHE_CHILD") {
        restart_child(PathBuf::from(path));
        return;
    }
    let (sandbox, _, cache, lease) = fixture(100_000);
    put(&cache, &lease, FIRST.track, 400);
    drop(cache);
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "audio_cache::tests::actual_process_restart_requires_new_authority_and_recovers_generated_audio",
            "--nocapture",
        ])
        .env("KARIN_SYNTHETIC_CACHE_CHILD", &sandbox.path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated synthetic restart worker failed"
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
}

fn restart_child(path: PathBuf) {
    let sandbox = Sandbox::reopen(path).unwrap();
    let authority = Arc::new(SyntheticAuthority::default());
    let cache = test_open(sandbox, authority.clone(), 11, 100_000).unwrap();
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    assert!(matches!(
        cache.get(&lease, FIRST.track),
        Err(CacheError::NoGrant)
    ));
    // Restarted storage supplies no entitlement itself. An explicit NEW fake
    // authority grant admits the persisted original audio; no replacement write.
    assert!(cache.path(FIRST.key()).exists());
    authority.grant(FIRST, 100, true);
    assert_eq!(
        &*sample(&cache, &lease, FIRST.track),
        &SyntheticAudio::tone(400).unwrap().0
    );
}

#[test]
fn cached_generated_wav_plays_through_existing_controller_and_clear_preserves_open_playback() {
    let (_, _, cache, lease) = fixture(100_000);
    put(&cache, &lease, FIRST.track, 16_000);
    let hit = cache.get(&lease, FIRST.track).unwrap().unwrap();
    let admitted = hit.admit().unwrap();
    assert_eq!(&*admitted, &SyntheticAudio::tone(16_000).unwrap().0);
    let player = PlaybackController::new(PlaybackOutput::Fakesink).unwrap();
    player
        .prepare(PlaybackSource::local_file(cache.path(FIRST.key()).join("media.bin")).unwrap())
        .unwrap();
    wait_player(&player, PlaybackState::Ready);
    player.play().unwrap();
    wait_player(&player, PlaybackState::Playing);
    let generation = player.snapshot().generation;
    player.pause().unwrap();
    wait_player(&player, PlaybackState::Paused);
    player.set_volume(0.25).unwrap();
    let volume_deadline = Instant::now() + Duration::from_secs(3);
    while (player.snapshot().volume - 0.25).abs() > f64::EPSILON {
        assert!(Instant::now() < volume_deadline);
        thread::sleep(Duration::from_millis(5));
    }
    player.resume().unwrap();
    wait_player(&player, PlaybackState::Playing);
    cache.clear().unwrap();
    assert_eq!(cache.stats().unwrap(), Stats::default());
    assert_eq!(hit.admit().unwrap_err(), CacheError::Cancelled);
    assert_eq!(
        &*admitted,
        &SyntheticAudio::tone(16_000).unwrap().0,
        "an admitted immutable buffer survives Clear"
    );
    let old_seek = player.snapshot().seek_serial;
    player.seek(Duration::from_millis(750)).unwrap();
    let seek_deadline = Instant::now() + Duration::from_secs(3);
    while player.snapshot().seek_serial == old_seek {
        assert!(player.snapshot().last_error.is_none());
        assert!(Instant::now() < seek_deadline);
        thread::sleep(Duration::from_millis(5));
    }
    wait_player(&player, PlaybackState::Ended);
    assert_eq!(player.snapshot().generation, generation);
    player.shutdown();
    let deadline = Instant::now() + Duration::from_secs(3);
    while player.snapshot().worker_running {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
}
fn wait_player(player: &PlaybackController, expected: PlaybackState) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = player.snapshot();
        assert!(
            snapshot.last_error.is_none(),
            "generated local playback failed safely"
        );
        if snapshot.state == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "generated local playback state deadline"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn production_missing_contract_refuses_before_touching_unavailable_storage() {
    let parent = Sandbox::create(&std::env::temp_dir()).unwrap();
    let path = parent.path.join("persistent-audio");
    let cache = AudioCache::open(
        path.clone(),
        Arc::new(MissingStorageContract),
        CachePolicy::default(),
    )
    .unwrap();
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    let resource = ResourceKey::track("42:7").unwrap();
    let moved = parent.path.join("temporarily-moved-audio");
    fs::rename(&path, &moved).unwrap();
    assert!(matches!(
        cache.get(&lease, resource),
        Err(CacheError::NoGrant)
    ));
    assert!(matches!(
        cache.begin(&lease, resource),
        Err(CacheError::NoGrant)
    ));
    assert!(matches!(cache.stats(), Err(CacheError::Unavailable)));
    fs::rename(moved, path).unwrap();
    assert_eq!(cache.stats().unwrap(), Stats::default());
}

#[test]
fn production_stream_reader_failures_and_external_cancel_preserve_completed_records() {
    struct BrokenReader {
        first: bool,
    }
    impl Read for BrokenReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.first {
                self.first = false;
                buffer[..4].copy_from_slice(b"data");
                Ok(4)
            } else {
                Err(std::io::ErrorKind::UnexpectedEof.into())
            }
        }
    }
    let parent = Sandbox::create(&std::env::temp_dir()).unwrap();
    let scope = account(1);
    let key = ResourceKey::track("42:7").unwrap();
    let authority = Arc::new(SyntheticAuthority::default());
    authority.allow(&scope, &key);
    let cache = AudioCache::open(
        parent.path.join("audio"),
        authority.clone(),
        CachePolicy { max_bytes: 100_000 },
    )
    .unwrap();
    let lease = cache.bind(Some(scope.clone())).unwrap().unwrap();
    let media = SyntheticAudio::tone(400).unwrap();
    cache
        .put_reader(cache.begin(&lease, key).unwrap(), media.0.as_slice())
        .unwrap();
    let before = cache.stats().unwrap();
    let request = cache.begin(&lease, key).unwrap();
    assert_eq!(
        cache.put_reader(request, BrokenReader { first: true }),
        Err(CacheError::Unavailable)
    );
    assert_eq!(cache.stats().unwrap(), before);
    assert_eq!(
        &*cache.get(&lease, key).unwrap().unwrap().admit().unwrap(),
        &media.0
    );
    let request = cache.begin(&lease, key).unwrap();
    let cancellation = request.cancellation();
    cancellation.cancel();
    assert_eq!(cache.check_request(&request), Err(CacheError::Cancelled));
    assert_eq!(
        cache.put_reader(request, media.0.as_slice()),
        Err(CacheError::Cancelled)
    );
    assert_eq!(cache.stats().unwrap(), before);
    authority.deny(&scope, &key);
    assert!(matches!(cache.get(&lease, key), Err(CacheError::Revoked)));
    assert_eq!(cache.stats().unwrap(), Stats::default());
}

#[test]
fn admitted_file_is_read_only_and_keeps_the_verified_inode_across_replace_and_clear() {
    use std::os::unix::fs::FileExt;
    let (_, _, cache, lease) = fixture(100_000);
    put(&cache, &lease, FIRST.track, 400);
    let hit = cache.get(&lease, FIRST.track).unwrap().unwrap();
    let admitted = hit.admit_file().unwrap();
    let inode = admitted.metadata().unwrap().ino();
    assert!(admitted.write_at(b"not permitted", 0).is_err());
    put(&cache, &lease, FIRST.track, 800);
    let replacement = cache
        .get(&lease, FIRST.track)
        .unwrap()
        .unwrap()
        .admit_file()
        .unwrap();
    assert_ne!(replacement.metadata().unwrap().ino(), inode);
    assert_eq!(admitted.metadata().unwrap().ino(), inode);
    cache.clear().unwrap();
    assert!(matches!(hit.admit_file(), Err(CacheError::Cancelled)));
    let mut original = vec![0; SyntheticAudio::tone(400).unwrap().0.len()];
    admitted.read_exact_at(&mut original, 0).unwrap();
    assert_eq!(original, SyntheticAudio::tone(400).unwrap().0);
}

#[test]
fn authorization_for_a_different_resource_is_never_accepted() {
    struct WrongResource;
    impl StorageAuthority for WrongResource {
        fn authorize(&self, identity: &ResourceIdentity) -> Result<StorageGrant> {
            Ok(StorageGrant {
                identity: ResourceIdentity {
                    track: 999u64.into(),
                    ..*identity
                },
                revision: 1,
                content: None,
                basis: AdmissionBasis::IndependentLicense,
            })
        }
        fn check(&self, _: &StorageGrant) -> Result<()> {
            Ok(())
        }
    }
    let parent = Sandbox::create(&std::env::temp_dir()).unwrap();
    let cache = AudioCache::open(
        parent.path.join("audio"),
        Arc::new(WrongResource),
        CachePolicy::default(),
    )
    .unwrap();
    let lease = cache.bind(Some(account(1))).unwrap().unwrap();
    assert!(matches!(
        cache.get(&lease, FIRST.track),
        Err(CacheError::Cancelled)
    ));
    assert!(matches!(
        cache.begin(&lease, FIRST.track),
        Err(CacheError::Cancelled)
    ));
    assert_eq!(cache.stats().unwrap(), Stats::default());
}

#[test]
fn policy_updates_and_old_cleanup_completion_cannot_remove_newer_fences() {
    let (_, _, cache, lease) = fixture(100_000);
    put(&cache, &lease, FIRST.track, 400);
    let worker = cache.clone();
    let (arrived, ready) = mpsc::sync_channel(0);
    let (resume, wait) = mpsc::sync_channel(0);
    let clear = thread::spawn(move || {
        worker.clear_before_complete(|| {
            arrived.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(3)).unwrap();
        })
    });
    ready.recv_timeout(Duration::from_secs(3)).unwrap();
    cache.fence().unwrap();
    cache.configure(CachePolicy { max_bytes: 50_000 }).unwrap();
    assert!(cache.lease().is_none());
    resume.send(()).unwrap();
    assert_eq!(clear.join().unwrap().unwrap(), Stats::default());
    assert!(
        cache.lease().is_none(),
        "old cleanup cannot undo newer cancellation"
    );
    let blocked = cache.bind(Some(account(1))).unwrap().unwrap();
    assert!(matches!(
        cache.get(&blocked, FIRST.track),
        Err(CacheError::Cancelled)
    ));
    cache.clear().unwrap();
    let fresh = cache.bind(Some(account(1))).unwrap().unwrap();
    assert!(cache.get(&fresh, FIRST.track).unwrap().is_none());
    let same = cache.lease().unwrap();
    cache.configure(CachePolicy { max_bytes: 50_000 }).unwrap();
    assert!(
        cache.get(&same, FIRST.track).unwrap().is_none(),
        "unchanged policy preserves lease"
    );
}

#[test]
fn persistent_open_rejects_symlink_ancestors_and_reuses_only_newly_authorized_bytes() {
    let parent = Sandbox::create(&std::env::temp_dir()).unwrap();
    let scope = account(1);
    let key = ResourceKey::track("42:7").unwrap();
    let authority = Arc::new(SyntheticAuthority::default());
    authority.allow(&scope, &key);
    let root = parent.path.join("real");
    let cache = AudioCache::open(root.clone(), authority, CachePolicy::default()).unwrap();
    let lease = cache.bind(Some(scope.clone())).unwrap().unwrap();
    let media = SyntheticAudio::tone(400).unwrap();
    cache
        .put_reader(cache.begin(&lease, key).unwrap(), media.0.as_slice())
        .unwrap();
    drop(cache);
    let new_authority = Arc::new(SyntheticAuthority::default());
    let reopened =
        AudioCache::open(root.clone(), new_authority.clone(), CachePolicy::default()).unwrap();
    let lease = reopened.bind(Some(scope.clone())).unwrap().unwrap();
    assert!(matches!(
        reopened.get(&lease, key),
        Err(CacheError::NoGrant)
    ));
    new_authority.allow(&scope, &key);
    assert_eq!(
        &*reopened.get(&lease, key).unwrap().unwrap().admit().unwrap(),
        &media.0
    );
    drop(reopened);
    let link = parent.path.join("alias");
    std::os::unix::fs::symlink(&root, &link).unwrap();
    assert!(matches!(
        AudioCache::open(
            link.join("nested"),
            Arc::new(MissingStorageContract),
            CachePolicy::default()
        ),
        Err(CacheError::Unavailable)
    ));
}

fn shared_fixture(
    limit: Option<u64>,
) -> (
    Arc<Sandbox>,
    Arc<SyntheticAuthority>,
    Arc<crate::cache_budget::CacheBudget>,
    crate::cache::Cache,
    AudioCache,
) {
    let root = Sandbox::create(&std::env::temp_dir()).unwrap();
    let metadata_root = root.path.join("persistent");
    let audio_root = root.path.join("audio");
    let budget = crate::cache_budget::CacheBudget::new(
        metadata_root.clone(),
        audio_root.clone(),
        true,
        limit,
    )
    .unwrap();
    let metadata = crate::cache::Cache::new_with_budget(
        metadata_root,
        crate::cache::CachePolicy::default(),
        budget.clone(),
    )
    .unwrap();
    metadata.bind(Some(account(1)));
    let authority = Arc::new(SyntheticAuthority::default());
    for track in [101, 102, 103] {
        authority.allow(&account(1), &ResourceKey::from(track));
    }
    let audio = AudioCache::open_with_budget(
        audio_root,
        authority.clone(),
        CachePolicy::default(),
        budget.clone(),
    )
    .unwrap();
    audio.bind(Some(account(1))).unwrap();
    (root, authority, budget, metadata, audio)
}

fn shared_artwork(marker: u8) -> Vec<u8> {
    let mut bytes = b"KARINRGB".to_vec();
    for value in [512u32, 512, 2048] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.push(1);
    bytes.resize(21 + 512 * 2048, marker);
    bytes
}

#[test]
fn shared_budget_counts_both_stores_restarts_and_fences_disabled_hits() {
    let (root, authority, budget, metadata, audio) = shared_fixture(None);
    let audio_lease = audio.lease().unwrap();
    put(&audio, &audio_lease, 101, 100_000);
    let key =
        crate::cache::CacheKey::new(crate::cache::EntryKind::Artwork, "shared-cover", 0).unwrap();
    metadata
        .put(&metadata.lease().unwrap(), &key, &shared_artwork(12))
        .unwrap();
    let total = audio.stats().unwrap().bytes + metadata.usage().unwrap().bytes;
    assert_eq!(budget.usage().unwrap().bytes, total);
    assert_eq!(budget.usage().unwrap().entries, 2);
    let hit = audio.get(&audio_lease, 101).unwrap().unwrap();
    let metadata_lease = metadata.lease().unwrap();
    budget.configure(false, None).unwrap();
    assert!(matches!(hit.admit_file(), Err(CacheError::Cancelled)));
    assert!(metadata.get(&metadata_lease, &key).is_err());
    assert!(matches!(
        audio.begin(&audio_lease, 102),
        Err(CacheError::Cancelled)
    ));
    assert_eq!(budget.usage().unwrap().bytes, total);
    budget.configure(true, None).unwrap();
    assert!(matches!(hit.admit_file(), Err(CacheError::Cancelled)));
    assert!(!metadata_lease.is_current());
    drop(hit);
    drop(audio);
    drop(metadata);
    drop(budget);
    let budget = crate::cache_budget::CacheBudget::new(
        root.path.join("persistent"),
        root.path.join("audio"),
        true,
        None,
    )
    .unwrap();
    let metadata = crate::cache::Cache::new_with_budget(
        root.path.join("persistent"),
        crate::cache::CachePolicy::default(),
        budget.clone(),
    )
    .unwrap();
    metadata.bind(Some(account(1)));
    let audio = AudioCache::open_with_budget(
        root.path.join("audio"),
        authority,
        CachePolicy::default(),
        budget.clone(),
    )
    .unwrap();
    let lease = audio.bind(Some(account(1))).unwrap().unwrap();
    assert!(audio.get(&lease, 101).unwrap().is_some());
    assert!(
        metadata
            .get(&metadata.lease().unwrap(), &key)
            .unwrap()
            .is_some()
    );
    assert_eq!(budget.usage().unwrap().bytes, total);
    budget.configure(true, Some(1024 * 1024)).unwrap();
    budget.trim().unwrap();
    assert!(budget.usage().unwrap().bytes <= 1024 * 1024);
    assert_eq!(
        budget.usage().unwrap().bytes,
        audio.stats().unwrap().bytes + metadata.usage().unwrap().bytes
    );
}

#[test]
fn shared_budget_reserves_inflight_audio_while_small_artwork_remains_writable() {
    struct HeldReader {
        entered: Option<mpsc::SyncSender<()>>,
        release: mpsc::Receiver<()>,
        input: std::io::Cursor<Vec<u8>>,
    }
    impl Read for HeldReader {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if let Some(entered) = self.entered.take() {
                entered.send(()).unwrap();
                self.release.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            self.input.read(out)
        }
    }
    let (_root, _authority, budget, metadata, audio) = shared_fixture(Some(80 * 1024 * 1024));
    let lease = audio.lease().unwrap();
    let request = audio.begin(&lease, 101).unwrap();
    let (entered, started) = mpsc::sync_channel(0);
    let (release, released) = mpsc::sync_channel(0);
    let worker = audio.clone();
    let writer = thread::spawn(move || {
        worker.put_reader(
            request,
            HeldReader {
                entered: Some(entered),
                release: released,
                input: std::io::Cursor::new(SyntheticAudio::tone(100_000).unwrap().0),
            },
        )
    });
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    for marker in 0..12u8 {
        use crate::music::{
            Track,
            cache_codec::{self, MetadataRecord},
        };
        let id = (1000 + u16::from(marker)).to_string();
        let key = crate::cache::CacheKey::new(crate::cache::EntryKind::Track, &id, 0).unwrap();
        let record = cache_codec::encode(
            crate::cache::EntryKind::Track,
            &MetadataRecord::Track(Track {
                id,
                title: "Generated metadata".into(),
                artists: vec!["Original fixture".into()],
                artist_links: Vec::new(),
                album_id: None,
                album_title: None,
                duration_ms: Some(1000),
                cover: None,
                explicit: false,
                available: true,
            }),
        )
        .unwrap();
        metadata
            .put(&metadata.lease().unwrap(), &key, &record)
            .unwrap();
    }
    let key =
        crate::cache::CacheKey::new(crate::cache::EntryKind::Artwork, "shared-cover", 0).unwrap();
    metadata
        .put(&metadata.lease().unwrap(), &key, &shared_artwork(33))
        .unwrap();
    assert_eq!(metadata.usage().unwrap().entries, 13);
    assert!(budget.usage().unwrap().bytes <= 80 * 1024 * 1024);
    budget.configure(false, Some(80 * 1024 * 1024)).unwrap();
    release.send(()).unwrap();
    assert_eq!(writer.join().unwrap(), Err(CacheError::Cancelled));
    assert_eq!(audio.stats().unwrap().entries, 0);
    assert_eq!(
        budget.usage().unwrap().bytes,
        metadata.usage().unwrap().bytes
    );
    budget.configure(true, Some(80 * 1024 * 1024)).unwrap();
    assert!(matches!(
        audio.begin(&lease, 102),
        Err(CacheError::Cancelled)
    ));
    let lease = audio.lease().unwrap();
    put(&audio, &lease, 102, 100_000);
    assert_eq!(
        budget.usage().unwrap().bytes,
        metadata.usage().unwrap().bytes + audio.stats().unwrap().bytes
    );
}

#[test]
fn paused_writer_preserves_completed_reads_and_live_stage_during_maintenance() {
    struct Paused {
        entered: Option<mpsc::SyncSender<()>>,
        resume: mpsc::Receiver<()>,
        original: std::io::Cursor<Vec<u8>>,
    }
    impl Read for Paused {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if let Some(entered) = self.entered.take() {
                entered.send(()).unwrap();
                self.resume
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?;
            }
            self.original.read(out)
        }
    }
    let (sandbox, _, cache, lease) = fixture(100_000);
    put(&cache, &lease, SECOND.track, 400);
    let original = SyntheticAudio::tone(800).unwrap().0;
    let expected = original.clone();
    let request = cache.begin(&lease, FIRST.track).unwrap();
    let (entered, waiting) = mpsc::sync_channel(0);
    let (resume, resumed) = mpsc::channel();
    let worker = cache.clone();
    let writer = thread::spawn(move || {
        worker.put_reader(
            request,
            Paused {
                entered: Some(entered),
                resume: resumed,
                original: std::io::Cursor::new(original),
            },
        )
    });
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let stage = fs::read_dir(&sandbox.path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .as_encoded_bytes()
                .starts_with(b".part-")
        })
        .unwrap();
    let read = cache.get(&lease, SECOND.track);
    let stats = cache.stats();
    let trimmed = cache.trim();
    let live_stage_preserved = stage.join("media.bin").is_file();
    let second_writer = cache.put_reader(
        cache.begin(&lease, SECOND.track).unwrap(),
        SyntheticAudio::tone(400).unwrap().0.as_slice(),
    );
    // Always release and join before asserting, including against the old
    // implementation that returned Busy for every read/maintenance operation.
    resume.send(()).unwrap();
    let result = writer.join().unwrap();
    assert_eq!(
        &*read.unwrap().unwrap().admit().unwrap(),
        &SyntheticAudio::tone(400).unwrap().0
    );
    assert_eq!(stats.unwrap().entries, 1);
    assert_eq!(trimmed.unwrap().entries, 1);
    assert!(live_stage_preserved, "a live source is not a crash orphan");
    assert_eq!(second_writer, Err(CacheError::Busy));
    result.unwrap();
    assert_eq!(&*sample(&cache, &lease, FIRST.track), &expected);
    assert_eq!(cache.stats().unwrap().entries, 2);
    assert!(!stage.exists());
}

#[test]
fn clear_unlinks_a_waiting_download_and_lease_checks_finish_it_without_new_bytes() {
    struct WaitingDownload {
        cache: AudioCache,
        lease: Lease,
        permit: StorageGrant,
        entered: Option<mpsc::SyncSender<()>>,
    }
    impl Read for WaitingDownload {
        fn read(&mut self, _out: &mut [u8]) -> std::io::Result<usize> {
            if let Some(entered) = self.entered.take() {
                entered.send(()).unwrap();
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.cache.check(&self.lease, self.permit).is_ok() {
                if Instant::now() >= deadline {
                    return Err(std::io::ErrorKind::TimedOut.into());
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(std::io::ErrorKind::Interrupted.into())
        }
    }
    let (_root, _authority, budget, _metadata, cache) =
        shared_fixture(Some(2 * 1024 * 1024 * 1024));
    let lease = cache.lease().unwrap();
    put(&cache, &lease, 102, 400);
    let active_file = cache
        .get(&lease, 102u64)
        .unwrap()
        .unwrap()
        .admit_file()
        .unwrap();
    let request = cache.begin(&lease, 101u64).unwrap();
    let (entered, waiting) = mpsc::sync_channel(0);
    let reader = WaitingDownload {
        cache: cache.clone(),
        lease: request.lease.clone(),
        permit: request.permit,
        entered: Some(entered),
    };
    let (completed, completion) = mpsc::channel();
    let worker = cache.clone();
    let writer = thread::spawn(move || {
        completed.send(worker.put_reader(request, reader)).unwrap();
    });
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    let cleared = cache.clear();
    let empty_before_writer_exit = fs::read_dir(&cache.0.sandbox.path)
        .unwrap()
        .all(|entry| entry.unwrap().file_name() == ".lock");
    // No extra body byte or native EOS is needed to observe Clear's lease fence.
    let result = completion.recv_timeout(Duration::from_secs(2)).unwrap();
    writer.join().unwrap();
    assert_eq!(cleared.unwrap(), Stats::default());
    assert!(empty_before_writer_exit);
    assert_eq!(result, Err(CacheError::Cancelled));
    assert_eq!(budget.usage().unwrap().bytes, 0);
    let mut admitted = active_file.as_ref().try_clone().unwrap();
    let mut bytes = Vec::new();
    admitted.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, SyntheticAudio::tone(400).unwrap().0);
    let fresh = cache.lease().unwrap();
    put(&cache, &fresh, 103, 400);
    assert_eq!(cache.stats().unwrap().entries, 1);
}
