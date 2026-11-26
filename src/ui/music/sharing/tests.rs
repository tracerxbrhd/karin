// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::ui::music::{
    test_input::wait_for_layout,
    tests::{capture_native, pump_until},
};
use std::process::Command;

fn input(args: &[&str]) {
    assert!(
        Command::new("xdotool")
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}

fn key(key: &str) {
    input(&["key", "--clearmodifiers", key]);
}

fn focus_window(name: &str) {
    let id = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", name])
        .output()
        .unwrap();
    assert!(id.status.success());
    let id = String::from_utf8(id.stdout).unwrap();
    input(&["windowfocus", "--sync", id.lines().next().unwrap()]);
}

#[test]
#[ignore = "uses native GTK input and xdotool under Xvfb"]
fn native_shortcuts_dispatch_and_preserve_widget_input() {
    gtk::init().unwrap();
    adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.ShortcutsFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let ui = crate::ui::build_shell(&app, None, |app, window| {
        MusicPane::with_provider(
            app,
            window,
            None,
            Arc::new(OfflineProvider),
            launch_system_browser,
        )
    });
    let window = &ui.window;
    window.set_title(Some("Karin synthetic HTTP fixture"));
    let this = &ui.music.controller;
    {
        let mut model = this.model.borrow_mut();
        model.account_valid = true;
        model.liked = Some(Ok(Page {
            items: Vec::new(),
            has_more: false,
        }));
        let empty = Page {
            items: Vec::new(),
            has_more: false,
        };
        model.playlists = Some(Ok(empty.clone()));
        model.liked_playlists = Some(Ok(empty));
    }
    this.render_controls();
    window.present();
    wait_for_layout(window, &this.widgets.nav_search);
    focus_window("^Karin synthetic HTTP fixture$");

    key("F1");
    pump_until(|| {
        window
            .visible_dialog()
            .is_some_and(|dialog| dialog.widget_name() == "karin-shortcuts" && dialog.is_mapped())
    });
    capture_native("keyboard-shortcuts");
    key("Escape");
    pump_until(|| window.visible_dialog().is_none());

    window.set_default_size(420, 720);
    pump_until(|| {
        window
            .surface()
            .is_some_and(|surface| surface.width() == 420)
    });
    key("F1");
    pump_until(|| {
        window.visible_dialog().is_some_and(|dialog| {
            dialog.is_mapped() && dialog.width() > 0 && dialog.width() <= window.width()
        })
    });
    capture_native("keyboard-shortcuts-compact");
    key("Escape");
    pump_until(|| window.visible_dialog().is_none());
    window.set_default_size(900, 650);
    pump_until(|| {
        window
            .surface()
            .is_some_and(|surface| surface.width() == 900)
    });

    for (keyboard, page) in [
        ("ctrl+2", "library"),
        ("ctrl+3", "playlists"),
        ("ctrl+1", "home"),
    ] {
        key(keyboard);
        pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some(page));
    }

    key("ctrl+f");
    pump_until(|| {
        this.widgets.search.is_mapped()
            && gtk::prelude::GtkWindowExt::focus(window).is_some_and(|focus| {
                focus == *this.widgets.search.upcast_ref::<gtk::Widget>()
                    || focus.is_ancestor(&this.widgets.search)
            })
    });
    input(&["key", "--clearmodifiers", "a", "space", "b"]);
    pump_until(|| this.widgets.search.text() == "a b");
    key("alt+Home");
    pump_until(|| {
        !this.widgets.search.is_mapped()
            && this.widgets.stack.visible_child_name().as_deref() == Some("home")
    });

    // Probe the same native input adapter with a mapped transport surface.
    // Playback fixtures separately verify what the real footer buttons do.
    let controls = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let free_focus = gtk::DrawingArea::new();
    free_focus.set_content_height(24);
    free_focus.set_focusable(true);
    let entry = gtk::Entry::new();
    let play = gtk::Button::with_label("Synthetic play");
    let previous = gtk::Button::with_label("Synthetic previous");
    let next = gtk::Button::with_label("Synthetic next");
    for child in [
        free_focus.upcast_ref::<gtk::Widget>(),
        entry.upcast_ref(),
        play.upcast_ref(),
        previous.upcast_ref(),
        next.upcast_ref(),
    ] {
        controls.append(child);
    }
    let transport = adw::ApplicationWindow::builder()
        .application(&app)
        .title("Karin synthetic shortcut transport")
        .content(&controls)
        .build();
    crate::ui::shortcuts::install_transport(&transport, &play, &previous, &next);
    let clicks = Rc::new(RefCell::new(Vec::new()));
    for (button, name) in [(&play, "play"), (&previous, "previous"), (&next, "next")] {
        let clicks = Rc::clone(&clicks);
        button.connect_clicked(move |_| clicks.borrow_mut().push(name));
    }
    transport.present();
    wait_for_layout(&transport, &play);
    focus_window("^Karin synthetic shortcut transport$");
    assert!(free_focus.grab_focus());
    for (key_name, count) in [("space", 1), ("ctrl+shift+p", 2), ("ctrl+shift+n", 3)] {
        key(key_name);
        pump_until(|| clicks.borrow().len() == count);
    }
    assert!(entry.grab_focus());
    input(&["key", "--clearmodifiers", "a", "space", "b"]);
    pump_until(|| entry.text() == "a b");
    assert!(next.grab_focus());
    key("space");
    pump_until(|| clicks.borrow().len() == 4);
    assert_eq!(*clicks.borrow(), ["play", "previous", "next", "next"]);
    transport.close();
    ui.music.close();
    window.close();
}
