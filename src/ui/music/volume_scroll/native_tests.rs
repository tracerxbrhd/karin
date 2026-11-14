// SPDX-License-Identifier: GPL-3.0-or-later
//! Real footer input over the existing synthetic HTTP/GStreamer application.

use super::super::{
    player::tests::resize_player_window,
    tests::{capture_native, pump_until},
    track_extras::tests::{Fixture, fixture_window, track},
    workspace::PanelMode,
};
use super::*;
use crate::{
    playback::PlaybackState,
    queue::{ContextKind, PlaybackContext},
};
use std::{collections::BTreeMap, process::Command, thread};

fn input(arguments: &[&str]) {
    assert!(
        Command::new("timeout")
            .args(["5", "xdotool"])
            .args(arguments)
            .status()
            .unwrap()
            .success(),
        "native footer input failed"
    );
}

fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    pointer_input(window, id, widget, "1");
}

fn pointer_input(
    window: &adw::ApplicationWindow,
    id: &str,
    widget: &impl IsA<gtk::Widget>,
    button: &str,
) {
    let (x, y) =
        super::super::test_input::pointer(window, widget, super::super::test_input::Target::Center);
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        button,
    ]);
}

fn wheel(
    window: &adw::ApplicationWindow,
    id: &str,
    widget: &impl IsA<gtk::Widget>,
    button: &str,
    delivered: &Cell<u32>,
) {
    let before = delivered.get();
    pointer_input(window, id, widget, button);
    pump_until(|| delivered.get() == before + 1);
}

fn desktop_background<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let client = thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        sender.send(result).unwrap();
    });
    let mut result = None;
    pump_until(|| {
        result = receiver.try_recv().ok();
        result.is_some()
    });
    client.join().unwrap();
    match result.unwrap() {
        Ok(value) => value,
        Err(error) => std::panic::resume_unwind(error),
    }
}

