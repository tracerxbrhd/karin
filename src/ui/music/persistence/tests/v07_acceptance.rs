// SPDX-License-Identifier: GPL-3.0-or-later
//! Three production paths in one profile, across actual OS process restarts.
//!
//! Karin contributors dedicate the original generated audio to CC0 1.0:
//! https://creativecommons.org/publicdomain/zero/1.0/. Only the protected wallet
//! is substituted. The normal application factory, MusicService, account check,
//! disk stores, private HTTP cache policy, queue, GStreamer and MPRIS are used.
//! This proves no real-account/API entitlement or Fedora/Flatpak acceptance.
use super::*;
use crate::{
    playback::PlaybackSourceKind,
    queue::RepeatMode,
    settings::{Appearance, UiStyle},
};
use ring::digest::{SHA256, digest};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    process::{Command, Stdio},
};

mod server;
use server::{Counts, Server};
const SELECTOR: &str = "ui::music::persistence::tests::v07_acceptance::native_v07_playback_restore_playlist_appearance_process_restart";
const TITLE: &str = "Karin v0.7 application acceptance";
const PHASE_ENV: &str = "KARIN_V07_PHASE";
const INPUT_ENV: &str = "KARIN_V07_INPUT";

#[derive(serde::Serialize, serde::Deserialize)]
struct Input {
    directory: PathBuf,
    origin: String,
    sha256: [u8; 32],
    bytes: u64,
    before: Counts,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct QueueState {
    ids: Vec<String>,
    current: Option<usize>,
    positions: Vec<Option<usize>>,
    upcoming: Vec<usize>,
    history: Vec<usize>,
    shuffle: bool,
    repeat: RepeatMode,
    partial: bool,
    kind: Option<crate::queue::ContextKind>,
    origin: Option<crate::queue::PlaybackOrigin>,
}
impl QueueState {
    fn capture(this: &Controller) -> Self {
        let model = this.model.borrow();
        let queue = &model.queue;
        Self {
            ids: queue.items().iter().map(|track| track.id.clone()).collect(),
            current: queue.current_index(),
            positions: (0..queue.items().len())
                .map(|index| queue.context().and_then(|ctx| ctx.source_position(index)))
                .collect(),
            upcoming: queue.upcoming_indices(),
            history: queue.history_indices(),
            shuffle: queue.shuffle(),
            repeat: queue.repeat_mode(),
            partial: queue.context().is_some_and(|context| context.is_partial()),
            kind: queue.context().map(|context| context.source_kind()),
            origin: queue.context().map(|context| context.origin().clone()),
        }
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
struct Expected {
    queue: QueueState,
    position_ms: u64,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct Report {
    phase: String,
    pid: u32,
    restored_items: usize,
    source: Option<String>,
    position_ms: u64,
    actual_seek: bool,
}

#[test]
#[ignore = "requires isolated X11/D-Bus, xdotool, actual GStreamer MP3 plugins and normal app subprocesses"]
fn native_v07_playback_restore_playlist_appearance_process_restart() {
    if let Ok(phase) = std::env::var(PHASE_ENV) {
        let input: Input =
            serde_json::from_slice(&fs::read(std::env::var_os(INPUT_ENV).unwrap()).unwrap())
                .unwrap();
        child(&input, &phase);
        return;
    }
    let started = Instant::now();
    let directory = Directory::new();
    let media = original_mp3(&directory.0);
    let bytes = media.len() as u64;
    let sha256 = digest(&SHA256, &media).as_ref().try_into().unwrap();
    let server = Server::new(media);
    let main = directory.0.join("main");
    prepare_profile(&main);
    let run = |profile: &Path, phase: &str| run_phase(&server, profile, phase, bytes, sha256);
    let mut reports = Vec::new();
    reports.push(run(&main, "seed"));
    let seeded = server.counts();
    assert!(seeded.media_gets >= 2 && seeded.background_gets >= 1);
    let mut saved_profiles = Vec::new();
    for name in [
        "unavailable",
        "wrong-account",
        "anonymous",
        "corrupt",
        "version",
        "stop",
        "logout",
        "manual-login",
        "fast-close",
        "late-logout",
        "restore-http-retry",
    ] {
        let profile = directory.0.join(name);
        copy_profile(&main, &profile);
        saved_profiles.push((name, profile));
    }
    reports.push(run(&main, "restart"));
    let resumed = server.counts();
    assert_eq!(
        resumed.media_gets, seeded.media_gets,
        "restored Play must read cached bytes"
    );
    assert_eq!(
        resumed.descriptors, seeded.descriptors,
        "fresh retained MP3 needs no XML resolution"
    );
    assert_eq!(
        resumed.heads, seeded.heads,
        "fresh retained MP3 needs no CDN probe"
    );
    assert_eq!(
        resumed.offers,
        seeded.offers + 1,
        "explicit resumed Play must recheck the authenticated offer"
    );
    assert_eq!(
        (
            resumed.changes,
            resumed.renames,
            resumed.visibility,
            resumed.deletes
        ),
        (3, 1, 1, 1)
    );
    assert!(server.mode.lock().unwrap().deleted);
    reports.push(run(&main, "deleted-source"));
    assert_eq!(
        server.counts().offers,
        resumed.offers,
        "displaying saved queue must not resolve audio"
    );
    for (name, profile) in saved_profiles {
        *server.mode.lock().unwrap() = server::Mode::default();
        match name {
            "unavailable" => server.mode.lock().unwrap().unavailable = true,
            "wrong-account" => server.mode.lock().unwrap().account = 5678,
            "corrupt" => mutate_snapshot(&profile, false),
            "version" => mutate_snapshot(&profile, true),
            "late-logout" => server.mode.lock().unwrap().hold_restore = Some(profile.clone()),
            "restore-http-retry" => {
                server.mode.lock().unwrap().restore_http_retry = Some(profile.clone())
            }
            _ => {}
        }
        let before = server.counts();
        reports.push(run(&profile, name));
        let after = server.counts();
        assert_eq!(
            after.offers,
            before.offers + u64::from(name == "fast-close"),
            "only the explicit user Play during held credential save may resolve audio"
        );
        if name == "fast-close" {
            assert!(
                after.media_gets > before.media_gets && after.media_gets - before.media_gets <= 3,
                "explicit Play/seek stays online while the credential-save barrier closes private cache admission"
            );
            assert_eq!(
                after.background_gets, before.background_gets,
                "unconfirmed protected save must not enable a cache writer"
            );
        } else {
            assert_eq!(
                after.media_gets, before.media_gets,
                "startup must not download audio"
            );
        }
        if name == "stop" {
            reports.push(run(&profile, "stopped"));
        }
        if matches!(name, "logout" | "late-logout") {
            reports.push(run(&profile, "logged-out"));
        }
        if name == "fast-close" {
            reports.push(run(&profile, "fast-restart"));
        }
    }
    *server.mode.lock().unwrap() = server::Mode::default();
    let partial = directory.0.join("partial");
    prepare_profile(&partial);
    reports.push(run(&partial, "partial-seed"));
    reports.push(run(&partial, "partial-restart"));
    assert!(
        started.elapsed() < Duration::from_secs(240),
        "normal-app acceptance stays bounded"
    );
    let pids = reports
        .iter()
        .map(|report| report.pid)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        pids.len(),
        reports.len(),
        "every startup is a new OS process"
    );
    let receipt = json!({"scope":"v07-normal-application-original-cc0","real_yandex_service":false,
        "protected_backend":"synthetic-only","cache_authority":"production-private-http",
        "processes":reports.len(),"phases":reports,"wire":server.counts(),
        "restored_play_media_get_delta":resumed.media_gets-seeded.media_gets,
        "restored_play_descriptor_delta":resumed.descriptors-seeded.descriptors,
        "restored_play_offer_delta":resumed.offers-seeded.offers,
        "all_mutations_independent_readback":true,"active_queue_and_engine_preserved":true,
        "no_autoplay_no_wave_no_resolver_before_play":true,"snapshot_has_no_urls_or_credentials":true,
        "media_original_bytes":bytes});
    if let Some(captures) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
        fs::create_dir_all(&captures).unwrap();
        fs::write(
            PathBuf::from(captures).join("v07-application-receipt.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
    }
}
fn prepare_profile(profile: &Path) {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(profile)
        .unwrap();
    for kind in ["config", "cache", "data", "state"] {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(profile.join(kind))
            .unwrap();
    }
}
fn copy_profile(from: &Path, to: &Path) {
    fs::DirBuilder::new().mode(0o700).create(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_profile(&path, &target);
        } else if path.extension().is_none_or(|extension| extension != "log") {
            fs::copy(&path, &target).unwrap();
            fs::set_permissions(target, fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
}
fn run_phase(
    server: &Server,
    directory: &Path,
    phase: &str,
    bytes: u64,
    sha256: [u8; 32],
) -> Report {
    let input = directory.join("input.json");
    fs::write(
        &input,
        serde_json::to_vec(&Input {
            directory: directory.into(),
            origin: server.origin.clone(),
            bytes,
            sha256,
            before: server.counts(),
        })
        .unwrap(),
    )
    .unwrap();
    let log = fs::File::create(directory.join(format!("{phase}.log"))).unwrap();
    let before = server.counts();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            SELECTOR,
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env(PHASE_ENV, phase)
        .env(INPUT_ENV, &input)
        .env("XDG_CONFIG_HOME", directory.join("config"))
        .env("XDG_CACHE_HOME", directory.join("cache"))
        .env("XDG_DATA_HOME", directory.join("data"))
        .env("XDG_STATE_HOME", directory.join("state"))
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log));
    let mut process = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if let Some(status) = process.try_wait().unwrap() {
            if !status.success() {
                retain_log(directory, phase);
                panic!("v0.7 native application phase {phase} failed; inspect isolated child log");
            }
            break;
        }
        if Instant::now() >= deadline {
            process.kill().unwrap();
            process.wait().unwrap();
            retain_log(directory, phase);
            panic!("v0.7 application child exceeded 45 seconds");
        }
        thread::sleep(Duration::from_millis(5));
    }
    let report: Report =
        serde_json::from_slice(&fs::read(directory.join(format!("{phase}.json"))).unwrap())
            .unwrap();
    let after = server.counts();
    if matches!(phase, "anonymous" | "logged-out") {
        assert_eq!(after.account, before.account);
    } else {
        assert_eq!(after.account, before.account + 1);
    }
    if !matches!(phase, "seed" | "restart" | "partial-seed" | "fast-close") {
        assert_eq!(
            (after.offers, after.descriptors, after.media_gets),
            (before.offers, before.descriptors, before.media_gets)
        );
    }
    report
}
fn retain_log(directory: &Path, phase: &str) {
    if let Some(captures) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
        fs::create_dir_all(&captures).unwrap();
        fs::copy(
            directory.join(format!("{phase}.log")),
            PathBuf::from(captures).join(format!("v07-application-{phase}.log")),
        )
        .unwrap();
    }
}
fn snapshots(profile: &Path) -> Vec<PathBuf> {
    let root = profile
        .join("data")
        .join(crate::config::APP_ID)
        .join("playback");
    if !root.exists() {
        return Vec::new();
    }
    fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect()
}
fn mutate_snapshot(profile: &Path, version: bool) {
    let paths = snapshots(profile);
    assert_eq!(paths.len(), 1);
    let path = &paths[0];
    let mut bytes = fs::read(path).unwrap();
    assert!(bytes.starts_with(b"KARPB1\0\0"));
    if version {
        let mut body: serde_json::Value = serde_json::from_slice(&bytes[40..]).unwrap();
        body["version"] = json!(999);
        let body = serde_json::to_vec(&body).unwrap();
        bytes.truncate(8);
        bytes.extend_from_slice(digest(&SHA256, &body).as_ref());
        bytes.extend_from_slice(&body);
    } else {
        let end = bytes.len() - 1;
        bytes[end] ^= 0x20;
    }
    fs::write(path, bytes).unwrap();
}

fn child(input: &Input, phase: &str) {
    gtk::init().unwrap();
    adw::init().unwrap();
    let settings = SettingsStore::for_application().unwrap();
    if matches!(phase, "seed" | "partial-seed") {
        settings.save(&Preferences::default()).unwrap();
    }
    let root = glib::user_cache_dir().join(crate::config::APP_ID);
    let budget = CacheBudget::new(
        root.join("persistent"),
        root.join("audio"),
        true,
        CacheLimit::GiB2.bytes(),
    )
    .unwrap();
    let cache =
        Cache::new_with_budget(root.join("persistent"), CachePolicy::default(), budget).unwrap();
    let authenticated = !matches!(
        phase,
        "anonymous" | "logged-out" | "manual-login" | "fast-close"
    );
    let backend = Backend::new(authenticated.then_some(TOKEN));
    let mut state = State::for_testing(
        settings.clone(),
        CredentialStore::with_backend(backend.clone()),
        Some(cache),
    );
    state.audio_root = Some(root.join("audio"));
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.V07Acceptance")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let ui = crate::ui::build_with_music_pane(
        &app,
        crate::application::ConnectionService::new(crate::api::YandexApi::new().unwrap()),
        |app, window| {
            MusicPane::with_provider_state(
                app,
                window,
                Some(MusicService::for_test(&input.origin)),
                Arc::new(OfflineProvider),
                launch_system_browser,
                state,
            )
        },
    );
    ui.window.set_title(Some(TITLE));
    ui.window.set_default_size(1000, 900);
    ui.window.present();
    let this = &ui.music.controller;
    if phase == "late-logout" {
        pump_until(|| {
            this.model.borrow().account_valid && input.directory.join("restore-blocked").exists()
        });
        this.logout();
        fs::write(input.directory.join("restore-release"), b"released").unwrap();
        pump_until(|| {
            !this.model.borrow().account_valid
                && settled(this)
                && this.playback_storage_idle_for_test()
        });
    } else {
        pump_until(|| {
            settled(this)
                && (!authenticated || this.model.borrow().account_valid)
                && this.playback_storage_idle_for_test()
        });
    }
    this.set_fixture_resolver_for_test(StreamResolver::for_test(
        &input.origin,
        &input.origin,
        &input.origin,
    ));
    assert!(
        this.music_snapshot().is_none(),
        "startup must not construct a GStreamer source"
    );
    assert!(
        !this.music_requested(),
        "startup must not request a resolver"
    );
    assert!(this.radio_idle_for_test());
    assert!(this.wave_session_for_test().is_none());
    let startup = remote_counts(input);
    assert_eq!(
        (
            startup.offers,
            startup.descriptors,
            startup.heads,
            startup.media_gets
        ),
        (
            input.before.offers,
            input.before.descriptors,
            input.before.heads,
            input.before.media_gets
        ),
        "validated startup must not resolve, probe or play media before explicit Play"
    );
    let mut source = None;
    let mut actual_seek = false;
    let mut position_ms = 0;
    let mut restored_items = this.model.borrow().queue.items().len();
    match phase {
        "seed" | "partial-seed" => {
            assert_eq!(restored_items, 0);
            this.open_playlist(playlist(phase == "partial-seed"), 0);
            let selected = if phase == "seed" { 2 } else { 0 };
            pump_until(|| {
                named_all(this.widgets.detail.upcast_ref(), "music-track-row").len() > selected
            });
            let rows = named_all(this.widgets.detail.upcast_ref(), "music-track-row");
            click(&ui.window, &rows[selected]);
            wait_source(this, PlaybackSourceKind::RemoteHttps);
            assert_eq!(this.model.borrow().queue.current_index(), Some(selected));
            wait_publication(this, input);
            let serial = this.music_serial();
            assert!(rows[selected].grab_focus());
            key("Return");
            pump_until(|| this.music_serial() > serial);
            wait_source(this, PlaybackSourceKind::CachedFile);
            if phase == "seed" {
                // These are the ordinary manual queue and local order actions.
                // All occurrences of ID 42 remain separate queue positions.
                this.set_music_shuffle(true);
                this.set_music_repeat(RepeatMode::Queue);
                this.skip_music(true, false);
                pump_until(|| !this.music_requested());
                this.skip_music(false, false);
                pump_until(|| this.model.borrow().queue.current_index() == Some(2));
                let duplicate = this.model.borrow().queue.items()[2].clone();
                this.enqueue(duplicate);
                this.remove_music_queue(1);
                assert_eq!(this.model.borrow().queue.current_index(), Some(1));
                wait_source(this, PlaybackSourceKind::CachedFile);
                let state = QueueState::capture(this);
                assert_eq!(state.ids, ["42", "42", "44", "42"]);
                assert_eq!(state.positions, [Some(0), Some(2), Some(3), None]);
                assert_ne!(
                    this.model.borrow().queue.occurrence_id(0),
                    this.model.borrow().queue.occurrence_id(1)
                );
                exercise_mutations(&ui.window, this, input);
                let expected = QueueState::capture(this);
                let native = this.music_snapshot().unwrap();
                let serial = this.music_serial();
                let cache_binding = this.audio_cache_binding().unwrap().0;
                let cache_stats = desktop(move || cache_binding.stats().unwrap());
                for appearance in [Appearance::Light, Appearance::System, Appearance::Dark] {
                    let mut values = this.persistence.borrow().values.clone();
                    values.appearance = appearance;
                    values.ui_style = UiStyle::System;
                    this.preferences_changed(values);
                    pump_until(|| settled(this));
                    assert_eq!(QueueState::capture(this), expected);
                    assert_eq!(this.music_serial(), serial);
                    assert_eq!(this.music_snapshot().unwrap().generation, native.generation);
                    assert_eq!(this.music_snapshot().unwrap().state, PlaybackState::Playing);
                    let cache = this.audio_cache_binding().unwrap().0;
                    assert_eq!(
                        desktop(move || cache.stats().unwrap()).entries,
                        cache_stats.entries
                    );
                }
                assert_eq!(
                    adw::StyleManager::default().color_scheme(),
                    adw::ColorScheme::ForceDark
                );
            } else {
                assert!(this.model.borrow().queue.context().unwrap().is_partial());
            }
            // A native acknowledged seek is required, rather than merely a UI
            // slider assignment or queued command. Pause freezes the receipt.
            pump_until(|| {
                this.music_snapshot()
                    .is_some_and(|snapshot| snapshot.seekable)
            });
            this.seek_music(Duration::from_millis(4200));
            pump_until(|| {
                this.music_snapshot().is_some_and(|snapshot| {
                    snapshot.seek_serial > 0
                        && snapshot.seek_position == Some(Duration::from_millis(4200))
                })
            });
            this.pause_music();
            pump_until(|| {
                this.music_snapshot()
                    .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
            });
            let position = this.music_snapshot().unwrap().position.unwrap();
            assert!(position >= Duration::from_millis(4100));
            position_ms = position.as_millis().try_into().unwrap();
            actual_seek = true;
            source = Some("cached-file".into());
            let expected = Expected {
                queue: QueueState::capture(this),
                position_ms,
            };
            fs::write(
                input.directory.join("expected.json"),
                serde_json::to_vec(&expected).unwrap(),
            )
            .unwrap();
            restored_items = expected.queue.ids.len();
            this.flush_playback_for_test();
            pump_until(|| this.playback_storage_idle_for_test());
        }
        "restart" | "deleted-source" | "partial-restart" | "fast-restart" => {
            let expected: Expected =
                serde_json::from_slice(&fs::read(input.directory.join("expected.json")).unwrap())
                    .unwrap();
            let mut saved_queue = expected.queue.clone();
            saved_queue.partial = true; // Saved collection, not a fresh server-membership claim.
            assert_eq!(
                QueueState::capture(this),
                saved_queue,
                "exact physical queue/order/history survives a new process"
            );
            position_ms = this
                .restored_playback_position_for_test()
                .unwrap()
                .as_millis()
                .try_into()
                .unwrap();
            assert!(
                position_ms.abs_diff(expected.position_ms) < 300,
                "saved native position is restored"
            );
            assert!(this.widgets.player_revealer.reveals_child());
            pump_until(|| this.system_media_available());
            pump_until(|| property("PlaybackStatus").get::<String>().as_deref() == Some("Stopped"));
            assert_eq!(property("CanSeek").get::<bool>(), Some(false));
            let exported = property("Position").get::<i64>().unwrap();
            assert!(exported >= 0 && (exported as u64 / 1000).abs_diff(position_ms) < 300);
            if phase == "restart" {
                let values = desktop(move || {
                    SettingsStore::for_application()
                        .unwrap()
                        .load()
                        .unwrap()
                        .preferences
                });
                assert_eq!(values.appearance, Appearance::Dark);
                assert_eq!(values.ui_style, UiStyle::System);
                assert_eq!(
                    adw::StyleManager::default().color_scheme(),
                    adw::ColorScheme::ForceDark
                );
                method("Play", ().to_variant());
                wait_source(this, PlaybackSourceKind::CachedFile);
                pump_until(|| {
                    this.music_snapshot().is_some_and(|snapshot| {
                        snapshot.seek_serial > 0
                            && snapshot.seek_position.is_some_and(|position| {
                                position.as_millis().abs_diff(u128::from(position_ms)) < 300
                            })
                    })
                });
                actual_seek = true;
                source = Some("cached-file".into());
                pump_until(|| {
                    property("PlaybackStatus").get::<String>().as_deref() == Some("Playing")
                });
                pump_until(|| property("CanSeek").get::<bool>() == Some(true));
                exercise_delete(&ui.window, this);
                method("Pause", ().to_variant());
                pump_until(|| {
                    this.music_snapshot()
                        .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
                });
                let seek_serial = this.music_snapshot().unwrap().seek_serial;
                method("Seek", (1_000_000_i64,).to_variant());
                pump_until(|| {
                    this.music_snapshot().is_some_and(|snapshot| {
                        snapshot.seek_serial > seek_serial
                            && snapshot.position.zip(snapshot.seek_position).is_some_and(
                                |(position, target)| {
                                    position.abs_diff(target) < Duration::from_millis(100)
                                },
                            )
                    })
                });
                this.set_music_repeat(RepeatMode::One);
                let position = this.music_snapshot().unwrap().position.unwrap();
                position_ms = position.as_millis().try_into().unwrap();
                fs::write(
                    input.directory.join("expected.json"),
                    serde_json::to_vec(&Expected {
                        queue: QueueState::capture(this),
                        position_ms,
                    })
                    .unwrap(),
                )
                .unwrap();
                this.flush_playback_for_test();
                pump_until(|| this.playback_storage_idle_for_test());
            }
            if phase == "deleted-source" {
                assert!(
                    this.persistence
                        .borrow()
                        .playback
                        .message
                        .is_some_and(|message| message.contains("сохранённая очередь"))
                );
            }
        }
        "restore-http-retry" => {
            assert!(
                this.playback_restore_retry_for_test(),
                "503 during real track normalization must expose restoration Retry"
            );
            assert!(this.model.borrow().queue.items().is_empty());
            assert!(this.restored_playback_position_for_test().is_none());
            let scope = this
                .persistence
                .borrow()
                .scope
                .clone()
                .expect("freshly validated account scope");
            let before_scope = scope.clone();
            let before = desktop(move || {
                let store =
                    crate::playback_session::PlaybackSessionStore::for_application().unwrap();
                assert!(
                    store.load(&before_scope).unwrap().snapshot.is_some(),
                    "fixture must begin with a valid account-bound continuation"
                );
                fs::read(store.path_for_test(&before_scope)).unwrap()
            });
            // An earlier failed write can leave this flag set when a later
            // restore loses its network response. Exercise the exact periodic
            // save admission without forcing past its equality deduplication:
            // unknown live state must never become a null tombstone.
            this.seed_playback_write_retry_for_test();
            this.save_playback_continuation(false);
            pump_until(|| {
                this.playback_storage_idle_for_test()
                    && !WorkFence::active()
                    && this
                        .persistence
                        .borrow()
                        .durable_work
                        .load(Ordering::Acquire)
                        == 0
            });
            desktop(move || {
                let store =
                    crate::playback_session::PlaybackSessionStore::for_application().unwrap();
                assert!(
                    store.load(&scope).unwrap().snapshot.is_some(),
                    "failed restoration plus periodic save must preserve the valid saved queue"
                );
                assert!(
                    fs::read(store.path_for_test(&scope)).unwrap() == before,
                    "failed restoration must leave the saved record byte-for-byte unchanged"
                );
            });
            assert!(this.playback_restore_retry_for_test());
            fs::write(input.directory.join("restore-http-ready"), b"ready").unwrap();
            let retry = this.persistence.borrow().retry_button.clone();
            pump_until(|| retry.is_visible() && retry.is_sensitive());
            click(&ui.window, &retry);
            pump_until(|| {
                !this.playback_restore_retry_for_test()
                    && this.playback_storage_idle_for_test()
                    && !this.model.borrow().queue.items().is_empty()
            });
            let expected: Expected =
                serde_json::from_slice(&fs::read(input.directory.join("expected.json")).unwrap())
                    .unwrap();
            let mut expected_queue = expected.queue;
            expected_queue.partial = true;
            assert_eq!(
                QueueState::capture(this),
                expected_queue,
                "native Retry must recover the original exact queue"
            );
            position_ms = this
                .restored_playback_position_for_test()
                .unwrap()
                .as_millis()
                .try_into()
                .unwrap();
            assert!(position_ms.abs_diff(expected.position_ms) < 300);
            restored_items = this.model.borrow().queue.items().len();
            assert!(this.music_snapshot().is_none());
            assert!(!this.music_requested());
            let after = remote_counts(input);
            assert_eq!(
                (
                    after.offers,
                    after.descriptors,
                    after.heads,
                    after.media_gets
                ),
                (
                    input.before.offers,
                    input.before.descriptors,
                    input.before.heads,
                    input.before.media_gets
                ),
                "a restore Retry validates metadata only and must not prepare audio"
            );
        }
        "unavailable" => {
            assert_eq!(QueueState::capture(this).ids, ["44"]);
            assert_eq!(
                this.restored_playback_position_for_test(),
                Some(Duration::ZERO)
            );
            assert!(
                this.persistence
                    .borrow()
                    .playback
                    .message
                    .is_some_and(|message| message.contains("недоступен"))
            );
        }
        "wrong-account" | "anonymous" | "corrupt" | "version" | "stopped" | "logged-out"
        | "late-logout" => {
            assert_eq!(restored_items, 0);
            assert!(this.restored_playback_position_for_test().is_none());
            assert!(!this.widgets.player_revealer.reveals_child());
            if !authenticated {
                assert!(!this.model.borrow().account_valid);
            }
        }
        "manual-login" | "fast-close" => {
            assert_eq!(restored_items, 0);
            backend.block_save();
            this.authenticate(token());
            pump_until(|| {
                this.model.borrow().account_valid && backend.save_entered.load(Ordering::Acquire)
            });
            assert!(
                this.model.borrow().queue.items().is_empty(),
                "unconfirmed credential save must not restore a personal queue"
            );
            assert!(this.music_snapshot().is_none());
            assert_eq!(remote_counts(input).offers, input.before.offers);
            if phase == "manual-login" {
                backend.release_save();
                pump_until(|| {
                    settled(this)
                        && this.playback_storage_idle_for_test()
                        && !this.model.borrow().queue.items().is_empty()
                });
                let expected: Expected = serde_json::from_slice(
                    &fs::read(input.directory.join("expected.json")).unwrap(),
                )
                .unwrap();
                let mut expected_queue = expected.queue;
                expected_queue.partial = true;
                assert_eq!(QueueState::capture(this), expected_queue);
                assert!(this.music_snapshot().is_none());
                assert!(!this.music_requested());
                assert_eq!(remote_counts(input).offers, input.before.offers);
                restored_items = this.model.borrow().queue.items().len();
                position_ms = this
                    .restored_playback_position_for_test()
                    .unwrap()
                    .as_millis()
                    .try_into()
                    .unwrap();
            } else {
                // A deliberate fresh selection supersedes any older continuation
                // while the ordinary protected credential transaction is pending.
                this.open_playlist(playlist(false), 0);
                pump_until(|| {
                    !named_all(this.widgets.detail.upcast_ref(), "music-track-row").is_empty()
                });
                let row = named(&this.widgets.detail, "music-track-row");
                click(&ui.window, &row);
                // The existing forget barrier deliberately keeps the private
                // cache closed until protected credential saving is confirmed.
                // Playback itself remains available through the same engine.
                wait_source(this, PlaybackSourceKind::RemoteHttps);
                pump_until(|| {
                    this.music_snapshot()
                        .is_some_and(|snapshot| snapshot.seekable)
                });
                this.seek_music(Duration::from_millis(6800));
                pump_until(|| {
                    this.music_snapshot().is_some_and(|snapshot| {
                        snapshot.seek_position == Some(Duration::from_millis(6800))
                            && snapshot
                                .position
                                .is_some_and(|position| position >= Duration::from_millis(6700))
                    })
                });
                this.pause_music();
                pump_until(|| {
                    this.music_snapshot()
                        .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
                });
                let position = this.music_snapshot().unwrap().position.unwrap();
                position_ms = position.as_millis().try_into().unwrap();
                actual_seek = true;
                source = Some("remote-https".into());
                fs::write(
                    input.directory.join("expected.json"),
                    serde_json::to_vec(&Expected {
                        queue: QueueState::capture(this),
                        position_ms,
                    })
                    .unwrap(),
                )
                .unwrap();
                assert!(backend.save_entered.load(Ordering::Acquire));
                restored_items = this.model.borrow().queue.items().len();
            }
        }
        "stop" => {
            assert!(restored_items > 0);
            pump_until(|| this.system_media_available());
            method("Stop", ().to_variant());
            pump_until(|| this.playback_storage_idle_for_test());
        }
        "logout" => {
            assert!(restored_items > 0);
            this.logout();
            pump_until(|| {
                !this.model.borrow().account_valid
                    && settled(this)
                    && this.playback_storage_idle_for_test()
            });
            assert!(this.model.borrow().queue.items().is_empty());
            assert!(this.music_snapshot().is_none());
        }
        _ => panic!("unknown application phase"),
    }
    let stopped = this.music_cleanup_observer_for_test();
    this.close();
    if phase == "fast-close" {
        backend.release_save();
    }
    ui.window.close();
    pump_until(|| {
        stopped()
            && !WorkFence::active()
            && this
                .persistence
                .borrow()
                .durable_work
                .load(Ordering::Acquire)
                == 0
    });
    if matches!(phase, "seed" | "partial-seed" | "fast-close") {
        check_snapshot(input);
    }
    if matches!(phase, "manual-login" | "fast-close") {
        assert_eq!(backend.value.lock().unwrap().as_deref(), Some(TOKEN));
    }
    fs::write(
        input.directory.join(format!("{phase}.json")),
        serde_json::to_vec(&Report {
            phase: phase.into(),
            pid: std::process::id(),
            restored_items,
            source,
            position_ms,
            actual_seek,
        })
        .unwrap(),
    )
    .unwrap();
}

fn playlist(partial: bool) -> Playlist {
    Playlist {
        id: crate::music::PlaylistId {
            owner: "1234".into(),
            kind: if partial { "78" } else { "77" }.into(),
        },
        title: "Disposable original playlist".into(),
        track_count: if partial { 220 } else { 4 },
        cover: None,
    }
}
fn wait_source(this: &Controller, kind: PlaybackSourceKind) {
    pump_until(|| {
        this.music_snapshot().is_some_and(|snapshot| {
            snapshot.state == PlaybackState::Playing
                && snapshot.source_kind == kind
                && !snapshot.playing_time.is_zero()
        })
    });
}
fn wait_publication(this: &Controller, input: &Input) {
    let root = input
        .directory
        .join("cache")
        .join(crate::config::APP_ID)
        .join("audio");
    let expected = input.sha256;
    let bytes = input.bytes;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if media_files(&root).iter().any(|path| {
                fs::read(path).is_ok_and(|body| {
                    body.len() as u64 == bytes && digest(&SHA256, &body).as_ref() == expected
                })
            }) {
                sender.send(true).unwrap();
                break;
            }
            if Instant::now() >= deadline {
                sender.send(false).unwrap();
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
    });
    let mut result = None;
    pump_until(|| {
        result = receiver.try_recv().ok();
        result.is_some()
    });
    worker.join().unwrap();
    assert_eq!(result, Some(true));
    pump_until(|| this.cache_work_settled_for_test());
}
fn media_files(root: &Path) -> Vec<PathBuf> {
    if !root.exists() {
        return Vec::new();
    }
    fs::read_dir(root)
        .unwrap()
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                media_files(&path)
            } else if path.file_name().is_some_and(|name| name == "media.bin") {
                vec![path]
            } else {
                Vec::new()
            }
        })
        .collect()
}
fn check_snapshot(input: &Input) {
    let files = snapshots(&input.directory);
    assert_eq!(files.len(), 1);
    let metadata = fs::metadata(&files[0]).unwrap();
    assert_eq!(metadata.mode() & 0o777, 0o600);
    assert!(metadata.len() < 256 * 1024);
    let bytes = fs::read(&files[0]).unwrap();
    assert!(bytes.starts_with(b"KARPB1\0\0"));
    assert_eq!(digest(&SHA256, &bytes[40..]).as_ref(), &bytes[8..40]);
    let text = String::from_utf8(bytes[40..].into()).unwrap();
    for forbidden in [
        TOKEN,
        "http:",
        "https:",
        "download-info",
        "get-mp3",
        "Original audio",
        "Disposable original playlist",
        "batchId",
        "radioSessionId",
    ] {
        assert!(
            !text.contains(forbidden),
            "playback state must not persist credentials/URLs/display models/radio markers"
        );
    }
}
fn named_all(root: &gtk::Widget, name: &str) -> Vec<gtk::Widget> {
    let mut result = Vec::new();
    if name == "*" || root.widget_name() == name {
        result.push(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        result.extend(named_all(&widget, name));
    }
    result
}
fn named(root: &impl IsA<gtk::Widget>, name: &str) -> gtk::Widget {
    named_all(root.as_ref(), name)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("missing native control {name}"))
}
fn window_id() -> String {
    let output = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--pid",
            &std::process::id().to_string(),
            "--name",
            &format!("^{TITLE}$"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .into()
}
fn key(value: &str) {
    assert!(
        Command::new("xdotool")
            .args(["windowfocus", "--sync", &window_id()])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("xdotool")
            .args(["key", "--clearmodifiers", value])
            .status()
            .unwrap()
            .success()
    );
}
fn click(window: &adw::ApplicationWindow, widget: &impl IsA<gtk::Widget>) {
    crate::ui::preferences::click_native_for_test(window, &window_id(), widget);
}
fn desktop<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        sender
            .send(std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)))
            .unwrap()
    });
    let mut result = None;
    pump_until(|| {
        result = receiver.try_recv().ok();
        result.is_some()
    });
    worker.join().unwrap();
    match result.unwrap() {
        Ok(value) => value,
        Err(error) => std::panic::resume_unwind(error),
    }
}
fn method(name: &str, parameters: glib::Variant) {
    let name = name.to_owned();
    desktop(move || {
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
            .unwrap()
            .call_sync(
                Some(crate::mpris::BUS_NAME),
                "/org/mpris/MediaPlayer2",
                "org.mpris.MediaPlayer2.Player",
                &name,
                Some(&parameters),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                3000,
                gio::Cancellable::NONE,
            )
            .unwrap();
    });
}
fn property(name: &str) -> glib::Variant {
    let name = name.to_owned();
    desktop(move || {
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
            .unwrap()
            .call_sync(
                Some(crate::mpris::BUS_NAME),
                "/org/mpris/MediaPlayer2",
                "org.freedesktop.DBus.Properties",
                "Get",
                Some(&("org.mpris.MediaPlayer2.Player", name).to_variant()),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                3000,
                gio::Cancellable::NONE,
            )
            .unwrap()
            .get::<(glib::Variant,)>()
            .unwrap()
            .0
    })
}

