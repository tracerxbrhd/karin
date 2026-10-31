// SPDX-License-Identifier: GPL-3.0-or-later
//! Interactive, account-free regression for navigation after synthetic input.
//!
//! The fixture page owns its JavaScript, as an ordinary website would. The test
//! sends actual pointer/keyboard events; it never injects scripts or reads DOM.

use super::*;
use crate::auth::{FrameContext, NavigationEvent, RejectionReason, SafeNavigationDiagnostic};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const TITLE: &str = "Karin interactive auth fixture";
const TYPED: &str = "13579";
const UNUSED_TOKEN: &str = "synthetic-unused-interactive-token";

#[derive(Clone, Copy)]
enum Finish {
    Valid,
    MissingState,
    WrongState,
}

#[derive(Default)]
struct Observed {
    input: AtomicUsize,
    qr: AtomicUsize,
    child: AtomicUsize,
    auxiliary: AtomicUsize,
    slow_started: AtomicBool,
    slow_release: AtomicBool,
    slow_finished: AtomicBool,
    initial_cookies: Mutex<Vec<bool>>,
}

struct Fixture {
    origin: String,
    auxiliary_origin: String,
    observed: Arc<Observed>,
    stop: Arc<AtomicBool>,
    servers: Vec<thread::JoinHandle<()>>,
}

