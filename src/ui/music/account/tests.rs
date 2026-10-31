// SPDX-License-Identifier: GPL-3.0-or-later
//! Real local mouse/keyboard input; no account or provider request is made.

use super::super::tests::{capture_native, pump_until};
use super::*;
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

static ACCOUNT_BROWSER_OPENS: AtomicUsize = AtomicUsize::new(0);
static ACCOUNT_BROWSER_FAILURE: AtomicBool = AtomicBool::new(false);

fn account_browser_fixture(uri: &str) -> Result<(), AuthError> {
    // No real browser/account is opened by this native input test. Any other
    // destination, account identifier or added query is a test failure.
    assert_eq!(uri, ACCOUNT_URI);
    ACCOUNT_BROWSER_OPENS.fetch_add(1, Ordering::SeqCst);
    if ACCOUNT_BROWSER_FAILURE.load(Ordering::SeqCst) {
        Err(AuthError::BrowserLoad)
    } else {
        Ok(())
    }
}

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

fn wait_for_target_layout(window: &adw::ApplicationWindow, widget: &impl IsA<gtk::Widget>) {
    super::super::test_input::wait_for_layout(window, widget);
}

fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    let (x, y) =
        super::super::test_input::pointer(window, widget, super::super::test_input::Target::Center);
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
        artist_links: Vec::new(),
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
    ACCOUNT_BROWSER_OPENS.store(0, Ordering::SeqCst);
    ACCOUNT_BROWSER_FAILURE.store(false, Ordering::SeqCst);
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
                account_browser_fixture,
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
    let avatar = this
        .widgets
        .account_menu
        .child()
        .and_downcast::<adw::Avatar>()
        .expect("account action is a circular native Avatar");
    assert_eq!(avatar.size(), 28);
    assert!(avatar.shows_initials());
    assert_eq!(avatar.text().as_deref(), Some("Synthetic account"));
    assert!(avatar.custom_image().is_none());
    assert!(!this.widgets.account_menu.must_always_show_arrow());
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
        wait_for_target_layout(window, &this.widgets.account_menu);
        ui.verify_layout().expect(
            "signed-in navigation and full Home title must fit before opening an account popover",
        );

        // A real click opens only a menu. It cannot stop audio, clear the
        // queue, invalidate a token or even create the confirmation yet.
        click(window, id, &this.widgets.account_menu);
        pump_until(|| this.widgets.account_menu.popover().unwrap().is_visible());
        assert!(this.logout_confirmation.borrow().is_none());
        assert_eq!(this.model.borrow().generation, original_generation);
        assert!(!token.is_expired());
        assert_eq!(this.model.borrow().queue.items().len(), 1);
        assert!(!this.widgets.account_launch_error.is_visible());
        assert!(
            this.widgets
                .account_open
                .is_ancestor(&this.widgets.account_menu.popover().unwrap())
        );
        capture_native(&format!("account-menu-{name}"));
        if name == "compact" {
            // Only the explicit browser action launches an external page;
            // account-menu opening and failed launches never end the session.
            ACCOUNT_BROWSER_FAILURE.store(true, Ordering::SeqCst);
            click(window, id, &this.widgets.account_open);
            pump_until(|| this.widgets.account_launch_error.is_visible());
            assert_eq!(ACCOUNT_BROWSER_OPENS.load(Ordering::SeqCst), 1);
            assert!(this.widgets.account_menu.popover().unwrap().is_visible());
            assert!(this.logout_confirmation.borrow().is_none());
            assert!(!token.is_expired());
            assert_eq!(this.model.borrow().queue.items().len(), 1);
            assert_eq!(this.model.borrow().generation, original_generation);
            capture_native("account-browser-retry-compact");
            ACCOUNT_BROWSER_FAILURE.store(false, Ordering::SeqCst);
            click(window, id, &this.widgets.account_open);
            pump_until(|| !this.widgets.account_menu.popover().unwrap().is_visible());
            assert_eq!(ACCOUNT_BROWSER_OPENS.load(Ordering::SeqCst), 2);
            assert!(!this.widgets.account_launch_error.is_visible());
            assert!(!token.is_expired());
            assert_eq!(this.model.borrow().queue.items().len(), 1);
            click(window, id, &this.widgets.account_menu);
            pump_until(|| this.widgets.account_menu.popover().unwrap().is_visible());
        }
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
    let launches = ACCOUNT_BROWSER_OPENS.load(Ordering::SeqCst);
    this.widgets.account_open.emit_clicked();
    assert_eq!(ACCOUNT_BROWSER_OPENS.load(Ordering::SeqCst), launches);
    assert!(!this.widgets.account_launch_error.is_visible());
    ui.music.close();
    this.widgets.account_open.emit_clicked();
    assert_eq!(ACCOUNT_BROWSER_OPENS.load(Ordering::SeqCst), launches);
    window.close();
}
