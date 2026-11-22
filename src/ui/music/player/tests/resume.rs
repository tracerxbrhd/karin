// SPDX-License-Identifier: GPL-3.0-or-later
//! Long pauses use controlled admission age, original audio and local HTTP only.

use super::*;
use crate::{
    playback::PlaybackSourceKind,
    queue::{ContextKind, QueueSnapshot, RepeatMode},
};
use std::collections::BTreeMap;

fn close_window(app: adw::Application, ui: crate::ui::MainWindow) {
    let engine = ui.music.controller.music_controller_for_test();
    let cleanup = ui.close_for_smoke();
    drop(app);
    pump_until(|| {
        cleanup(false)
            && engine
                .as_ref()
                .is_none_or(|engine| !engine.snapshot().worker_running)
    });
    assert!(cleanup(true));
}

fn wait_retired_engine(engine: &PlaybackController) {
    // Session/widget retirement is synchronous; native pipeline teardown is
    // deliberately worker-owned. Await that actual receipt under the same
    // native-fixture deadline, while checking that late refresh cannot play.
    pump_until(|| {
        let snapshot = engine.snapshot();
        assert_ne!(
            snapshot.state,
            PlaybackState::Playing,
            "a retired paused occurrence must never start a late source"
        );
        !snapshot.worker_running
    });
    assert!(!engine.snapshot().worker_running);
}

fn metadata_path() -> glib::variant::ObjectPath {
    desktop_property("Metadata")
        .get::<BTreeMap<String, glib::Variant>>()
        .unwrap()["mpris:trackid"]
        .get::<glib::variant::ObjectPath>()
        .unwrap()
}

fn start_paused(this: &Rc<Controller>, id: &str) -> PlaybackController {
    this.play_music_context(
        PlaybackContext::new(
            ContextKind::Playlist,
            Some("8".into()),
            vec![track("42"), track(id), track("43")],
            1,
            false,
        )
        .unwrap(),
    );
    wait_playing(this, 1);
    let engine = this.player.borrow().controller.clone().unwrap();
    pump_until(|| !engine.snapshot().playing_time.is_zero());
    let seek = engine.snapshot().seek_serial;
    // Fixture setup uses the actual native command without registering a user
    // MPRIS Seeked. Only later deliberate seek input may publish that signal.
    engine.seek(Duration::from_millis(500)).unwrap();
    pump_until(|| {
        let snapshot = engine.snapshot();
        snapshot.seek_serial > seek
            && snapshot.position.is_some_and(|position| {
                position >= Duration::from_millis(450) && position < Duration::from_millis(900)
            })
    });
    this.pause_music();
    pump_until(|| engine.snapshot().state == PlaybackState::Paused);
    pump_until(|| this.system_media_available());
    engine
}

fn age_paused(engine: &PlaybackController, minutes: u64) {
    let before = engine.snapshot();
    engine
        .advance_source_age_for_test(Duration::from_secs(minutes * 60))
        .unwrap();
    pump_until(|| engine.snapshot().source_expired);
    let after = engine.snapshot();
    assert_eq!(after.state, PlaybackState::Paused);
    assert_eq!(
        after.last_error, None,
        "time passing is not a playback failure"
    );
    assert_eq!(after.position, before.position);
    assert_eq!(after.playing_time, before.playing_time);
}

