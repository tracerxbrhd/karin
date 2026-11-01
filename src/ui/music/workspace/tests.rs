// SPDX-License-Identifier: GPL-3.0-or-later
//! The production native shell with synthetic in-memory selection/context.
//! No MusicService, remote login, persisted credential or audio source is used.

use super::super::{
    test_input,
    tests::{capture_native, pump_until},
};
use super::*;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct WindowManager(Option<Child>);
impl WindowManager {
    fn start() -> Self {
        let present = Command::new("wmctrl")
            .arg("-m")
            .output()
            .is_ok_and(|result| result.status.success());
        let owned = (!present).then(|| {
            Command::new("openbox")
                .arg("--sm-disable")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("the isolated maximized-layout fixture requires Openbox")
        });
        pump_until(|| {
            Command::new("wmctrl")
                .arg("-m")
                .output()
                .is_ok_and(|result| result.status.success())
        });
        Self(owned)
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

fn track(id: &str, title: &str) -> Track {
    Track {
        id: id.into(),
        title: title.into(),
        artists: vec!["Synthetic artist".into()],
        artist_links: vec![Artist {
            id: "11".into(),
            name: "Synthetic artist".into(),
            cover: None,
        }],
        album_id: Some("7".into()),
        album_title: Some("Synthetic album".into()),
        duration_ms: Some(123_000),
        cover: None,
        explicit: false,
        available: true,
    }
}

fn input(args: &[&str]) {
    assert!(
        Command::new("timeout")
            .args(["5", "xdotool"])
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}

fn settle() {
    let end = Instant::now() + Duration::from_millis(120);
    pump_until(|| Instant::now() >= end);
}

fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    let (x, y) = test_input::pointer(window, widget, test_input::Target::Center);
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "1",
    ]);
    settle();
}

fn resize(ui: &crate::ui::MainWindow, width: i32, height: i32) {
    eprintln!("Karin synthetic shell phase: resize {width}x{height}");
    ui.window.set_default_size(width, height);
    let previous = Cell::new(None);
    pump_until(|| {
        // The CSD surface includes a compositor-dependent shadow/frame. The
        // logical GTK content still needs to reach the requested breakpoint.
        let current = (ui.window.width(), ui.window.height());
        if previous.replace(Some(current)) != Some(current) {
            eprintln!(
                "Karin synthetic shell allocation: {}x{}",
                current.0, current.1
            );
        }
        (current.0 - width).abs() <= 32 && (current.1 - height).abs() <= 32
    });
    layout(ui);
    settle();
}

fn resize_logical(ui: &crate::ui::MainWindow, width: i32, height: i32) {
    resize(ui, width, height);
    // Breakpoints observe GTK's content size, not the X11 CSD shadow. Measure
    // that actual difference instead of assuming a decoration width or scale.
    let outer_width = width + (width - ui.window.width());
    let outer_height = height + (height - ui.window.height());
    ui.window.set_default_size(outer_width, outer_height);
    pump_until(|| ui.window.width() == width && ui.window.height() == height);
    layout(ui);
    settle();
}

fn assert_central_row_actions(
    ui: &crate::ui::MainWindow,
    this: &Controller,
    content: &gtk::Widget,
) -> gtk::MenuButton {
    let row = this.widgets.library_rows.row_at_index(0).unwrap();
    assert!(row.is_mapped() && row.is_activatable());
    let body = row.child().unwrap();
    let mut child = body.first_child();
    let mut actions = Vec::new();
    let mut menu = None;
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.is::<gtk::Button>() || widget.is::<gtk::MenuButton>() {
            actions.push(widget.clone());
        }
        if let Some(button) = widget.downcast_ref::<gtk::MenuButton>() {
            menu = Some(button.clone());
        }
    }
    assert_eq!(actions.len(), 2, "the playable row exposes Play and More");
    let center = content.compute_bounds(&ui.window).unwrap();
    let pane = this.track_pane.widget.compute_bounds(&ui.window).unwrap();
    for action in actions {
        assert!(action.is_mapped() && action.is_sensitive());
        let bounds = action.compute_bounds(&ui.window).unwrap();
        assert!(bounds.width() >= 24.0 && bounds.height() >= 24.0);
        assert!(
            bounds.x() >= center.x()
                && bounds.x() + bounds.width() <= center.x() + center.width() + 1.0
        );
        assert!(
            bounds.x() + bounds.width() <= pane.x() - 8.0,
            "the pinned medium pane cannot cover a row action"
        );
        let picked = ui
            .window
            .pick(
                f64::from(bounds.x() + bounds.width() / 2.0),
                f64::from(bounds.y() + bounds.height() / 2.0),
                gtk::PickFlags::DEFAULT,
            )
            .expect("the visible central action has a native hit target");
        assert!(
            picked == action || picked.is_ancestor(&action),
            "native input must reach the central row action, not the information pane"
        );
    }
    menu.unwrap()
}

