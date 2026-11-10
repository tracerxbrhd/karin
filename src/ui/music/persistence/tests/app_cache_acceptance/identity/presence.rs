// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual native cache markers, including cold row-only restore and eviction.
use super::*;
use crate::music::{Page, Playlist, PlaylistId};
use crate::ui::music::workspace::PanelMode;

const CASES: [&str; 2] = ["presence-live", "presence-restart"];
const SAVED: &str = "Аудио сохранено в кеше. При воспроизведении проверяется доступ.";
const LOCAL: &str = "Воспроизводится из локального кеша";
type RowActivations = Rc<Cell<(u32, u32)>>;

pub(super) fn run_parent() {
    let directory = Directory::new();
    for kind in ["config", "cache", "data", "state"] {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(directory.0.join(kind))
            .unwrap();
    }
    let input = Input {
        directory: directory.0.clone(),
        origin: String::new(),
        sha256: [0; 32],
        bytes: 0,
        license: "CC0-1.0".into(),
    };
    let path = directory.0.join("presence-input.json");
    fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
    let mut cases = Vec::new();
    for phase in CASES {
        let report = run_phase(&path, phase, &directory.0);
        let details: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.0.join("presence-result.json")).unwrap())
                .unwrap();
        assert_eq!(details["pid"], report.pid);
        assert_eq!(details["phase"], phase);
        assert_eq!(details["passed"], true);
        cases.push(details);
    }
    assert_ne!(cases[0]["pid"], cases[1]["pid"]);
    if let Some(path) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
        fs::create_dir_all(&path).unwrap();
        fs::write(PathBuf::from(path).join("audio-cache-presence-receipt.json"),serde_json::to_vec_pretty(&json!({"scope":"ordinary-application-native-cache-presence","real_yandex_service":false,"cases":cases,"processes":CASES.len()})).unwrap()).unwrap();
    }
}

