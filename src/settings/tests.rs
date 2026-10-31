// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::{os::unix::fs::symlink, thread};

struct Fixture {
    directory: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let mut random = [0; 16];
        SystemRandom::new().fill(&mut random).unwrap();
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let directory = std::env::temp_dir().join(format!("karin-settings-{suffix}"));
        DirBuilder::new().mode(0o700).create(&directory).unwrap();
        Self { directory }
    }

    fn store(&self) -> SettingsStore {
        SettingsStore::at_directory(self.directory.join("application")).unwrap()
    }

    fn file(&self) -> PathBuf {
        self.directory.join("application").join(FILE_NAME)
    }

    fn replace(&self, bytes: &[u8]) {
        std::fs::write(self.file(), bytes).unwrap();
        std::fs::set_permissions(self.file(), std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn persistent_preferences_roundtrip_preserves_private_modes_and_all_choices() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let missing = store.load().unwrap();
    assert_eq!(missing, defaults(LoadStatus::Missing));
    assert!(!fixture.file().exists());
    assert!(!fixture.file().parent().unwrap().exists());
    let preferences = Preferences {
        appearance: Appearance::Dark,
        remember_login: false,
        forget_pending: true,
        cache_enabled: false,
        cache_limit_bytes: 512 * 1024 * 1024,
        track_info_open: false,
    };
    store.save(&preferences).unwrap();
    let reopened = fixture.store().load().unwrap();
    assert_eq!(reopened.preferences, preferences);
    assert_eq!(reopened.status, LoadStatus::Loaded);
    let file = std::fs::metadata(fixture.file()).unwrap();
    let directory = std::fs::metadata(fixture.file().parent().unwrap()).unwrap();
    assert_eq!(file.permissions().mode() & 0o7777, 0o600);
    assert_eq!(directory.permissions().mode() & 0o7777, 0o700);
    assert_eq!(file.uid(), rustix::process::geteuid().as_raw());
    assert_eq!(file.nlink(), 1);
    assert_eq!(
        std::fs::read_dir(fixture.file().parent().unwrap())
            .unwrap()
            .count(),
        1,
        "committed writes leave no temporary preference files"
    );
    assert!(!format!("{store:?}").contains("karin-settings-"));
}

#[test]
fn malformed_oversized_unknown_and_out_of_range_settings_recover_with_defaults() {
    let fixture = Fixture::new();
    let store = fixture.store();
    store.save(&Preferences::default()).unwrap();
    let invalid = [
        b"{".to_vec(),
        b"null".to_vec(),
        br#"{"version":2,"preferences":{}}"#.to_vec(),
        br#"{"version":1,"preferences":{"appearance":"unsupported"}}"#.to_vec(),
        br#"{"version":1,"preferences":{"cache_limit_bytes":0}}"#.to_vec(),
        br#"{"version":1,"preferences":{"cache_limit_bytes":18446744073709551615}}"#.to_vec(),
        br#"{"version":1,"preferences":{"access_token":"synthetic-never-accepted"}}"#.to_vec(),
        vec![b' '; MAX_FILE_BYTES as usize + 1],
    ];
    for bytes in invalid {
        fixture.replace(&bytes);
        assert_eq!(
            store.load().unwrap(),
            defaults(LoadStatus::RecoveredCorrupt)
        );
    }
    store
        .update(|preferences| preferences.appearance = Appearance::Light)
        .unwrap();
    assert_eq!(store.load().unwrap().status, LoadStatus::Loaded);
    assert_eq!(
        store.load().unwrap().preferences.appearance,
        Appearance::Light
    );
    let persisted = std::fs::read_to_string(fixture.file()).unwrap();
    assert!(!persisted.contains("synthetic-never-accepted"));
}

#[test]
fn omitted_new_preferences_use_safe_defaults_without_accepting_unknown_fields() {
    let fixture = Fixture::new();
    let store = fixture.store();
    store.save(&Preferences::default()).unwrap();
    fixture.replace(br#"{"version":1,"preferences":{"appearance":"light"}}"#);
    let loaded = store.load().unwrap();
    assert_eq!(loaded.status, LoadStatus::Loaded);
    assert_eq!(loaded.preferences.appearance, Appearance::Light);
    assert!(loaded.preferences.remember_login && loaded.preferences.cache_enabled);
    assert!(!loaded.preferences.forget_pending);
    assert_eq!(loaded.preferences.cache_limit_bytes, DEFAULT_CACHE_BYTES);
}

#[test]
fn unsafe_links_permissions_and_nonregular_files_never_get_followed_or_overwritten() {
    let fixture = Fixture::new();
    let store = fixture.store();
    store.save(&Preferences::default()).unwrap();
    let victim = fixture.directory.join("outside-file");
    std::fs::write(&victim, b"unchanged unrelated material").unwrap();
    std::fs::remove_file(fixture.file()).unwrap();
    symlink(&victim, fixture.file()).unwrap();
    assert_eq!(store.load(), Err(SettingsError::UnsafeLocation));
    assert_eq!(
        store.save(&Preferences::default()),
        Err(SettingsError::UnsafeLocation)
    );
    assert_eq!(
        std::fs::read(&victim).unwrap(),
        b"unchanged unrelated material"
    );
    std::fs::remove_file(fixture.file()).unwrap();
    std::fs::hard_link(&victim, fixture.file()).unwrap();
    std::fs::set_permissions(&victim, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(store.load(), Err(SettingsError::UnsafeLocation));
    assert_eq!(
        store.save(&Preferences::default()),
        Err(SettingsError::UnsafeLocation)
    );
    std::fs::remove_file(fixture.file()).unwrap();
    std::fs::create_dir(fixture.file()).unwrap();
    assert_eq!(store.load(), Err(SettingsError::UnsafeLocation));
    std::fs::remove_dir(fixture.file()).unwrap();
    store.save(&Preferences::default()).unwrap();
    std::fs::set_permissions(fixture.file(), std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(store.load(), Err(SettingsError::UnsafeLocation));
    std::fs::set_permissions(fixture.file(), std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::set_permissions(
        fixture.file().parent().unwrap(),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert_eq!(store.load(), Err(SettingsError::UnsafeLocation));

    let linked = fixture.directory.join("linked-application");
    symlink(fixture.file().parent().unwrap(), &linked).unwrap();
    let linked_store = SettingsStore::at_directory(linked).unwrap();
    assert_eq!(linked_store.load(), Err(SettingsError::UnsafeLocation));
    assert!(SettingsStore::at_directory("relative").is_err());
    assert!(SettingsStore::at_directory(fixture.directory.join("../other")).is_err());
}

#[test]
fn damaged_fifo_is_rejected_before_it_can_block_read_or_save() {
    use std::{sync::mpsc, time::Duration};
    let fixture = Fixture::new();
    let store = fixture.store();
    store.save(&Preferences::default()).unwrap();
    std::fs::remove_file(fixture.file()).unwrap();
    let directory = File::open(fixture.file().parent().unwrap()).unwrap();
    rustix::fs::mkfifoat(&directory, FILE_NAME, Mode::RUSR | Mode::WUSR).unwrap();

    for writing in [false, true] {
        let worker_store = store.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result = if writing {
                worker_store.save(&Preferences::default())
            } else {
                worker_store.load().map(|_| ())
            };
            sender.send(result).unwrap();
        });
        let completed = receiver.recv_timeout(Duration::from_secs(1));
        if completed.is_err() {
            // Bound the regression itself: an old blocking reader can be
            // released by a nonblocking writer before the assertion fails.
            let writer = rustix::fs::openat(
                &directory,
                FILE_NAME,
                OFlags::WRONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .expect("release a regressed blocking FIFO reader");
            drop(writer);
        }
        worker.join().unwrap();
        assert_eq!(completed, Ok(Err(SettingsError::UnsafeLocation)));
    }
}

#[test]
fn failed_validation_preserves_the_complete_previous_settings_file() {
    let fixture = Fixture::new();
    let store = fixture.store();
    store.save(&Preferences::default()).unwrap();
    let original = std::fs::read(fixture.file()).unwrap();
    let invalid = Preferences {
        cache_limit_bytes: MAX_CACHE_BYTES + 1,
        ..Preferences::default()
    };
    assert_eq!(store.save(&invalid), Err(SettingsError::InvalidPreferences));
    assert_eq!(std::fs::read(fixture.file()).unwrap(), original);
}

#[test]
fn concurrent_updates_preserve_independent_preferences_and_atomic_file_shape() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let appearance_store = store.clone();
    let appearance = thread::spawn(move || {
        appearance_store
            .update(|preferences| preferences.appearance = Appearance::Dark)
            .unwrap();
    });
    let storage_store = store.clone();
    let storage = thread::spawn(move || {
        storage_store
            .update(|preferences| preferences.cache_enabled = false)
            .unwrap();
    });
    appearance.join().unwrap();
    storage.join().unwrap();
    let reopened = fixture.store().load().unwrap();
    assert_eq!(reopened.preferences.appearance, Appearance::Dark);
    assert!(!reopened.preferences.cache_enabled);
    assert_eq!(reopened.status, LoadStatus::Loaded);
    assert_eq!(
        std::fs::read_dir(fixture.file().parent().unwrap())
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn latest_ui_reservation_preserves_logout_barrier_against_stale_worker_saves() {
    let fixture = Fixture::new();
    let store = fixture.store();
    store.save(&Preferences::default()).unwrap();
    let stale = store.reserve_save().unwrap();
    let pending_logout = Preferences {
        forget_pending: true,
        ..Preferences::default()
    };
    let latest = store.reserve_save().unwrap();
    assert!(store.save_if_current(&latest, &pending_logout).unwrap());
    assert!(
        !store
            .save_if_current(&stale, &Preferences::default())
            .unwrap()
    );
    assert!(fixture.store().load().unwrap().preferences.forget_pending);
    let other = fixture.store();
    assert!(
        !other
            .save_if_current(&latest, &Preferences::default())
            .unwrap()
    );
    assert!(store.load().unwrap().preferences.forget_pending);
    let completed = store.reserve_save().unwrap();
    assert!(
        store
            .save_if_current(&completed, &Preferences::default())
            .unwrap()
    );
    assert!(!fixture.store().load().unwrap().preferences.forget_pending);
}