fn original_context_ages_and_mpris() {
    for minutes in [10, 30, 60] {
        let fixture = Fixture::new();
        let (app, ui) = player_window(&fixture);
        let this = &ui.music.controller;
        let engine = start_paused(this, "42");
        this.set_music_shuffle(true);
        this.set_music_repeat(RepeatMode::One);
        this.set_music_volume(0.37);
        pump_until(|| (engine.snapshot().volume - 0.37).abs() < 0.001);
        let before = engine.snapshot();
        let occurrence = this
            .model
            .borrow()
            .queue
            .current_occurrence_id()
            .unwrap()
            .clone();
        let context = QueueSnapshot::capture(&this.model.borrow().queue)
            .unwrap()
            .unwrap();
        let serial = this.music_serial();
        let path = metadata_path();
        let offers = fixture.offers(0);
        let bus = desktop_background(|| {
            gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap()
        });
        let seeks = Rc::new(RefCell::new(Vec::<i64>::new()));
        let observed = seeks.clone();
        let seek_signal = bus.subscribe_to_signal(
            Some(crate::mpris::BUS_NAME),
            Some("org.mpris.MediaPlayer2.Player"),
            Some("Seeked"),
            Some("/org/mpris/MediaPlayer2"),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                observed
                    .borrow_mut()
                    .push(signal.parameters.get::<(i64,)>().unwrap().0)
            },
        );
        let expired_message = Rc::new(Cell::new(false));
        let noticed = expired_message.clone();
        this.widgets.status.connect_subtitle_notify(move |row| {
            if row
                .subtitle()
                .is_some_and(|message| message.contains("истёк"))
            {
                noticed.set(true);
            }
        });
        age_paused(&engine, minutes);
        assert_eq!(
            fixture.offers(0),
            offers,
            "Pause never polls delivery in the background"
        );
        assert_eq!(metadata_path(), path);
        match minutes {
            10 => native_click(&ui.window, &this.widgets.play),
            30 => desktop_method("Play", ().to_variant()),
            _ => {
                native_click(&ui.window, &this.widgets.current_art_button);
                pump_until(|| this.track_pane.play.is_mapped());
                native_click(&ui.window, &this.track_pane.play);
            }
        }
        pump_until(|| {
            let snapshot = engine.snapshot();
            snapshot.state == PlaybackState::Playing
                && !snapshot.source_expired
                && snapshot.position.is_some_and(|position| {
                    position >= before.position.unwrap()
                        && position - before.position.unwrap() < Duration::from_millis(300)
                })
                && fixture.offers(0) == offers + 1
        });
        assert_eq!(engine.snapshot().generation, before.generation);
        assert_eq!(
            engine.snapshot().source_revision,
            before.source_revision + 1
        );
        assert_eq!(engine.snapshot().seek_serial, before.seek_serial);
        assert!(engine.snapshot().playing_time >= before.playing_time);
        assert!((engine.snapshot().volume - 0.37).abs() < 0.001);
        assert_eq!(this.music_serial(), serial);
        assert_eq!(
            this.model.borrow().queue.current_occurrence_id(),
            Some(&occurrence)
        );
        assert_eq!(
            QueueSnapshot::capture(&this.model.borrow().queue).unwrap(),
            Some(context)
        );
        assert_eq!(
            metadata_path(),
            path,
            "source renewal is not an MPRIS track change"
        );
        assert!(desktop_property("Shuffle").get::<bool>().unwrap());
        assert_eq!(
            desktop_property("LoopStatus").get::<String>().unwrap(),
            "Track"
        );
        assert_eq!(desktop_property("Volume").get::<f64>(), Some(0.37));
        assert!(
            seeks.borrow().is_empty(),
            "internal position restoration is not Seeked"
        );
        assert!(
            !expired_message.get(),
            "successful renewal never presents the technical expiry error"
        );
        assert_eq!(fixture.counts.lock().unwrap().wave_sessions, 0);
        drop(seek_signal);
        close_window(app, ui);
        assert!(!engine.snapshot().worker_running);
    }
}

