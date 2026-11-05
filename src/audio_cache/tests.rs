// SPDX-License-Identifier: GPL-3.0-or-later
//! All audio below is generated PCM; no account or service resource is used.
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
    let cache = AudioCache::open(sandbox.clone(), authority.clone(), FIRST.device, budget).unwrap();
    let lease = cache.bind(Some(FIRST.account)).unwrap().unwrap();
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
    assert_eq!(format!("{hit:?}"), "SyntheticAudioHit([opaque])");
}

#[test]
fn corrupted_truncated_and_swapped_records_become_misses() {
    let (_, _, cache, lease) = fixture(100_000);
    for failure in 0..4 {
        put(&cache, &lease, FIRST.track, 400);
        let path = cache.path(FIRST.key());
        match failure {
            0 => fs::write(path.join("audio.wav"), b"partial").unwrap(),
            1 => fs::write(path.join("header"), b"partial").unwrap(),
            2 => {
                let mut bytes = fs::read(path.join("audio.wav")).unwrap();
                bytes[50] ^= 1;
                fs::write(path.join("audio.wav"), bytes).unwrap();
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
    write_private(&partial.join("audio.wav"), b"incomplete synthetic record").unwrap();
    drop(cache);
    let cache = AudioCache::open(sandbox.clone(), authority, 11, 100_000).unwrap();
    let lease = cache.bind(Some(1)).unwrap().unwrap();
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
            cache.bind(Some(2))?;
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
    let cache = AudioCache::open(sandbox, authority.clone(), 11, one * 2).unwrap();
    let lease = cache.bind(Some(1)).unwrap().unwrap();
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
    let second = cache.bind(Some(2)).unwrap().unwrap();
    assert!(cache.get(&second, FIRST.track).unwrap().is_none());
    assert!(matches!(
        cache.get(&first, FIRST.track),
        Err(CacheError::Cancelled)
    ));
    assert_eq!(hit.admit().unwrap_err(), CacheError::Cancelled);
    drop(cache);
    let other = AudioCache::open(sandbox, authority.clone(), 12, 100_000).unwrap();
    let other_lease = other.bind(Some(1)).unwrap().unwrap();
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
    let fresh = cache.bind(Some(1)).unwrap().unwrap();
    assert!(cache.get(&fresh, SECOND.track).unwrap().is_none());
    assert_eq!(fs::read_dir(&sandbox.path).unwrap().count(), 1);
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
        AudioCache::open(sandbox.clone(), authority, 11, 100_000),
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
        AudioCache::open(sandbox.clone(), authority.clone(), 11, 100_000),
        Err(CacheError::Busy)
    ));
    cache.clear().unwrap();
    assert_eq!(old_hit.admit().unwrap_err(), CacheError::Cancelled);
    drop(cache);
    let reopened = AudioCache::open(sandbox, authority, 11, 100_000).unwrap();
    let current = reopened.bind(Some(1)).unwrap().unwrap();
    assert_eq!(
        reopened.put(old_request, &SyntheticAudio::tone(400).unwrap()),
        Err(CacheError::Cancelled)
    );
    assert!(reopened.get(&current, SECOND.track).unwrap().is_none());
    assert_eq!(old_hit.admit().unwrap_err(), CacheError::Cancelled);
}

#[test]
fn active_owner_recovers_orphan_bytes_before_quota_or_stats() {
    let one = (HEADER + SyntheticAudio::tone(400).unwrap().0.len()) as u64;
    let (sandbox, _, cache, lease) = fixture(one);
    let orphan = sandbox.path.join(format!(".part-{}", "b".repeat(32)));
    fs::DirBuilder::new().mode(0o700).create(&orphan).unwrap();
    write_private(
        &orphan.join("audio.wav"),
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
    let cache = AudioCache::open(sandbox.clone(), authority, 11, 100_000).unwrap();
    let lease = cache.bind(Some(1)).unwrap().unwrap();
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
    let cache = AudioCache::open(sandbox, authority.clone(), 11, 100_000).unwrap();
    let lease = cache.bind(Some(1)).unwrap().unwrap();
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
        .prepare(PlaybackSource::local_file(cache.path(FIRST.key()).join("audio.wav")).unwrap())
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
