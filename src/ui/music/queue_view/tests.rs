// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn track(available: bool) -> Track {
    Track {
        id: "42".into(),
        title: "Synthetic queue track".into(),
        artists: Vec::new(),
        artist_links: Vec::new(),
        album_id: None,
        album_title: None,
        duration_ms: None,
        cover: None,
        explicit: false,
        available,
    }
}

#[test]
fn queue_next_marker_follows_policy_and_exact_available_occurrences() {
    let items = vec![track(true), track(false), track(true), track(true)];
    let mut queue = MusicQueue::new();
    queue.replace_context(PlaybackContext::new(ContextKind::Queue, None, items, 0, false).unwrap());
    assert_eq!(queue.upcoming_indices().first(), Some(&2));
    queue.select(1);
    assert_eq!(queue.upcoming_indices().first(), Some(&2));
    queue.select(2);
    assert_eq!(queue.upcoming_indices().first(), Some(&3));
    queue.select(3);
    assert_eq!(queue.upcoming_indices().first(), None);
    queue.set_repeat_mode(crate::queue::RepeatMode::Queue);
    assert_eq!(queue.upcoming_indices().first(), Some(&0));
    queue.clear();
    assert!(queue.upcoming_indices().is_empty());
}

#[test]
fn queue_presentation_retains_physical_occurrences_and_unavailable_positions() {
    assert_eq!(display_order(4, Some(3), &[2, 0, 3]), vec![3, 2, 0, 1]);
    assert_eq!(display_order(4, Some(0), &[2, 3]), vec![0, 2, 3, 1]);
    assert_eq!(display_order(2, Some(0), &[0, 1, 0, 99]), vec![0, 1]);
    assert!(display_order(0, None, &[]).is_empty());
}

#[test]
fn queue_labels_do_not_invent_listening_history() {
    // Only the selected and next available occurrence receive state badges.
    // An earlier position is not proof that the user has listened to it.
    assert_eq!(queue_badge(0, Some(2), Some(3)), None);
    assert_eq!(queue_badge(1, Some(2), Some(3)), None);
    assert_eq!(queue_badge(2, Some(2), Some(3)), Some("Сейчас"));
    assert_eq!(queue_badge(3, Some(2), Some(3)), Some("Далее"));
    assert_eq!(queue_badge(4, Some(2), Some(3)), None);
    assert_eq!(queue_badge(0, None, None), None);
}