impl Fixture {
    fn new(finish: Finish) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let auxiliary = TcpListener::bind("127.0.0.1:0").unwrap();
        auxiliary.set_nonblocking(true).unwrap();
        let auxiliary_origin = format!("http://{}", auxiliary.local_addr().unwrap());
        let observed = Arc::new(Observed::default());
        let stop = Arc::new(AtomicBool::new(false));
        let mut servers = Vec::new();
        let main_observed = observed.clone();
        let main_stop = stop.clone();
        let main_origin = origin.clone();
        let external = auxiliary_origin.clone();
        servers.push(thread::spawn(move || {
            let mut workers = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(90);
            while !main_stop.load(Ordering::Acquire) {
                assert!(
                    Instant::now() < deadline,
                    "interactive fixture did not stop"
                );
                match listener.accept() {
                    Ok((stream, _)) => {
                        let observed = main_observed.clone();
                        let stop = main_stop.clone();
                        let origin = main_origin.clone();
                        let external = external.clone();
                        workers.push(thread::spawn(move || {
                            serve(stream, &origin, &external, finish, &observed, &stop)
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => panic!("interactive fixture accept failed"),
                }
            }
            for worker in workers {
                worker.join().expect("interactive fixture response worker");
            }
        }));
        let external_stop = stop.clone();
        let external_observed = observed.clone();
        servers.push(thread::spawn(move || {
            while !external_stop.load(Ordering::Acquire) {
                match auxiliary.accept() {
                    Ok((mut stream, _)) => {
                        external_observed.auxiliary.fetch_add(1, Ordering::AcqRel);
                        let _ = stream.write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => panic!("auxiliary fixture accept failed"),
                }
            }
        }));
        Self {
            origin,
            auxiliary_origin,
            observed,
            stop,
            servers,
        }
    }

    fn attempt(&self) -> Arc<AuthAttempt> {
        AuthAttempt::for_test_session(
            &format!("{}/authorize", self.origin),
            &format!("{}/complete", self.origin),
        )
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.observed.slow_release.store(true, Ordering::Release);
        for server in self.servers.drain(..) {
            server.join().expect("interactive fixture server shutdown");
        }
    }
}

fn serve(
    mut stream: TcpStream,
    origin: &str,
    external: &str,
    finish: Finish,
    observed: &Observed,
    stop: &AtomicBool,
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut byte = [0];
    while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
        if stream.read(&mut byte).unwrap_or(0) != 1 {
            return;
        }
        request.push(byte[0]);
    }
    assert!(
        request.ends_with(b"\r\n\r\n"),
        "fixture request exceeded its limit"
    );
    let request = String::from_utf8(request).unwrap();
    let target = request.split_whitespace().nth(1).unwrap();
    let uri = reqwest::Url::parse(&format!("{origin}{target}")).unwrap();
    let mut cookie = "";
    let body = match uri.path() {
        "/authorize" => {
            observed.initial_cookies.lock().unwrap().push(request.lines().any(|line| line.to_ascii_lowercase().starts_with("cookie:")));
            let state = uri.query_pairs().find(|(name, _)| name == "state").unwrap().1.into_owned();
            cookie = "Set-Cookie: Session_id=synthetic-interactive-session; Path=/; HttpOnly; SameSite=Lax\r\n";
            page(origin, external, &state, finish)
        }
        "/child" => r#"<!doctype html><html><body>Local child<script>location.hash='synthetic-child-fragment';fetch('/observe/child',{cache:'no-store'});</script></body></html>"#.to_owned(),
        "/observe/input" => { observed.input.fetch_add(1, Ordering::AcqRel); "ok".to_owned() }
        "/observe/qr" => { observed.qr.fetch_add(1, Ordering::AcqRel); "ok".to_owned() }
        "/observe/child" => { observed.child.fetch_add(1, Ordering::AcqRel); "ok".to_owned() }
        "/complete" => "<!doctype html><html><body>Local return fixture</body></html>".to_owned(),
        "/slow" => {
            observed.slow_started.store(true, Ordering::Release);
            let deadline = Instant::now() + Duration::from_secs(3);
            while !observed.slow_release.load(Ordering::Acquire) && !stop.load(Ordering::Acquire) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            observed.slow_finished.store(true, Ordering::Release);
            "<!doctype html><html><body>Late local response</body></html>".to_owned()
        }
        _ => "<!doctype html><html><body>Local empty fixture</body></html>".to_owned(),
    };
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\n{cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn page(origin: &str, external: &str, state: &str, finish: Finish) -> String {
    let mut good = reqwest::Url::parse(&format!("{origin}/complete")).unwrap();
    good.set_fragment(Some(&format!("state={state}&access_token={UNUSED_TOKEN}")));
    let mut final_return = good.clone();
    match finish {
        Finish::Valid => {}
        Finish::MissingState => {
            final_return.set_fragment(Some(&format!("access_token={UNUSED_TOKEN}")))
        }
        Finish::WrongState => final_return.set_fragment(Some(&format!(
            "state=synthetic-wrong-state&access_token={UNUSED_TOKEN}"
        ))),
    }
    const HTML: &str = r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Interactive local fixture</title>
<style>body{margin:20px;font:16px sans-serif}input,button{position:fixed;left:24px;width:280px;height:32px;box-sizing:border-box}input{top:64px}#qr{top:120px}#finish{top:176px}#external{top:232px}#blank{top:288px}iframe{position:relative;top:330px;width:100px;height:40px}</style></head>
<body><p>Interactive HTTP fixture; no Yandex account or real QR data.</p>
<input aria-label="Synthetic input" autocomplete="off" autofocus oninput="if(this.value.length>=5&&!window.inputDone){window.inputDone=true;inputStep()}">
<button id="qr" onclick="qrStep()">Synthetic QR-like action</button>
<button id="finish" onclick="location.href=finalReturn">Complete local return</button>
<button id="external" onclick="location.href=externalTarget">Attempt external main page</button>
<button id="blank" onclick="location.href='about:blank'">Attempt blank main page</button>
<script>
const externalTarget=__EXTERNAL__;
const goodReturn=__GOOD_RETURN__;
const finalReturn=__FINAL_RETURN__;
function child(source,name){const frame=document.createElement('iframe');if(name)frame.name=name;frame.src=source;document.body.appendChild(frame)}
function inputStep(){location.hash='synthetic-phone-step';child('about:blank','empty-child');child('/child#synthetic-child-start','');child(externalTarget,'blocked-child');fetch('/observe/input',{cache:'no-store'})}
function qrStep(){location.hash='synthetic-qr-step';child(goodReturn,'');child('/child#synthetic-qr-child','named-child');fetch('/observe/qr',{cache:'no-store'})}
</script></body></html>"#;
    HTML.replace(
        "__EXTERNAL__",
        &serde_json::to_string(&format!(
            "{external}/auxiliary?qr=synthetic-qr-data&phone=synthetic-phone-data"
        ))
        .unwrap(),
    )
    .replace(
        "__GOOD_RETURN__",
        &serde_json::to_string(good.as_str()).unwrap(),
    )
    .replace(
        "__FINAL_RETURN__",
        &serde_json::to_string(final_return.as_str()).unwrap(),
    )
}

struct Page {
    attempt: Arc<AuthAttempt>,
    browser: AuthWebView,
    window: gtk::Window,
    errors: Rc<RefCell<Vec<AuthError>>>,
    sessions: Rc<Cell<usize>>,
}

impl Page {
    fn open(fixture: &Fixture) -> Self {
        let attempt = fixture.attempt();
        let errors = Rc::new(RefCell::new(Vec::new()));
        let error_reports = errors.clone();
        let sessions = Rc::new(Cell::new(0));
        let completed_sessions = sessions.clone();
        let loaded = Rc::new(Cell::new(false));
        let page_loaded = loaded.clone();
        let browser = AuthWebView::new(
            attempt.clone(),
            Rc::new(|_| panic!("session fixture returned a code")),
            Rc::new(move |cookies| {
                assert_eq!(format!("{cookies:?}"), "SessionCookies([redacted])");
                completed_sessions.set(completed_sessions.get() + 1);
            }),
            Rc::new(move |error| error_reports.borrow_mut().push(error)),
            Rc::new(move |loading| page_loaded.set(!loading)),
            Rc::new(|_| {}),
        );
        let window = gtk::Window::builder()
            .title(TITLE)
            .default_width(900)
            .default_height(650)
            .child(browser.widget())
            .build();
        window.present();
        browser.load(&attempt.authorization_url());
        pump_until(|| loaded.get() || !errors.borrow().is_empty());
        assert!(
            errors.borrow().is_empty(),
            "local interactive page failed to load"
        );
        assert!(browser.widget().is_mapped());
        assert!(browser.widget().width() >= 400 && browser.widget().height() >= 350);
        let frames = Rc::new(Cell::new(0));
        let painted = frames.clone();
        browser.widget().add_tick_callback(move |_, _| {
            painted.set(painted.get() + 1);
            if painted.get() >= 3 {
                gtk::glib::ControlFlow::Break
            } else {
                gtk::glib::ControlFlow::Continue
            }
        });
        pump_until(|| frames.get() >= 3);
        Self {
            attempt,
            browser,
            window,
            errors,
            sessions,
        }
    }

    fn click(&self, x: i32, y: i32) {
        let output = Command::new("timeout")
            .args([
                "5",
                "xdotool",
                "search",
                "--onlyvisible",
                "--pid",
                &std::process::id().to_string(),
                "--name",
                &format!("^{TITLE}$"),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "interactive fixture window was not found"
        );
        let id = String::from_utf8(output.stdout).unwrap();
        let id = id.lines().next().unwrap();
        assert!(id.bytes().all(|byte| byte.is_ascii_digit()));
        assert!(
            Command::new("timeout")
                .args(["5", "xdotool", "windowactivate", "--sync", id])
                .status()
                .unwrap()
                .success()
        );
        let bounds = self
            .browser
            .widget()
            .compute_bounds(&self.window)
            .expect("interactive WebKit allocation");
        let scale = self.browser.widget().scale_factor();
        let px = ((bounds.x() as i32 + x) * scale).to_string();
        let py = ((bounds.y() as i32 + y) * scale).to_string();
        assert!(
            Command::new("xdotool")
                .args(["mousemove", "--window", id, &px, &py, "click", "1"])
                .status()
                .unwrap()
                .success()
        );
    }

    fn input(&self, fixture: &Fixture) -> SafeNavigationDiagnostic {
        self.click(100, 80);
        assert!(
            Command::new("xdotool")
                .args(["type", "--clearmodifiers", "--delay", "20", TYPED])
                .status()
                .unwrap()
                .success()
        );
        pump_until(|| {
            fixture.observed.input.load(Ordering::Acquire) >= 1
                && fixture.observed.child.load(Ordering::Acquire) >= 1
                && self.browser.inner.first_rejection.borrow().is_some()
                || !self.errors.borrow().is_empty()
        });
        self.assert_active();
        let uri = self.browser.widget().uri().unwrap();
        let uri = reqwest::Url::parse(uri.as_str()).unwrap();
        assert!(
            uri.fragment() == Some("synthetic-phone-step"),
            "main document lost the ordinary input fragment"
        );
        assert_eq!(
            fixture.observed.auxiliary.load(Ordering::Acquire),
            0,
            "untrusted child was fetched"
        );
        let diagnostic = self
            .browser
            .inner
            .first_rejection
            .borrow()
            .expect("blocked child must record a safe first event");
        assert_eq!(diagnostic.reason(), RejectionReason::UnsupportedScheme);
        assert!(matches!(
            diagnostic.frame(),
            FrameContext::Unknown | FrameContext::Subframe
        ));
        self.assert_private(diagnostic, fixture);
        diagnostic
    }

    fn qr(&self, fixture: &Fixture, first: SafeNavigationDiagnostic) {
        let before = self.browser.inner.callback_candidates.get();
        self.click(100, 136);
        pump_until(|| {
            fixture.observed.qr.load(Ordering::Acquire) >= 1
                && self.browser.inner.callback_candidates.get() > before
                || !self.errors.borrow().is_empty()
        });
        self.assert_active();
        assert_eq!(
            self.sessions.get(),
            0,
            "nested return completed authorization"
        );
        assert_eq!(
            *self.browser.inner.first_rejection.borrow(),
            Some(first),
            "later child rejection replaced the first event"
        );
        let uri = self.browser.widget().uri().unwrap();
        assert!(
            reqwest::Url::parse(uri.as_str()).unwrap().fragment() == Some("synthetic-qr-step"),
            "main document lost the ordinary QR-like fragment"
        );
        assert_eq!(fixture.observed.auxiliary.load(Ordering::Acquire), 0);
    }

    fn assert_active(&self) {
        assert!(
            self.errors.borrow().is_empty(),
            "auxiliary navigation destroyed the main session"
        );
        assert!(!self.browser.inner.closed.get());
        assert!(!self.attempt.is_cancelled());
        assert_eq!(self.sessions.get(), 0);
    }

    fn cancelled_load_event(&self) {
        // A synthetic native WebKit signal tests cancellation handling without
        // loading or logging the URI/error message supplied by the event.
        let uri = "https://passport.yandex.ru/synthetic-private-path?phone=synthetic-private-phone&state=synthetic-private-state#synthetic-private-qr";
        let error = gtk::glib::Error::new(
            webkit6::NetworkError::Cancelled,
            "synthetic-private-cancelled-load-message",
        );
        assert!(self.browser.widget().emit_by_name::<bool>(
            "load-failed",
            &[&webkit6::LoadEvent::Started, &uri, &error],
        ));
    }

    fn assert_private(&self, report: SafeNavigationDiagnostic, fixture: &Fixture) {
        let output = format!("{report} {report:?}");
        let authorization = reqwest::Url::parse(&self.attempt.authorization_url()).unwrap();
        let state = authorization
            .query_pairs()
            .find(|(name, _)| name == "state")
            .unwrap()
            .1
            .into_owned();
        for secret in [
            state.as_str(),
            TYPED,
            UNUSED_TOKEN,
            "/authorize",
            "/auxiliary",
            "/complete",
            "synthetic-phone-data",
            "synthetic-qr-data",
            "synthetic-interactive-session",
            "Session_id",
            fixture.auxiliary_origin.as_str(),
        ] {
            assert!(
                !output.contains(secret),
                "diagnostic exposed synthetic sensitive material"
            );
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        self.attempt.cancel();
        self.browser.shutdown();
        self.window.set_child(gtk::Widget::NONE);
        self.window.close();
    }
}

fn pump_until(mut predicate: impl FnMut() -> bool) {
    let context = gtk::glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "interactive WebKit condition timed out"
        );
        if context.pending() {
            context.iteration(false);
        } else {
            thread::sleep(Duration::from_millis(5));
        }
    }
}

fn settle_late_events() {
    let deadline = Instant::now() + Duration::from_millis(180);
    pump_until(|| Instant::now() >= deadline);
}

#[test]
fn native_webkit_cancelled_load_categories_are_distinct_from_transport_failure() {
    let cancelled = gtk::glib::Error::new(webkit6::NetworkError::Cancelled, "synthetic-private");
    let interrupted = gtk::glib::Error::new(
        webkit6::PolicyError::FrameLoadInterruptedByPolicyChange,
        "synthetic-private",
    );
    let failure = gtk::glib::Error::new(webkit6::NetworkError::Failed, "synthetic-private");
    assert!(cancelled_load(&cancelled));
    assert!(cancelled_load(&interrupted));
    assert!(!cancelled_load(&failure));
}

#[test]
#[ignore = "requires Xvfb/Openbox, xdotool and the enabled WebKit sandbox; run separately"]
fn webkit_auth_interactive_navigation_frames_and_first_rejection() {
    gtk::init().expect("GTK startup");
    for finish in [Finish::Valid, Finish::MissingState, Finish::WrongState] {
        let fixture = Fixture::new(finish);
        let page = Page::open(&fixture);
        let first = page.input(&fixture);
        page.cancelled_load_event();
        page.assert_active();
        assert_eq!(*page.browser.inner.first_rejection.borrow(), Some(first));
        page.qr(&fixture, first);
        page.click(100, 192);
        pump_until(|| page.sessions.get() != 0 || !page.errors.borrow().is_empty());
        settle_late_events();
        match finish {
            Finish::Valid => {
                assert_eq!(
                    page.sessions.get(),
                    1,
                    "main return did not complete exactly once"
                );
                assert!(page.errors.borrow().is_empty());
            }
            Finish::MissingState => {
                assert_eq!(*page.errors.borrow(), [AuthError::SessionStateMissing])
            }
            Finish::WrongState => {
                assert_eq!(*page.errors.borrow(), [AuthError::SessionStateMismatch])
            }
        }
        let primary = page.browser.inner.first_error.get();
        page.cancelled_load_event();
        assert_eq!(page.browser.inner.first_error.get(), primary);
        assert_eq!(
            *page.browser.inner.first_rejection.borrow(),
            Some(first),
            "callback/load cancellation overwrote the first event"
        );
        assert!(page.browser.inner.closed.get());
        assert_eq!(fixture.observed.auxiliary.load(Ordering::Acquire), 0);
    }

    // Reject an actual main-page navigation without ever fetching its target.
    let fixture = Fixture::new(Finish::Valid);
    let page = Page::open(&fixture);
    page.click(100, 248);
    pump_until(|| page.browser.inner.first_rejection.borrow().is_some());
    settle_late_events();
    let first = page.browser.inner.first_rejection.borrow().unwrap();
    assert_eq!(first.reason(), RejectionReason::UnsupportedScheme);
    assert!(matches!(
        first.event(),
        NavigationEvent::PolicyNavigation | NavigationEvent::MainUri
    ));
    assert_eq!(fixture.observed.auxiliary.load(Ordering::Acquire), 0);
    assert_eq!(page.sessions.get(), 0);
    assert!(
        page.errors
            .borrow()
            .iter()
            .all(|error| *error == AuthError::UntrustedNavigation),
        "cancelled load replaced the primary navigation error"
    );
    page.assert_private(first, &fixture);
    drop(page);

    // about:blank is useful for children, not a trusted main document.
    let page = Page::open(&fixture);
    page.click(100, 304);
    pump_until(|| !page.errors.borrow().is_empty());
    settle_late_events();
    assert_eq!(*page.errors.borrow(), [AuthError::UntrustedNavigation]);
    assert_eq!(page.sessions.get(), 0);
    drop(page);

    // A cancelled pending request must not revive the view or consume a retry.
    let page = Page::open(&fixture);
    let first = page.input(&fixture);
    page.browser
        .widget()
        .load_uri(&format!("{}/slow", fixture.origin));
    pump_until(|| fixture.observed.slow_started.load(Ordering::Acquire));
    page.attempt.cancel();
    page.browser.shutdown();
    fixture.observed.slow_release.store(true, Ordering::Release);
    pump_until(|| fixture.observed.slow_finished.load(Ordering::Acquire));
    settle_late_events();
    assert!(page.errors.borrow().is_empty());
    assert_eq!(page.sessions.get(), 0);
    assert_eq!(*page.browser.inner.first_rejection.borrow(), Some(first));
    drop(page);
    let retry = Page::open(&fixture);
    retry.click(100, 192);
    pump_until(|| retry.sessions.get() != 0 || !retry.errors.borrow().is_empty());
    assert_eq!(retry.sessions.get(), 1);
    assert!(retry.errors.borrow().is_empty());
    assert!(
        fixture
            .observed
            .initial_cookies
            .lock()
            .unwrap()
            .iter()
            .all(|present| !present),
        "retry imported a previous browser session"
    );
}
