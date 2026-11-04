// SPDX-License-Identifier: GPL-3.0-or-later
//! Real local session D-Bus, without an account, network music or an audio URI.
use super::*;

use std::{
    process::Command as Process,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn fixture() -> Snapshot {
    Snapshot {
        status: Status::Playing,
        metadata: Some(Metadata {
            track_id: 7,
            title: "Synthetic MPRIS track".into(),
            artists: vec!["Synthetic artist".into()],
            album: Some("Synthetic album".into()),
            length_us: Some(30_000_000),
            artwork: None,
        }),
        position_us: 1_000_000,
        volume: 0.7,
        repeat_mode: RepeatMode::Off,
        shuffle: false,
        can_play: true,
        can_pause: true,
        can_seek: true,
        can_go_next: true,
        can_go_previous: false,
        can_control: true,
    }
}

#[test]
fn metadata_has_typed_opaque_identity_and_no_delivery_fields() {
    let snapshot = fixture().normalized();
    let metadata = metadata_variant(snapshot.metadata.as_ref());
    let values = metadata.get::<BTreeMap<String, glib::Variant>>().unwrap();
    assert_eq!(
        values["mpris:trackid"].type_(),
        glib::VariantTy::OBJECT_PATH
    );
    assert_eq!(
        values["mpris:trackid"].get::<ObjectPath>().unwrap(),
        track_path(7)
    );
    assert_eq!(values["mpris:length"].get::<i64>(), Some(30_000_000));
    assert_eq!(
        values["xesam:artist"].get::<Vec<String>>().unwrap(),
        ["Synthetic artist"]
    );
    assert_eq!(values.len(), 5);
    for forbidden in [
        "xesam:url",
        "url",
        "access_token",
        "cookies",
        "stream",
        "source",
    ] {
        assert!(!values.contains_key(forbidden));
    }
    assert!(
        metadata_variant(None)
            .get::<BTreeMap<String, glib::Variant>>()
            .unwrap()
            .is_empty()
    );
    let node = gio::DBusNodeInfo::for_xml(XML).unwrap();
    let root = node.lookup_interface(ROOT).unwrap();
    let player = node.lookup_interface(PLAYER).unwrap();
    assert!(root.lookup_method("Raise").is_some());
    assert!(root.lookup_method("Quit").is_some());
    for method in [
        "Play",
        "Pause",
        "PlayPause",
        "Stop",
        "Next",
        "Previous",
        "Seek",
        "SetPosition",
    ] {
        assert!(player.lookup_method(method).is_some());
    }
    assert!(player.lookup_signal("Seeked").is_some());
    for supported in ["LoopStatus", "Shuffle"] {
        assert!(player.lookup_property(supported).is_some());
        assert!(snapshot.properties().contains_key(supported));
    }
    assert_eq!(snapshot.properties()["LoopStatus"].str(), Some("None"));
    assert_eq!(snapshot.properties()["Shuffle"].get::<bool>(), Some(false));
    assert_eq!(snapshot.properties()["Rate"].get::<f64>(), Some(1.0));
    assert!(gio::dbus_is_name(BUS_NAME));
}

#[test]
fn snapshot_bounds_preserve_intrinsic_control_and_truthful_track_capabilities() {
    let mut snapshot = fixture();
    snapshot.position_us = i64::MAX;
    snapshot.volume = f64::NAN;
    snapshot.metadata.as_mut().unwrap().title = format!("\0{}\n", "я".repeat(1024));
    let normalized = snapshot.normalized();
    assert_eq!(normalized.position_us, 30_000_000);
    assert_eq!(normalized.volume, 0.7);
    assert_eq!(normalized.metadata.unwrap().title.chars().count(), 512);
    let mut snapshot = fixture();
    snapshot.metadata.as_mut().unwrap().track_id = 0;
    let normalized = snapshot.normalized();
    assert!(normalized.metadata.is_none());
    assert!(normalized.can_control);
    assert!(!normalized.can_seek && !normalized.can_play && !normalized.can_go_next);
    assert_eq!(normalized.position_us, 0);
    assert_eq!(normalized.status, Status::Stopped);
    let mut snapshot = fixture();
    snapshot.can_control = false;
    let normalized = snapshot.normalized();
    assert!(!normalized.can_play && !normalized.can_pause && !normalized.can_seek);
}

fn png_file() -> (std::os::fd::OwnedFd, std::path::PathBuf) {
    use std::io::Write;
    let (fd, path) = glib::file_open_tmp(Some("karin-mpris-cover-XXXXXX")).unwrap();
    // Header-only synthetic file is sufficient for the admission boundary;
    // production data is PNG-encoded from the already decoded cover pixels.
    let mut header = [0u8; 33];
    header[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    header[11] = 13;
    header[12..16].copy_from_slice(b"IHDR");
    header[19] = 1;
    header[23] = 1;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .write_all(&header)
        .unwrap();
    (fd, path)
}

#[test]
fn artwork_admission_rejects_remote_text_symlinks_oversized_and_invalid_dimensions() {
    assert!(LocalArtwork::from_file(Path::new("https://example.invalid/private.png")).is_none());
    assert!(LocalArtwork::from_file(Path::new("relative.png")).is_none());
    let (_fd, path) = png_file();
    let cover = LocalArtwork::from_file(&path).unwrap();
    assert!(cover.0.starts_with("file:///"));
    let link = path.with_extension("symlink");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(LocalArtwork::from_file(&link).is_none());
    std::fs::remove_file(link).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[16..20].copy_from_slice(&2049u32.to_be_bytes());
    std::fs::write(&path, &bytes).unwrap();
    assert!(LocalArtwork::from_file(&path).is_none());
    std::fs::write(&path, b"synthetic token-like text, not a cover").unwrap();
    assert!(LocalArtwork::from_file(&path).is_none());
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(2 * 1024 * 1024 + 1)
        .unwrap();
    assert!(LocalArtwork::from_file(&path).is_none());
    std::fs::remove_file(path).unwrap();
}

fn pump(context: &glib::MainContext, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        while context.pending() {
            context.iteration(false);
        }
        if done() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "isolated MPRIS fixture timed out"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn call(
    connection: &gio::DBusConnection,
    interface: &str,
    method: &str,
    parameters: &glib::Variant,
) -> Result<glib::Variant, glib::Error> {
    connection.call_sync(
        Some(BUS_NAME),
        OBJECT_PATH,
        interface,
        method,
        Some(parameters),
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        3_000,
        gio::Cancellable::NONE,
    )
}

fn get(connection: &gio::DBusConnection, name: &str) -> glib::Variant {
    call(connection, PROPERTIES, "Get", &(PLAYER, name).to_variant())
        .unwrap()
        .get::<(glib::Variant,)>()
        .unwrap()
        .0
}

fn set(
    connection: &gio::DBusConnection,
    name: &str,
    value: glib::Variant,
) -> Result<glib::Variant, glib::Error> {
    call(
        connection,
        PROPERTIES,
        "Set",
        &(PLAYER, name, value).to_variant(),
    )
}

fn cli(program: &str, arguments: &[&str]) -> String {
    let output = Process::new("timeout")
        .arg("5")
        .arg(program)
        .args(arguments)
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "synthetic MPRIS command failed: {program}"
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
#[ignore = "requires isolated dbus-run-session, playerctl and gdbus; no account or audio source"]
fn native_mpris_session_bus_methods_properties_signals_and_shutdown() {
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            let state = Rc::new(RefCell::new(fixture()));
            let commands = Rc::new(RefCell::new(Vec::<Command>::new()));
            let slot = Rc::new(RefCell::new(None::<std::rc::Weak<Mpris>>));
            let changed = state.clone();
            let recorded = commands.clone();
            let service_slot = slot.clone();
            let service = Rc::new(Mpris::new(Rc::new(move |command| {
                recorded.borrow_mut().push(command.clone());
                let mut snapshot = changed.borrow_mut();
                let mut seeked = None;
                match command {
                    Command::Play => snapshot.status = Status::Playing,
                    Command::Pause => snapshot.status = Status::Paused,
                    Command::PlayPause => {
                        snapshot.status = if snapshot.status == Status::Playing {
                            Status::Paused
                        } else {
                            Status::Playing
                        }
                    }
                    Command::Stop => {
                        snapshot.status = Status::Stopped;
                        snapshot.position_us = 0;
                    }
                    Command::SetVolume(value) => snapshot.volume = value,
                    Command::SetShuffle(enabled) => snapshot.shuffle = enabled,
                    Command::SetRepeat(mode) => snapshot.repeat_mode = mode,
                    Command::SetPosition { position_us, .. } => {
                        snapshot.position_us = position_us;
                        seeked = Some(position_us);
                    }
                    Command::Seek(offset) => {
                        snapshot.position_us = snapshot
                            .position_us
                            .saturating_add(offset)
                            .clamp(0, 30_000_000);
                        seeked = Some(snapshot.position_us);
                    }
                    Command::Next => {
                        snapshot.metadata.as_mut().unwrap().track_id = 8;
                        snapshot.metadata.as_mut().unwrap().title =
                            "Synthetic next MPRIS track".into();
                        snapshot.position_us = 0;
                        snapshot.can_seek = false;
                        snapshot.can_go_next = false;
                        snapshot.can_go_previous = true;
                    }
                    Command::Previous => {
                        snapshot.metadata.as_mut().unwrap().track_id = 7;
                        snapshot.metadata.as_mut().unwrap().title = "Synthetic MPRIS track".into();
                        snapshot.can_seek = true;
                        snapshot.can_go_next = true;
                        snapshot.can_go_previous = false;
                    }
                    Command::Quit => *snapshot = Snapshot::default(),
                    Command::Raise => {}
                }
                let publish = snapshot.clone();
                drop(snapshot);
                if let Some(service) = service_slot
                    .borrow()
                    .as_ref()
                    .and_then(std::rc::Weak::upgrade)
                {
                    service.publish(publish);
                    if let Some(position) = seeked {
                        service.seeked(position);
                    }
                }
            })));
            *slot.borrow_mut() = Some(Rc::downgrade(&service));
            let (_fd, cover_path) = png_file();
            state.borrow_mut().metadata.as_mut().unwrap().artwork =
                LocalArtwork::from_file(&cover_path);
            service.publish(state.borrow().clone());
            pump(&context, || service.is_available());
            let signals = Rc::new(RefCell::new(Vec::<(String, glib::Variant)>::new()));
            let received = signals.clone();
            let bus = service.inner.connection.borrow().as_ref().unwrap().clone();
            let subscription = bus.subscribe_to_signal(
                Some(BUS_NAME),
                None,
                None,
                Some(OBJECT_PATH),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    received
                        .borrow_mut()
                        .push((signal.signal_name.into(), signal.parameters.clone()));
                },
            );
            let (finished, result) = mpsc::channel();
            let worker = thread::spawn(move || {
                let outcome = std::panic::catch_unwind(|| {
                    let client =
                        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
                    assert_eq!(get(&client, "PlaybackStatus").str(), Some("Playing"));
                    assert_eq!(get(&client, "Position").get::<i64>(), Some(1_000_000));
                    let metadata = get(&client, "Metadata")
                        .get::<BTreeMap<String, glib::Variant>>()
                        .unwrap();
                    assert_eq!(
                        metadata["mpris:trackid"].get::<ObjectPath>().unwrap(),
                        track_path(7)
                    );
                    assert_eq!(metadata["mpris:length"].get::<i64>(), Some(30_000_000));
                    assert!(
                        metadata["mpris:artUrl"]
                            .str()
                            .unwrap()
                            .starts_with("file:///")
                    );
                    assert!(!metadata.contains_key("xesam:url"));
                    let introspection = cli(
                        "gdbus",
                        &[
                            "introspect",
                            "--session",
                            "--dest",
                            BUS_NAME,
                            "--object-path",
                            OBJECT_PATH,
                        ],
                    );
                    assert!(introspection.contains(PLAYER) && introspection.contains(ROOT));
                    let exported = call(&client, PROPERTIES, "GetAll", &(PLAYER,).to_variant())
                        .unwrap()
                        .get::<(BTreeMap<String, glib::Variant>,)>()
                        .unwrap()
                        .0;
                    for supported in ["LoopStatus", "Shuffle"] {
                        assert!(introspection.contains(supported));
                        assert!(exported.contains_key(supported));
                    }
                    assert_eq!(get(&client, "LoopStatus").str(), Some("None"));
                    assert_eq!(get(&client, "Shuffle").get::<bool>(), Some(false));
                    for forbidden in ["access_token", "get-mp3", "storage.yandex.net", "OAuth "] {
                        assert!(!introspection.contains(forbidden));
                    }
                    let selection = "io.github.tracerxbrhd.Karin";
                    assert_eq!(
                        cli("playerctl", &["--player", selection, "status"]).trim(),
                        "Playing"
                    );
                    assert!(
                        cli(
                            "playerctl",
                            &[
                                "--player",
                                selection,
                                "metadata",
                                "--format",
                                "{{title}}|{{artist}}"
                            ]
                        )
                        .contains("Synthetic MPRIS track|Synthetic artist")
                    );
                    cli("playerctl", &["--player", selection, "pause"]);
                    assert_eq!(get(&client, "PlaybackStatus").str(), Some("Paused"));
                    cli("playerctl", &["--player", selection, "play"]);
                    assert_eq!(get(&client, "PlaybackStatus").str(), Some("Playing"));
                    cli("playerctl", &["--player", selection, "play-pause"]);
                    assert_eq!(get(&client, "PlaybackStatus").str(), Some("Paused"));
                    cli("playerctl", &["--player", selection, "volume", "0.35"]);
                    assert_eq!(get(&client, "Volume").get::<f64>(), Some(0.35));
                    set(&client, "Volume", (-1.0f64).to_variant()).unwrap();
                    assert_eq!(get(&client, "Volume").get::<f64>(), Some(0.0));
                    assert!(set(&client, "Volume", f64::NAN.to_variant()).is_err());
                    set(&client, "Rate", 0.0f64.to_variant()).unwrap();
                    assert_eq!(get(&client, "PlaybackStatus").str(), Some("Paused"));
                    assert_eq!(get(&client, "Rate").get::<f64>(), Some(1.0));
                    assert!(set(&client, "Rate", 2.0f64.to_variant()).is_err());
                    cli("playerctl", &["--player", selection, "shuffle", "On"]);
                    assert_eq!(get(&client, "Shuffle").get::<bool>(), Some(true));
                    cli("playerctl", &["--player", selection, "loop", "Playlist"]);
                    assert_eq!(get(&client, "LoopStatus").str(), Some("Playlist"));
                    set(&client, "LoopStatus", "Track".to_variant()).unwrap();
                    assert_eq!(get(&client, "LoopStatus").str(), Some("Track"));
                    set(&client, "LoopStatus", "None".to_variant()).unwrap();
                    assert_eq!(get(&client, "LoopStatus").str(), Some("None"));
                    set(&client, "Shuffle", false.to_variant()).unwrap();
                    assert_eq!(get(&client, "Shuffle").get::<bool>(), Some(false));
                    assert!(set(&client, "Shuffle", "On".to_variant()).is_err());
                    assert!(set(&client, "LoopStatus", true.to_variant()).is_err());
                    assert!(
                        set(
                            &client,
                            "LoopStatus",
                            "invalid-untrusted-value".to_variant()
                        )
                        .is_err()
                    );
                    call(
                        &client,
                        PLAYER,
                        "SetPosition",
                        &(track_path(7), 5_000_000i64).to_variant(),
                    )
                    .unwrap();
                    assert_eq!(get(&client, "Position").get::<i64>(), Some(5_000_000));
                    for parameters in [
                        (track_path(8), 10_000_000i64),
                        (track_path(7), -1),
                        (track_path(7), 30_000_001),
                    ] {
                        call(&client, PLAYER, "SetPosition", &parameters.to_variant()).unwrap();
                    }
                    assert_eq!(get(&client, "Position").get::<i64>(), Some(5_000_000));
                    call(&client, PLAYER, "Seek", &(-2_000_000i64,).to_variant()).unwrap();
                    assert_eq!(get(&client, "Position").get::<i64>(), Some(3_000_000));
                    cli("playerctl", &["--player", selection, "next"]);
                    assert_eq!(get(&client, "PlaybackStatus").str(), Some("Paused"));
                    assert_eq!(get(&client, "CanSeek").get::<bool>(), Some(false));
                    call(&client, PLAYER, "Seek", &(1_000_000i64,).to_variant()).unwrap();
                    call(
                        &client,
                        PLAYER,
                        "SetPosition",
                        &(track_path(8), 1_000_000i64).to_variant(),
                    )
                    .unwrap();
                    call(&client, PLAYER, "Next", &().to_variant()).unwrap();
                    assert_eq!(get(&client, "Position").get::<i64>(), Some(0));
                    cli("playerctl", &["--player", selection, "previous"]);
                    assert_eq!(get(&client, "PlaybackStatus").str(), Some("Paused"));
                    assert_eq!(get(&client, "CanSeek").get::<bool>(), Some(true));
                    cli("playerctl", &["--player", selection, "position", "2"]);
                    assert_eq!(get(&client, "Position").get::<i64>(), Some(2_000_000));
                    cli("playerctl", &["--player", selection, "stop"]);
                    assert_eq!(get(&client, "PlaybackStatus").str(), Some("Stopped"));
                    let unsupported = call(
                        &client,
                        PLAYER,
                        "OpenUri",
                        &("https://example.invalid/synthetic-private-input",).to_variant(),
                    )
                    .unwrap_err();
                    assert!(!unsupported.to_string().contains("synthetic-private-input"));
                    call(&client, ROOT, "Raise", &().to_variant()).unwrap();
                    call(&client, ROOT, "Quit", &().to_variant()).unwrap();
                    assert!(
                        get(&client, "Metadata")
                            .get::<BTreeMap<String, glib::Variant>>()
                            .unwrap()
                            .is_empty()
                    );
                    assert_eq!(get(&client, "Position").get::<i64>(), Some(0));
                    assert_eq!(get(&client, "CanPlay").get::<bool>(), Some(false));
                    call(&client, PLAYER, "Play", &().to_variant()).unwrap();
                });
                finished.send(outcome).unwrap();
            });
            let mut outcome = None;
            pump(&context, || {
                outcome = result.try_recv().ok();
                outcome.is_some()
            });
            worker.join().unwrap();
            if let Err(error) = outcome.unwrap() {
                std::panic::resume_unwind(error);
            }
            // Signal delivery is asynchronous even after a property call has replied.
            pump(&context, || {
                signals
                    .borrow()
                    .iter()
                    .filter(|(name, _)| name == "Seeked")
                    .count()
                    >= 3
            });
            let signals = signals.borrow();
            let mut seeks = Vec::new();
            let mut changed_volume = false;
            let mut changed_track = false;
            let mut changed_modes = (false, false);
            for (name, value) in signals.iter() {
                if name == "Seeked" {
                    seeks.push(value.get::<(i64,)>().unwrap().0);
                }
                if name == "PropertiesChanged" {
                    let (interface, properties, invalidated) = value
                        .get::<(String, BTreeMap<String, glib::Variant>, Vec<String>)>()
                        .unwrap();
                    assert_eq!(interface, PLAYER);
                    assert!(invalidated.is_empty());
                    assert!(
                        !properties.contains_key("Position")
                            && !properties.contains_key("CanControl")
                    );
                    changed_volume |= properties.contains_key("Volume");
                    changed_track |= properties.contains_key("Metadata");
                    changed_modes.0 |= properties.contains_key("LoopStatus");
                    changed_modes.1 |= properties.contains_key("Shuffle");
                }
            }
            assert_eq!(seeks, [5_000_000, 3_000_000, 2_000_000]);
            assert!(changed_volume && changed_track);
            assert_eq!(changed_modes, (true, true));
            drop(signals);
            let commands = commands.borrow();
            assert!(commands.contains(&Command::Raise) && commands.contains(&Command::Quit));
            assert!(commands.contains(&Command::SetShuffle(true)));
            assert!(commands.contains(&Command::SetShuffle(false)));
            for mode in [RepeatMode::Off, RepeatMode::Queue, RepeatMode::One] {
                assert!(commands.contains(&Command::SetRepeat(mode)));
            }
            assert_eq!(
                commands
                    .iter()
                    .filter(|command| matches!(command, Command::Next))
                    .count(),
                1
            );
            assert_eq!(
                commands
                    .iter()
                    .filter(|command| matches!(command, Command::Previous))
                    .count(),
                1
            );
            let position_commands = commands
                .iter()
                .filter(|command| matches!(command, Command::SetPosition { .. } | Command::Seek(_)))
                .collect::<Vec<_>>();
            assert_eq!(
                position_commands.len(),
                3,
                "stale, negative and unseekable requests must never dispatch"
            );
            assert_eq!(
                position_commands[0],
                &Command::SetPosition {
                    track_id: 7,
                    position_us: 5_000_000
                }
            );
            assert_eq!(position_commands[1], &Command::Seek(-2_000_000));
            assert!(matches!(
                position_commands[2],
                Command::SetPosition {
                    track_id: 7,
                    position_us: 2_000_000
                } | Command::Seek(2_000_000)
            ));
            drop(commands);
            service.shutdown();
            service.shutdown();
            assert!(!service.is_available());
            drop(subscription);
            let (sent, result) = mpsc::channel();
            let worker = thread::spawn(move || {
                let client =
                    gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
                let reply = client
                    .call_sync(
                        Some("org.freedesktop.DBus"),
                        "/org/freedesktop/DBus",
                        "org.freedesktop.DBus",
                        "NameHasOwner",
                        Some(&(BUS_NAME,).to_variant()),
                        None,
                        gio::DBusCallFlags::NO_AUTO_START,
                        3_000,
                        gio::Cancellable::NONE,
                    )
                    .unwrap();
                sent.send(reply.get::<(bool,)>().unwrap().0).unwrap();
            });
            let mut running = None;
            pump(&context, || {
                running = result.try_recv().ok();
                running.is_some()
            });
            worker.join().unwrap();
            assert_eq!(running, Some(false));
            std::fs::remove_file(cover_path).unwrap();
        })
        .unwrap();
}