/// This deliberately has no MusicService or resolver: it verifies the actual
/// native input dispatch contract, not HTTP or sound. The joined MP3/resolver
/// fixture and the complete catalog fixture cover those independently.
#[test]
#[ignore = "requires isolated Xvfb, xdotool and native GTK input"]
fn native_queue_space_mouse_enter_dispatch_exact_occurrence_once() {
    use super::super::tests::pump_until;
    use std::{process::Command, time::Duration};

    gtk::init().unwrap();
    adw::init().unwrap();
    crate::ui::style::install();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.QueueDispatchFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let ui = crate::ui::build_with_music_pane(
        &app,
        crate::application::ConnectionService::new(crate::api::YandexApi::new().unwrap()),
        |app, window| {
            MusicPane::with_provider(
                app,
                window,
                None,
                Arc::new(OfflineProvider),
                launch_system_browser,
            )
        },
    );
    ui.window
        .set_title(Some("Karin synthetic queue dispatch fixture"));
    ui.window.set_default_size(900, 650);
    let this = Rc::clone(&ui.music.controller);
    let mut first = track(true);
    first.title = "Synthetic first occurrence".into();
    let unavailable = track(false);
    let mut duplicate = track(true);
    duplicate.title = "Synthetic duplicate occurrence".into();
    let mut other = track(true);
    other.id = "43".into();
    other.title = "Synthetic other track".into();
    let context = PlaybackContext::new(
        ContextKind::Playlist,
        Some("9".into()),
        vec![first, unavailable, duplicate, other],
        0,
        false,
    )
    .unwrap();
    {
        let mut model = this.model.borrow_mut();
        model.account_valid = true;
        model.token = Some(AccessToken::for_test(
            "synthetic-queue-dispatch-token",
            Duration::from_secs(300),
        ));
        model.queue.replace_context(context);
    }
    this.render_controls();
    this.render_queue();
    ui.window.present();
    pump_until(|| ui.window.is_mapped());

    let input = |args: &[&str]| {
        assert!(
            Command::new("timeout")
                .args(["5", "xdotool"])
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    };
    let found = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic queue dispatch fixture$",
        ])
        .output()
        .unwrap();
    assert!(found.status.success());
    let found = String::from_utf8(found.stdout).unwrap();
    let id = found.lines().next().unwrap();
    input(&["windowfocus", id]);
    let click = |widget: &gtk::Widget| {
        let (x, y) = super::super::test_input::pointer(
            &ui.window,
            widget,
            super::super::test_input::Target::Leading,
        );
        input(&[
            "mousemove",
            "--window",
            id,
            &x.to_string(),
            &y.to_string(),
            "click",
            "1",
        ]);
    };
    assert!(
        !this.widgets.player_revealer.reveals_child()
            && !this.widgets.queue.is_mapped()
            && this.music_snapshot().is_none(),
        "a selected queue does not fabricate an active player/footer"
    );
    // This input-only fixture intentionally has no audio request. Open its
    // real pinned queue through the available system shortcut, rather than
    // clicking a correctly hidden idle-footer control.
    input(&["key", "--clearmodifiers", "ctrl+j"]);
    pump_until(|| this.info_is_open() && this.widgets.queue_rows.is_mapped());
    assert_eq!(this.panel_mode.get(), workspace::PanelMode::Queue);

    let activations = Rc::new(Cell::new(0_u32));
    this.widgets.queue_rows.connect_row_activated({
        let activations = activations.clone();
        move |_, _| activations.set(activations.get() + 1)
    });
    let duplicate = this.queue_row_at_physical(2).unwrap();
    assert!(duplicate.action_name().is_none());
    assert!(duplicate.action_target_value().is_none());
    assert!(duplicate.grab_focus());
    input(&["key", "--clearmodifiers", "space"]);
    pump_until(|| this.model.borrow().queue.current_index() == Some(2));
    assert_eq!(activations.get(), 1);
    assert_eq!(
        this.widgets.current.text(),
        "Synthetic duplicate occurrence"
    );

    let other = this.queue_row_at_physical(3).unwrap();
    click(other.upcast_ref());
    pump_until(|| this.model.borrow().queue.current_index() == Some(3));
    assert_eq!(activations.get(), 2);
    assert_eq!(this.widgets.current.text(), "Synthetic other track");

    let first = this.queue_row_at_physical(0).unwrap();
    assert!(first.grab_focus());
    input(&["key", "--clearmodifiers", "Return"]);
    pump_until(|| this.model.borrow().queue.current_index() == Some(0));
    assert_eq!(activations.get(), 3);
    assert_eq!(this.widgets.current.text(), "Synthetic first occurrence");
    assert_eq!(this.model.borrow().queue.items().len(), 4);
    assert!(!this.queue_row_at_physical(1).unwrap().is_activatable());
    assert_eq!(this.panel_mode.get(), workspace::PanelMode::Queue);
    assert!(this.music_snapshot().is_none(), "this is not an audio test");

    drop(duplicate);
    drop(other);
    drop(first);
    let cleanup = ui.close_for_smoke();
    drop(this);
    drop(app);
    pump_until(|| cleanup(false));
    assert!(cleanup(true));
}

