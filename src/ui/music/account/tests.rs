// SPDX-License-Identifier: GPL-3.0-or-later
//! Real local mouse/keyboard input; no account or provider request is made.

use super::super::tests::{capture_native, pump_until};
use super::*;
use std::time::{Duration, Instant};

fn input(arguments: &[&str]) {
    assert!(
        std::process::Command::new("xdotool")
            .args(arguments)
            .status()
            .unwrap()
            .success()
    );
}

fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut result = vec![widget.clone()];
    let mut child = widget.first_child();
    while let Some(widget) = child {
        result.extend(descendants(&widget));
        child = widget.next_sibling();
    }
    result
}

fn response_button(dialog: &adw::AlertDialog, label: &str) -> gtk::Button {
    descendants(dialog.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some(label))
        .expect("native confirmation button")
}

fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref();
    pump_until(|| {
        widget.is_mapped()
            && widget.width() > 0
            && widget.height() > 0
            && widget
                .native()
                .and_then(|native| native.surface())
                .is_some_and(|surface| surface.is_mapped())
    });
    // A popup's mapped signal precedes its final native allocation. Observe
    // stable geometry on two real frames before sending an X11 coordinate;
    // otherwise a fast test can click the account menu's previous popup spot.
    let stable = Rc::new(Cell::new(0));
    let previous = Rc::new(RefCell::new(None));
    let ready = Rc::new(Cell::new(false));
    let painted = ready.clone();
    let host = window.downgrade();
    widget.add_tick_callback(move |widget, _| {
        let Some(window) = host.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let Some(rect) = widget.compute_bounds(&window) else {
            return glib::ControlFlow::Continue;
        };
        let geometry = (
            rect.x().to_bits(),
            rect.y().to_bits(),
            rect.width().to_bits(),
            rect.height().to_bits(),
        );
        if previous.borrow().as_ref() == Some(&geometry) {
            stable.set(stable.get() + 1);
        } else {
            stable.set(1);
        }
        *previous.borrow_mut() = Some(geometry);
        if stable.get() >= 2 {
            painted.set(true);
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
    pump_until(|| ready.get());
    let rect = widget
        .compute_bounds(window)
        .expect("rendered account input target");
    let x = (rect.x() + rect.width() / 2.0) as i32;
    let y = (rect.y() + rect.height() / 2.0) as i32;
    assert!(x >= 0 && x < window.width() && y >= 0 && y < window.height());
    input(&[
        "mousemove",
        "--sync",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
    ]);
    input(&["click", "1"]);
}

fn confirmation(this: &Rc<Controller>, phase: &str) -> LogoutConfirmation {
    let context = glib::MainContext::default();
    // Same budget as the shared native fixture helper; the diagnostic contains
    // only fixed phase labels and booleans/counts, never session data.
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if this
            .logout_confirmation
            .borrow()
            .as_ref()
            .is_some_and(|confirmation| confirmation.ready.get() && confirmation.dialog.is_mapped())
        {
            break;
        }
        if Instant::now() >= deadline {
            let window = this.window.upgrade().unwrap();
            let confirmation = this.logout_confirmation.borrow();
            let model = this.model.borrow();
            let retired = this
                .retiring_logins
                .borrow()
                .iter()
                .filter(|old| {
                    old.upgrade()
                        .is_some_and(|dialog| dialog.parent().is_some())
                })
                .count();
            panic!(
                "account confirmation timeout phase={phase} current={} mapped={} parented={} ready={} close_requested={} native_dialogs={} retired_parented={retired} account_valid={} closed={} token_present={} account_menu_visible={} popover_visible={}",
                confirmation.is_some(),
                confirmation
                    .as_ref()
                    .is_some_and(|confirmation| confirmation.dialog.is_mapped()),
                confirmation
                    .as_ref()
                    .is_some_and(|confirmation| confirmation.dialog.parent().is_some()),
                confirmation
                    .as_ref()
                    .is_some_and(|confirmation| confirmation.ready.get()),
                confirmation
                    .as_ref()
                    .is_some_and(|confirmation| confirmation.close_requested.get()),
                window.dialogs().n_items(),
                model.account_valid,
                model.closed,
                model.token.is_some(),
                this.widgets.account_menu.is_visible(),
                this.widgets
                    .account_menu
                    .popover()
                    .is_some_and(|popover| popover.is_visible()),
            );
        }
        if context.pending() {
            context.iteration(false);
        } else {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    this.logout_confirmation.borrow().as_ref().unwrap().clone()
}

fn wait_removed(this: &Rc<Controller>, window: &adw::ApplicationWindow, old: &LogoutConfirmation) {
    pump_until(|| {
        this.logout_confirmation.borrow().is_none()
            && window.dialogs().n_items() == 0
            && old.dialog.parent().is_none()
    });
}

fn synthetic_track() -> Track {
    Track {
        id: "42".into(),
        title: "Synthetic account-menu track".into(),
        artists: vec!["Synthetic artist".into()],
        album_id: None,
        album_title: None,
        duration_ms: Some(123_000),
        cover: None,
        explicit: false,
        available: true,
    }
}

#[test]
#[ignore = "requires an isolated GTK/X11 display and xdotool; run explicitly under Xvfb"]
fn native_account_menu_logout_cancel_default_confirm_and_stale_session() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.AccountFixture")
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
    let window = &ui.window;
    window.set_title(Some("Karin synthetic HTTP fixture"));
    window.set_default_size(900, 650);
    let this = &ui.music.controller;
    let token = AccessToken::for_test("synthetic-account-menu-token", Duration::from_secs(300));
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token.clone());
        model.account_valid = true;
        model.queue.append(synthetic_track()).unwrap();
    }
    this.widgets.account.set_text("Synthetic account");
    this.render_controls();
    this.render_queue();
    window.present();
    pump_until(|| window.is_mapped() && this.widgets.account_menu.is_mapped());
    let found = std::process::Command::new("xdotool")
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
    assert!(id.bytes().all(|byte| byte.is_ascii_digit()));
    input(&["windowfocus", "--sync", id]);
    let original_generation = this.model.borrow().generation;
    assert!(
        this.widgets
            .logout
            .is_ancestor(&this.widgets.account_menu.popover().unwrap())
    );
    assert!(
        !descendants(window.upcast_ref())
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
            .any(|button| button.icon_name().as_deref() == Some("system-log-out-symbolic"))
    );

    for (name, width, height) in [
        ("compact", 420, 720),
        ("medium", 900, 650),
        ("wide", 1440, 900),
    ] {
        window.set_default_size(width, height);
        pump_until(|| {
            window
                .surface()
                .is_some_and(|surface| surface.width() == width && surface.height() == height)
        });

        // A real click opens only a menu. It cannot stop audio, clear the
        // queue, invalidate a token or even create the confirmation yet.
        click(window, id, &this.widgets.account_menu);
        pump_until(|| this.widgets.account_menu.popover().unwrap().is_visible());
        assert!(this.logout_confirmation.borrow().is_none());
        assert_eq!(this.model.borrow().generation, original_generation);
        assert!(!token.is_expired());
        assert_eq!(this.model.borrow().queue.items().len(), 1);
        capture_native(&format!("account-menu-{name}"));
        click(window, id, &this.widgets.logout);
        let old = confirmation(this, &format!("{name}-default-cancel"));
        assert_eq!(old.dialog.default_response().as_deref(), Some("cancel"));
        assert_eq!(old.dialog.close_response(), "cancel");
        assert_eq!(
            old.dialog.response_appearance("logout"),
            adw::ResponseAppearance::Destructive
        );
        let cancel = response_button(&old.dialog, "Отмена");
        assert!(
            cancel.has_focus(),
            "the safe Cancel action owns initial keyboard focus"
        );
        capture_native(&format!("logout-confirmation-{name}"));
        // Default Enter is safe; no direct call to logout or response signal.
        input(&["key", "--clearmodifiers", "Return"]);
        wait_removed(this, window, &old);
        assert!(!token.is_expired());
        assert!(this.model.borrow().account_valid);
        assert_eq!(this.model.borrow().generation, original_generation);
        assert_eq!(this.model.borrow().queue.items().len(), 1);

        assert!(this.widgets.account_menu.grab_focus());
        input(&["key", "--clearmodifiers", "space"]);
        pump_until(|| this.widgets.account_menu.popover().unwrap().is_visible());
        assert!(this.widgets.logout.grab_focus());
        input(&["key", "--clearmodifiers", "Return"]);
        let escaped = confirmation(this, &format!("{name}-escape"));
        input(&["key", "--clearmodifiers", "Escape"]);
        wait_removed(this, window, &escaped);
        assert!(!token.is_expired());
        assert_eq!(this.model.borrow().queue.items().len(), 1);
    }

    // A dialog authorizes only the captured session. A response already queued
    // before session replacement cannot log out a subsequently established one.
    this.widgets.logout.emit_clicked();
    let stale = confirmation(this, "stale-generation");
    let replacement = AccessToken::for_test(
        "synthetic-replacement-account-token",
        Duration::from_secs(300),
    );
    {
        let mut model = this.model.borrow_mut();
        model.generation += 1;
        model.token = Some(replacement.clone());
    }
    response_button(&stale.dialog, "Выйти из аккаунта").emit_clicked();
    wait_removed(this, window, &stale);
    assert!(!replacement.is_expired());
    assert!(this.model.borrow().account_valid);
    assert_eq!(this.model.borrow().queue.items().len(), 1);
    stale.dialog.emit_by_name::<()>("response", &[&"logout"]);
    assert!(!replacement.is_expired());

    click(window, id, &this.widgets.account_menu);
    pump_until(|| this.widgets.account_menu.popover().unwrap().is_visible());
    click(window, id, &this.widgets.logout);
    let confirmed = confirmation(this, "final-pointer-confirm");
    let generation = this.model.borrow().generation;
    this.widgets.logout.emit_clicked();
    assert_eq!(
        this.logout_confirmation.borrow().as_ref().unwrap().dialog,
        confirmed.dialog
    );
    assert_eq!(window.dialogs().n_items(), 1);
    assert_eq!(this.model.borrow().generation, generation);
    click(
        window,
        id,
        &response_button(&confirmed.dialog, "Выйти из аккаунта"),
    );
    wait_removed(this, window, &confirmed);
    assert!(replacement.is_expired());
    assert!(this.model.borrow().token.is_none());
    assert!(!this.model.borrow().account_valid);
    assert!(this.model.borrow().queue.items().is_empty());
    assert!(!this.widgets.account_menu.is_visible());
    ui.music.close();
    window.close();
}