struct WindowManager(Option<Child>);
impl WindowManager {
    fn start() -> Self {
        let existing = Command::new("xprop")
            .args(["-root", "_NET_SUPPORTING_WM_CHECK"])
            .output()
            .unwrap();
        if String::from_utf8_lossy(&existing.stdout).contains("window id #") {
            return Self(None);
        }
        let child = Command::new("openbox")
            .arg("--sm-disable")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let existing = Command::new("xprop")
                .args(["-root", "_NET_SUPPORTING_WM_CHECK"])
                .output()
                .unwrap();
            if String::from_utf8_lossy(&existing.stdout).contains("window id #") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "isolated native window manager started"
            );
            thread::sleep(Duration::from_millis(5));
        }
        Self(Some(child))
    }
}
impl Drop for WindowManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(super) fn run_child(original: &Input, phase: &str) {
    let _manager = WindowManager::start();
    let server = Server::new(&original.directory);
    server.mode.lock().unwrap().digest = true;
    let input = make_input(&original.directory, &server);
    if phase == "presence-live" {
        server.hold_background.store(true, Ordering::Release);
    }
    let (_app, ui) = application(&input);
    let this = &ui.music.controller;
    let row = this.widgets.library_rows.row_at_index(0).unwrap();
    assert!(this.model.borrow().queue.current().is_none());
    assert!(!this.widgets.player_revealer.reveals_child());
    if phase == "presence-live" {
        wait_presence(this, "42", Presence::Missing);
        pointer(&ui.window, &row);
        wait_source(this, PlaybackSourceKind::RemoteHttps);
        pump_until(|| server.hold.entered.load(Ordering::Acquire));
        this.pause_music();
        pump_until(|| {
            this.music_snapshot()
                .is_some_and(|s| s.state == PlaybackState::Paused)
        });
        wait_presence(this, "42", Presence::Downloading);
        assert_eq!(server.counts().background_gets, 1);
        assert!(
            media_files(&audio_root(&input)).is_empty(),
            "incomplete background input has no saved marker/file"
        );
        server.hold.release();
        server.hold_background.store(false, Ordering::Release);
        wait_publication(this, &input);
        wait_presence(this, "42", Presence::Stored);
        assert_saved(&input, None);
        this.sync_track_pane();
        this.set_info_open(true);
        this.tick_cache_presence();
        pump_until(|| {
            this.cache_indicators_for_test("42")
                .iter()
                .any(|slot| slot.widget_name() == "music-track-pane-cache")
        });
        assert_roles(
            this,
            &[
                "music-track-cache",
                "music-queue-cache",
                "music-current-cache",
                "music-track-pane-cache",
            ],
        );
        assert_labels(this, false);
        // Cache validation must not temporarily remove already confirmed icons.
        server.hold_heads.store(true, Ordering::Release);
        this.set_info_open(false);
        let serial = this.music_serial();
        assert!(row.grab_focus());
        key(&ui.window, "Return");
        pump_until(|| {
            this.music_serial() > serial && server.hold_head.entered.load(Ordering::Acquire)
        });
        wait_presence(this, "42", Presence::Stored);
        assert_labels(this, false);
        server.hold_head.release();
        server.hold_heads.store(false, Ordering::Release);
        wait_source(this, PlaybackSourceKind::CachedFile);
        wait_presence(this, "42", Presence::Stored);
        pump_until(|| {
            this.widgets
                .current_cache
                .tooltip_text()
                .is_some_and(|t| t.contains(LOCAL))
        });
        assert_labels(this, true);
        this.pause_music();
        pump_until(|| {
            this.music_snapshot()
                .is_some_and(|s| s.state == PlaybackState::Paused)
        });
        let counts = server.counts();
        assert_eq!(counts.native_gets, 1);
        assert_eq!(counts.background_gets, 1);
        // The same actual track appears in each existing common row factory.
        this.open_playlist(
            Playlist {
                id: PlaylistId {
                    owner: "1234".into(),
                    kind: "77".into(),
                },
                title: "Original fixture playlist".into(),
                track_count: 2,
                cover: None,
            },
            0,
        );
        pump_until(|| {
            this.widgets.stack.visible_child_name().as_deref() == Some("detail")
                && !this.model.borrow().busy
        });
        wait_presence(this, "42", Presence::Stored);
        assert!(find_named(this.widgets.detail.upcast_ref(), "music-track-cache").is_some());
        capture_native("audio-cache-saved-playlist");
        this.widgets.search.set_text("Original");
        this.search(0);
        pump_until(|| {
            this.widgets.search_rows.row_at_index(0).is_some() && !this.model.borrow().busy
        });
        wait_presence(this, "42", Presence::Stored);
        let found = this.widgets.search_rows.row_at_index(0).unwrap();
        assert!(find_named(found.upcast_ref(), "music-track-cache").is_some());
        pointer(&ui.window, &found);
        wait_source(this, PlaybackSourceKind::CachedFile);
        wait_presence(this, "42", Presence::Stored);
        this.pause_music();
        pump_until(|| {
            this.music_snapshot()
                .is_some_and(|s| s.state == PlaybackState::Paused)
        });
        assert_eq!(
            server.counts().native_gets,
            1,
            "a saved search row click must use local data"
        );
        assert_eq!(server.counts().background_gets, 1);
        capture_native("audio-cache-saved-search");
        pointer(&ui.window, &this.widgets.nav_liked);
        pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("library"));
        this.toggle_queue_panel();
        pump_until(|| this.panel_mode.get() == PanelMode::Queue);
        wait_presence(this, "42", Presence::Stored);
        capture_native("audio-cache-saved-queue");
        this.set_panel_mode(PanelMode::Closed);
        // A second real selected source leaves track 42 off the fixed targets.
        this.skip_music(true, false);
        // Next preserves the paused user's intent. Explicit Play arms the
        // newly selected source and its otherwise deliberately unarmed fill.
        this.play_music();
        wait_source(this, PlaybackSourceKind::RemoteHttps);
        wait_presence(this, "43", Presence::Stored);
        this.pause_music();
        pump_until(|| {
            this.music_snapshot()
                .is_some_and(|s| s.state == PlaybackState::Paused)
        });
        large_view(&ui, this, &server);
        let current = this
            .model
            .borrow()
            .queue
            .items()
            .iter()
            .find(|track| track.id == "42")
            .unwrap()
            .clone();
        this.select_track_info(current, None, false);
        this.set_info_open(true);
        this.tick_cache_presence();
        assert!(
            this.track_pane.cache_indicator.is_visible(),
            "rebinding a known saved offscreen song has no false missing frame"
        );
        wait_presence(this, "42", Presence::Stored);
        scroll_to_top(&ui, this);
        let geometry = geometry(&ui, this);
        // A renderer or presence observer must never request another audio body.
        let after = server.counts();
        assert_eq!(after.native_gets, 2);
        assert_eq!(after.background_gets, 2);
        let report = json!({"phase":phase,"pid":std::process::id(),"passed":true,"first_source":"remote-https","replay_source":"cached-file","spinner_from_actual_worker":true,"publication_synced_roles":4,"playlist_search_queue":true,"large_loaded_rows":500,"no_row_http":true,"light_dark":true,"actual_maximized":true,"scale":ui.window.scale_factor(),"geometry":geometry,"wire":after});
        close_child(&ui, this);
        finish(&input, phase, report, "remote-https", Some("cached-file"));
    } else {
        // This process has no current song, footer or Info selection. Presence
        // comes from the actual completed disk record before any stream check.
        wait_presence(this, "42", Presence::Stored);
        assert!(this.model.borrow().queue.current().is_none());
        assert_eq!(server.counts().offers, 0);
        assert_eq!(server.counts().gets(), 0);
        assert!(
            this.cache_indicators_for_test("42")
                .iter()
                .any(|slot| slot.widget_name() == "music-track-cache" && slot.is_mapped())
        );
        pointer(&ui.window, &row);
        wait_primary_source(this, PlaybackSourceKind::CachedFile, None);
        wait_presence(this, "42", Presence::Stored);
        assert_eq!(
            server.counts().gets(),
            0,
            "cold restored cache must not request an audio body"
        );
        // Actual aggregate LRU eviction is performed on its worker. The small
        // byte budget is test-only; the six user choices remain unchanged.
        let budget = this.persistence.borrow().cache_budget.clone().unwrap();
        let before = this.music_snapshot().unwrap();
        let work = budget.clone();
        let worker = thread::spawn(move || {
            work.configure(true, Some(1)).unwrap();
            work.trim().unwrap();
        });
        worker.join().unwrap();
        wait_presence(this, "42", Presence::Missing);
        wait_presence(this, "43", Presence::Missing);
        assert!(media_files(&audio_root(&input)).is_empty());
        assert_eq!(this.music_snapshot().unwrap().generation, before.generation);
        assert_eq!(this.music_snapshot().unwrap().state, PlaybackState::Playing);
        budget
            .configure(true, Some(2 * 1024 * 1024 * 1024))
            .unwrap();
        fresh_primary(&ui, this, PlaybackSourceKind::RemoteHttps);
        wait_publication(this, &input);
        wait_presence_at(this, "42", Presence::Stored, "after-lru-repair");
        assert_labels(this, false);
        // Clear removes the actual data while the admitted descriptor continues.
        let generation = this.music_snapshot().unwrap().generation;
        this.clear_persistent_cache();
        pump_until(|| {
            !this.persistence.borrow().cache_busy && !this.persistence.borrow().audio_purge_pending
        });
        wait_presence_at(this, "42", Presence::Missing, "after-explicit-clear");
        assert!(media_files(&audio_root(&input)).is_empty());
        assert_eq!(this.music_snapshot().unwrap().generation, generation);
        assert_eq!(this.music_snapshot().unwrap().state, PlaybackState::Playing);
        // A fresh online save must reset both tooltip AND accessible local cue.
        fresh_primary(&ui, this, PlaybackSourceKind::RemoteHttps);
        wait_publication(this, &input);
        wait_presence(this, "42", Presence::Stored);
        assert_labels(this, false);
        let counts = server.counts();
        let mut values = this.persistence.borrow().values.clone();
        values.cache_enabled = false;
        this.preferences_changed(values);
        wait_presence(this, "42", Presence::Missing);
        assert_eq!(this.music_snapshot().unwrap().state, PlaybackState::Playing);
        let mut values = this.persistence.borrow().values.clone();
        values.cache_enabled = true;
        this.preferences_changed(values);
        pump_until(|| settled(this));
        wait_presence(this, "42", Presence::Stored);
        assert_eq!(
            server.counts().gets(),
            counts.gets(),
            "off/on presence needs no stream requests"
        );
        // Corruption cannot leave a saved icon. This is worker-side disk input,
        // never a manually assigned UI state or a fake storage authority.
        let path = record(&input);
        thread::spawn(move || fs::write(path, b"incomplete fixture input").unwrap())
            .join()
            .unwrap();
        wait_presence(this, "42", Presence::Missing);
        server.mode.lock().unwrap().broken_background = true;
        fresh_primary(&ui, this, PlaybackSourceKind::RemoteHttps);
        pump_until(|| this.cache_work_settled_for_test());
        wait_presence(this, "42", Presence::Missing);
        assert!(
            !completed_media_matches(&audio_root(&input), input.bytes, input.sha256),
            "partial repair cannot expose an integrity-verified complete record"
        );
        server.mode.lock().unwrap().broken_background = false;
        fresh_primary(&ui, this, PlaybackSourceKind::RemoteHttps);
        wait_publication(this, &input);
        wait_presence(this, "42", Presence::Stored);
        server.mode.lock().unwrap().account = 5678;
        this.authenticate(AccessToken::for_test(
            "synthetic-storage-other-token",
            Duration::from_secs(300),
        ));
        pump_until(|| this.model.borrow().account_valid && settled(this));
        pointer(&ui.window, &this.widgets.nav_liked);
        pump_until(|| this.widgets.library_rows.row_at_index(0).is_some());
        wait_presence(this, "42", Presence::Missing);
        this.logout();
        pump_until(|| !this.model.borrow().account_valid);
        wait_presence(this, "42", Presence::Missing);
        let report = json!({"phase":phase,"pid":std::process::id(),"passed":true,"startup_stored_without_player":true,"startup_media_gets":0,"replay_source":"cached-file","lru_eviction_removed_icons":true,"clear_kept_playing":true,"clear_off_account":true,"partial_corrupt_no_icon":true,"accessible_label_reset":true,"wire":server.counts()});
        close_child(&ui, this);
        finish(&input, phase, report, "cached-file", None);
    }
}
fn wait_primary_source(
    this: &Controller,
    kind: PlaybackSourceKind,
    previous_serial: Option<(u64, RowActivations)>,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    pump_until(|| {
        if Instant::now() >= deadline {
            let model = this.model.borrow();
            let current = model
                .queue
                .current()
                .map_or("none", |track| match track.id.as_str() {
                    "42" => "primary",
                    "43" => "secondary",
                    _ => "other",
                });
            panic!(
                "native primary source wait current={current} actual-source={:?} serial={} previous={} row-activations={:?} account-valid={} cache-settled={}",
                this.music_snapshot()
                    .map(|snapshot| (snapshot.state, snapshot.source_kind)),
                this.music_serial(),
                previous_serial.as_ref().map_or(0, |(serial, _)| *serial),
                previous_serial
                    .as_ref()
                    .map(|(_, activations)| activations.get()),
                model.account_valid,
                this.cache_work_settled_for_test(),
            );
        }
        previous_serial
            .as_ref()
            .is_none_or(|(serial, _)| this.music_serial() > *serial)
            && this
                .model
                .borrow()
                .queue
                .current()
                .is_some_and(|track| track.id == "42")
            && this.music_snapshot().is_some_and(|snapshot| {
                snapshot.state == PlaybackState::Playing
                    && !snapshot.playing_time.is_zero()
                    && snapshot.source_kind == kind
            })
    });
}
fn fresh_primary(ui: &crate::ui::MainWindow, this: &Controller, kind: PlaybackSourceKind) {
    // Continuing playback after eviction/Clear is asserted before this call.
    // These repeated maintenance requests use independent native keyboard
    // activation. GtkListBox deliberately suppresses the second mouse press
    // of a rapid double click, even after Stop/reflow. First playback, saved
    // replay and search-row mouse activation are exercised separately.
    let started = Instant::now();
    this.stop_music();
    eprintln!(
        "Karin native cache fixture: step=fresh-primary-stopped elapsed-ms={}",
        started.elapsed().as_millis()
    );
    // Wait for the actual footer's closed allocation before activating the
    // stopped view, rather than racing an unfinished native layout.
    pump_until(|| {
        !this.widgets.player_revealer.is_child_revealed()
            && this.widgets.player_revealer.height() == 0
    });
    let serial = this.music_serial();
    let row = this.widgets.library_rows.row_at_index(0).unwrap();
    let activations = Rc::new(Cell::new((0_u32, 0_u32)));
    let observed = activations.clone();
    let primary = row.downgrade();
    let handler = this
        .widgets
        .library_rows
        .connect_row_activated(move |_, row| {
            let (mut primary_count, mut other_count) = observed.get();
            if primary.upgrade().as_ref() == Some(row) {
                primary_count += 1;
            } else {
                other_count += 1;
            }
            observed.set((primary_count, other_count));
        });
    assert!(row.grab_focus());
    key(&ui.window, "Return");
    eprintln!(
        "Karin native cache fixture: step=fresh-primary-keyboard elapsed-ms={}",
        started.elapsed().as_millis()
    );
    wait_primary_source(this, kind, Some((serial, activations.clone())));
    this.widgets.library_rows.disconnect(handler);
    assert_eq!(
        activations.get(),
        (1, 0),
        "one native Enter activates its actual primary row exactly once"
    );
    eprintln!(
        "Karin native cache fixture: step=fresh-primary-source elapsed-ms={}",
        started.elapsed().as_millis()
    );
}
fn finish(
    input: &Input,
    phase: &str,
    details: serde_json::Value,
    source: &str,
    replay: Option<&str>,
) {
    fs::write(
        input.directory.join("presence-result.json"),
        serde_json::to_vec(&details).unwrap(),
    )
    .unwrap();
    write_phase(
        input,
        PhaseReport {
            phase: phase.into(),
            pid: std::process::id(),
            account_revalidated: true,
            cache_policy: "private-http".into(),
            first_source: source.into(),
            replay_source: replay.map(str::to_owned),
            clear_kept_playing: phase == "presence-restart",
            default_limit_bytes: 2 * 1024 * 1024 * 1024,
        },
    );
}
fn assert_roles(this: &Controller, names: &[&str]) {
    let slots = this.cache_indicators_for_test("42");
    for name in names {
        assert!(
            slots.iter().any(|slot| slot.widget_name() == *name),
            "saved status has expected ordinary view role {name}"
        );
    }
}
fn assert_labels(this: &Controller, local: bool) {
    for slot in this.cache_indicators_for_test("42") {
        let expected = if slot.widget_name() == "music-current-cache" && local {
            format!("{SAVED}\n{LOCAL}")
        } else {
            SAVED.into()
        };
        assert_eq!(slot.tooltip_text().as_deref(), Some(expected.as_str()));
        let image = slot.last_child().and_downcast::<gtk::Image>().unwrap();
        let expected = std::ffi::CString::new(expected).unwrap();
        // GTK's public test API reads the actual accessibility context. These
        // two child processes use GTK_A11Y=test; the ordinary 36 do not change.
        let mismatch = unsafe {
            gtk::ffi::gtk_test_accessible_check_property(
                image.as_ptr().cast(),
                gtk::ffi::GTK_ACCESSIBLE_PROPERTY_LABEL,
                expected.as_ptr(),
            )
        };
        let mismatch: Option<glib::GString> = unsafe { glib::translate::from_glib_full(mismatch) };
        assert!(
            mismatch.is_none(),
            "saved icon accessible label matches actual state: {mismatch:?}"
        );
    }
}
fn large_view(ui: &crate::ui::MainWindow, this: &Rc<Controller>, server: &Server) {
    let original = this
        .model
        .borrow()
        .queue
        .items()
        .iter()
        .find(|t| t.id == "42")
        .unwrap()
        .clone();
    let mut tracks = vec![original.clone()];
    for index in 0..499 {
        let mut track = original.clone();
        track.id = (1000 + index).to_string();
        track.title = format!("Original permitted large fixture {index}");
        tracks.push(track);
    }
    let serial = this.music_serial();
    let before = server.counts();
    let queue = this.model.borrow().queue.context().cloned();
    this.render_liked(
        Ok(Page {
            items: tracks,
            has_more: false,
        }),
        0,
    );
    assert!(this.model.borrow().queue.context() == queue.as_ref());
    assert!(
        this.model
            .borrow()
            .liked
            .as_ref()
            .is_some_and(|page| { page.as_ref().is_ok_and(|page| page.items.len() == 500) })
    );
    this.widgets.stack.set_visible_child_name("library");
    this.set_info_open(false);
    pump_until(|| this.widgets.library_rows.row_at_index(499).is_some());
    let row = this.widgets.library_rows.row_at_index(10).unwrap();
    let scroll = this
        .content_scroll
        .borrow()
        .upgrade()
        .expect("current library scroll viewport");
    let (x, y) = super::super::super::super::super::test_input::pointer(
        &ui.window,
        &scroll,
        super::super::super::super::super::test_input::Target::Center,
    );
    let id = window_id();
    assert!(
        Command::new("xdotool")
            .args([
                "mousemove",
                "--sync",
                "--window",
                &id,
                &x.to_string(),
                &y.to_string(),
                "click",
                "--repeat",
                "12",
                "--delay",
                "15",
                "5"
            ])
            .status()
            .unwrap()
            .success()
    );
    pump_until(|| row.is_mapped() && !this.cache_presence_query_running_for_test());
    let end = Instant::now() + Duration::from_millis(650);
    pump_until(|| Instant::now() >= end && !this.cache_presence_query_running_for_test());
    assert_eq!(this.music_serial(), serial);
    assert_eq!(this.model.borrow().queue.items().len(), 2);
    assert_eq!(server.counts().offers, before.offers);
    assert_eq!(server.counts().gets(), before.gets());
    assert_eq!(
        this.cache_presence_for_test("42"),
        Presence::Stored,
        "an offscreen saved row remains a known disk fact"
    );
}
fn geometry(ui: &crate::ui::MainWindow, this: &Rc<Controller>) -> Vec<serde_json::Value> {
    let mut records = Vec::new();
    for (name, width, height) in [
        ("compact", 420, 720),
        ("medium", 900, 650),
        ("wide", 1440, 900),
    ] {
        ui.window.unmaximize();
        if name == "compact" {
            this.set_info_open(false);
        } else {
            this.set_info_open(true);
        }
        resize_logical(ui, width, height);
        pump_until(|| !this.cache_presence_query_running_for_test());
        wait_presence(this, "42", Presence::Stored);
        wait_presence(this, "43", Presence::Stored);
        for id in ["42", "43"] {
            for slot in this.cache_indicators_for_test(id) {
                if slot.is_mapped() {
                    assert!(slot.width() >= 16 && slot.height() >= 16);
                    let bounds = slot.compute_bounds(&ui.window).unwrap();
                    assert!(
                        bounds.x() >= 0.
                            && bounds.x() + bounds.width() <= ui.window.width() as f32 + 1.
                    );
                }
            }
        }
        capture_native(&format!("audio-cache-presence-{name}"));
        records.push(json!({"shape":name,"width":ui.window.width(),"height":ui.window.height(),"scale":ui.window.scale_factor(),"surface_width":ui.window.surface().unwrap().width(),"surface_height":ui.window.surface().unwrap().height(),"maximized":ui.window.is_maximized(),"themes":["light","dark"],"captures":[format!("audio-cache-presence-{name}-light.png"),format!("audio-cache-presence-{name}-dark.png")]}));
    }
    ui.window.maximize();
    pump_until(|| ui.window.is_maximized());
    capture_native("audio-cache-presence-maximized");
    records.push(json!({"shape":"maximized","width":ui.window.width(),"height":ui.window.height(),"scale":ui.window.scale_factor(),"surface_width":ui.window.surface().unwrap().width(),"surface_height":ui.window.surface().unwrap().height(),"maximized":ui.window.is_maximized(),"themes":["light","dark"],"captures":["audio-cache-presence-maximized-light.png","audio-cache-presence-maximized-dark.png"]}));
    ui.window.unmaximize();
    ui.window.set_default_size(900, 650);
    records
}