/// Real X11 mouse drag, row keyboard gesture and the existing HTTP/GStreamer
/// player. No emitted drop signal substitutes for GTK's native DnD transport.
#[test]
#[ignore = "requires isolated Xvfb, xdotool and GStreamer HTTP/MP3 plugins"]
fn native_v08_queue_next_drag_keyboard_and_restoration() {
    use super::super::{
        player::tests::{Fixture, player_window},
        tests::pump_until,
    };
    use crate::{playback::PlaybackState, queue::QueueSnapshot};
    use std::process::Command;

    gtk::init().unwrap();
    adw::init().unwrap();
    let fixture = Fixture::new();
    let (_app, ui) = player_window(&fixture);
    let this = &ui.music.controller;
    for title in [
        "Original current",
        "Original duplicate A",
        "Original duplicate B",
        "Original final",
    ] {
        let mut item = track(true);
        item.title = title.into();
        item.duration_ms = Some(29_000);
        this.enqueue(item);
    }
    this.play_music_queue(0);
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|state| state.state == PlaybackState::Playing)
    });
    let current = this
        .model
        .borrow()
        .queue
        .current_occurrence_id()
        .unwrap()
        .clone();
    let artwork = this
        .widgets
        .current_art
        .first_child()
        .expect("current artwork widget");
    let playing = this.music_controller_for_test().unwrap();
    let generation = playing.snapshot().generation;
    this.pause_music();
    pump_until(|| playing.snapshot().state == PlaybackState::Paused);
    this.toggle_queue_panel();
    pump_until(|| this.widgets.queue_rows.is_mapped());
    let input = |args: &[&str]| {
        assert!(
            Command::new("timeout")
                .args(["5", "xdotool"])
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    };
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
    let events = gtk::EventControllerLegacy::new();
    events.set_propagation_phase(gtk::PropagationPhase::Capture);
    let observed = Cell::new(0_u8);
    events.connect_event(move |_, event| {
        let (bit, stage) = match event.event_type() {
            gtk::gdk::EventType::DragEnter => (1, "gdk-enter"),
            gtk::gdk::EventType::DragMotion => (2, "gdk-motion"),
            gtk::gdk::EventType::DropStart => (4, "gdk-drop"),
            _ => return glib::Propagation::Proceed,
        };
        if observed.get() & bit == 0 {
            observed.set(observed.get() | bit);
            eprintln!("Karin synthetic queue drag: stage={stage}");
        }
        glib::Propagation::Proceed
    });
    ui.window.add_controller(events);
    let selected = this.model.borrow().queue.occurrence_id(3).unwrap().clone();
    let target = this.model.borrow().queue.occurrence_id(1).unwrap().clone();
    let source_row = this.queue_row_at_physical(3).unwrap();
    let target_row = this.queue_row_at_physical(1).unwrap();
    let (sx, sy) = super::super::test_input::pointer(
        &ui.window,
        &source_row,
        super::super::test_input::Target::Leading,
    );
    let (tx, ty) = super::super::test_input::pointer(
        &ui.window,
        &target_row,
        super::super::test_input::Target::Leading,
    );
    input(&[
        "mousemove",
        "--sync",
        "--window",
        window,
        &sx.to_string(),
        &sy.to_string(),
    ]);
    input(&["mousedown", "1"]);
    // Cross the native drag threshold while GTK still owns the pressed input.
    let distance = (gtk::Settings::default()
        .unwrap()
        .property::<i32>("gtk-dnd-drag-threshold")
        + 4)
        * ui.window.scale_factor();
    input(&[
        "mousemove",
        "--sync",
        "--window",
        window,
        &(sx + distance).to_string(),
        &(sy - distance).to_string(),
    ]);
    let drag = source_row.observe_controllers();
    // GtkDragSource deliberately ignores threshold crossings during its first
    // 100 ms (gtkdragsource.c MIN_TIME_TO_DND); its timer does not replay the
    // last motion. A human drag continues moving while the button is held.
    // Send that real continuous X11 gesture, rather than one early motion and
    // an idle wait. The existing eight-second observation bound is unchanged.
    let moving_window = window.to_owned();
    let mut step = 0;
    let movement = glib::timeout_add_local(std::time::Duration::from_millis(25), move || {
        step += 1;
        assert!(
            Command::new("timeout")
                .args([
                    "5",
                    "xdotool",
                    "mousemove",
                    "--sync",
                    "--window",
                    &moving_window,
                    &(sx + distance + step % 3).to_string(),
                    &(sy - distance).to_string()
                ])
                .status()
                .unwrap()
                .success()
        );
        glib::ControlFlow::Continue
    });
    pump_until(|| {
        (0..drag.n_items())
            .filter_map(|index| drag.item(index))
            .filter_map(|item| item.downcast::<gtk::DragSource>().ok())
            .any(|source| source.drag().is_some())
    });
    movement.remove();
    let native_drag = (0..drag.n_items())
        .filter_map(|index| drag.item(index))
        .filter_map(|item| item.downcast::<gtk::DragSource>().ok())
        .find_map(|source| source.drag())
        .expect("observed native drag");
    eprintln!(
        "Karin synthetic queue drag: stage=native-source-started typed={} move={}",
        native_drag
            .formats()
            .contains_type(QueueDrag::static_type()),
        native_drag.actions().contains(gtk::gdk::DragAction::MOVE)
    );
    input(&[
        "mousemove",
        "--sync",
        "--window",
        window,
        &tx.to_string(),
        &(ty - target_row.height() / 4).to_string(),
    ]);
    let drop = target_row.observe_controllers();
    // XDND position/status negotiation is asynchronous. Keep delivering real
    // pointer motion over the destination, as a continuous native drag does,
    // until GTK observes that particular DropTarget. A single position sent
    // while X11 is waiting for its preceding status is not a completed gesture.
    let moving_window = window.to_owned();
    let destination_y = ty - target_row.height() * ui.window.scale_factor() / 4;
    let mut step = 0;
    let movement = glib::timeout_add_local(std::time::Duration::from_millis(25), move || {
        step += 1;
        assert!(
            Command::new("timeout")
                .args([
                    "5",
                    "xdotool",
                    "mousemove",
                    "--sync",
                    "--window",
                    &moving_window,
                    &(tx + step % 3).to_string(),
                    &destination_y.to_string()
                ])
                .status()
                .unwrap()
                .success()
        );
        glib::ControlFlow::Continue
    });
    pump_until(|| {
        (0..drop.n_items())
            .filter_map(|index| drop.item(index))
            .filter_map(|item| item.downcast::<gtk::DropTarget>().ok())
            .any(|target| target.current_drop().is_some())
    });
    movement.remove();
    eprintln!("Karin synthetic queue drag: stage=native-target-entered");
    input(&["mouseup", "1"]);
    pump_until(|| {
        let model = this.model.borrow();
        model.queue.occurrence_id(model.queue.upcoming_indices()[0]) == Some(&selected)
    });
    assert_eq!(
        this.model.borrow().queue.current_occurrence_id(),
        Some(&current)
    );
    assert_eq!(playing.snapshot().generation, generation);
    assert_eq!(playing.snapshot().state, PlaybackState::Paused);
    assert_eq!(
        this.widgets.current_art.first_child().as_ref(),
        Some(&artwork)
    );
    assert_eq!(this.model.borrow().queue.occurrence_index(&target), Some(2));

    // Keyboard reorders this exact duplicate, keeps row focus and changes no
    // playing source. An unrelated duplicate ID cannot receive the operation.
    let index = this
        .model
        .borrow()
        .queue
        .occurrence_index(&selected)
        .unwrap();
    let row = this.queue_row_at_physical(index).unwrap();
    assert!(row.grab_focus());
    this.widgets.play.emit_clicked();
    pump_until(|| playing.snapshot().state == PlaybackState::Playing);
    input(&["key", "--clearmodifiers", "alt+Down"]);
    pump_until(|| this.model.borrow().queue.occurrence_index(&selected) == Some(2));
    assert_eq!(
        this.model.borrow().queue.current_occurrence_id(),
        Some(&current)
    );
    assert_eq!(playing.snapshot().generation, generation);
    assert_eq!(
        this.widgets.current_art.first_child().as_ref(),
        Some(&artwork)
    );
    assert!(this.promote_queue_occurrence(&selected));
    assert_eq!(this.model.borrow().queue.occurrence_id(1), Some(&selected));

    this.pause_music();
    pump_until(|| playing.snapshot().state == PlaybackState::Paused);
    let mut next = track(true);
    next.title = "Original manual next".into();
    this.enqueue_next(next);
    let manual = this.model.borrow().queue.occurrence_id(1).unwrap().clone();
    assert_eq!(playing.snapshot().state, PlaybackState::Paused);
    assert_eq!(
        this.widgets.current_art.first_child().as_ref(),
        Some(&artwork)
    );
    assert_eq!(playing.snapshot().generation, generation);
    assert!(this.shift_queue_occurrence(&manual, 1));
    assert_eq!(playing.snapshot().state, PlaybackState::Paused);

    let snapshot = QueueSnapshot::capture(&this.model.borrow().queue)
        .unwrap()
        .unwrap();
    let normalized = this.model.borrow().queue.items()[0].clone();
    let restored = snapshot.restore(&[normalized]).unwrap().queue;
    this.stop_music();
    pump_until(|| !playing.snapshot().worker_running);
    this.model.borrow_mut().queue = restored;
    this.follow_current_occurrence();
    this.render_queue();
    assert!(
        this.music_snapshot()
            .is_none_or(|snapshot| snapshot.state != PlaybackState::Playing)
    );
    let future = this.model.borrow().queue.editable_upcoming();
    let restored_selected = this
        .model
        .borrow()
        .queue
        .occurrence_id(*future.last().unwrap())
        .unwrap()
        .clone();
    assert!(this.shift_queue_occurrence(&restored_selected, -1));
    assert!(
        this.music_snapshot().is_none(),
        "restored editing cannot prepare audio"
    );
    assert!(
        QueueSnapshot::capture(&this.model.borrow().queue)
            .unwrap()
            .is_some()
    );

    // An old typed payload is inert after queue replacement, even with all
    // provider IDs equal. Native DnD above is independently observed.
    let replacement = this.model.borrow().queue.context().unwrap().clone();
    this.model.borrow_mut().queue.replace_context(replacement);
    assert!(!this.promote_queue_occurrence(&restored_selected));
    let cleanup = ui.close_for_smoke();
    pump_until(|| cleanup(false));
    assert!(cleanup(true));
}