fn layout(ui: &crate::ui::MainWindow) {
    let previous = RefCell::new(None);
    let stable_since = Cell::new(Instant::now());
    pump_until(|| match ui.verify_layout() {
        Ok(()) => true,
        Err(error) => {
            if previous.borrow().as_ref() != Some(&error) {
                eprintln!("Karin synthetic shell layout pending: {error}");
                previous.replace(Some(error));
                stable_since.set(Instant::now());
            }
            if stable_since.get().elapsed() >= Duration::from_millis(500)
                && ui.window.width() < 500
                && ui.header.width() > ui.window.width()
                && ui.header_items.iter().any(|item| {
                    item.is::<adw::WindowTitle>() && item.is_visible() && !item.is_mapped()
                })
            {
                capture_unmapped_header_diagnostic(ui);
            }
            false
        }
    });
}

/// A failed-state observer only: preserve the actual frame, theme, animations
/// and focus before collecting bounded native geometry. No text is serialized.
fn capture_unmapped_header_diagnostic(ui: &crate::ui::MainWindow) {
    use serde_json::json;
    static CAPTURED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if CAPTURED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let Some(directory) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    if std::fs::create_dir_all(&directory).is_err() {
        eprintln!("Karin synthetic header diagnostic: output-unavailable");
        return;
    }
    let name = format!(
        "workspace-header-unmapped-diagnostic-scale{}",
        ui.window.scale_factor()
    );
    // XGetImage of this synthetic client window only; unlike capture_native,
    // this observer changes neither color scheme nor motion preferences.
    let found = Command::new("timeout")
        .args([
            "3",
            "xdotool",
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic HTTP fixture$",
        ])
        .output();
    let window_id = found
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|ids| ids.lines().next().map(str::to_owned))
        .filter(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()));
    let screenshot = window_id.is_some_and(|id| {
        Command::new("timeout")
            .args(["5", "import", "-silent", "-window", &id])
            .arg(directory.join(format!("{name}.png")))
            .status()
            .is_ok_and(|status| status.success())
    });
    fn node(
        widget: &gtk::Widget,
        window: &adw::ApplicationWindow,
        depth: usize,
    ) -> serde_json::Value {
        let bounds = widget
            .compute_bounds(window)
            .map(|bounds| [bounds.x(), bounds.y(), bounds.width(), bounds.height()]);
        let (h_min, h_nat, _, _) = widget.measure(gtk::Orientation::Horizontal, -1);
        let (v_min, v_nat, _, _) =
            widget.measure(gtk::Orientation::Vertical, widget.width().max(1));
        let role = [
            "music-header",
            "music-primary-navigation",
            "music-player-bar",
            "music-panel-content",
        ]
        .into_iter()
        .find(|name| widget.widget_name().as_str() == *name);
        json!({"depth":depth,"type":widget.type_().name(),"role":role,
            "allocation":[widget.width(),widget.height()],"bounds_in_window":bounds,
            "horizontal":[h_min,h_nat],"vertical_at_allocated_width":[v_min,v_nat],
            "request":[widget.width_request(),widget.height_request()],
            "visible":widget.is_visible(),"child_visible":widget.is_child_visible(),
            "mapped":widget.is_mapped(),"has_focus":widget.has_focus()})
    }
    fn tree(
        widget: &gtk::Widget,
        window: &adw::ApplicationWindow,
        depth: usize,
        maximum: usize,
        nodes: &mut Vec<serde_json::Value>,
    ) {
        if depth > 10 || nodes.len() >= maximum {
            return;
        }
        nodes.push(node(widget, window, depth));
        let mut child = widget.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            tree(&widget, window, depth + 1, maximum, nodes);
            if nodes.len() >= maximum {
                break;
            }
        }
    }
    let mut root = Vec::new();
    tree(ui.window.upcast_ref(), &ui.window, 0, 160, &mut root);
    let mut header = Vec::new();
    tree(ui.header.upcast_ref(), &ui.window, 0, 80, &mut header);
    let mut ancestors = Vec::new();
    let mut ancestor = ui.header.clone().upcast::<gtk::Widget>().parent();
    while let Some(widget) = ancestor {
        ancestors.push(node(&widget, &ui.window, ancestors.len()));
        ancestor = widget.parent();
        if ancestors.len() >= 12 {
            break;
        }
    }
    let data = json!({"scope":"failed-state-observer-not-acceptance",
        "category":"compact-unmapped-title-oversized-header","screenshot_saved":screenshot,
        "theme_changed":false,"animations_changed":false,"scale":ui.window.scale_factor(),
        "window":[ui.window.width(),ui.window.height()],"header_ancestors":ancestors,
        "header_tree":header,"root_tree":root});
    let saved = serde_json::to_vec_pretty(&data)
        .ok()
        .is_some_and(|bytes| std::fs::write(directory.join(format!("{name}.json")), bytes).is_ok());
    eprintln!("Karin synthetic header diagnostic: screenshot={screenshot} geometry={saved}");
}