// Capture this exact isolated fixture window, with its own stable title. The
// shared catalog helper intentionally targets a different fixture's title.
fn capture_native(name: &str) {
    let Some(directory) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") else {
        return;
    };
    let directory = PathBuf::from(directory);
    fs::create_dir_all(&directory).unwrap();
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
    for (theme, scheme) in [
        ("light", adw::ColorScheme::ForceLight),
        ("dark", adw::ColorScheme::ForceDark),
    ] {
        adw::StyleManager::default().set_color_scheme(scheme);
        let deadline = Instant::now() + Duration::from_millis(180);
        pump_until(|| Instant::now() >= deadline);
        assert!(
            Command::new("import")
                .args(["-silent", "-screen", "-window", &window_id()])
                .arg(directory.join(format!("{name}-{theme}.png")))
                .status()
                .unwrap()
                .success()
        );
    }
}

fn scroll_to_top(ui: &crate::ui::MainWindow, this: &Rc<Controller>) {
    this.set_info_open(false);
    let scroll = this
        .content_scroll
        .borrow()
        .upgrade()
        .expect("visible library viewport");
    let (x, y) = super::super::super::super::super::test_input::pointer(
        &ui.window,
        &scroll,
        super::super::super::super::super::test_input::Target::Center,
    );
    let id = window_id();
    assert!(
        Command::new("xdotool")
            .args([
                "mousemove",
                "--sync",
                "--window",
                &id,
                &x.to_string(),
                &y.to_string(),
                "click",
                "--repeat",
                "20",
                "--delay",
                "15",
                "4"
            ])
            .status()
            .unwrap()
            .success()
    );
    let first = this.widgets.library_rows.row_at_index(0).unwrap();
    pump_until(|| {
        let Some(row) = first.compute_bounds(&ui.window) else {
            return false;
        };
        let Some(viewport) = scroll.compute_bounds(&ui.window) else {
            return false;
        };
        row.y() >= viewport.y() && row.y() + row.height() <= viewport.y() + viewport.height()
    });
}

fn resize_logical(ui: &crate::ui::MainWindow, width: i32, height: i32) {
    // Same measured CSD compensation as the existing native workspace suite.
    // The final assertions still require EXACT requested GTK content geometry.
    ui.window.set_default_size(width, height);
    pump_until(|| {
        (ui.window.width() - width).abs() <= 32 && (ui.window.height() - height).abs() <= 32
    });
    pump_until(|| ui.verify_layout().is_ok());
    let deadline = Instant::now() + Duration::from_millis(120);
    pump_until(|| Instant::now() >= deadline);
    let outer_width = width + (width - ui.window.width());
    let outer_height = height + (height - ui.window.height());
    ui.window.set_default_size(outer_width, outer_height);
    pump_until(|| ui.window.width() == width && ui.window.height() == height);
    pump_until(|| ui.verify_layout().is_ok());
    let deadline = Instant::now() + Duration::from_millis(120);
    pump_until(|| Instant::now() >= deadline);
}
