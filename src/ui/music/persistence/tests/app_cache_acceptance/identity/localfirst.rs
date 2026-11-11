// SPDX-License-Identifier: GPL-3.0-or-later
//! Local-first admission through the normal application, not a storage mock.
//! Original CC0 MP3s and the published legacy offer/XML shape exercise bounded
//! private HTTP freshness. These tests make no live-service acceptance claim.
use super::*;

const CASES: [&str; 19] = [
    "localfirst-first",
    "localfirst-restart",
    "localfirst-signed-replica",
    "localfirst-logical-change",
    "localfirst-variant-change",
    "localfirst-gain-change",
    "localfirst-denied",
    "localfirst-preview",
    "localfirst-protected",
    "localfirst-unavailable",
    "localfirst-restored",
    "localfirst-expired-replica",
    "localfirst-refreshed-restart",
    "localfirst-expired",
    "localfirst-no-cache",
    "localfirst-rollback",
    "localfirst-heuristic-first",
    "localfirst-heuristic-restart",
    "localfirst-heuristic-expired",
];

pub(super) fn http_date(seconds_ago: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let date = glib::DateTime::from_unix_utc(i64::try_from(now).unwrap() - seconds_ago).unwrap();
    let weekday = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        [usize::try_from(date.day_of_week() - 1).unwrap()];
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][usize::try_from(date.month() - 1).unwrap()];
    format!(
        "{weekday}, {:02} {month} {} {:02}:{:02}:{:02} GMT",
        date.day_of_month(),
        date.year(),
        date.hour(),
        date.minute(),
        date.second()
    )
}

fn first(phase: &str) -> bool {
    matches!(phase, "localfirst-first" | "localfirst-heuristic-first")
}
fn refill(phase: &str) -> bool {
    matches!(
        phase,
        "localfirst-logical-change" | "localfirst-variant-change" | "localfirst-gain-change"
    )
}
fn denied(phase: &str) -> bool {
    matches!(
        phase,
        "localfirst-denied"
            | "localfirst-preview"
            | "localfirst-protected"
            | "localfirst-unavailable"
    )
}
fn revalidates(phase: &str) -> bool {
    matches!(
        phase,
        "localfirst-expired"
            | "localfirst-no-cache"
            | "localfirst-rollback"
            | "localfirst-heuristic-expired"
    )
}
fn refresh(phase: &str) -> bool {
    phase == "localfirst-expired-replica"
}

fn facts(input: &Input) -> Vec<serde_json::Value> {
    let mut facts = file_facts(input);
    facts.sort_by_key(|file| file["inode"].as_u64().unwrap());
    facts
}

fn bindings(input: &Input) -> std::collections::BTreeSet<([u8; 32], [u8; 32], u8)> {
    media_files(&audio_root(input))
        .into_iter()
        .filter_map(|media| {
            Metadata::decode(&fs::read(media.parent().unwrap().join("http")).unwrap())
                .unwrap()
                .binding
                .map(|binding| (binding.logical.unwrap(), binding.variant, binding.schema))
        })
        .collect()
}

fn expire(input: &Input, phase: &str) {
    // This models a persisted clock/freshness boundary, without changing any
    // media byte, validator, logical offer or physical provider URI.
    let mut changed = 0;
    for media in media_files(&audio_root(input)) {
        let path = media.parent().unwrap().join("http");
        let mut metadata = Metadata::decode(&fs::read(&path).unwrap()).unwrap();
        if phase == "localfirst-no-cache" {
            metadata.directives.no_cache = true;
        } else {
            metadata.directives.no_cache = false;
            // Both records are aged consistently; a subsequent actual HTTP
            // validation is required before either one can become local.
            age_metadata(&mut metadata, phase == "localfirst-rollback");
        }
        fs::write(path, metadata.encode().unwrap()).unwrap();
        changed += 1;
    }
    assert_eq!(changed, 2);
}

fn age_metadata(metadata: &mut Metadata, rollback: bool) {
    let observed = metadata
        .observed
        .as_mut()
        .expect("normal full response persists its observation");
    if rollback {
        observed.response_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 86_400;
        observed.corrected_initial_age = 0;
    } else {
        observed.corrected_initial_age = 86_400;
    }
}