fn assert_selection(this: &Controller, id: &str, kind: ContextKind, index: usize) {
    assert_eq!(this.track_pane.selected().unwrap().id, id);
    let selection = this.info_selection.borrow();
    let context = selection.as_ref().unwrap().context.as_ref().unwrap();
    assert_eq!(context.source_kind(), kind);
    assert_eq!(context.current_index(), index);
}

#[test]
fn current_occurrence_distinguishes_duplicate_positions_and_source() {
    let item = track("41", "Synthetic occurrence");
    let first = PlaybackContext::new(
        ContextKind::Playlist,
        Some("7".into()),
        vec![item.clone(), item.clone()],
        0,
        false,
    )
    .unwrap();
    let second = PlaybackContext::new(
        ContextKind::Playlist,
        Some("7".into()),
        vec![item.clone(), item.clone()],
        1,
        false,
    )
    .unwrap();
    let other_source =
        PlaybackContext::new(ContextKind::Album, Some("7".into()), vec![item], 0, false).unwrap();
    assert!(CurrentOccurrence::from_context(&first) != CurrentOccurrence::from_context(&second));
    assert!(
        CurrentOccurrence::from_context(&first) != CurrentOccurrence::from_context(&other_source)
    );
    assert!(CurrentOccurrence::from_context(&first) == CurrentOccurrence::from_context(&first));
}