fn editor_ready(this: &Controller, count: usize) -> adw::Dialog {
    pump_until(|| {
        this.playlist_management_dialog_for_test()
            .is_some_and(|dialog| {
                let rows = named_all(dialog.upcast_ref(), "music-playlist-manage-occurrence");
                rows.len() == count
                    && named_all(dialog.upcast_ref(), "music-playlist-manage-refresh")
                        .first()
                        .is_some_and(|widget| widget.is_sensitive())
                    && !this.mutation_busy.get()
            })
    });
    this.playlist_management_dialog_for_test().unwrap()
}
fn close_editor(window: &adw::ApplicationWindow, this: &Controller) {
    let dialog = this.playlist_management_dialog_for_test().unwrap();
    click(window, &named(&dialog, "music-playlist-manage-close"));
    pump_until(|| this.playlist_management_dialog_for_test().is_none());
}
fn assert_player_unchanged(this: &Controller, queue: &QueueState, serial: u64, generation: u64) {
    assert_eq!(
        &QueueState::capture(this),
        queue,
        "confirmed server edits must not rewrite the admitted queue"
    );
    assert_eq!(
        this.music_serial(),
        serial,
        "a metadata edit must not resolve/restart music"
    );
    let native = this.music_snapshot().unwrap();
    assert_eq!(native.generation, generation);
    assert_eq!(native.state, PlaybackState::Playing);
}
fn exercise_mutations(window: &adw::ApplicationWindow, this: &Rc<Controller>, input: &Input) {
    let queue = QueueState::capture(this);
    let serial = this.music_serial();
    let generation = this.music_snapshot().unwrap().generation;
    let mut addition = this.model.borrow().queue.current().unwrap().clone();
    addition.id = "43".into();
    addition.album_id = Some("7".into());
    addition.title = "Original audio 43".into();
    this.present_add_to_playlist(addition);
    pump_until(|| {
        this.playlist_management_dialog_for_test()
            .is_some_and(|dialog| {
                !named_all(dialog.upcast_ref(), "music-playlist-manage-pick").is_empty()
            })
    });
    let picker = this.playlist_management_dialog_for_test().unwrap();
    click(window, &named(&picker, "music-playlist-manage-pick"));
    pump_until(|| {
        !this.mutation_busy.get()
            && named(&picker, "music-playlist-manage-message")
                .downcast::<gtk::Label>()
                .unwrap()
                .text()
                .contains("Трек добавлен")
    });
    assert_player_unchanged(this, &queue, serial, generation);
    close_editor(window, this);
    this.present_playlist_editor(playlist(false));
    let dialog = editor_ready(this, 5);
    let metadata_group = named(&dialog, "music-playlist-manage-visibility")
        .ancestor(adw::PreferencesGroup::static_type())
        .expect("mounted native metadata group")
        .downgrade();
    click(window, &named(&dialog, "music-playlist-manage-refresh"));
    let dialog = editor_ready(this, 5);
    // Exact third occurrence 42:8 moves over 43:7 in one native keyboard action.
    let occurrences = named_all(dialog.upcast_ref(), "music-playlist-manage-occurrence");
    let stale_remove = named(&occurrences[2], "music-playlist-manage-remove")
        .downcast::<gtk::Button>()
        .unwrap();
    let up = named(&occurrences[2], "music-playlist-manage-move-up");
    assert!(up.grab_focus());
    key("Return");
    pump_until(|| {
        !this.mutation_busy.get()
            && named(&dialog, "music-playlist-manage-message")
                .downcast::<gtk::Label>()
                .unwrap()
                .text()
                .contains("Изменение подтверждено")
    });
    let dialog = editor_ready(this, 5);
    assert_player_unchanged(this, &queue, serial, generation);
    let confirmed = remote_counts(input).changes;
    stale_remove.emit_clicked();
    // An obsolete occurrence button may still be retained by an accessibility
    // client. A readback must retire its position binding, without a new POST.
    assert!(!this.mutation_busy.get());
    assert_eq!(remote_counts(input).changes, confirmed);
    // The second physical occurrence is now the moved 42:8. Remove only it.
    let occurrences = named_all(dialog.upcast_ref(), "music-playlist-manage-occurrence");
    click(
        window,
        &named(&occurrences[1], "music-playlist-manage-remove"),
    );
    let dialog = editor_ready(this, 4);
    assert_player_unchanged(this, &queue, serial, generation);
    let title = named(&dialog, "music-playlist-manage-rename-title")
        .downcast::<adw::EntryRow>()
        .unwrap();
    title.set_text("Переименованная original collection");
    let rename = named(&dialog, "music-playlist-manage-rename");
    pump_until(|| rename.is_sensitive());
    click(window, &rename);
    pump_until(|| {
        !this.mutation_busy.get()
            && title.text() == "Переименованная original collection"
            && named(&dialog, "music-playlist-manage-message")
                .downcast::<gtk::Label>()
                .unwrap()
                .text()
                .contains("Изменение подтверждено")
    });
    assert_player_unchanged(this, &queue, serial, generation);
    let visibility = named(&dialog, "music-playlist-manage-visibility")
        .downcast::<adw::ComboRow>()
        .unwrap();
    assert_eq!(visibility.selected(), 0);
    assert_eq!(
        metadata_group.upgrade(),
        visibility.ancestor(adw::PreferencesGroup::static_type()),
        "confirmed readback must retain the native metadata group rather than reparent its rows"
    );
    assert!(
        visibility.is_sensitive() && title.is_sensitive(),
        "confirmed metadata fields must become interactive again"
    );
    let target = visibility
        .activatable_widget()
        .unwrap_or_else(|| visibility.clone().upcast());
    eprintln!(
        "Karin synthetic visibility native target: row-mapped={} row-sensitive={} row-focusable={} row-activatable={} row-can-target={} row-size={}x{} target-type={} target-sensitive={} target-mapped={} target-size={}x{}",
        visibility.is_mapped(),
        visibility.is_sensitive(),
        visibility.is_focusable(),
        visibility.is_activatable(),
        visibility.can_target(),
        visibility.width(),
        visibility.height(),
        target.type_().name(),
        target.is_sensitive(),
        target.is_mapped(),
        target.width(),
        target.height()
    );
    if let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") {
        fs::create_dir_all(&directory).unwrap();
        assert!(
            Command::new("import")
                .args(["-silent", "-screen", "-window", &window_id()])
                .arg(PathBuf::from(directory).join("v07-playlist-editor-before-visibility.png"))
                .status()
                .unwrap()
                .success()
        );
    }
    click(window, &target);
    pump_until(|| {
        named_all(visibility.upcast_ref(), "*")
            .iter()
            .any(|widget| {
                widget
                    .clone()
                    .downcast::<gtk::Label>()
                    .is_ok_and(|label| label.text() == "Все" && label.is_mapped())
            })
    });
    let choice = named_all(visibility.upcast_ref(), "*")
        .into_iter()
        .find_map(|widget| {
            widget
                .downcast::<gtk::Label>()
                .ok()
                .filter(|label| label.text() == "Все" && label.is_mapped())
        })
        .unwrap();
    // AdwComboRow uses a native GtkListView, whose item wrapper is an
    // internal GtkListItemWidget. A GtkListBoxRow ancestor would instead be
    // the outer ComboRow, so clicking it would merely close the popup.
    let mut row: gtk::Widget = choice.upcast();
    loop {
        let parent = row
            .parent()
            .expect("mapped choice belongs to a native list");
        if parent.is::<gtk::ListView>() {
            break;
        }
        assert!(
            parent != *visibility.upcast_ref::<gtk::Widget>(),
            "choice must resolve inside the popup list, not the outer row"
        );
        row = parent;
    }
    click(window, &row);
    pump_until(|| {
        visibility.selected() == 1
            && !this.mutation_busy.get()
            && named(&dialog, "music-playlist-manage-message")
                .downcast::<gtk::Label>()
                .unwrap()
                .text()
                .contains("Изменение подтверждено")
    });
    assert_player_unchanged(this, &queue, serial, generation);
    capture_editor_matrix(window, &dialog);
    assert_player_unchanged(this, &queue, serial, generation);
    close_editor(window, this);
}
fn response_button(dialog: &adw::AlertDialog, label: &str) -> gtk::Button {
    named_all(dialog.upcast_ref(), "*")
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some(label))
        .expect("native confirmation response")
}
fn exercise_delete(window: &adw::ApplicationWindow, this: &Rc<Controller>) {
    let queue = QueueState::capture(this);
    let serial = this.music_serial();
    let generation = this.music_snapshot().unwrap().generation;
    this.present_playlist_editor(playlist(false));
    let dialog = editor_ready(this, 4);
    click(window, &named(&dialog, "music-playlist-manage-delete"));
    pump_until(|| {
        this.playlist_management_confirmation_for_test()
            .is_some_and(|dialog| dialog.is_mapped())
    });
    let confirmation = this.playlist_management_confirmation_for_test().unwrap();
    assert_eq!(confirmation.default_response().as_deref(), Some("cancel"));
    assert_eq!(confirmation.close_response(), "cancel");
    let cancel = response_button(&confirmation, "Отмена");
    super::super::super::test_input::wait_for_layout(window, &cancel);
    assert!(cancel.has_focus());
    key("Return");
    pump_until(|| this.playlist_management_confirmation_for_test().is_none());
    assert_player_unchanged(this, &queue, serial, generation);
    assert!(this.playlist_management_dialog_for_test().is_some());
    click(window, &named(&dialog, "music-playlist-manage-delete"));
    pump_until(|| {
        this.playlist_management_confirmation_for_test()
            .is_some_and(|dialog| dialog.is_mapped())
    });
    let confirmation = this.playlist_management_confirmation_for_test().unwrap();
    click(window, &response_button(&confirmation, "Удалить плейлист"));
    pump_until(|| {
        !this.mutation_busy.get()
            && named(&dialog, "music-playlist-manage-message")
                .downcast::<gtk::Label>()
                .unwrap()
                .text()
                .contains("Плейлист удалён")
    });
    assert_player_unchanged(this, &queue, serial, generation);
    close_editor(window, this);
}