pub(super) fn run_parent() {
    let mut directory = Directory::new();
    let mut server = Server::new(&directory.0);
    let mut saved_mode = Mode::default();
    let mut reports = Vec::new();
    for phase in CASES {
        if first(phase) {
            if phase == "localfirst-heuristic-first" {
                directory = Directory::new();
                server = Server::new(&directory.0);
            }
            for kind in ["config", "cache", "data", "state"] {
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(directory.0.join(kind))
                    .unwrap();
            }
            *server.mode.lock().unwrap() = Mode {
                cache_policy: if phase == "localfirst-heuristic-first" {
                    "private"
                } else {
                    "private, max-age=3600"
                },
                current_dates: true,
                omit_validators: phase == "localfirst-first",
                ..Mode::default()
            };
        } else {
            let mut m = server.mode.lock().unwrap();
            m.timestamp += 1;
            m.offer_denied = false;
            m.preview = false;
            m.protection = false;
            m.unavailable = false;
            match phase {
                "localfirst-signed-replica" => {
                    m.host = true;
                    m.path = 1;
                }
                "localfirst-logical-change" => {
                    m.object = true;
                    m.version = 2;
                    m.omit_validators = false;
                }
                "localfirst-variant-change" => {
                    m.bitrate = 192;
                    m.version = 3;
                }
                "localfirst-gain-change" => m.gain = true,
                "localfirst-denied" => m.offer_denied = true,
                "localfirst-preview" => m.preview = true,
                "localfirst-protected" => m.protection = true,
                "localfirst-unavailable" => m.unavailable = true,
                "localfirst-expired-replica" => {
                    m.host = !m.host;
                    m.path = 2;
                }
                _ if revalidates(phase) => *m = saved_mode.clone(),
                _ => {}
            }
        }
        let input = make_input(&directory.0, &server);
        if revalidates(phase) || refresh(phase) {
            expire(&input, phase);
        }
        let path = directory.0.join("localfirst-input.json");
        fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
        let before = server.counts();
        let before_files = facts(&input);
        let before_bindings = bindings(&input);
        let result = run_phase(&path, phase, &directory.0);
        let after = server.counts();
        let after_files = facts(&input);
        let application_binding_changed = before_bindings != bindings(&input);
        let details: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.0.join("localfirst-result.json")).unwrap())
                .unwrap();
        assert_eq!(details["pid"], result.pid);
        assert_eq!(details["phase"], phase);
        assert_eq!(details["passed"], true);
        assert_eq!(after.account - before.account, 1);
        assert_eq!(
            after.offers - before.offers,
            if first(phase) {
                4
            } else if phase == "localfirst-restart" {
                3
            } else if phase == "localfirst-unavailable" {
                0
            } else if refresh(phase) {
                2
            } else {
                1
            }
        );
        assert_eq!(
            after.native_gets - before.native_gets,
            if first(phase) {
                2
            } else {
                u64::from(refill(phase) || refresh(phase))
            }
        );
        assert_eq!(
            after.background_gets - before.background_gets,
            if first(phase) {
                2
            } else {
                u64::from(refill(phase) || refresh(phase))
            }
        );
        assert_eq!(
            after.descriptors - before.descriptors,
            if first(phase) {
                2
            } else {
                u64::from(refill(phase) || revalidates(phase) || refresh(phase))
            }
        );
        assert_eq!(
            after.heads - before.heads,
            u64::from(revalidates(phase) || refresh(phase))
        );
        assert_eq!(
            after.media_bytes - before.media_bytes,
            if first(phase) {
                4 * input.bytes
            } else if refill(phase) || refresh(phase) {
                2 * input.bytes
            } else {
                0
            },
            "only actual initial/replacement playback and its independent save transfer full media"
        );
        assert_eq!(
            after_files.len(),
            2,
            "both ordinary queue tracks remain complete"
        );
        assert!(after_files.iter().all(|file| file["completed"] == true));
        if phase == "localfirst-first" {
            for media in media_files(&audio_root(&input)) {
                let metadata =
                    Metadata::decode(&fs::read(media.parent().unwrap().join("http")).unwrap())
                        .unwrap();
                assert!(
                    metadata.etag.is_none()
                        && metadata.last_modified.is_none()
                        && metadata.modified_unix.is_none(),
                    "explicit fresh storage/reuse must work without a validator or advertised digest"
                );
            }
        }
        let expected_sha = input
            .sha256
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert!(
            after_files
                .iter()
                .any(|file| file["sha256"] == expected_sha && file["bytes"] == input.bytes),
            "the current selected resource is stored in full with its original SHA-256"
        );
        if refill(phase) {
            assert!(
                application_binding_changed,
                "a new selected object/quality/gain is recorded after the observed full replacement response"
            );
        }
        if !first(phase) && !refill(phase) && !refresh(phase) {
            assert_eq!(
                before_files, after_files,
                "reusing, refusing or validating must preserve audio SHA/bytes/inode"
            );
        }
        if first(phase) || refill(phase) || refresh(phase) {
            saved_mode = server.mode.lock().unwrap().clone();
        }
        reports.push(json!({"phase":phase,"pid":result.pid,"passed":true,"first_source":result.first_source,"replay_source":result.replay_source,"before":before,"after":after,"before_files":before_files,"after_files":after_files,"expected_bytes":input.bytes,"expected_payload_sha256":expected_sha,"application_binding_changed":application_binding_changed,"validator_free_first_save":phase=="localfirst-first","fresh_local_hit":!first(phase)&&!refill(phase)&&!denied(phase)&&!revalidates(phase)&&!refresh(phase),"fresh_response_refresh":refresh(phase),"http_revalidation":revalidates(phase),"selected_variant_changed":refill(phase),"details":details}));
    }
    assert_eq!(
        reports
            .iter()
            .map(|r| r["pid"].as_u64().unwrap())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        CASES.len()
    );
    if let Some(path) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
        fs::create_dir_all(&path).unwrap();
        fs::write(PathBuf::from(path).join("audio-cache-localfirst-receipt.json"),serde_json::to_vec_pretty(&json!({"scope":"ordinary-application-local-first-private-http","real_yandex_service":false,"processes":CASES.len(),"cases":reports})).unwrap()).unwrap();
    }
}

