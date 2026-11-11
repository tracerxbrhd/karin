// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::queue::{ContextKind, PlaybackContext, RepeatMode};
use std::{os::unix::fs::symlink, sync::mpsc, thread};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let mut bytes = [0; 16];
        SystemRandom::new().fill(&mut bytes).unwrap();
        let suffix = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let directory = std::env::temp_dir().join(format!("karin-playback-{suffix}"));
        DirBuilder::new().mode(0o700).create(&directory).unwrap();
        Self(directory)
    }
    fn store(&self) -> PlaybackSessionStore {
        PlaybackSessionStore::at_directory(self.0.join("application/playback")).unwrap()
    }
    fn path(&self) -> PathBuf {
        self.store().path_for_test(&scope())
    }
    fn write(&self, bytes: &[u8]) {
        std::fs::write(self.path(), bytes).unwrap();
        std::fs::set_permissions(self.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn put(&self, snapshot: &PlaybackSnapshot) {
        let store = self.store();
        assert!(
            store
                .save_if_current(&store.reserve_save(&scope()).unwrap(), Some(snapshot))
                .unwrap()
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn scope() -> AccountScope {
    AccountScope::from_account_id("101").unwrap()
}
fn track(id: &str) -> Track {
    Track {
        id: id.into(),
        title: "Private synthetic title".into(),
        artists: vec!["Private synthetic credit".into()],
        artist_links: vec![],
        album_id: None,
        album_title: None,
        duration_ms: Some(180_000),
        cover: None,
        explicit: false,
        available: true,
    }
}
fn snapshot(position: u64) -> PlaybackSnapshot {
    let mut queue = MusicQueue::with_seed(231);
    queue.replace_context(
        PlaybackContext::new(
            ContextKind::LikedTracks,
            None,
            vec![track("one"), track("two"), track("one")],
            2,
            true,
        )
        .unwrap(),
    );
    queue.set_shuffle(true);
    queue.set_repeat_mode(RepeatMode::Queue);
    PlaybackSnapshot::capture(&queue, Duration::from_secs(position))
        .unwrap()
        .unwrap()
}
fn frame(body: &[u8]) -> Vec<u8> {
    [MAGIC.as_slice(), digest(&SHA256, body).as_ref(), body].concat()
}

#[test]
fn private_account_bound_roundtrip_survives_reopen_without_display_or_service_secrets() {
    let fixture = Fixture::new();
    let store = fixture.store();
    assert!(
        !fixture.path().parent().unwrap().exists(),
        "construction must perform no I/O"
    );
    assert_eq!(store.load(&scope()).unwrap().status, LoadStatus::Missing);
    let saved = snapshot(102);
    let ticket = store.reserve_save(&scope()).unwrap();
    assert!(store.save_if_current(&ticket, Some(&saved)).unwrap());
    let reopened = fixture.store().load(&scope()).unwrap();
    assert_eq!(reopened.status, LoadStatus::Loaded);
    assert_eq!(reopened.snapshot, Some(saved.clone()));
    let other = AccountScope::from_account_id("102").unwrap();
    assert_eq!(store.load(&other).unwrap().status, LoadStatus::Missing);
    let file = std::fs::metadata(fixture.path()).unwrap();
    let directory = std::fs::metadata(fixture.path().parent().unwrap()).unwrap();
    assert_eq!(file.permissions().mode() & 0o7777, 0o600);
    assert_eq!(directory.permissions().mode() & 0o7777, 0o700);
    assert_eq!(file.uid(), rustix::process::geteuid().as_raw());
    assert_eq!(file.nlink(), 1);
    assert_eq!(
        std::fs::read_dir(fixture.path().parent().unwrap())
            .unwrap()
            .count(),
        1
    );
    let bytes = std::fs::read(fixture.path()).unwrap();
    let json = std::str::from_utf8(&bytes[40..]).unwrap();
    for forbidden in [
        "Private synthetic title",
        "Private synthetic credit",
        "access_token",
        "cookie",
        "https:",
        "wave_session",
    ] {
        assert!(!json.contains(forbidden));
        assert!(!format!("{saved:?} {store:?} {ticket:?}").contains(forbidden));
    }
    assert_eq!(
        fixture.path().file_name().unwrap().to_str().unwrap().len(),
        69
    );
    assert_ne!(
        fixture.path().file_name().unwrap().to_str().unwrap(),
        "101.json"
    );
    let normalized = [track("one"), track("two")];
    let restored = reopened.snapshot.unwrap().restore(&normalized).unwrap();
    assert_eq!(restored.position, Duration::from_secs(102));
    assert_eq!(restored.queue.current_index(), Some(2));
    assert_eq!(restored.queue.items().len(), 3);
    assert_eq!(restored.queue.current().unwrap().title, normalized[0].title);
}

#[test]
fn checksum_version_shape_and_account_corruption_never_admit_personal_intent() {
    let fixture = Fixture::new();
    fixture.put(&snapshot(42));
    let valid = std::fs::read(fixture.path()).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&valid[40..]).unwrap();
    let mut cases = vec![
        vec![],
        valid[..39].to_vec(),
        vec![b'x'; MAX_FILE as usize + 1],
    ];
    let mut damaged = valid.clone();
    damaged[45] ^= 1;
    cases.push(damaged);
    let mut wrong_magic = valid.clone();
    wrong_magic[0] ^= 1;
    cases.push(wrong_magic);
    for modify in [
        |value: &mut serde_json::Value| value["version"] = 99.into(),
        |value: &mut serde_json::Value| {
            value["account"][0] = ((value["account"][0].as_u64().unwrap() + 1) % 256).into()
        },
        |value: &mut serde_json::Value| {
            value["snapshot"]["position_ms"] = (MAX_POSITION_MS + 1).into()
        },
        |value: &mut serde_json::Value| value["snapshot"]["queue"]["current"] = 500.into(),
        |value: &mut serde_json::Value| {
            value["snapshot"]["access_token"] = "synthetic-never-admitted".into()
        },
        |value: &mut serde_json::Value| {
            value["snapshot"]["queue"]["origin"]["cookie"] = "synthetic-never-admitted".into()
        },
    ] {
        let mut invalid = body.clone();
        modify(&mut invalid);
        cases.push(frame(&serde_json::to_vec(&invalid).unwrap()));
    }
    for (case, invalid) in cases.into_iter().enumerate() {
        fixture.write(&invalid);
        let loaded = fixture.store().load(&scope()).unwrap();
        assert_eq!(
            loaded.status,
            LoadStatus::RecoveredCorrupt,
            "corruption case {case}"
        );
        assert!(loaded.snapshot.is_none());
    }
    fixture.write(&valid);
    let other = AccountScope::from_account_id("202").unwrap();
    let store = fixture.store();
    std::fs::copy(fixture.path(), store.path_for_test(&other)).unwrap();
    let loaded = store.load(&other).unwrap();
    assert_eq!(loaded.status, LoadStatus::RecoveredCorrupt);
    assert!(loaded.snapshot.is_none());
}

#[test]
fn newest_reservation_wins_and_durable_forget_cannot_be_undone_by_late_close() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let old = store.reserve_save(&scope()).unwrap();
    let current = store.reserve_save(&scope()).unwrap();
    assert!(!store.save_if_current(&old, Some(&snapshot(1))).unwrap());
    assert!(store.save_if_current(&current, Some(&snapshot(2))).unwrap());
    assert_eq!(store.load(&scope()).unwrap().snapshot, Some(snapshot(2)));
    let late_close = store.reserve_save(&scope()).unwrap();
    let forget = store.reserve_save(&scope()).unwrap();
    assert!(store.save_if_current(&forget, None).unwrap());
    assert!(
        !store
            .save_if_current(&late_close, Some(&snapshot(3)))
            .unwrap()
    );
    let reopened = fixture.store().load(&scope()).unwrap();
    assert_eq!(reopened.status, LoadStatus::Cleared);
    assert!(reopened.snapshot.is_none());
    let bytes = std::fs::read(fixture.path()).unwrap();
    let json = std::str::from_utf8(&bytes[40..]).unwrap();
    assert!(!json.contains("one"));
    assert!(!json.contains("two"));
    let after_logout = store.reserve_save(&scope()).unwrap();
    store.invalidate().unwrap();
    assert!(
        !store
            .save_if_current(&after_logout, Some(&snapshot(4)))
            .unwrap()
    );
    assert_eq!(store.load(&scope()).unwrap().status, LoadStatus::Cleared);
}

#[test]
fn replacement_window_load_waits_for_final_close_write_on_shared_store() {
    let fixture = Fixture::new();
    let old_window = fixture.store();
    let new_window = fixture.store();
    assert!(Arc::ptr_eq(&old_window.0, &new_window.0));
    fixture.put(&snapshot(1));
    let final_save = old_window.reserve_save(&scope()).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        started_tx.send(()).unwrap();
        done_tx
            .send(new_window.load(&scope()).map(|loaded| loaded.snapshot))
            .unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(
        done_rx.recv_timeout(Duration::from_millis(40)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    assert!(
        old_window
            .save_if_current(&final_save, Some(&snapshot(88)))
            .unwrap()
    );
    assert_eq!(
        done_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap(),
        Some(snapshot(88))
    );
    worker.join().unwrap();
}

#[test]
fn abandoned_tickets_and_errors_release_load_fences_but_foreign_tickets_do_not() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let abandoned = store.reserve_save(&scope()).unwrap();
    let clone = abandoned.clone();
    drop(abandoned);
    assert!(store.0.pending.lock().unwrap().is_some());
    drop(clone);
    assert!(store.0.pending.lock().unwrap().is_none());
    assert_eq!(store.load(&scope()).unwrap().status, LoadStatus::Missing);
    let another = Fixture::new();
    let foreign_store = another.store();
    let foreign = foreign_store.reserve_save(&scope()).unwrap();
    let valid = store.reserve_save(&scope()).unwrap();
    let pending = *store.0.pending.lock().unwrap();
    assert!(!store.save_if_current(&foreign, None).unwrap());
    assert_eq!(*store.0.pending.lock().unwrap(), pending);
    assert!(store.save_if_current(&valid, Some(&snapshot(12))).unwrap());
    assert_eq!(store.load(&scope()).unwrap().status, LoadStatus::Loaded);

    let mut invalid = snapshot(0);
    invalid.position_ms = MAX_POSITION_MS + 1;
    let invalid_ticket = store.reserve_save(&scope()).unwrap();
    assert_eq!(
        store.save_if_current(&invalid_ticket, Some(&invalid)),
        Err(SnapshotError::InvalidSnapshot)
    );
    assert!(store.0.pending.lock().unwrap().is_none());
    assert_eq!(store.load(&scope()).unwrap().snapshot, Some(snapshot(12)));
}

#[test]
fn symlinks_hardlinks_fifos_and_nonprivate_records_are_refused_without_modification() {
    let fixture = Fixture::new();
    fixture.put(&snapshot(12));
    let victim = fixture.0.join("unrelated");
    std::fs::write(&victim, b"must stay untouched").unwrap();
    std::fs::set_permissions(&victim, std::fs::Permissions::from_mode(0o600)).unwrap();
    let store = fixture.store();
    std::fs::remove_file(fixture.path()).unwrap();
    symlink(&victim, fixture.path()).unwrap();
    assert_eq!(
        store.load(&scope()).err(),
        Some(SnapshotError::UnsafeLocation)
    );
    assert_eq!(
        store.save_if_current(&store.reserve_save(&scope()).unwrap(), None),
        Err(SnapshotError::UnsafeLocation)
    );
    assert_eq!(std::fs::read(&victim).unwrap(), b"must stay untouched");
    std::fs::remove_file(fixture.path()).unwrap();
    std::fs::hard_link(&victim, fixture.path()).unwrap();
    assert_eq!(
        store.load(&scope()).err(),
        Some(SnapshotError::UnsafeLocation)
    );
    assert_eq!(
        store.save_if_current(&store.reserve_save(&scope()).unwrap(), None),
        Err(SnapshotError::UnsafeLocation)
    );
    std::fs::remove_file(fixture.path()).unwrap();
    let directory = File::open(fixture.path().parent().unwrap()).unwrap();
    rustix::fs::mkfifoat(
        &directory,
        name(account_key(&scope())).as_str(),
        Mode::RUSR | Mode::WUSR,
    )
    .unwrap();
    assert_eq!(
        store.load(&scope()).err(),
        Some(SnapshotError::UnsafeLocation)
    );
    std::fs::remove_file(fixture.path()).unwrap();
    fixture.put(&snapshot(12));
    std::fs::set_permissions(fixture.path(), std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        store.load(&scope()).err(),
        Some(SnapshotError::UnsafeLocation)
    );
    assert_eq!(
        store.save_if_current(&store.reserve_save(&scope()).unwrap(), None),
        Err(SnapshotError::UnsafeLocation)
    );
    std::fs::set_permissions(fixture.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(store.load(&scope()).unwrap().status, LoadStatus::Loaded);
    assert_eq!(
        std::fs::read_dir(fixture.path().parent().unwrap())
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn unsafe_directory_paths_are_rejected_and_parent_symlinks_remain_linux_compatible() {
    assert!(matches!(
        PlaybackSessionStore::at_directory("relative/path"),
        Err(SnapshotError::UnsafeLocation)
    ));
    assert!(matches!(
        PlaybackSessionStore::at_directory("/tmp/../private"),
        Err(SnapshotError::UnsafeLocation)
    ));
    let fixture = Fixture::new();
    let public = fixture.0.join("public");
    DirBuilder::new().mode(0o755).create(&public).unwrap();
    std::fs::set_permissions(&public, std::fs::Permissions::from_mode(0o755)).unwrap();
    let store = PlaybackSessionStore::at_directory(&public).unwrap();
    assert_eq!(
        store.load(&scope()).err(),
        Some(SnapshotError::UnsafeLocation)
    );
    let link = fixture.0.join("link");
    symlink(&public, &link).unwrap();
    assert_eq!(
        PlaybackSessionStore::at_directory(link)
            .unwrap()
            .load(&scope())
            .err(),
        Some(SnapshotError::UnsafeLocation)
    );
    let home_alias = fixture.0.join("home-alias");
    symlink(&public, &home_alias).unwrap();
    let store =
        PlaybackSessionStore::at_directory(home_alias.join("application/playback")).unwrap();
    assert!(
        store
            .save_if_current(&store.reserve_save(&scope()).unwrap(), Some(&snapshot(7)))
            .unwrap()
    );
    assert_eq!(store.load(&scope()).unwrap().snapshot, Some(snapshot(7)));
}

#[test]
fn current_loss_resets_seek_and_fresh_duration_bounds_saved_position() {
    let original = snapshot(102);
    let mut short = track("one");
    short.duration_ms = Some(20_000);
    let restored = original.restore(&[short, track("two")]).unwrap();
    assert_eq!(restored.position, Duration::from_millis(19_750));
    let restored = original.restore(&[track("two")]).unwrap();
    assert!(restored.current_changed);
    assert_eq!(restored.position, Duration::ZERO);
    assert_eq!(restored.omitted, 2);
    assert_eq!(restored.queue.items().len(), 1);
    let restored = original.restore(&[]).unwrap();
    assert!(restored.queue.items().is_empty());
    assert_eq!(restored.position, Duration::ZERO);
}

#[test]
fn account_unknown_forget_repair_removes_all_private_playback_records_and_fences_late_writes() {
    let fixture = Fixture::new();
    let store = fixture.store();
    fixture.put(&snapshot(21));
    let other = AccountScope::from_account_id("303").unwrap();
    assert!(
        store
            .save_if_current(&store.reserve_save(&other).unwrap(), Some(&snapshot(22)))
            .unwrap()
    );
    let orphan = fixture
        .path()
        .parent()
        .unwrap()
        .join(format!(".playback-{}.tmp", "a".repeat(32)));
    std::fs::write(&orphan, b"interrupted staging write").unwrap();
    std::fs::set_permissions(&orphan, std::fs::Permissions::from_mode(0o600)).unwrap();
    let obsolete = store.reserve_save(&scope()).unwrap();
    store.clear_all().unwrap();
    assert!(
        !store
            .save_if_current(&obsolete, Some(&snapshot(23)))
            .unwrap()
    );
    assert!(!orphan.exists());
    assert_eq!(store.load(&scope()).unwrap().status, LoadStatus::Missing);
    assert_eq!(store.load(&other).unwrap().status, LoadStatus::Missing);
    assert_eq!(
        std::fs::read_dir(fixture.path().parent().unwrap())
            .unwrap()
            .count(),
        0
    );
    assert!(
        store
            .save_if_current(&store.reserve_save(&scope()).unwrap(), Some(&snapshot(24)))
            .unwrap()
    );
    assert_eq!(store.load(&scope()).unwrap().snapshot, Some(snapshot(24)));
}

#[test]
fn account_unknown_forget_repair_refuses_unexpected_entries_before_deleting_any_record() {
    let fixture = Fixture::new();
    let store = fixture.store();
    fixture.put(&snapshot(11));
    let unrelated = fixture.path().parent().unwrap().join("unrelated");
    std::fs::write(&unrelated, b"not an application playback record").unwrap();
    assert_eq!(store.clear_all(), Err(SnapshotError::UnsafeLocation));
    assert_eq!(
        std::fs::read(&unrelated).unwrap(),
        b"not an application playback record"
    );
    assert_eq!(store.load(&scope()).unwrap().snapshot, Some(snapshot(11)));
    std::fs::remove_file(&unrelated).unwrap();
    let alias = fixture
        .path()
        .parent()
        .unwrap()
        .join(format!("{}.json", "a".repeat(64)));
    symlink(fixture.path(), &alias).unwrap();
    assert_eq!(store.clear_all(), Err(SnapshotError::UnsafeLocation));
    assert_eq!(store.load(&scope()).unwrap().snapshot, Some(snapshot(11)));
    std::fs::remove_file(alias).unwrap();
    store.clear_all().unwrap();
    assert_eq!(store.load(&scope()).unwrap().status, LoadStatus::Missing);
}