fn remote_counts(input: &Input) -> Counts {
    let origin = input.origin.clone();
    desktop(move || {
        serde_json::from_slice(
            &reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap()
                .get(format!("{origin}__fixture/counts"))
                .send()
                .unwrap()
                .bytes()
                .unwrap(),
        )
        .unwrap()
    })
}

fn capture_editor_matrix(window: &adw::ApplicationWindow, dialog: &adw::Dialog) {
    let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") else {
        return;
    };
    fs::create_dir_all(&directory).unwrap();
    let scheme = adw::StyleManager::default().color_scheme();
    // This records settled static layout; the separate ordinary-motion gates
    // remain enabled. Every control above was exercised through native input.
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
    for (name, width, height) in [
        ("compact", 420, 720),
        ("medium", 900, 650),
        ("wide", 1440, 900),
    ] {
        window.set_default_size(width, height);
        pump_until(|| {
            (window.width() - width).abs() <= 32 && (window.height() - height).abs() <= 32
        });
        window.set_default_size(
            width + (width - window.width()),
            height + (height - window.height()),
        );
        pump_until(|| window.width() == width && window.height() == height);
        let close = named(dialog, "music-playlist-manage-close");
        super::super::super::test_input::wait_for_layout(window, &close);
        for (theme, color) in [
            ("light", adw::ColorScheme::ForceLight),
            ("dark", adw::ColorScheme::ForceDark),
        ] {
            adw::StyleManager::default().set_color_scheme(color);
            super::super::super::test_input::wait_for_layout(window, &close);
            let bounds = close
                .compute_bounds(window)
                .expect("close control belongs to this window");
            assert!(
                bounds.x() >= 0.0
                    && bounds.y() >= 0.0
                    && bounds.x() + bounds.width() <= width as f32
                    && bounds.y() + bounds.height() <= height as f32,
                "native dialog close remains inside the viewport"
            );
            assert!(
                Command::new("import")
                    .args(["-silent", "-screen", "-window", &window_id()])
                    .arg(
                        PathBuf::from(&directory)
                            .join(format!("v07-playlist-editor-{name}-{theme}.png"))
                    )
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
    adw::StyleManager::default().set_color_scheme(scheme);
    window.set_default_size(1000, 900);
    pump_until(|| (window.width() - 1000).abs() <= 32 && (window.height() - 900).abs() <= 32);
    super::super::super::test_input::wait_for_layout(
        window,
        &named(dialog, "music-playlist-manage-close"),
    );
}