fn ordinary_resume_and_expiration_race() {
    let fixture = Fixture::new();
    let (app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    let engine = start_paused(this, "42");
    let offers = fixture.offers(0);
    let before = engine.snapshot();
    this.play_music();
    pump_until(|| engine.snapshot().state == PlaybackState::Playing);
    assert_eq!(
        fixture.offers(0),
        offers,
        "fresh admission uses ordinary Resume"
    );
    assert_eq!(engine.snapshot().generation, before.generation);
    this.pause_music();
    pump_until(|| engine.snapshot().state == PlaybackState::Paused);
    let before = engine.snapshot();
    engine.expire_on_next_resume_for_test().unwrap();
    assert!(!engine.snapshot().source_expired);
    let expired_message = Rc::new(Cell::new(false));
    let noticed = expired_message.clone();
    this.widgets.status.connect_subtitle_notify(move |row| {
        if row
            .subtitle()
            .is_some_and(|message| message.contains("истёк"))
        {
            noticed.set(true);
        }
    });
    this.play_music();
    pump_until(|| {
        engine.snapshot().state == PlaybackState::Playing
            && !engine.snapshot().source_expired
            && fixture.offers(0) == offers + 1
    });
    assert_eq!(engine.snapshot().generation, before.generation);
    assert_eq!(
        engine.snapshot().source_revision,
        before.source_revision + 1
    );
    assert_eq!(engine.snapshot().seek_serial, before.seek_serial);
    assert!(engine.snapshot().position.unwrap() >= before.position.unwrap());
    assert!(!expired_message.get());
    close_window(app, ui);
}

#[derive(Clone, Copy)]
enum PendingAction {
    Pause,
    Stop,
    Next,
    Previous,
    Seek,
    Logout,
    AccountGeneration,
    FutureQueueEdit,
    Close,
}

fn pending_actions_fence_late_play() {
    for action in [
        PendingAction::Pause,
        PendingAction::Stop,
        PendingAction::Next,
        PendingAction::Previous,
        PendingAction::Seek,
        PendingAction::Logout,
        PendingAction::AccountGeneration,
        PendingAction::FutureQueueEdit,
        PendingAction::Close,
    ] {
        let fixture = Fixture::new();
        fixture.release_descriptor();
        let (app, ui) = player_window(&fixture);
        let this = &ui.music.controller;
        let engine = start_paused(this, "45");
        let before = engine.snapshot();
        let occurrence = this
            .model
            .borrow()
            .queue
            .current_occurrence_id()
            .unwrap()
            .clone();
        let offers = fixture.offers(3);
        let seeks = Rc::new(RefCell::new(Vec::<i64>::new()));
        let seek_signal = if matches!(action, PendingAction::Seek) {
            let observed = seeks.clone();
            let bus = desktop_background(|| {
                gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap()
            });
            Some(bus.subscribe_to_signal(
                Some(crate::mpris::BUS_NAME),
                Some("org.mpris.MediaPlayer2.Player"),
                Some("Seeked"),
                Some("/org/mpris/MediaPlayer2"),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    observed
                        .borrow_mut()
                        .push(signal.parameters.get::<(i64,)>().unwrap().0)
                },
            ))
        } else {
            None
        };
        age_paused(&engine, 30);
        fixture.hold_descriptor();
        this.play_music();
        pump_until(|| fixture.counts.lock().unwrap().slow_started);
        assert!(this.music_requested());
        assert_eq!(engine.snapshot().state, PlaybackState::Paused);
        this.play_music();
        assert_eq!(
            fixture.offers(3),
            offers + 1,
            "double Play coalesces into one refresh"
        );
        if matches!(action, PendingAction::Close) {
            let cleanup = ui.close_for_smoke();
            fixture.release_descriptor();
            drop(app);
            pump_until(|| cleanup(false) && !engine.snapshot().worker_running);
            assert!(cleanup(true));
            continue;
        }
        match action {
            PendingAction::Pause => this.pause_music(),
            PendingAction::Stop => desktop_method("Stop", ().to_variant()),
            PendingAction::Next => this.skip_music(true, false),
            PendingAction::Previous => this.skip_music(false, false),
            PendingAction::Seek => this.seek_music(Duration::from_millis(1_700)),
            PendingAction::Logout => this.logout(),
            PendingAction::AccountGeneration => {
                this.model.borrow_mut().generation += 1;
            }
            PendingAction::FutureQueueEdit => {
                this.enqueue(track("42"));
                this.remove_music_queue(2);
                assert_eq!(
                    this.model.borrow().queue.current_occurrence_id(),
                    Some(&occurrence)
                );
            }
            PendingAction::Close => unreachable!(),
        }
        fixture.release_descriptor();
        pump_until(|| this.cache_work_settled_for_test() && !this.music_requested());
        match action {
            PendingAction::Pause | PendingAction::AccountGeneration => {
                assert_eq!(engine.snapshot().state, PlaybackState::Paused);
                assert_eq!(engine.snapshot().playing_time, before.playing_time);
                assert_eq!(engine.snapshot().generation, before.generation);
                assert_eq!(
                    this.model.borrow().queue.current_occurrence_id(),
                    Some(&occurrence)
                );
            }
            PendingAction::Stop => {
                assert!(this.music_snapshot().is_none());
                wait_retired_engine(&engine);
                assert!(!engine.snapshot().worker_running);
                assert_eq!(
                    this.model.borrow().queue.current_occurrence_id(),
                    Some(&occurrence)
                );
            }
            PendingAction::Next | PendingAction::Previous => {
                let expected = if matches!(action, PendingAction::Next) {
                    "43"
                } else {
                    "42"
                };
                pump_until(|| {
                    this.model
                        .borrow()
                        .queue
                        .current()
                        .is_some_and(|track| track.id == expected)
                        && this
                            .music_snapshot()
                            .is_some_and(|snapshot| snapshot.state == PlaybackState::Playing)
                });
                assert_ne!(
                    this.model.borrow().queue.current_occurrence_id(),
                    Some(&occurrence)
                );
                wait_retired_engine(&engine);
                assert!(!engine.snapshot().worker_running);
            }
            PendingAction::Seek => {
                pump_until(|| {
                    let snapshot = engine.snapshot();
                    snapshot.state == PlaybackState::Playing
                        && snapshot
                            .position
                            .is_some_and(|position| position >= Duration::from_millis(1_650))
                });
                assert_eq!(engine.snapshot().generation, before.generation);
                assert_eq!(engine.snapshot().seek_serial, before.seek_serial + 1);
                assert_eq!(
                    engine.snapshot().seek_position,
                    Some(Duration::from_millis(1_700))
                );
                assert_eq!(
                    this.model.borrow().queue.current_occurrence_id(),
                    Some(&occurrence)
                );
                pump_until(|| seeks.borrow().contains(&1_700_000));
                assert_eq!(
                    seeks.borrow().as_slice(),
                    &[1_700_000],
                    "only the real user seek is acknowledged"
                );
            }
            PendingAction::Logout => {
                assert!(this.music_snapshot().is_none());
                assert!(this.model.borrow().token.is_none());
                assert!(this.model.borrow().queue.items().is_empty());
                wait_retired_engine(&engine);
                assert!(!engine.snapshot().worker_running);
            }
            PendingAction::FutureQueueEdit => {
                pump_until(|| engine.snapshot().state == PlaybackState::Playing);
                assert_eq!(engine.snapshot().generation, before.generation);
                assert_eq!(
                    this.model.borrow().queue.current_occurrence_id(),
                    Some(&occurrence)
                );
                assert_eq!(this.model.borrow().queue.items().len(), 3);
            }
            PendingAction::Close => unreachable!(),
        }
        assert_eq!(
            fixture.offers(3),
            offers + 1,
            "late response cannot create another refresh"
        );
        drop(seek_signal);
        close_window(app, ui);
    }
}

