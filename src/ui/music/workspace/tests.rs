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

fn layout(ui: &crate::ui::MainWindow) {
    let previous = RefCell::new(None);
    pump_until(|| match ui.verify_layout() {
        Ok(()) => true,
        Err(error) => {
            if previous.borrow().as_ref() != Some(&error) {
                eprintln!("Karin synthetic shell layout pending: {error}");
                previous.replace(Some(error));
            }
            false
        }
    });
}

fn assert_selection(this: &Controller, id: &str, kind: ContextKind, index: usize) {
    assert_eq!(this.track_pane.selected().unwrap().id, id);
    let selection = this.info_selection.borrow();
    let context = selection.as_ref().unwrap().context.as_ref().unwrap();
    assert_eq!(context.source_kind(), kind);
    assert_eq!(context.current_index(), index);
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
    let split = this
        .info_split
        .borrow()
        .as_ref()
        .and_then(glib::WeakRef::upgrade)
        .unwrap();
    assert!(!split.is_collapsed());
    assert!(split.shows_sidebar());
    assert_selection(&this, "41", ContextKind::LikedTracks, 0);
    capture_native("workspace-shell-wide-current");

    // An explicit selection is distinct from the actual queue/current item.
    let selected_context = PlaybackContext::new(
        ContextKind::Album,
        Some("7".into()),
        vec![selected.clone()],
        0,
        false,
    )
    .unwrap();
    this.select_track_info(selected, Some(selected_context), false);
    assert_selection(&this, "43", ContextKind::Album, 0);
    assert_eq!(this.model.borrow().queue.current().unwrap().id, "41");
    this.model.borrow_mut().queue.select(1);
    this.render_music_player();
    assert_selection(&this, "43", ContextKind::Album, 0);
    capture_native("workspace-shell-wide-selected");

    click(&ui.window, id, &this.track_pane.close);
    pump_until(|| !split.shows_sidebar());
    assert!(!this.track_info_preference());
    assert!(this.widgets.current_art_button.has_focus());
    resize(&ui, 900, 650);
    assert!(split.is_collapsed());
    assert!(!split.shows_sidebar());
    resize(&ui, 1440, 900);
    assert!(!split.shows_sidebar());
    assert!(!this.track_info_preference());

    // Actual accelerator reopens the current occurrence, not the old selection.
    input(&["key", "--clearmodifiers", "ctrl+i"]);
    pump_until(|| split.shows_sidebar() && this.track_pane.close.has_focus());
    assert!(this.track_info_preference());
    assert_selection(&this, "42", ContextKind::LikedTracks, 1);
    ui.show_page("diagnostics");
    pump_until(|| !split.shows_sidebar());
    assert!(!this.music_surface_visible.get());
    capture_native("workspace-shell-diagnostics-pane-hidden");
    ui.show_page("music-home");
    pump_until(|| split.shows_sidebar());
    assert!(this.music_surface_visible.get());
    assert_selection(&this, "42", ContextKind::LikedTracks, 1);

    for (shape, width, height) in [("compact", 420, 720), ("medium", 900, 650)] {
        resize(&ui, width, height);
        assert!(split.is_collapsed());
        assert!(
            !split.shows_sidebar(),
            "resize must not auto-open an overlay"
        );
        this.model.borrow_mut().queue.select(0);
        this.render_music_player();
        assert!(
            !split.shows_sidebar(),
            "a current-track update is not a popup"
        );
        assert_selection(&this, "41", ContextKind::LikedTracks, 0);
        capture_native(&format!("workspace-shell-{shape}-content"));
        click(&ui.window, id, &this.widgets.current_art_button);
        pump_until(|| split.shows_sidebar() && this.track_pane.close.has_focus());
        capture_native(&format!("workspace-shell-{shape}-manual-info"));

        // A compact Details action must expose its destination rather than
        // leaving the overlay above it. None service gives a local error page;
        // this tests GTK navigation, not a remote detail/API acceptance.
        this.track_pane.details.grab_focus();
        input(&["key", "--clearmodifiers", "Return"]);
        pump_until(|| {
            !split.shows_sidebar()
                && this.widgets.stack.visible_child_name().as_deref() == Some("detail")
        });
        assert!(
            this.track_info_preference(),
            "compact navigation must preserve the wide preference"
        );
        assert_eq!(this.model.borrow().queue.current().unwrap().id, "41");
        capture_native(&format!("workspace-shell-{shape}-detail-destination"));
        ui.show_page("music-home");
        pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("home"));
        assert!(!split.shows_sidebar());
    }

    resize(&ui, 1440, 900);
    assert!(split.shows_sidebar());
    eprintln!("Karin synthetic shell phase: maximize");
    ui.window.maximize();
    pump_until(|| ui.window.is_maximized());
    layout(&ui);
    assert!(split.shows_sidebar());
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
    this.widgets.stack.set_visible_child_name("library");
    this.widgets.library_mode.set_selected(1);
    this.playlist_grid.set_error();
    click(&ui.window, id, &this.playlist_grid.retry);
    assert!(!this.model.borrow().busy);
    assert!(this.model.borrow().account_valid);
    assert_eq!(this.model.borrow().queue.items().len(), 2);
    assert!(this.music_snapshot().is_none());

    drop(split);
    drop(this);
    eprintln!("Karin synthetic shell phase: close and release");
    let cleaned = ui.close_for_smoke();
    pump_until(|| cleaned(false));
}