pub(super) fn run_child(input: &Input, phase: &str) {
    let (_app, ui) = application(input);
    let this = &ui.music.controller;
    let row = this.widgets.library_rows.row_at_index(0).unwrap();
    let existing = !first(phase);
    wait_presence(
        this,
        "42",
        if existing {
            Presence::Stored
        } else {
            Presence::Missing
        },
    );
    assert!(this.model.borrow().queue.current().is_none());
    pointer(&ui.window, &row);
    let source = if denied(phase) {
        "none"
    } else if first(phase) || refill(phase) || refresh(phase) {
        "remote-https"
    } else {
        "cached-file"
    };
    if phase == "localfirst-unavailable" {
        assert!(!row.is_sensitive() || !row.is_activatable());
        assert!(this.model.borrow().queue.current().is_none());
    } else if denied(phase) {
        pump_until(|| {
            !this.music_requested()
                && this.widgets.status.title() == "Не удалось получить источник аудио"
        });
        assert!(
            this.music_snapshot()
                .is_none_or(|s| s.source_kind != PlaybackSourceKind::CachedFile
                    && s.state != PlaybackState::Playing)
        );
    } else {
        wait_source(
            this,
            if first(phase) || refill(phase) || refresh(phase) {
                PlaybackSourceKind::RemoteHttps
            } else {
                PlaybackSourceKind::CachedFile
            },
        );
    }
    let mut replay = None;
    if first(phase) {
        wait_publication(this, input);
        let serial = this.music_serial();
        assert!(row.grab_focus());
        key(&ui.window, "Return");
        pump_until(|| this.music_serial() > serial);
        wait_source(this, PlaybackSourceKind::CachedFile);
        replay = Some("cached-file".to_owned());
        desktop_method("Next", ().to_variant());
        pump_until(|| {
            this.model
                .borrow()
                .queue
                .current()
                .is_some_and(|t| t.id == "43")
        });
        wait_source(this, PlaybackSourceKind::RemoteHttps);
        pump_until(|| this.cache_work_settled_for_test());
        wait_presence(this, "43", Presence::Stored);
        desktop_method("Previous", ().to_variant());
        pump_until(|| {
            this.model
                .borrow()
                .queue
                .current()
                .is_some_and(|t| t.id == "42")
        });
        wait_source(this, PlaybackSourceKind::CachedFile);
    } else if refill(phase) || refresh(phase) {
        pump_until(|| this.cache_work_settled_for_test());
        wait_presence(this, "42", Presence::Stored);
        assert!(completed_media_matches(
            &audio_root(input),
            input.bytes,
            input.sha256
        ));
        if refresh(phase) {
            let serial = this.music_serial();
            assert!(row.grab_focus());
            key(&ui.window, "Return");
            pump_until(|| this.music_serial() > serial);
            wait_source(this, PlaybackSourceKind::CachedFile);
            replay = Some("cached-file".to_owned());
        }
    }
    let controls = phase == "localfirst-restart";
    if controls {
        cached_controls(this);
    }
    pump_until(|| this.cache_work_settled_for_test());
    let (cache, _) = this.audio_cache_binding().unwrap();
    pump_until(|| cache.lookup_settled_for_test());
    let details = json!({"phase":phase,"pid":std::process::id(),"passed":true,"source":source,"startup_stored_without_player":existing,"sha256_checked_by_production_lookup":!denied(phase),"real_cached_gstreamer_source":source=="cached-file"||replay.is_some(),"mpris_pause_play_seek_volume_next_previous":controls,"queue_first_save":first(phase)});
    close_child(&ui, this);
    fs::write(
        input.directory.join("localfirst-result.json"),
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
            replay_source: replay,
            clear_kept_playing: false,
            default_limit_bytes: 2 * 1024 * 1024 * 1024,
        },
    );
}