fn rejected_refresh_is_bounded_and_retry_remains_explicit() {
    let fixture = Fixture::new();
    let (app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    let engine = start_paused(this, "42");
    let occurrence = this
        .model
        .borrow()
        .queue
        .current_occurrence_id()
        .unwrap()
        .clone();
    age_paused(&engine, 60);
    let offers = fixture.offers(0);
    fixture.counts.lock().unwrap().reject_delivery_track = Some(42);
    this.play_music();
    pump_until(|| !this.music_requested() && this.cache_work_settled_for_test());
    assert!(this.player.borrow().failure.is_some());
    assert!(
        this.widgets
            .status
            .subtitle()
            .is_some_and(|message| !message.is_empty())
    );
    assert_eq!(fixture.offers(0), offers + 1);
    assert_eq!(
        this.model.borrow().queue.current_occurrence_id(),
        Some(&occurrence)
    );
    let until = Instant::now() + Duration::from_millis(300);
    pump_until(|| Instant::now() >= until);
    assert_eq!(
        fixture.offers(0),
        offers + 1,
        "ticks do not retry a rejected refresh"
    );
    fixture.counts.lock().unwrap().reject_delivery_track = None;
    this.play_music();
    pump_until(|| {
        this.music_snapshot().is_some_and(|snapshot| {
            snapshot.state == PlaybackState::Playing && !snapshot.source_expired
        })
    });
    assert_eq!(fixture.offers(0), offers + 2);
    assert_eq!(
        this.model.borrow().queue.current_occurrence_id(),
        Some(&occurrence)
    );
    close_window(app, ui);
}

fn current_access_renewal_can_reuse_the_existing_audio_cache() {
    use crate::audio_cache::{
        AudioCache, CachePolicy, OnlineStreamAuthority, ResourceKey,
        test_support::SyntheticAuthority,
    };
    let fixture = Fixture::new();
    let (app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    let directory = std::env::temp_dir().join(format!(
        "karin-native-resume-cache-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let directory = GeneratedDirectory(directory);
    let token = this.model.borrow().token.as_ref().unwrap().clone();
    let scope = this.service.as_ref().unwrap().cache_scope(&token).unwrap();
    let permission = Arc::new(SyntheticAuthority::default());
    permission.allow(&scope, &ResourceKey::track("42").unwrap());
    let budget = crate::cache_budget::CacheBudget::new(
        directory.0.join("metadata"),
        directory.0.join("audio"),
        true,
        Some(2 * 1024 * 1024 * 1024),
    )
    .unwrap();
    let cache = AudioCache::open_with_budget(
        directory.0.join("audio"),
        Arc::new(OnlineStreamAuthority::with_storage_permission(permission)),
        CachePolicy { max_bytes: 1 },
        budget,
    )
    .unwrap();
    this.install_audio_cache_for_test(cache.clone(), scope);
    let engine = start_paused(this, "42");
    pump_until(|| {
        cache.stats().is_ok_and(|stats| stats.entries == 1) && this.cache_work_settled_for_test()
    });
    let before = engine.snapshot();
    let offers = fixture.offers(0);
    let media = fixture.counts.lock().unwrap().media[0];
    age_paused(&engine, 30);
    this.play_music();
    pump_until(|| {
        let snapshot = engine.snapshot();
        snapshot.state == PlaybackState::Playing
            && snapshot.source_kind == PlaybackSourceKind::CachedFile
            && snapshot
                .position
                .is_some_and(|position| position >= before.position.unwrap())
    });
    assert_eq!(
        fixture.offers(0),
        offers + 1,
        "local reuse still checks current authenticated track access"
    );
    assert_eq!(
        fixture.counts.lock().unwrap().media[0],
        media,
        "renewal reads the retained inode, without another media download"
    );
    assert_eq!(engine.snapshot().generation, before.generation);
    assert_eq!(engine.snapshot().seek_serial, before.seek_serial);
    drop(cache);
    close_window(app, ui);
}

fn wave_renewal_keeps_one_cycle_and_natural_eos() {
    let fixture = Fixture::new();
    let (app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    this.start_wave();
    wait_playing(this, 0);
    pump_until(|| {
        fixture
            .counts
            .lock()
            .unwrap()
            .wave_events
            .iter()
            .any(|event| event["event"]["type"] == "trackStarted")
    });
    let engine = this.player.borrow().controller.clone().unwrap();
    let seek = engine.snapshot().seek_serial;
    engine.seek(Duration::from_millis(700)).unwrap();
    pump_until(|| engine.snapshot().seek_serial > seek);
    this.pause_music();
    pump_until(|| engine.snapshot().state == PlaybackState::Paused);
    let before = engine.snapshot();
    let serial = this.music_serial();
    let occurrence = this
        .model
        .borrow()
        .queue
        .current_occurrence_id()
        .unwrap()
        .clone();
    let cycle = this.wave_cycle_for_test();
    let path = metadata_path();
    age_paused(&engine, 30);
    this.play_music();
    pump_until(|| {
        engine.snapshot().state == PlaybackState::Playing && !engine.snapshot().source_expired
    });
    assert_eq!(engine.snapshot().generation, before.generation);
    assert!(engine.snapshot().playing_time >= before.playing_time);
    assert_eq!(this.music_serial(), serial);
    assert_eq!(this.wave_cycle_for_test(), cycle);
    assert_eq!(
        this.model.borrow().queue.current_occurrence_id(),
        Some(&occurrence)
    );
    assert_eq!(metadata_path(), path);
    {
        let counts = fixture.counts.lock().unwrap();
        assert_eq!(counts.wave_sessions, 1);
        for event in ["radioStarted", "trackStarted"] {
            assert_eq!(
                counts
                    .wave_events
                    .iter()
                    .filter(|value| value["event"]["type"] == event)
                    .count(),
                1
            );
        }
        assert!(
            !counts
                .wave_events
                .iter()
                .any(|value| value["event"]["type"] == "skip")
        );
    }
    pump_until(|| {
        this.model.borrow().queue.current_occurrence_id() != Some(&occurrence)
            && this
                .music_snapshot()
                .is_some_and(|snapshot| snapshot.state == PlaybackState::Playing)
    });
    pump_until(|| {
        fixture
            .counts
            .lock()
            .unwrap()
            .wave_events
            .iter()
            .any(|value| value["event"]["type"] == "trackFinished")
    });
    assert_eq!(fixture.counts.lock().unwrap().wave_sessions, 1);
    assert_eq!(
        fixture
            .counts
            .lock()
            .unwrap()
            .wave_events
            .iter()
            .filter(|value| value["event"]["type"] == "trackFinished")
            .count(),
        1
    );
    close_window(app, ui);
}

#[test]
#[ignore = "requires isolated session D-Bus, Xvfb, xdotool and real generated MP3 playback"]
fn native_music_resume_expired_source_preserves_occurrence_position_and_cancellation() {
    gtk::init().unwrap();
    adw::init().unwrap();
    crate::ui::style::install();
    original_context_ages_and_mpris();
    ordinary_resume_and_expiration_race();
    pending_actions_fence_late_play();
    rejected_refresh_is_bounded_and_retry_remains_explicit();
    current_access_renewal_can_reuse_the_existing_audio_cache();
    wave_renewal_keeps_one_cycle_and_natural_eos();
}