#[test]
#[ignore = "requires isolated Xvfb, xdotool, Openbox and GTK; synthetic metadata only"]
fn native_workspace_full_shell_adaptive_pane_selection_navigation_and_cleanup() {
    gtk::init().unwrap();
    adw::init().unwrap();
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
    let _wm = WindowManager::start();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.WorkspaceShellFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let ui = super::super::super::build_with_music_pane(
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
    ui.window.set_title(Some("Karin synthetic HTTP fixture"));
    ui.window.set_default_size(1440, 900);
    let this = Rc::clone(&ui.music.controller);
    assert!(
        !this.track_pane.widget.is_visible(),
        "the unavailable pane must not be allocated during initial breakpoints"
    );
    let first = track("41", "Synthetic current track");
    let second = track("42", "Synthetic next track");
    let selected = track("43", "Synthetic selected album track");
    let context =
        PlaybackContext::new(ContextKind::LikedTracks, None, vec![first, second], 0, true).unwrap();
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(AccessToken::for_test(
            "synthetic-workspace-shell-token",
            Duration::from_secs(300),
        ));
        model.account_valid = true;
        model.queue.replace_context(context.clone());
        model.liked = Some(Ok(Page {
            items: context.items().to_vec(),
            has_more: true,
        }));
        model.playlists = Some(Ok(Page {
            items: Vec::new(),
            has_more: false,
        }));
    }
    this.widgets.account.set_text("Synthetic fixture account");
    this.render_controls();
    this.render_home_preview();
    this.render_queue();
    this.render_music_player();
    ui.window.present();
    pump_until(|| ui.window.is_mapped());
    resize(&ui, 1440, 900);
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
    let id = found.lines().next().unwrap();
    input(&["windowfocus", id]);
    let reveal = this
        .info_host
        .borrow()
        .as_ref()
        .unwrap()
        .revealer()
        .unwrap();
    let content = this.info_host.borrow().as_ref().unwrap().content().unwrap();
    let presentation = || this.info_host.borrow().as_ref().unwrap().presentation();
    assert_eq!(presentation(), InfoPresentation::Wide);
    assert!(this.info_is_open());
    assert_selection(&this, "41", ContextKind::LikedTracks, 0);
    let queue_before_cover = this.model.borrow().queue.context().cloned();
    assert!(
        !this.widgets.player_revealer.reveals_child(),
        "a selected queue alone must not expose a player"
    );
    // This metadata-only fixture intentionally has no player/request. Exercise
    // its controller selection path without inventing a visible idle footer;
    // the real-player refinement fixture covers mouse/Enter/Space on the zone.
    this.show_current_track_info();
    assert!(
        this.info_is_open(),
        "show-current information preserves the pinned open intent"
    );
    assert_selection(&this, "41", ContextKind::LikedTracks, 0);
    assert_eq!(
        this.model.borrow().queue.context(),
        queue_before_cover.as_ref()
    );
    capture_native("workspace-shell-wide-current");

    // Explicit metadata is independent of the queue/player, and harmless
    // transport updates must preserve it until the actual occurrence changes.
    let selected_context = PlaybackContext::new(
        ContextKind::Album,
        Some("7".into()),
        vec![selected.clone()],
        0,
        false,
    )
    .unwrap();
    this.select_track_info(selected.clone(), Some(selected_context.clone()), false);
    assert_selection(&this, "43", ContextKind::Album, 0);
    this.pause_music();
    this.seek_music(Duration::from_secs(12));
    this.set_music_volume(0.4);
    assert_selection(&this, "43", ContextKind::Album, 0);
    assert_eq!(this.model.borrow().queue.current().unwrap().id, "41");
    assert!(this.music_snapshot().is_none());
    capture_native("workspace-shell-wide-selected");
    // The explicit show-current path neither closes the panel nor starts an
    // idle queue while another track's metadata is being inspected.
    this.show_current_track_info();
    assert!(this.info_is_open());
    assert_selection(&this, "41", ContextKind::LikedTracks, 0);
    assert_eq!(
        this.model.borrow().queue.context(),
        queue_before_cover.as_ref()
    );
    assert!(this.music_snapshot().is_none());
    this.select_track_info(selected.clone(), Some(selected_context.clone()), false);

    // A new occurrence atomically returns to the current context. A pending
    // detail response from before that transition cannot overwrite the target.
    let stale_epoch = this.info_target_epoch();
    this.model.borrow_mut().queue.select(1);
    this.render_music_player();
    assert_selection(&this, "42", ContextKind::LikedTracks, 1);
    assert!(!this.select_track_info_if_current(
        stale_epoch,
        selected.clone(),
        Some(selected_context.clone())
    ));
    assert_selection(&this, "42", ContextKind::LikedTracks, 1);
    assert!(this.info_is_open());

    // Duplicate IDs at different positions are different occurrences. The
    // queue retains both; neither a label nor an ID-only test can stand in for it.
    let duplicate = track("41", "Synthetic duplicate occurrence");
    let duplicates = PlaybackContext::new(
        ContextKind::Playlist,
        Some("9".into()),
        vec![duplicate.clone(), duplicate],
        0,
        false,
    )
    .unwrap();
    this.model.borrow_mut().queue.replace_context(duplicates);
    this.follow_current_occurrence();
    this.render_music_player();
    this.select_track_info(selected.clone(), Some(selected_context.clone()), false);
    let first_occurrence_epoch = this.info_target_epoch();
    this.model.borrow_mut().queue.select(1);
    this.render_music_player();
    assert_selection(&this, "41", ContextKind::Playlist, 1);
    assert!(this.info_target_epoch() != first_occurrence_epoch);
    assert!(this.info_is_open());
    this.select_track_info(selected.clone(), Some(selected_context.clone()), false);
    let before_remove_epoch = this.info_target_epoch();
    this.remove_music_queue(0);
    assert_eq!(this.model.borrow().queue.current_index(), Some(0));
    assert_eq!(this.model.borrow().queue.current().unwrap().id, "41");
    assert_selection(&this, "43", ContextKind::Album, 0);
    assert_eq!(this.info_target_epoch(), before_remove_epoch);
    this.model
        .borrow_mut()
        .queue
        .replace_context(context.clone());
    this.model.borrow_mut().queue.select(1);
    this.follow_current_occurrence();
    this.render_queue();

    click(&ui.window, id, &this.track_pane.close);
    pump_until(|| !this.info_is_open());
    assert!(!this.track_info_preference());
    assert!(
        gtk::prelude::GtkWindowExt::focus(&ui.window)
            .is_some_and(|focus| focus.is_mapped() && !focus.is_ancestor(&this.track_pane.widget)),
        "closing metadata without an active player returns focus to a mapped page control"
    );
    pump_until(|| reveal.progress() == 0.0);
    assert_eq!(content.margin_end(), 0);
    assert!(!this.track_pane.close.is_mapped());
    for _ in 0..12 {
        input(&["key", "--clearmodifiers", "Tab"]);
        settle();
        assert!(
            gtk::prelude::GtkWindowExt::focus(&ui.window).is_none_or(|focus| {
                focus != *this.track_pane.widget.upcast_ref::<gtk::Widget>()
                    && !focus.is_ancestor(&this.track_pane.widget)
            }),
            "closed information is not a keyboard-focus destination"
        );
    }
    layout(&ui);
    capture_native("workspace-shell-wide-closed-info");
    resize(&ui, 900, 650);
    assert_eq!(presentation(), InfoPresentation::Medium);
    assert!(!this.info_is_open());
    resize(&ui, 1440, 900);
    assert!(!this.info_is_open());
    assert!(!this.track_info_preference());
    capture_native("workspace-shell-wide-restored-closed-info");

    // The actual accelerator opens current information, then Diagnostics only
    // suspends presentation. It does not rewrite the user's open intent.
    input(&["key", "--clearmodifiers", "ctrl+i"]);
    pump_until(|| this.info_is_open() && this.track_pane.close.has_focus());
    assert!(this.track_info_preference());
    assert_selection(&this, "42", ContextKind::LikedTracks, 1);
    ui.show_page("diagnostics");
    pump_until(|| !this.info_is_open());
    assert!(this.track_info_preference());
    assert!(!this.music_surface_visible.get());
    assert!(!this.track_pane.widget.is_visible());
    assert!(!ui.window.lookup_action("music-info").unwrap().is_enabled());
    input(&["key", "--clearmodifiers", "ctrl+i"]);
    settle();
    assert!(!this.info_is_open());
    capture_native("workspace-shell-diagnostics-pane-hidden");
    // Ctrl+J is a genuine global action: it must not be lost because the
    // diagnostics surface is still mapped when its first handler executes.
    input(&["key", "--clearmodifiers", "ctrl+j"]);
    pump_until(|| {
        this.music_surface_visible.get()
            && this.info_is_open()
            && this.panel_mode.get() == PanelMode::Queue
    });
    assert!(this.widgets.queue_count.is_mapped());
    assert!(this.widgets.clear_queue.is_mapped());
    assert!(
        this.widgets
            .queue_count
            .ancestor(gtk::ScrolledWindow::static_type())
            .is_none(),
        "source/partial count and clear remain pinned outside queue scrolling"
    );
    input(&["key", "--clearmodifiers", "ctrl+i"]);
    pump_until(|| this.info_is_open() && this.panel_mode.get() == PanelMode::TrackInfo);
    ui.show_page("music-home");
    pump_until(|| this.info_is_open());
    assert_selection(&this, "42", ContextKind::LikedTracks, 1);

    // These are real pointer, scroll and keyboard events outside the medium
    // pane. No backdrop or automatic close/reopen handler is involved.
    resize(&ui, 900, 650);
    assert_eq!(presentation(), InfoPresentation::Medium);
    assert!(this.info_is_open());
    assert_eq!(content.margin_end(), 316);
    click(&ui.window, id, &this.widgets.nav_liked);
    pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("library"));
    assert!(this.info_is_open());
    let (x, y) = test_input::pointer(
        &ui.window,
        &this.widgets.nav_liked,
        test_input::Target::Center,
    );
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "5",
    ]);
    input(&["key", "--clearmodifiers", "Tab"]);
    settle();
    assert!(this.info_is_open());
    click(&ui.window, id, &this.widgets.nav_playlists);
    pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("playlists"));
    assert!(this.info_is_open());
    let (x, y) = test_input::pointer(
        &ui.window,
        &this.widgets.nav_playlists,
        test_input::Target::Leading,
    );
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "5",
    ]);
    settle();
    assert!(
        this.info_is_open(),
        "scrolling uncovered music content cannot dismiss information"
    );
    // Native motion is retained. Neither outside input nor the end of a close
    // transition reopens the view; reduced-motion uses the same intent state.
    assert!(reveal.surface().is_some(), "one native surface is retained");
    assert!(reveal.transition_duration() > 0);
    let settings = gtk::Settings::default().unwrap();
    settings.set_gtk_enable_animations(true);
    pump_until(|| reveal.is_child_revealed());
    this.set_info_open(false);
    assert!(!this.info_is_open());
    pump_until(|| !reveal.is_child_revealed());
    assert!(!this.track_info_preference());
    this.set_info_open(true);
    pump_until(|| reveal.is_child_revealed());
    assert!(this.track_info_preference());
    settings.set_gtk_enable_animations(false);
    this.set_info_open(false);
    pump_until(|| !reveal.is_child_revealed());
    this.set_info_open(true);
    pump_until(|| reveal.is_child_revealed());
    capture_native("workspace-shell-medium-pinned-outside-navigation");

    // Keep real central actions readable and hit-testable even at the exact
    // 720px medium boundary, rather than merely reporting mapped widgets.
    click(&ui.window, id, &this.widgets.nav_liked);
    pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("library"));
    for width in [720, 900] {
        resize_logical(&ui, width, 650);
        assert_eq!(presentation(), InfoPresentation::Medium);
        assert!(this.info_is_open());
        assert_eq!(content.margin_end(), 316);
        let menu = assert_central_row_actions(&ui, &this, &content);
        click(&ui.window, id, &menu);
        let popover = menu.popover().unwrap();
        pump_until(|| popover.is_mapped());
        assert!(this.info_is_open());
        input(&["key", "--clearmodifiers", "Escape"]);
        pump_until(|| !popover.is_visible());
        assert!(this.info_is_open());
        capture_native(&format!("workspace-shell-medium-{width}-central-actions"));
    }
    // The compact primary segments no longer consume a left rail. Exercise
    // the real content-width boundary instead: one mounted surface and mode
    // survive both directions between the medium and dedicated subviews.
    resize_logical(&ui, 720, 650);
    let before_expansion = this.model.borrow().queue.context().cloned();
    for mode in [PanelMode::TrackInfo, PanelMode::Queue] {
        this.set_panel_mode(mode);
        resize_logical(&ui, 600, 650);
        pump_until(|| presentation() == InfoPresentation::Compact && !content.is_visible());
        assert!(this.info_is_open());
        assert_eq!(this.panel_mode.get(), mode);
        resize_logical(&ui, 720, 650);
        pump_until(|| presentation() == InfoPresentation::Medium && content.is_visible());
        assert_eq!(this.panel_mode.get(), mode);
        assert_eq!(
            this.model.borrow().queue.context(),
            before_expansion.as_ref()
        );
        assert!(this.music_snapshot().is_none());
    }
    this.set_panel_mode(PanelMode::TrackInfo);
    ui.show_page("music-home");

    // A mounted pane survives both directions of every breakpoint. Compact
    // presents that same component as an explicit subview, not a modal overlay.
    for (shape, width, height, expected) in [
        ("compact", 420, 720, InfoPresentation::Compact),
        ("medium", 900, 650, InfoPresentation::Medium),
        ("wide", 1440, 900, InfoPresentation::Wide),
    ] {
        if expected == InfoPresentation::Compact {
            // show_library populates a model/view; it is not a navigation
            // action. Focus a genuinely mapped page control before hiding the
            // central subtree, rather than an off-page retained widget.
            ui.show_page("music-library");
            pump_until(|| {
                this.widgets.stack.visible_child_name().as_deref() == Some("library")
                    && this.widgets.library_refresh.is_mapped()
                    && this.widgets.library_refresh.is_sensitive()
            });
            layout(&ui);
            test_input::wait_for_layout(&ui.window, &this.widgets.library_refresh);
            assert!(this.widgets.library_refresh.grab_focus());
            assert!(this.widgets.library_refresh.has_focus());
            eprintln!("Karin synthetic shell phase: mapped-library-control-focused-before-compact");
        }
        resize(&ui, width, height);
        assert_eq!(presentation(), expected);
        assert!(this.info_is_open());
        assert!(this.track_info_preference());
        assert_eq!(content.is_visible(), expected != InfoPresentation::Compact);
        if expected == InfoPresentation::Compact {
            eprintln!("Karin synthetic shell phase: wait-compact-close-focus");
            let previous_focus = RefCell::new(None);
            pump_until(|| {
                let focus = gtk::prelude::GtkWindowExt::focus(&ui.window);
                let mut ancestors = Vec::new();
                let mut parent = focus.as_ref().and_then(|widget| widget.parent());
                while let Some(widget) = parent {
                    ancestors.push(widget.type_().name().to_owned());
                    parent = widget.parent();
                    if ancestors.len() >= 8 {
                        break;
                    }
                }
                let current = (
                    focus
                        .as_ref()
                        .map(|widget| widget.type_().name().to_owned()),
                    focus
                        .as_ref()
                        .map(|widget| widget.widget_name().to_string()),
                    focus.as_ref().is_some_and(|widget| widget.is_mapped()),
                    focus
                        .as_ref()
                        .is_some_and(|widget| widget.is_ancestor(&this.track_pane.widget)),
                    ancestors,
                );
                if previous_focus.borrow().as_ref() != Some(&current) {
                    eprintln!("Karin synthetic compact-close-focus: {current:?}");
                    previous_focus.replace(Some(current));
                }
                this.track_pane.close.has_focus()
            });
            assert!(
                this.track_pane.close.has_focus(),
                "hiding a focused music subtree must move focus into the information subview"
            );
        }
        assert_eq!(
            content.margin_end(),
            match expected {
                InfoPresentation::Wide => 340,
                InfoPresentation::Medium => 316,
                InfoPresentation::Compact => 0,
            }
        );
        assert_selection(&this, "42", ContextKind::LikedTracks, 1);
        capture_native(&format!("workspace-shell-{shape}-pinned-info"));
        this.model.borrow_mut().queue.select(0);
        this.render_music_player();
        assert_selection(&this, "41", ContextKind::LikedTracks, 0);
        assert!(this.info_is_open());
        this.model.borrow_mut().queue.select(1);
        this.render_music_player();
    }

    // Details navigation preserves the pane and audio context. Compact Back is
    // an explicit close, and a later occurrence must not reopen that intent.
    resize(&ui, 420, 720);
    this.track_pane.details.grab_focus();
    input(&["key", "--clearmodifiers", "Return"]);
    pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("detail"));
    assert!(this.info_is_open());
    assert_eq!(this.model.borrow().queue.current().unwrap().id, "42");
    click(&ui.window, id, &this.track_pane.close);
    pump_until(|| !this.info_is_open() && content.is_visible());
    assert!(!this.track_info_preference());
    this.model.borrow_mut().queue.select(0);
    this.render_music_player();
    assert!(!this.info_is_open());
    capture_native("workspace-shell-compact-detail-explicit-back");
    ui.show_page("music-home");
    input(&["key", "--clearmodifiers", "ctrl+i"]);
    pump_until(|| this.info_is_open());
    resize(&ui, 1440, 900);
    eprintln!("Karin synthetic shell phase: maximize");
    ui.window.maximize();
    pump_until(|| ui.window.is_maximized());
    layout(&ui);
    assert!(this.info_is_open());
    capture_native("workspace-shell-maximized");
    eprintln!("Karin synthetic shell phase: restore");
    ui.window.unmaximize();
    pump_until(|| !ui.window.is_maximized());
    // The state notification can precede the WM's restore configure event.
    // Complete that GTK frame before issuing a different requested size.
    settle();
    resize(&ui, 900, 650);

    // Exercise the real retry callback after a failed playlist view. Keeping
    // model.borrow() alive across load_library_refresh used to panic here.
    eprintln!("Karin synthetic shell phase: playlist retry");
    this.set_info_open(false);
    settle();
    this.widgets.library_mode.set_selected(1);
    this.widgets.stack.set_visible_child_name("playlists");
    this.playlist_grid.set_error();
    click(&ui.window, id, &this.playlist_grid.retry);
    assert!(!this.model.borrow().busy);
    assert!(this.model.borrow().account_valid);
    assert_eq!(this.model.borrow().queue.items().len(), 2);
    assert!(this.music_snapshot().is_none());

    // Details can be the first music action, before any playing/queued track.
    // Ctrl+I must preserve that explicit metadata target across close/reopen,
    // rather than replacing it with a nonexistent current occurrence.
    eprintln!("Karin synthetic shell phase: explicit-information-without-queue");
    this.model.borrow_mut().queue.clear();
    this.render_music_player();
    this.select_track_info(selected.clone(), Some(selected_context.clone()), false);
    assert!(!this.widgets.current_art_button.is_sensitive());
    assert!(!this.info_is_open());
    input(&["key", "--clearmodifiers", "ctrl+i"]);
    pump_until(|| this.info_is_open() && this.track_pane.close.has_focus());
    assert_selection(&this, "43", ContextKind::Album, 0);
    assert!(this.info_selection.borrow().as_ref().unwrap().target == InfoTarget::ExplicitSelection);
    assert!(this.model.borrow().queue.context().is_none());
    assert!(this.music_snapshot().is_none());
    input(&["key", "--clearmodifiers", "ctrl+i"]);
    pump_until(|| {
        !this.info_is_open()
            && gtk::prelude::GtkWindowExt::focus(&ui.window).is_some_and(|focus| {
                focus.is_mapped()
                    && focus.is_sensitive()
                    && (focus == *this.widgets.stack.upcast_ref::<gtk::Widget>()
                        || focus.is_ancestor(&this.widgets.stack))
            })
    });
    assert_selection(&this, "43", ContextKind::Album, 0);
    input(&["key", "--clearmodifiers", "ctrl+i"]);
    pump_until(|| this.info_is_open() && this.track_pane.close.has_focus());
    assert_selection(&this, "43", ContextKind::Album, 0);
    assert!(this.info_selection.borrow().as_ref().unwrap().target == InfoTarget::ExplicitSelection);
    capture_native("workspace-shell-explicit-information-no-queue-open");
    click(&ui.window, id, &this.track_pane.close);
    pump_until(|| {
        !this.info_is_open()
            && gtk::prelude::GtkWindowExt::focus(&ui.window).is_some_and(|focus| {
                focus.is_mapped()
                    && focus.is_sensitive()
                    && (focus == *this.widgets.stack.upcast_ref::<gtk::Widget>()
                        || focus.is_ancestor(&this.widgets.stack))
            })
    });
    assert_selection(&this, "43", ContextKind::Album, 0);
    assert!(this.model.borrow().queue.context().is_none());
    assert!(this.music_snapshot().is_none());
    capture_native("workspace-shell-explicit-information-no-queue-closed");
    this.model
        .borrow_mut()
        .queue
        .replace_context(context.clone());
    this.follow_current_occurrence();
    this.render_queue();

    let logout_epoch = this.info_target_epoch();
    this.logout();
    pump_until(|| this.track_pane.selected().is_none() && !this.info_is_open());
    assert!(!this.select_track_info_if_current(logout_epoch, selected, Some(selected_context)));
    assert!(!this.track_pane.widget.is_visible());

    drop(reveal);
    drop(content);
    drop(this);
    eprintln!("Karin synthetic shell phase: close and release");
    let cleaned = ui.close_for_smoke();
    let closing_since = Instant::now();
    let reported = Cell::new(false);
    pump_until(|| {
        let complete = cleaned(false);
        if !complete && closing_since.elapsed() >= Duration::from_secs(1) && !reported.replace(true)
        {
            cleaned(true);
        }
        complete
    });
}
