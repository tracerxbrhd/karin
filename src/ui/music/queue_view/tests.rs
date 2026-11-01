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