fn desktop_volume(value: Option<&str>) -> f64 {
    let value = value.map(str::to_owned);
    let result = desktop_background(move || {
        let mut command = Command::new("timeout");
        command.args([
            "5",
            "playerctl",
            "--player",
            "io.github.tracerxbrhd.Karin",
            "volume",
        ]);
        if let Some(value) = value {
            command.arg(value);
        }
        command.output().unwrap()
    });
    assert!(
        result.status.success(),
        "synthetic MPRIS volume client failed"
    );
    if result.stdout.is_empty() {
        return 0.0;
    }
    String::from_utf8(result.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn assert_desktop_volume(observed: &Cell<Option<f64>>, expected: f64) {
    // The worker command is acknowledged before the next ordinary UI poll
    // publishes its actual snapshot. Await that real D-Bus notification rather
    // than assuming a synchronous property update from set_music_volume().
    pump_until(|| {
        observed
            .get()
            .is_some_and(|value| (value - expected).abs() < 0.001)
    });
    assert!((desktop_volume(None) - expected).abs() < 0.001);
}

#[test]
#[ignore = "requires isolated session D-Bus, Xvfb, xdotool/playerctl and real GStreamer MP3 playback"]
fn native_v08_footer_lyrics_and_volume_scroll() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let fixture = Fixture::new();
    let (app, ui) = fixture_window(&fixture);
    ui.window.set_title(Some("Karin synthetic HTTP fixture"));
    let this = Rc::clone(&ui.music.controller);
    this.play_music_context(
        PlaybackContext::new(
            ContextKind::Playlist,
            Some("synthetic-footer-context".into()),
            vec![track(42), track(43), track(42)],
            0,
            false,
        )
        .unwrap(),
    );
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == PlaybackState::Playing && !s.playing_time.is_zero())
    });
    this.pause_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == PlaybackState::Paused)
            && this.widgets.player_revealer.is_child_revealed()
            && this.system_media_available()
    });
    let playing = this.music_controller_for_test().unwrap();
    let generation = playing.snapshot().generation;
    let serial = this.music_serial();
    let occurrence = this
        .model
        .borrow()
        .queue
        .current_occurrence_id()
        .unwrap()
        .clone();
    let host = this.track_pane.widget.clone();
    this.select_track_info(track(43), None, false);
    this.set_panel_mode(PanelMode::TrackInfo);
    pump_until(|| this.info_is_open() && this.widgets.lyrics.is_mapped());
    assert_eq!(this.track_pane.selected().unwrap().id, "43");

    let found = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic HTTP fixture$",
        ])
        .output()
        .unwrap();
    assert!(found.status.success());
    let found = String::from_utf8(found.stdout).unwrap();
    let window = found.lines().next().unwrap();
    input(&["windowfocus", window]);

    // Receipt at an ancestor's capture phase proves even a saturated gesture
    // was actually dispatched before checking its unchanged boundary value.
    let delivered = Rc::new(Cell::new(0));
    let observed = delivered.clone();
    let receipt = gtk::EventControllerLegacy::new();
    receipt.set_propagation_phase(gtk::PropagationPhase::Capture);
    receipt.connect_event(move |_, event| {
        if event.event_type() == gtk::gdk::EventType::Scroll {
            observed.set(observed.get() + 1);
        }
        glib::Propagation::Proceed
    });
    ui.window.add_controller(receipt);

    // The actual scroll event must stop at the menu target, without opening
    // its popover or reaching a scrollable ancestor behind the footer.
    let escaped = Rc::new(Cell::new(0));
    let outer = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    let observed = escaped.clone();
    outer.connect_scroll(move |_, _, _| {
        observed.set(observed.get() + 1);
        glib::Propagation::Proceed
    });
    ui.music.player_bar.add_controller(outer);
    this.set_music_volume(0.5);
    pump_until(|| (playing.snapshot().volume - 0.5).abs() < 0.001);
    let popup = this.widgets.volume_menu.popover().unwrap();
    let bus = desktop_background(|| {
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap()
    });
    let observed_volume = Rc::new(Cell::new(None));
    let published = Rc::clone(&observed_volume);
    let volume_subscription = bus.subscribe_to_signal(
        Some(crate::mpris::BUS_NAME),
        Some("org.freedesktop.DBus.Properties"),
        Some("PropertiesChanged"),
        Some("/org/mpris/MediaPlayer2"),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let (interface, properties, _) = signal
                .parameters
                .get::<(String, BTreeMap<String, glib::Variant>, Vec<String>)>()
                .unwrap();
            if interface == "org.mpris.MediaPlayer2.Player"
                && let Some(value) = properties.get("Volume").and_then(|v| v.get::<f64>())
            {
                published.set(Some(value));
            }
        },
    );
    wheel(
        &ui.window,
        window,
        &this.widgets.volume_menu,
        "4",
        &delivered,
    );
    pump_until(|| (playing.snapshot().volume - 0.55).abs() < 0.001);
    assert!((this.widgets.volume.value() - 0.55).abs() < 0.001);
    assert_desktop_volume(&observed_volume, 0.55);
    assert!(!popup.is_mapped(), "wheel must not open the volume popup");
    assert_eq!(
        escaped.get(),
        0,
        "volume scroll cannot bubble into another surface"
    );
    assert_eq!(this.track_pane.selected().unwrap().id, "43");
    wheel(
        &ui.window,
        window,
        &this.widgets.volume_menu,
        "5",
        &delivered,
    );
    pump_until(|| (playing.snapshot().volume - 0.5).abs() < 0.001);

    this.set_music_volume(1.0);
    wheel(
        &ui.window,
        window,
        &this.widgets.volume_menu,
        "4",
        &delivered,
    );
    assert_eq!(this.music_volume(), 1.0);
    this.set_music_volume(0.0);
    wheel(
        &ui.window,
        window,
        &this.widgets.volume_menu,
        "5",
        &delivered,
    );
    assert_eq!(this.music_volume(), 0.0);
    desktop_volume(Some("0.35"));
    pump_until(|| (playing.snapshot().volume - 0.35).abs() < 0.001);
    assert!((this.widgets.volume.value() - 0.35).abs() < 0.001);
    wheel(
        &ui.window,
        window,
        &this.widgets.volume_menu,
        "4",
        &delivered,
    );
    pump_until(|| (playing.snapshot().volume - 0.4).abs() < 0.001);
    assert_desktop_volume(&observed_volume, 0.4);

    click(&ui.window, window, &this.widgets.volume_menu);
    pump_until(|| popup.is_mapped() && this.widgets.volume.is_mapped());
    assert!(this.widgets.volume.grab_focus());
    input(&["key", "--clearmodifiers", "Right"]);
    pump_until(|| (playing.snapshot().volume - 0.41).abs() < 0.001);
    assert_desktop_volume(&observed_volume, 0.41);
    input(&["key", "--clearmodifiers", "Escape"]);
    pump_until(|| !popup.is_mapped());
    assert_eq!(this.panel_mode.get(), PanelMode::TrackInfo);

    let activations = Rc::new(Cell::new(0));
    let observed = activations.clone();
    this.widgets
        .lyrics
        .connect_clicked(move |_| observed.set(observed.get() + 1));
    click(&ui.window, window, &this.widgets.lyrics);
    pump_until(|| this.panel_mode.get() == PanelMode::Lyrics && this.widgets.lyrics.is_active());
    assert_eq!(activations.get(), 1);
    assert_eq!(
        this.track_pane.selected().unwrap().id,
        "42",
        "footer Lyrics always selects the actual current occurrence"
    );
    assert_eq!(
        this.widgets.lyrics.tooltip_text().as_deref(),
        Some("Текст песни")
    );
    assert!(this.widgets.lyrics.grab_focus());
    input(&["key", "--clearmodifiers", "space"]);
    pump_until(|| this.panel_mode.get() == PanelMode::Closed && !this.widgets.lyrics.is_active());
    assert_eq!(activations.get(), 2);
    this.select_track_info(track(43), None, false);
    this.set_panel_mode(PanelMode::TrackInfo);
    assert!(this.widgets.lyrics.grab_focus());
    input(&["key", "--clearmodifiers", "Return"]);
    pump_until(|| this.panel_mode.get() == PanelMode::Lyrics);
    assert_eq!(activations.get(), 3);
    assert_eq!(this.track_pane.selected().unwrap().id, "42");
    click(&ui.window, window, &this.widgets.queue);
    pump_until(|| this.panel_mode.get() == PanelMode::Queue && !this.widgets.lyrics.is_active());
    click(&ui.window, window, &this.widgets.current_art_button);
    pump_until(|| this.panel_mode.get() == PanelMode::TrackInfo);
    assert_eq!(this.track_pane.widget, host);

    for (width, height, label) in [
        (420, 720, "compact"),
        (900, 650, "medium"),
        (1440, 900, "wide"),
    ] {
        resize_player_window(&ui, width, height);
        pump_until(|| this.widgets.lyrics.is_mapped() && this.widgets.volume_menu.is_mapped());
        let footer = ui.music.player_bar.compute_bounds(&ui.window).unwrap();
        for control in [
            this.widgets.lyrics.upcast_ref::<gtk::Widget>(),
            this.widgets.queue.upcast_ref(),
            this.widgets.volume_menu.upcast_ref(),
        ] {
            let rect = control.compute_bounds(&ui.window).unwrap();
            assert!(
                rect.width() >= 44.0 && rect.height() >= 44.0,
                "footer utility retains a usable native target"
            );
            assert!(
                rect.x() >= footer.x() - 1.0
                    && rect.x() + rect.width() <= footer.x() + footer.width() + 1.0
            );
            assert!(
                rect.y() >= footer.y() - 1.0
                    && rect.y() + rect.height() <= footer.y() + footer.height() + 1.0
            );
        }
        capture_native(&format!("v08-footer-lyrics-volume-{label}"));
    }
    assert_eq!(this.music_serial(), serial);
    assert_eq!(
        this.model.borrow().queue.current_occurrence_id(),
        Some(&occurrence)
    );
    assert_eq!(playing.snapshot().generation, generation);
    assert_eq!(playing.snapshot().state, PlaybackState::Paused);
    assert_eq!(escaped.get(), 0);
    drop(volume_subscription);
    let cleanup = ui.close_for_smoke();
    drop(this);
    drop(app);
    pump_until(|| cleanup(false));
    assert!(cleanup(true));
    pump_until(|| !playing.snapshot().worker_running);
}
