// SPDX-License-Identifier: GPL-3.0-or-later
//! Native cache-presentation regressions. Views and worker replies are synthetic;
//! persistence is disabled and the loopback listener must receive no request.

use super::super::tests::{capture_native, pump_until};
use super::*;
use crate::cache::{AccountScope, Cache, CachePolicy};
use ring::rand::{SecureRandom, SystemRandom};
use std::{
    cell::Cell,
    fs,
    net::TcpListener,
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let mut nonce = [0; 16];
        SystemRandom::new().fill(&mut nonce).unwrap();
        let suffix: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
        let path = std::env::temp_dir().join(format!("karin-cache-ui-{suffix}"));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_stale(this: &Rc<Controller>) {
    assert!(this.widgets.cache_notice.is_revealed());
    assert!(
        this.widgets
            .cache_notice
            .title()
            .contains("сохранённой копии")
    );
    assert!(!this.widgets.cache_hint.is_visible());
}
fn assert_network(this: &Rc<Controller>) {
    assert!(!this.widgets.cache_notice.is_revealed());
    assert!(!this.widgets.cache_hint.is_visible());
}

/// A real GIO worker finishes while a different GTK view is active. The test
/// exercises the production callback/provenance path, without performing HTTP.
fn late_network_library_reply(this: &Rc<Controller>, page: u32) {
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let entered = Arc::new(AtomicBool::new(false));
    let finished = Rc::new(Cell::new(false));
    let worker_gate = Arc::clone(&gate);
    let worker_entered = Arc::clone(&entered);
    let completed = Rc::clone(&finished);
    this.run_catalog(
        Operation::InitialLibrary,
        Some((0, page)),
        false,
        move |_, _, _, _| {
            worker_entered.store(true, Ordering::Release);
            let (lock, changed) = &*worker_gate;
            let ready = lock.lock().unwrap();
            let (ready, timeout) = changed
                .wait_timeout_while(ready, Duration::from_secs(5), |ready| !*ready)
                .unwrap();
            assert!(
                *ready && !timeout.timed_out(),
                "synthetic library gate was not released"
            );
            Ok(Cached {
                value: (),
                provenance: Provenance::Network,
                lease: None,
                cache_error: None,
            })
        },
        move |_, result| {
            result.unwrap();
            completed.set(true);
        },
    );
    pump_until(|| entered.load(Ordering::Acquire));
    assert_stale(this);
    {
        let (lock, changed) = &*gate;
        *lock.lock().unwrap() = true;
        changed.notify_all();
    }
    pump_until(|| finished.get() && !this.model.borrow().busy);
}

fn retired_cached_reply_is_retried(this: &Rc<Controller>, clear: bool) {
    let scratch = Scratch::new();
    let cache = Cache::new(scratch.0.join("cache"), CachePolicy::default()).unwrap();
    cache.bind(Some(AccountScope::from_account_id("1234").unwrap()));
    let lease = cache.lease().unwrap();
    let old_lease = lease.clone();
    let case = if clear { "clear" } else { "disable" };
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let entered = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let finished = Rc::new(Cell::new(false));
    let label = gtk::Label::new(Some("Synthetic current data is loading"));
    label.set_wrap(true);
    clear_box(&this.widgets.detail);
    this.widgets.detail.append(&label);
    this.detail_cache.set(Some((Provenance::StaleCache, false)));
    this.widgets.stack.set_visible_child_name("detail");
    this.refresh_catalog_notice();
    let (worker_gate, worker_entered, worker_calls) =
        (Arc::clone(&gate), Arc::clone(&entered), Arc::clone(&calls));
    let completed = Rc::clone(&finished);
    let displayed = label.clone();
    this.run_catalog(
        Operation::Detail,
        None,
        false,
        move |_, _, _, force| {
            worker_calls.fetch_add(1, Ordering::AcqRel);
            if force {
                return Ok(Cached {
                    value: format!("Synthetic network reply after {case}"),
                    provenance: Provenance::Network,
                    lease: None,
                    cache_error: None,
                });
            }
            worker_entered.store(true, Ordering::Release);
            let (lock, changed) = &*worker_gate;
            let ready = lock.lock().unwrap();
            let (ready, timeout) = changed
                .wait_timeout_while(ready, Duration::from_secs(5), |ready| !*ready)
                .unwrap();
            assert!(
                *ready && !timeout.timed_out(),
                "synthetic cached reply gate was not released"
            );
            Ok(Cached {
                value: format!("Retired cached reply before {case}"),
                provenance: Provenance::FreshCache,
                lease: Some(lease.clone()),
                cache_error: None,
            })
        },
        move |_, result| {
            displayed.set_text(&result.unwrap());
            completed.set(true);
        },
    );
    pump_until(|| entered.load(Ordering::Acquire));
    if clear {
        cache.clear().unwrap();
    } else {
        cache
            .configure(CachePolicy {
                enabled: false,
                ..CachePolicy::default()
            })
            .unwrap();
    }
    assert!(!old_lease.is_current());
    {
        let (lock, changed) = &*gate;
        *lock.lock().unwrap() = true;
        changed.notify_all();
    }
    pump_until(|| finished.get() && !this.model.borrow().busy);
    assert_eq!(
        calls.load(Ordering::Acquire),
        2,
        "exactly one fresh retry follows a retired cache reply"
    );
    assert_eq!(
        label.text().as_str(),
        format!("Synthetic network reply after {case}")
    );
    assert_network(this);
    assert_eq!(this.detail_cache.get(), Some((Provenance::Network, false)));
    assert_eq!(cache.usage().unwrap().entries, 0);
    capture_native(&format!("cache-{case}-retired-reply-replaced"));
}

#[test]
#[ignore = "requires isolated session D-Bus and GTK display; run explicitly under Xvfb"]
fn native_catalog_cache_provenance_interleaving_and_retired_replies() {
    gtk::init().unwrap();
    adw::init().unwrap();
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let service = MusicService::for_test(&format!("http://{}/", listener.local_addr().unwrap()));
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.CacheProvenanceFixture")
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
                Some(service),
                Arc::new(OfflineProvider),
                launch_system_browser,
            )
        },
    );
    let window = ui.window.clone();
    window.set_title(Some("Karin synthetic HTTP fixture"));
    window.set_default_size(900, 650);
    window.present();
    let this = Rc::clone(&ui.music.controller);
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(AccessToken::for_test(
            "synthetic-cache-ui-token",
            Duration::from_secs(60),
        ));
        model.account_valid = true;
        model.liked = Some(Ok(Page {
            items: Vec::new(),
            has_more: false,
        }));
        model.library_displaying = Some((0, 0));
    }
    this.widgets.account.set_text("Синтетический аккаунт");
    this.render_controls();
    this.remember_library_origin(0, 0, Provenance::FreshCache, false);
    this.detail_cache.set(Some((Provenance::StaleCache, false)));
    this.widgets.detail.append(&gtk::Label::new(Some(
        "Синтетическая карточка: сохранённые сведения",
    )));
    this.widgets.stack.set_visible_child_name("detail");
    this.refresh_catalog_notice();
    pump_until(|| this.widgets.cache_notice.height() > 0);
    assert_stale(&this);
    capture_native("cache-detail-stale-before-background-library");
    late_network_library_reply(&this, 0);
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("detail")
    );
    assert_stale(&this);
    capture_native("cache-detail-stale-after-background-library");

    // Navigation must choose that view's remembered origin, not the last
    // independent operation to complete. Do not manually refresh after switching.
    this.widgets.stack.set_visible_child_name("library");
    pump_until(|| !this.widgets.cache_notice.is_revealed());
    assert_network(&this);
    this.widgets.stack.set_visible_child_name("search");
    this.remember_library_origin(0, 0, Provenance::FreshCache, false);
    this.widgets.stack.set_visible_child_name("library");
    pump_until(|| this.widgets.cache_hint.is_visible());
    assert!(!this.widgets.cache_notice.is_revealed());
    assert!(this.widgets.cache_hint.text().contains("Сохранённые"));
    capture_native("cache-library-fresh-saved-hint");

    // A late page-zero initial read must not erase the currently displayed
    // page-one provenance in the same library section.
    this.remember_library_origin(0, 1, Provenance::StaleCache, false);
    this.model.borrow_mut().library_displaying = Some((0, 1));
    // Map the newly selected synthetic page through the ordinary route signal,
    // exactly as navigation does; never manually refresh after switching.
    this.widgets.stack.set_visible_child_name("search");
    this.widgets.stack.set_visible_child_name("library");
    pump_until(|| this.widgets.cache_notice.is_revealed());
    // Queue is a companion surface, not a different provenance-bearing page.
    // Opening it must leave the underlying library's saved-data status intact.
    this.set_panel_mode(workspace::PanelMode::Queue);
    assert_stale(&this);
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("library")
    );
    this.set_panel_mode(workspace::PanelMode::Closed);
    this.widgets.stack.set_visible_child_name("search");
    assert_network(&this);
    this.widgets.stack.set_visible_child_name("library");
    pump_until(|| this.widgets.cache_notice.is_revealed());
    assert_stale(&this);
    late_network_library_reply(&this, 0);
    assert_stale(&this);
    assert_eq!(this.model.borrow().library_displaying, Some((0, 1)));
    capture_native("cache-library-page1-stale-after-page0-background");

    this.widgets.stack.set_visible_child_name("detail");
    pump_until(|| this.widgets.cache_notice.is_revealed());
    assert_stale(&this);
    this.set_panel_mode(workspace::PanelMode::Queue);
    assert_stale(&this);
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("detail")
    );
    this.set_panel_mode(workspace::PanelMode::Closed);
    this.widgets.stack.set_visible_child_name("home");
    assert_network(&this); // page0 was freshly replaced by the network reply
    this.widgets.stack.set_visible_child_name("detail");
    assert_stale(&this);

    retired_cached_reply_is_retried(&this, true);
    retired_cached_reply_is_retried(&this, false);
    this.model.borrow_mut().account_valid = false;
    this.detail_cache.set(Some((Provenance::StaleCache, true)));
    this.refresh_catalog_notice();
    assert_network(&this);
    listener.set_nonblocking(true).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "native provenance fixture made no HTTP request"
    );
    this.close();
    window.close();
    drop(this);
    drop(ui);
    drop(window);
}