fn desktop<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        sender.send(result).unwrap();
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
fn desktop_method(method: &str, parameters: glib::Variant) {
    let method = method.to_owned();
    desktop(move || {
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
            .unwrap()
            .call_sync(
                Some(crate::mpris::BUS_NAME),
                "/org/mpris/MediaPlayer2",
                "org.mpris.MediaPlayer2.Player",
                &method,
                Some(&parameters),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                3_000,
                gio::Cancellable::NONE,
            )
            .unwrap();
    });
}
fn desktop_property(name: &str) -> glib::Variant {
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
                3_000,
                gio::Cancellable::NONE,
            )
            .unwrap()
            .get::<(glib::Variant,)>()
            .unwrap()
            .0
    })
}
fn cached_controls(this: &Rc<Controller>) {
    pump_until(|| this.system_media_available());
    // The native worker snapshot becomes Playing before the periodic GTK
    // bridge publishes it. Observe the actual D-Bus property, rather than
    // assuming those two independently scheduled observations are atomic.
    wait_cached_desktop_status(this, PlaybackState::Playing, "Playing");
    desktop_method("Pause", ().to_variant());
    pump_until(|| {
        this.music_snapshot().is_some_and(|s| {
            s.state == PlaybackState::Paused
                && s.source_kind == PlaybackSourceKind::CachedFile
                && s.seekable
        })
    });
    wait_cached_desktop_status(this, PlaybackState::Paused, "Paused");
    pump_until(|| desktop_property("CanSeek").get::<bool>() == Some(true));
    let seek_serial = this.music_snapshot().unwrap().seek_serial;
    desktop_method("Seek", (1_000_000_i64,).to_variant());
    pump_until(|| {
        this.music_snapshot().is_some_and(|s| {
            s.seek_serial > seek_serial
                && s.seek_position
                    .is_some_and(|p| p >= Duration::from_millis(900))
        })
    });
    desktop(|| {
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
            .unwrap()
            .call_sync(
                Some(crate::mpris::BUS_NAME),
                "/org/mpris/MediaPlayer2",
                "org.freedesktop.DBus.Properties",
                "Set",
                Some(
                    &(
                        "org.mpris.MediaPlayer2.Player",
                        "Volume",
                        0.23_f64.to_variant(),
                    )
                        .to_variant(),
                ),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                3_000,
                gio::Cancellable::NONE,
            )
            .unwrap();
    });
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| (s.volume - 0.23).abs() < 0.0001)
    });
    desktop_method("Play", ().to_variant());
    wait_source(this, PlaybackSourceKind::CachedFile);
    wait_cached_desktop_status(this, PlaybackState::Playing, "Playing");
    desktop_method("Next", ().to_variant());
    pump_until(|| {
        this.model
            .borrow()
            .queue
            .current()
            .is_some_and(|t| t.id == "43")
    });
    wait_source(this, PlaybackSourceKind::CachedFile);
    desktop_method("Previous", ().to_variant());
    pump_until(|| {
        this.model
            .borrow()
            .queue
            .current()
            .is_some_and(|t| t.id == "42")
    });
    wait_source(this, PlaybackSourceKind::CachedFile);
}

fn wait_cached_desktop_status(this: &Controller, state: PlaybackState, status: &str) {
    pump_until(|| {
        this.music_snapshot().is_some_and(|snapshot| {
            snapshot.state == state && snapshot.source_kind == PlaybackSourceKind::CachedFile
        }) && desktop_property("PlaybackStatus")
            .get::<String>()
            .as_deref()
            == Some(status)
    });
}
