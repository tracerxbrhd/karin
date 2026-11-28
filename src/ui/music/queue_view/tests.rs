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

    let size = 624;
    let tracks = (0..size)
        .map(|index| {
            let mut item = track(true);
            item.title = format!("Synthetic collection occurrence {index}");
            item
        })
        .collect();
    this.model.borrow_mut().queue.replace_context(
        PlaybackContext::with_source_positions(
            ContextKind::LikedTracks,
            None,
            tracks,
            0,
            false,
            (0..size).collect(),
        )
        .unwrap(),
    );
    this.render_queue();
    pump_until(|| this.widgets.queue_paging.is_mapped());
    assert!(this.widgets.queue_rows.row_at_index(199).is_some());
    assert!(this.widgets.queue_rows.row_at_index(200).is_none());
    for start in [200, 400, 600] {
        let next = this.widgets.queue_paging.last_child().unwrap();
        click(&next);
        pump_until(|| this.queue_row_at_physical(start).is_some());
    }
    assert!(this.widgets.queue_rows.row_at_index(23).is_some());
    assert!(this.widgets.queue_rows.row_at_index(24).is_none());
    let later = this.queue_row_at_physical(601).unwrap();
    click(later.upcast_ref());
    pump_until(|| this.model.borrow().queue.current_index() == Some(601));
    assert_eq!(this.model.borrow().queue.items().len(), size);
    assert_eq!(
        this.widgets.current.text(),
        "Synthetic collection occurrence 601"
    );
    drop(later);

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
    // The expanded player borrows the same native queue. Exercise the drag
    // there and return to the pinned panel for the keyboard half of the test.
    this.open_now_playing();
    this.show_now_playing_section(super::super::now_playing::Section::Queue);
    pump_until(|| this.now_playing.is_active() && this.widgets.queue_rows.is_mapped());
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
    use super::super::test_input::{Target, pointer, wait_for_layout};

    let selected = this.model.borrow().queue.occurrence_id(3).unwrap().clone();
    let target = this.model.borrow().queue.occurrence_id(1).unwrap().clone();
    let source_row = this.queue_row_at_physical(3).unwrap();
    let target_row = this.queue_row_at_physical(1).unwrap();
    let next_row = this.queue_row_at_physical(2).unwrap();
    let grip = source_row.child().unwrap().first_child().unwrap();
    assert_eq!(grip.widget_name(), "music-queue-drag-handle");
    let (sx, sy) = pointer(&ui.window, &grip, Target::Center);
    let (_, ty) = pointer(&ui.window, &target_row, Target::Center);
    let (_, middle) = pointer(&ui.window, &next_row, Target::Center);
    let scale = ui.window.scale_factor();
    let height = source_row.height();
    let layer = this
        .widgets
        .queue_reorder
        .widget()
        .last_child()
        .and_downcast::<gtk::Fixed>()
        .expect("queue's clipped card layer");
    let origin = source_row.compute_bounds(&layer).unwrap();
    let middle_origin = next_row.compute_bounds(&layer).unwrap();
    let view = |name: &str| {
        let mut child = layer.first_child();
        while let Some(widget) = child {
            if widget.widget_name() == name || widget.has_css_class(name) {
                return Some(widget.downcast::<gtk::Picture>().unwrap());
            }
            child = widget.next_sibling();
        }
        None
    };
    let move_to = |x: i32, y: i32| {
        input(&[
            "mousemove",
            "--sync",
            "--window",
            window,
            &x.to_string(),
            &y.to_string(),
        ]);
    };
    let start_drag = || {
        move_to(sx, sy);
        input(&["mousedown", "1"]);
        move_to(sx, middle);
        pump_until(|| {
            view("music-queue-reorder-grabbed")
                .is_some_and(|preview| preview.is_mapped() && preview.height() > 0)
        });
    };
    let capture = |file: &str| {
        if let Some(directory) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
            wait_for_layout(&ui.window, &target_row);
            assert!(
                Command::new("import")
                    .args(["-silent", "-screen", "-window", window])
                    .arg(std::path::Path::new(&directory).join(file))
                    .status()
                    .unwrap()
                    .success()
            );
        }
    };

    let settings = gtk::Settings::default().unwrap();
    let was_animated = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);
    start_drag();
    let preview = view("music-queue-reorder-grabbed").unwrap();
    let image = preview.paintable().unwrap();
    assert_eq!(image.intrinsic_width(), source_row.width());
    assert_eq!(image.intrinsic_height(), height);
    let snapshot = gtk::Snapshot::new();
    image.snapshot(&snapshot, f64::from(source_row.width()), f64::from(height));
    let ink = snapshot
        .to_node()
        .expect("the in-queue card paints the track row");
    assert!(ink.bounds().width() > 100.0 && ink.bounds().height() >= 40.0);
    assert_eq!(source_row.opacity(), 0.0);
    let neighbor = view("music-queue-reorder-neighbor").unwrap();
    pump_until(|| {
        neighbor.compute_bounds(&layer).unwrap().y() > middle_origin.y() + height as f32 / 2.0
    });
    let intermediate = preview.compute_bounds(&layer).unwrap();
    assert!((intermediate.x() - origin.x()).abs() < 1.0);
    assert!(intermediate.y() < origin.y() - height as f32 / 2.0);
    assert_eq!(this.model.borrow().queue.occurrence_id(3), Some(&selected));
    capture("queue-inline-first-slot.png");
    move_to(sx + 80 * scale, ty);
    pump_until(|| {
        preview.compute_bounds(&layer).unwrap().y() < intermediate.y() - height as f32 / 2.0
    });
    assert!((preview.compute_bounds(&layer).unwrap().x() - origin.x()).abs() < 1.0);
    capture("queue-inline-second-slot.png");
    // Releasing over the player leaves the exact queue occurrence in place.
    move_to(120 * scale, ty);
    input(&["mouseup", "1"]);
    pump_until(|| layer.first_child().is_none() && source_row.opacity() == 1.0);
    assert_eq!(this.model.borrow().queue.occurrence_id(3), Some(&selected));

    settings.set_gtk_enable_animations(false);
    start_drag();
    input(&["key", "Escape"]);
    pump_until(|| layer.first_child().is_none() && source_row.opacity() == 1.0);
    input(&["mouseup", "1"]);
    assert!(this.now_playing.is_active());
    assert_eq!(this.model.borrow().queue.occurrence_id(3), Some(&selected));
    start_drag();
    move_to(sx, ty);
    pump_until(|| {
        (preview.is_mapped() || view("music-queue-reorder-grabbed").is_some())
            && target_row.opacity() == 0.0
    });
    input(&["mouseup", "1"]);
    pump_until(|| {
        let model = this.model.borrow();
        model.queue.occurrence_id(model.queue.upcoming_indices()[0]) == Some(&selected)
    });
    settings.set_gtk_enable_animations(was_animated);
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
    assert!(layer.first_child().is_none());
    assert!(this.close_now_playing(false));
    pump_until(|| this.widgets.queue_rows.is_mapped());

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

    // A full queue must reject Play Next visibly, without evicting another
    // occurrence or changing the paused engine. Fill the bounded model in one
    // batch; the ordinary controller action performs its one native render.
    let original_len = this.model.borrow().queue.items().len();
    {
        let mut model = this.model.borrow_mut();
        while model.queue.items().len() < 500 {
            model.queue.append(track(true)).unwrap();
        }
    }
    let full_occurrences = {
        let model = this.model.borrow();
        (0..500)
            .map(|index| model.queue.occurrence_id(index).unwrap().clone())
            .collect::<Vec<_>>()
    };
    this.enqueue_next(track(true));
    pump_until(|| {
        this.widgets.status_revealer.is_child_revealed() && this.widgets.status.is_mapped()
    });
    assert_eq!(this.widgets.status.title(), "Очередь заполнена");
    let message = this.widgets.status.subtitle().unwrap();
    assert!(
        message.contains("500") && message.contains("Удалите") && message.contains("повторите")
    );
    assert!(this.widgets.status.width() > 0 && this.widgets.status.height() > 0);
    {
        let model = this.model.borrow();
        assert_eq!(model.queue.items().len(), 500);
        assert_eq!(model.queue.current_occurrence_id(), Some(&current));
        assert_eq!(
            (0..500)
                .map(|index| model.queue.occurrence_id(index).unwrap().clone())
                .collect::<Vec<_>>(),
            full_occurrences,
            "capacity rejection cannot silently evict or replace any occurrence"
        );
    }
    assert_eq!(playing.snapshot().state, PlaybackState::Paused);
    assert_eq!(playing.snapshot().generation, generation);
    assert!(playing.snapshot().worker_running);
    assert_eq!(this.music_snapshot().unwrap().state, PlaybackState::Paused);
    assert_eq!(this.music_snapshot().unwrap().generation, generation);

    // The user makes one future slot, then deliberately retries the same action.
    this.remove_music_queue(499);
    assert_eq!(this.model.borrow().queue.items().len(), 499);
    this.enqueue_next(track(true));
    pump_until(|| {
        !this.widgets.status_revealer.reveals_child() && !this.widgets.status.is_mapped()
    });
    assert!(
        this.widgets
            .status
            .subtitle()
            .is_none_or(|message| message.is_empty())
    );
    {
        let model = this.model.borrow();
        assert_eq!(model.queue.items().len(), 500);
        assert_eq!(model.queue.current_occurrence_id(), Some(&current));
        assert!(!full_occurrences.contains(model.queue.occurrence_id(1).unwrap()));
        assert_eq!(model.queue.upcoming_indices().first(), Some(&1));
    }
    assert_eq!(playing.snapshot().state, PlaybackState::Paused);
    assert_eq!(playing.snapshot().generation, generation);
    assert!(playing.snapshot().worker_running);
    assert_eq!(this.music_snapshot().unwrap().state, PlaybackState::Paused);
    assert_eq!(this.music_snapshot().unwrap().generation, generation);
    assert_eq!(
        this.widgets.current_art.first_child().as_ref(),
        Some(&artwork)
    );
    // A long queue can scroll under a held card without moving the occurrence
    // until release. Cancel to leave the queue unchanged for the restart test.
    let scroll = this
        .widgets
        .queue_rows
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_downcast::<gtk::ScrolledWindow>()
        .unwrap();
    let adjustment = scroll.vadjustment();
    adjustment.set_value(0.0);
    let viewport = scroll.child().unwrap();
    let (_, viewport_center_y) = pointer(&ui.window, &viewport, Target::Center);
    let edge_y = viewport_center_y + (viewport.height() / 2 - 8) * scale;
    let source_grip = this
        .queue_row_at_physical(1)
        .unwrap()
        .child()
        .unwrap()
        .first_child()
        .unwrap();
    let (grip_x, grip_y) = pointer(&ui.window, &source_grip, Target::Center);
    let untouched = this.model.borrow().queue.occurrence_id(1).unwrap().clone();
    move_to(grip_x, grip_y);
    input(&["mousedown", "1"]);
    move_to(grip_x, edge_y);
    pump_until(|| adjustment.value() >= 128.0);
    assert_eq!(this.model.borrow().queue.occurrence_id(1), Some(&untouched));
    input(&["key", "Escape"]);
    pump_until(|| layer.first_child().is_none());
    input(&["mouseup", "1"]);
    adjustment.set_value(0.0);
    assert_eq!(this.model.borrow().queue.occurrence_id(1), Some(&untouched));

    wait_for_layout(&ui.window, &source_grip);
    move_to(grip_x, grip_y);
    input(&["mousedown", "1"]);
    move_to(grip_x, grip_y + 64 * scale);
    pump_until(|| view("music-queue-reorder-grabbed").is_some());
    this.render_queue();
    move_to(grip_x, grip_y + 128 * scale);
    input(&["mouseup", "1"]);
    pump_until(|| layer.first_child().is_none());
    assert_eq!(this.model.borrow().queue.occurrence_id(1), Some(&untouched));
    // Retire only the synthetic filler so the existing restart checks stay
    // focused on the earlier edited occurrences plus the accepted new next.
    {
        let mut model = this.model.borrow_mut();
        while model.queue.items().len() > original_len + 1 {
            let last = model.queue.items().len() - 1;
            model.queue.remove(last).unwrap();
        }
    }
    this.render_queue();

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

    // A retired occurrence is inert after queue replacement, even when every
    // provider ID is equal.
    let replacement = this.model.borrow().queue.context().unwrap().clone();
    this.model.borrow_mut().queue.replace_context(replacement);
    assert!(!this.promote_queue_occurrence(&restored_selected));
    let cleanup = ui.close_for_smoke();
    pump_until(|| cleanup(false));
    assert!(cleanup(true));
}
