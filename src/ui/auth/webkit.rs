// SPDX-License-Identifier: GPL-3.0-or-later

//! Short-lived authorization browser. Only this attempt's OAuth-endpoint cookies
//! may cross the session boundary. NavigationAction has no reliable frame flag:
//! rejecting an auxiliary navigation must not destroy the main login document.
//! OAuth completion is consumed only from the actual WebView main URI.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use std::io::Write as _;

use webkit6::prelude::*;

use crate::auth::{
    AuthAttempt, AuthError, AuthorizationCode, FrameContext, NavigationDecision, NavigationEvent,
    RejectionReason, SafeNavigationDiagnostic, SessionCookies,
};

pub(in crate::ui) struct AuthWebView {
    inner: Rc<Browser>,
}

struct Browser {
    view: webkit6::WebView,
    session: webkit6::NetworkSession,
    attempt: Arc<AuthAttempt>,
    closed: Cell<bool>,
    collecting: Cell<bool>,
    first_error: Cell<Option<AuthError>>,
    first_rejection: RefCell<Option<SafeNavigationDiagnostic>>,
    #[cfg(test)]
    callback_candidates: Cell<u32>,
    cookie_cancellable: gtk::gio::Cancellable,
    on_redirect: Rc<dyn Fn(AuthorizationCode)>,
    on_session: Rc<dyn Fn(SessionCookies)>,
    on_error: Rc<dyn Fn(AuthError)>,
    on_loading: Rc<dyn Fn(bool)>,
    on_origin: Rc<dyn Fn(&str)>,
}

impl AuthWebView {
    pub(in crate::ui) fn new(
        attempt: Arc<AuthAttempt>,
        on_redirect: Rc<dyn Fn(AuthorizationCode)>,
        on_session: Rc<dyn Fn(SessionCookies)>,
        on_error: Rc<dyn Fn(AuthError)>,
        on_loading: Rc<dyn Fn(bool)>,
        on_origin: Rc<dyn Fn(&str)>,
    ) -> Self {
        let session = webkit6::NetworkSession::new_ephemeral();
        session.set_persistent_credential_storage_enabled(false);
        session.set_tls_errors_policy(webkit6::TLSErrorsPolicy::Fail);
        let settings = webkit6::Settings::new();
        settings.set_enable_developer_extras(false);
        settings.set_enable_write_console_messages_to_stdout(false);
        settings.set_javascript_can_access_clipboard(false);
        settings.set_javascript_can_open_windows_automatically(false);
        settings.set_enable_media(false);
        let view = webkit6::WebView::builder()
            .network_session(&session)
            .settings(&settings)
            .build();
        view.set_hexpand(true);
        view.set_vexpand(true);
        let inner = Rc::new(Browser {
            view,
            session,
            attempt,
            closed: Cell::new(false),
            collecting: Cell::new(false),
            first_error: Cell::new(None),
            first_rejection: RefCell::new(None),
            #[cfg(test)]
            callback_candidates: Cell::new(0),
            cookie_cancellable: gtk::gio::Cancellable::new(),
            on_redirect,
            on_session,
            on_error,
            on_loading,
            on_origin,
        });
        Self::connect(&inner);
        Self { inner }
    }

    pub(in crate::ui) fn widget(&self) -> &webkit6::WebView {
        &self.inner.view
    }

    pub(in crate::ui) fn load(&self, authorization_url: &str) {
        if !self.inner.live() {
            return;
        }
        match self.inner.attempt.navigation(authorization_url) {
            NavigationDecision::AllowTrusted => self.inner.view.load_uri(authorization_url),
            NavigationDecision::CodeReturn | NavigationDecision::SessionReturn => {
                self.inner.fail(AuthError::InvalidCallback)
            }
            NavigationDecision::Reject(error) => {
                self.inner.record_rejection(
                    NavigationEvent::MainUri,
                    self.inner.reason(authorization_url),
                    FrameContext::Main,
                    Some(authorization_url),
                );
                self.inner.fail(error);
            }
        }
    }

    /// Stop before detaching the widget from the dialog's owning container.
    pub(in crate::ui) fn shutdown(&self) {
        self.inner.stop();
    }

    fn connect(inner: &Rc<Browser>) {
        let weak = Rc::downgrade(inner);
        inner
            .view
            .connect_decide_policy(move |view, decision, kind| {
                let Some(browser) = weak.upgrade() else {
                    decision.ignore();
                    return true;
                };
                if !browser.live() {
                    decision.ignore();
                    return true;
                }
                match kind {
                    webkit6::PolicyDecisionType::NewWindowAction => {
                        browser.record_rejection(
                            NavigationEvent::PolicyNewWindow,
                            RejectionReason::Popup,
                            FrameContext::Unknown,
                            decision
                                .downcast_ref::<webkit6::NavigationPolicyDecision>()
                                .and_then(|navigation| navigation.navigation_action())
                                .and_then(|mut action| action.request())
                                .and_then(|request| request.uri())
                                .as_deref(),
                        );
                        decision.ignore();
                        true
                    }
                    webkit6::PolicyDecisionType::NavigationAction => {
                        let uri = decision
                            .downcast_ref::<webkit6::NavigationPolicyDecision>()
                            .and_then(|navigation| navigation.navigation_action())
                            .and_then(|mut action| action.request())
                            .and_then(|request| request.uri());
                        let Some(uri) = uri else {
                            browser.record_rejection(
                                NavigationEvent::PolicyNavigation,
                                RejectionReason::MissingAddress,
                                FrameContext::Unknown,
                                None,
                            );
                            decision.ignore();
                            return true;
                        };
                        // Empty child documents are normal for interactive login.
                        // This is not evidence that the target is a subframe. If
                        // one becomes the actual main URI, the main handler below
                        // rejects it before it can replace a trusted login page.
                        if service_document(uri.as_str())
                            && view
                                .uri()
                                .and_then(|current| {
                                    browser.attempt.display_origin(current.as_str())
                                })
                                .is_some()
                        {
                            decision.use_();
                            return true;
                        }
                        match browser.attempt.navigation(uri.as_str()) {
                            NavigationDecision::AllowTrusted => decision.use_(),
                            NavigationDecision::CodeReturn | NavigationDecision::SessionReturn => {
                                #[cfg(test)]
                                browser
                                    .callback_candidates
                                    .set(browser.callback_candidates.get() + 1);
                                // Exact expected callback candidates are not proof
                                // of top-frame navigation. In particular, never
                                // consume state or acquire cookies for an iframe.
                                decision.use_();
                            }
                            NavigationDecision::Reject(_) => {
                                browser.record_rejection(
                                    NavigationEvent::PolicyNavigation,
                                    browser.reason(uri.as_str()),
                                    FrameContext::Unknown,
                                    Some(uri.as_str()),
                                );
                                // This blocks the request even when it would be a
                                // top-level navigation. Unknown frame context must
                                // not turn an optional denied frame into logout.
                                decision.ignore();
                            }
                        }
                        true
                    }
                    webkit6::PolicyDecisionType::Response => {
                        let Some(policy) =
                            decision.downcast_ref::<webkit6::ResponsePolicyDecision>()
                        else {
                            browser.record_rejection(
                                NavigationEvent::PolicyResponse,
                                RejectionReason::MissingAddress,
                                FrameContext::Unknown,
                                None,
                            );
                            decision.ignore();
                            return true;
                        };
                        let main_resource = policy.is_main_frame_main_resource();
                        // FALSE is not proof of a subframe: WebKit also tests the
                        // requester's main-resource status in this compound API.
                        let context = if main_resource {
                            FrameContext::Main
                        } else {
                            FrameContext::Unknown
                        };
                        let Some(response) = policy.response() else {
                            browser.record_rejection(
                                NavigationEvent::PolicyResponse,
                                RejectionReason::MissingAddress,
                                context,
                                None,
                            );
                            if main_resource {
                                browser.fail(AuthError::BrowserLoad);
                            }
                            decision.ignore();
                            return true;
                        };
                        let uri = policy
                            .request()
                            .and_then(|request| request.uri())
                            .or_else(|| response.uri());
                        let Some(uri) = uri else {
                            browser.record_rejection(
                                NavigationEvent::PolicyResponse,
                                RejectionReason::MissingAddress,
                                context,
                                None,
                            );
                            if main_resource {
                                browser.fail(AuthError::BrowserLoad);
                            }
                            decision.ignore();
                            return true;
                        };
                        match browser.attempt.navigation(uri.as_str()) {
                            NavigationDecision::AllowTrusted => {}
                            NavigationDecision::CodeReturn | NavigationDecision::SessionReturn => {
                                if !main_resource {
                                    browser.record_rejection(
                                        NavigationEvent::PolicyResponse,
                                        RejectionReason::UnconfirmedMainDocument,
                                        context,
                                        Some(uri.as_str()),
                                    );
                                    decision.ignore();
                                    return true;
                                }
                                // Main-resource proof does not authorize copying a
                                // fragment from a previous unknown-frame candidate.
                                // Use only WebView's actual main URI. If its update
                                // is deferred, COMMITTED/notify::uri validates that
                                // actual URI before completing authentication.
                                Browser::handle_main_uri(&browser);
                                if !browser.live() {
                                    decision.ignore();
                                    return true;
                                }
                            }
                            NavigationDecision::Reject(error) => {
                                if !(service_document(uri.as_str()) && !main_resource) {
                                    browser.record_rejection(
                                        NavigationEvent::PolicyResponse,
                                        browser.reason(uri.as_str()),
                                        context,
                                        Some(uri.as_str()),
                                    );
                                    if main_resource {
                                        browser.fail(error);
                                    }
                                    decision.ignore();
                                    return true;
                                }
                            }
                        }
                        let attachment = response
                            .http_headers()
                            .and_then(|headers| headers.one("Content-Disposition"))
                            .is_some_and(|value| {
                                value.split(';').next().is_some_and(|kind| {
                                    kind.trim().eq_ignore_ascii_case("attachment")
                                })
                            });
                        if attachment
                            || !policy.is_mime_type_supported()
                            || (main_resource && response.status_code() >= 400)
                        {
                            browser.record_rejection(
                                NavigationEvent::PolicyResponse,
                                if attachment {
                                    RejectionReason::Download
                                } else {
                                    RejectionReason::LoadFailure
                                },
                                context,
                                Some(uri.as_str()),
                            );
                            if main_resource {
                                browser.fail(AuthError::BrowserLoad);
                            }
                            decision.ignore();
                        } else {
                            decision.use_();
                        }
                        true
                    }
                    _ => false,
                }
            });

        let weak = Rc::downgrade(inner);
        inner.view.connect_uri_notify(move |_| {
            if let Some(browser) = weak.upgrade() {
                Browser::handle_main_uri(&browser);
            }
        });

        let weak = Rc::downgrade(inner);
        inner.view.connect_load_changed(move |_, event| {
            let Some(browser) = weak.upgrade() else {
                return;
            };
            if !browser.live() {
                return;
            }
            Browser::handle_main_uri(&browser);
            if !browser.live() {
                return;
            }
            match event {
                webkit6::LoadEvent::Started => (browser.on_loading)(true),
                webkit6::LoadEvent::Finished => (browser.on_loading)(false),
                _ => {}
            }
        });

        let weak = Rc::downgrade(inner);
        inner.view.connect_load_failed(move |_, _, uri, error| {
            if let Some(browser) = weak.upgrade()
                && browser.live()
            {
                // WebKit's NavigationClient emits load-failed only for its
                // main frame. Policy ignores, superseding requests and our
                // stop_loading produce benign cancellation errors; they are
                // not authentication failures and must not replace the first
                // navigation rejection with a generic BrowserLoad error.
                if cancelled_load(error) {
                    return true;
                }
                browser.record_rejection(
                    NavigationEvent::LoadFailed,
                    RejectionReason::LoadFailure,
                    FrameContext::Main,
                    Some(uri),
                );
                browser.fail(AuthError::BrowserLoad);
            }
            // Suppress WebKit's default URI-bearing error page.
            true
        });
        let weak = Rc::downgrade(inner);
        inner
            .view
            .connect_load_failed_with_tls_errors(move |_, uri, _, _| {
                if let Some(browser) = weak.upgrade()
                    && browser.live()
                {
                    browser.record_rejection(
                        NavigationEvent::Tls,
                        RejectionReason::TlsFailure,
                        FrameContext::Main,
                        Some(uri),
                    );
                    browser.fail(AuthError::BrowserTls);
                }
                true
            });
        let weak = Rc::downgrade(inner);
        inner.view.connect_web_process_terminated(move |_, _| {
            if let Some(browser) = weak.upgrade()
                && browser.live()
            {
                browser.record_rejection(
                    NavigationEvent::WebProcess,
                    RejectionReason::ProcessTerminated,
                    FrameContext::Unknown,
                    None,
                );
                browser.fail(AuthError::BrowserProcess);
            }
        });
        inner.view.connect_permission_request(|_, request| {
            request.deny();
            true
        });
        inner.view.connect_authenticate(|_, request| {
            request.cancel();
            true
        });
        inner
            .session
            .connect_download_started(|_, download| download.cancel());
        let weak = Rc::downgrade(inner);
        inner.view.connect_close(move |_| {
            if let Some(browser) = weak.upgrade() {
                browser.fail(AuthError::Cancelled);
            }
        });
    }
}

fn service_document(raw: &str) -> bool {
    matches!(raw, "about:blank" | "about:srcdoc")
}

fn cancelled_load(error: &gtk::glib::Error) -> bool {
    error.matches(webkit6::NetworkError::Cancelled)
        || error.matches(webkit6::PolicyError::FrameLoadInterruptedByPolicyChange)
}

impl Browser {
    fn live(&self) -> bool {
        !self.closed.get() && !self.collecting.get() && !self.attempt.is_cancelled()
    }

    fn reason(&self, raw: &str) -> RejectionReason {
        self.attempt
            .navigation_rejection_reason(raw)
            .unwrap_or(RejectionReason::InvalidCallback)
    }

    fn record_rejection(
        &self,
        event: NavigationEvent,
        reason: RejectionReason,
        context: FrameContext,
        raw: Option<&str>,
    ) {
        let diagnostic = SafeNavigationDiagnostic::new(event, reason, context, raw);
        {
            let mut first = self.first_rejection.borrow_mut();
            if first.is_some() {
                return;
            }
            *first = Some(diagnostic);
        }
        // Desktop launchers can close stderr; reporting must never panic inside
        // a GTK callback or replace the latched navigation event with I/O failure.
        let _ = writeln!(
            std::io::stderr().lock(),
            "Karin auth navigation: {diagnostic}"
        );
    }

    fn handle_main_uri(browser: &Rc<Self>) {
        if !browser.live() {
            return;
        }
        let Some(uri) = browser.view.uri() else {
            return;
        };
        match browser.attempt.navigation(uri.as_str()) {
            NavigationDecision::AllowTrusted => {
                let origin = browser.attempt.display_origin(uri.as_str());
                (browser.on_origin)(origin.as_deref().unwrap_or(""));
            }
            NavigationDecision::CodeReturn => {
                (browser.on_origin)("");
                match browser.attempt.complete_code_return(uri.as_str()) {
                    Ok(code) => {
                        browser.stop();
                        (browser.on_redirect)(code);
                    }
                    Err(error) => {
                        if !matches!(error, AuthError::Denied | AuthError::Rejected) {
                            browser.record_rejection(
                                NavigationEvent::MainUri,
                                RejectionReason::InvalidCallback,
                                FrameContext::Main,
                                Some(uri.as_str()),
                            );
                        }
                        browser.fail(error);
                    }
                }
            }
            NavigationDecision::SessionReturn => {
                (browser.on_origin)("");
                Browser::collect_session(browser, uri.as_str());
            }
            NavigationDecision::Reject(error) => {
                browser.record_rejection(
                    NavigationEvent::MainUri,
                    browser.reason(uri.as_str()),
                    FrameContext::Main,
                    Some(uri.as_str()),
                );
                browser.fail(error);
            }
        }
    }

    fn collect_session(browser: &Rc<Self>, returned: &str) {
        if !browser.live() {
            return;
        }
        browser.collecting.set(true);
        if let Err(error) = browser.attempt.complete_session_return(returned) {
            if !matches!(error, AuthError::Denied | AuthError::Rejected) {
                browser.record_rejection(
                    NavigationEvent::MainUri,
                    RejectionReason::InvalidCallback,
                    FrameContext::Main,
                    Some(returned),
                );
            }
            browser.fail(error);
            return;
        }
        browser.view.stop_loading();
        browser.view.set_visible(false);
        (browser.on_origin)("");
        let Some(manager) = browser.session.cookie_manager() else {
            browser.fail(AuthError::BrowserSessionRead);
            return;
        };
        let weak = Rc::downgrade(browser);
        manager.cookies(
            browser.attempt.session_cookie_uri(),
            Some(&browser.cookie_cancellable),
            move |result| {
                let Some(browser) = weak.upgrade() else {
                    return;
                };
                if browser.closed.get() || browser.attempt.is_cancelled() {
                    return;
                }
                if browser.attempt.expired() {
                    browser.fail(AuthError::Expired);
                    return;
                }
                let result =
                    result
                        .map_err(|_| AuthError::BrowserSessionRead)
                        .and_then(|cookies| {
                            SessionCookies::from_pairs(cookies.into_iter().filter_map(
                                |mut cookie| {
                                    Some((cookie.name()?.to_string(), cookie.value()?.to_string()))
                                },
                            ))
                        });
                match result {
                    Ok(cookies) => {
                        browser.stop();
                        (browser.on_session)(cookies);
                    }
                    Err(error) => browser.fail(error),
                }
            },
        );
    }

    fn stop(&self) {
        if self.closed.replace(true) {
            return;
        }
        self.cookie_cancellable.cancel();
        self.view.stop_loading();
        self.view.set_visible(false);
        (self.on_loading)(false);
        (self.on_origin)("");
        if let Some(manager) = self.session.website_data_manager() {
            manager.clear(
                webkit6::WebsiteDataTypes::ALL,
                gtk::glib::TimeSpan::from_microseconds(0),
                gtk::gio::Cancellable::NONE,
                |_| {},
            );
        }
    }

    fn fail(&self, error: AuthError) {
        if self.closed.get() || self.attempt.is_cancelled() || self.first_error.get().is_some() {
            return;
        }
        // Latch before stop_loading can synchronously emit a cancellation.
        self.first_error.set(Some(error));
        self.stop();
        (self.on_error)(error);
    }
}

impl Drop for AuthWebView {
    fn drop(&mut self) {
        self.inner.stop();
    }
}

#[cfg(all(test, feature = "cassette-auth"))]
mod interactive_tests;

#[cfg(test)]
mod tests {
    use adw::prelude::AdwDialogExt;
    use gtk::gdk::prelude::TextureExtManual;
    use std::{
        cell::Cell,
        io::{Read, Write},
        net::TcpListener,
        rc::Rc,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    use super::*;
    use crate::auth::BrowserMode;

    #[test]
    #[ignore = "requires GTK display; run separately from other GTK test entry points"]
    fn webkit_auth_construction_and_shutdown_without_navigation() {
        gtk::init().expect("GTK startup");
        let attempt =
            AuthAttempt::for_test("http://127.0.0.1:9/authorize", BrowserMode::Embedded).unwrap();
        let callbacks = Rc::new(Cell::new(0));
        let redirected = callbacks.clone();
        let errored = callbacks.clone();
        let loading_calls = Rc::new(Cell::new(0));
        let loading = loading_calls.clone();
        let browser = AuthWebView::new(
            attempt.clone(),
            Rc::new(move |_| redirected.set(redirected.get() + 1)),
            Rc::new(|_| panic!("code fixture returned a session")),
            Rc::new(move |_| errored.set(errored.get() + 1)),
            Rc::new(move |_| loading.set(loading.get() + 1)),
            Rc::new(|_| {}),
        );
        assert!(browser.inner.session.is_ephemeral());
        assert!(
            !browser
                .inner
                .session
                .is_persistent_credential_storage_enabled()
        );
        assert!(browser.widget().uri().is_none());
        attempt.cancel();
        browser.shutdown();
        browser.shutdown();
        browser.load(&attempt.authorization_url());
        assert!(browser.inner.closed.get());
        assert!(browser.widget().uri().is_none());
        assert_eq!(callbacks.get(), 0);
        assert_eq!(loading_calls.get(), 1);
    }

    /// Run alone under a display and D-Bus session with the WebKit sandbox intact.
    #[test]
    #[ignore = "requires GTK display, D-Bus and working WebKit process sandbox"]
    fn webkit_auth_local_startup_cookie_isolation_and_cancel() {
        gtk::init().expect("GTK startup");
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback fixture");
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let cookies = Arc::new(Mutex::new(Vec::new()));
        let cancel_started = Arc::new(AtomicBool::new(false));
        let server_cancel_started = cancel_started.clone();
        let server_cookies = cookies.clone();
        let server_origin = origin.clone();
        let server = thread::spawn(move || {
            for _ in 0..5 {
                let deadline = Instant::now() + Duration::from_secs(10);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "fixture request timed out");
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => panic!("fixture accept failed"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
                    assert_eq!(stream.read(&mut byte).unwrap(), 1);
                    request.push(byte[0]);
                }
                let request = String::from_utf8(request).unwrap();
                let target = request.split_whitespace().nth(1).unwrap();
                let uri = reqwest::Url::parse(&format!("{server_origin}{target}")).unwrap();
                let had_cookie = request
                    .lines()
                    .any(|line| line.to_ascii_lowercase().starts_with("cookie:"));
                let mut next = match uri.path() {
                    "/authorize" => {
                        server_cookies.lock().unwrap().push(had_cookie);
                        let mut next = uri.clone();
                        next.set_path("/continue");
                        next
                    }
                    "/continue" => {
                        assert!(had_cookie, "fixture cookie missing within same session");
                        let callback = uri
                            .query_pairs()
                            .find(|(key, _)| key == "redirect_uri")
                            .unwrap()
                            .1
                            .into_owned();
                        reqwest::Url::parse(&callback).unwrap()
                    }
                    "/cancel" => {
                        server_cancel_started.store(true, Ordering::Release);
                        thread::sleep(Duration::from_millis(300));
                        let callback = uri
                            .query_pairs()
                            .find(|(key, _)| key == "redirect_uri")
                            .unwrap()
                            .1
                            .into_owned();
                        reqwest::Url::parse(&callback).unwrap()
                    }
                    _ => panic!("unexpected fixture route"),
                };
                if uri.path() != "/authorize" {
                    let state = uri
                        .query_pairs()
                        .find(|(key, _)| key == "state")
                        .unwrap()
                        .1
                        .into_owned();
                    next.query_pairs_mut()
                        .append_pair("state", &state)
                        .append_pair("code", "synthetic-code");
                }
                let set_cookie = if uri.path() == "/authorize" {
                    "Set-Cookie: karin_fixture=synthetic; Path=/; SameSite=Lax\r\n"
                } else {
                    ""
                };
                let response = if uri.path() == "/authorize" {
                    let next = next.as_str().replace('&', "&amp;");
                    let body = format!(
                        r#"<!doctype html><html><head><meta http-equiv="refresh" content="0;url={next}"></head><body><iframe src="about:blank"></iframe>synthetic authorization page</body></html>"#
                    );
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n{set_cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                } else {
                    format!(
                        "HTTP/1.1 302 Found\r\nLocation: {next}\r\n{set_cookie}Content-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });

        for _ in 0..2 {
            let attempt =
                AuthAttempt::for_test(&format!("{origin}/authorize"), BrowserMode::Embedded)
                    .unwrap();
            let main_loop = gtk::glib::MainLoop::new(None, false);
            let returned = Rc::new(Cell::new(false));
            let redirect_returned = returned.clone();
            let redirect_loop = main_loop.clone();
            let error_loop = main_loop.clone();
            let failed = Rc::new(Cell::new(false));
            let error_failed = failed.clone();
            let browser = AuthWebView::new(
                attempt.clone(),
                Rc::new(move |code| {
                    assert_eq!(code.expose(), "synthetic-code");
                    redirect_returned.set(true);
                    redirect_loop.quit();
                }),
                Rc::new(|_| panic!("code fixture returned a session")),
                Rc::new(move |_| {
                    error_failed.set(true);
                    error_loop.quit();
                }),
                Rc::new(|_| {}),
                Rc::new(|_| {}),
            );
            assert!(browser.inner.session.is_ephemeral());
            assert!(
                !browser
                    .inner
                    .session
                    .is_persistent_credential_storage_enabled()
            );
            let manager = browser.inner.session.website_data_manager().unwrap();
            assert!(manager.is_ephemeral());
            assert!(manager.base_data_directory().is_none());
            assert!(manager.base_cache_directory().is_none());
            let window = gtk::Window::builder().child(browser.widget()).build();
            window.present();
            let timeout_loop = main_loop.clone();
            let timed_out = Rc::new(Cell::new(false));
            let timeout_fired = timed_out.clone();
            let timeout = gtk::glib::timeout_add_local_once(Duration::from_secs(5), move || {
                timeout_fired.set(true);
                timeout_loop.quit()
            });
            browser.load(&attempt.authorization_url());
            main_loop.run();
            if !timed_out.get() {
                timeout.remove();
            }
            browser.shutdown();
            window.set_child(gtk::Widget::NONE);
            window.close();
            assert!(returned.get(), "synthetic redirect was not intercepted");
            assert!(!failed.get(), "browser reported a fixed error category");
        }

        let attempt =
            AuthAttempt::for_test(&format!("{origin}/cancel"), BrowserMode::Embedded).unwrap();
        let callbacks = Rc::new(Cell::new(0));
        let redirected = callbacks.clone();
        let errored = callbacks.clone();
        let browser = Rc::new(AuthWebView::new(
            attempt.clone(),
            Rc::new(move |_| redirected.set(redirected.get() + 1)),
            Rc::new(|_| panic!("code fixture returned a session")),
            Rc::new(move |_| errored.set(errored.get() + 1)),
            Rc::new(|_| {}),
            Rc::new(|_| {}),
        ));
        let window = gtk::Window::builder().child(browser.widget()).build();
        window.present();
        browser.load(&attempt.authorization_url());
        let main_loop = gtk::glib::MainLoop::new(None, false);
        let cancel_browser = browser.clone();
        let cancel_loop = main_loop.clone();
        let cancelled = Rc::new(Cell::new(false));
        let cancellation_done = cancelled.clone();
        let cancel_source = gtk::glib::timeout_add_local(Duration::from_millis(5), move || {
            if !cancel_started.load(Ordering::Acquire) {
                return gtk::glib::ControlFlow::Continue;
            }
            attempt.cancel();
            cancel_browser.shutdown();
            cancellation_done.set(true);
            let late_response_loop = cancel_loop.clone();
            gtk::glib::timeout_add_local_once(Duration::from_millis(500), move || {
                late_response_loop.quit()
            });
            gtk::glib::ControlFlow::Break
        });
        let timeout_loop = main_loop.clone();
        let timed_out = Rc::new(Cell::new(false));
        let timeout_fired = timed_out.clone();
        let timeout = gtk::glib::timeout_add_local_once(Duration::from_secs(5), move || {
            timeout_fired.set(true);
            timeout_loop.quit();
        });
        main_loop.run();
        if !timed_out.get() {
            timeout.remove();
        }
        if !cancelled.get() {
            cancel_source.remove();
        }
        window.set_child(gtk::Widget::NONE);
        window.close();
        assert_eq!(
            callbacks.get(),
            0,
            "cancelled browser delivered a late result"
        );
        assert!(browser.inner.closed.get());
        assert!(cancelled.get(), "pending fixture request was not cancelled");
        server.join().expect("loopback fixture worker");
        assert_eq!(
            *cookies.lock().unwrap(),
            [false, false],
            "attempts shared cookies"
        );
        #[cfg(feature = "cassette-auth")]
        session_grant_and_music_fixtures();
        active_dialog_geometry();
    }

    fn pump_until(mut ready: impl FnMut() -> bool) {
        let context = gtk::glib::MainContext::default();
        let deadline = Instant::now() + Duration::from_secs(8);
        while !ready() {
            assert!(Instant::now() < deadline, "WebKit dialog did not settle");
            if context.pending() {
                context.iteration(false);
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
    }

    /// The actual browser is mapped and loads real local HTML inside Adw.Dialog.
    /// Requires a window manager for genuine maximization, as in CI's Xvfb setup.
    fn active_dialog_geometry() {
        adw::init().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/page", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(90);
            while !server_stop.load(Ordering::Acquire) {
                assert!(Instant::now() < deadline, "dialog fixture server timed out");
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(_) => panic!("dialog fixture accept failed"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
                    if stream.read(&mut byte).unwrap_or(0) != 1 {
                        break;
                    }
                    request.push(byte[0]);
                }
                let body = "<!doctype html><html lang=\"ru\"><head><meta charset=\"utf-8\"><title>Local fixture</title></head><body><h1>Локальная HTTP-фикстура</h1><p>Это проверка настоящего WebKit внутри нативного окна Karin. Здесь нет аккаунта Яндекса и сетевой авторизации.</p><button>Проверка фокуса страницы</button><iframe src=\"about:blank\" title=\"Empty child frame\"></iframe></body></html>";
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        for (theme, scheme) in [
            ("light", adw::ColorScheme::ForceLight),
            ("dark", adw::ColorScheme::ForceDark),
        ] {
            adw::StyleManager::default().set_color_scheme(scheme);
            for (size, width, height) in [
                ("compact", 420, 720),
                ("medium", 900, 650),
                ("wide", 1440, 900),
                ("maximized", 900, 650),
            ] {
                let attempt = AuthAttempt::for_test(&endpoint, BrowserMode::Embedded).unwrap();
                let dialog = super::super::LoginDialog::new();
                dialog.dialog.set_title("Локальная WebKit-фикстура");
                let loaded = Rc::new(Cell::new(false));
                let page_loaded = loaded.clone();
                let failed = Rc::new(Cell::new(None));
                let browser_failed = failed.clone();
                let loading_dialog = dialog.clone();
                let origin_dialog = dialog.clone();
                let browser = AuthWebView::new(
                    attempt.clone(),
                    Rc::new(|_| panic!("static page returned an authorization code")),
                    Rc::new(|_| panic!("static page returned a browser session")),
                    Rc::new(move |error| browser_failed.set(Some(error))),
                    Rc::new(move |loading| {
                        loading_dialog.browser_loading(loading);
                        if !loading {
                            page_loaded.set(true);
                        }
                    }),
                    Rc::new(move |origin| origin_dialog.set_origin(origin)),
                );
                let view = browser.widget().clone();
                let window = adw::Window::builder()
                    .title("Karin WebKit synthetic fixture")
                    .default_width(width)
                    .default_height(height)
                    .content(&gtk::Box::new(gtk::Orientation::Vertical, 0))
                    .build();
                window.present();
                if size == "maximized" {
                    window.maximize();
                }
                dialog.set_webview(browser);
                dialog.dialog.present(Some(&window));
                view.load_uri(&attempt.authorization_url());
                // A cached WebKit process can finish loading before Adwaita's
                // opening animation has placed the bottom bar inside the sheet.
                // Wait for the actual geometry, retaining every final bound.
                let context = gtk::glib::MainContext::default();
                let deadline = Instant::now() + Duration::from_secs(8);
                let mut inside_since = None;
                loop {
                    if failed.get().is_some() {
                        break;
                    }
                    let button = dialog.cancel.compute_bounds(&dialog.dialog);
                    let inside = loaded.get()
                        && dialog.presentation_ready()
                        && view.width() > 0
                        && view.height() >= 360
                        && dialog.cancel.is_mapped()
                        && (size != "maximized" || window.is_maximized())
                        && button.is_some_and(|button| {
                            button.x() >= 0.0
                                && button.y() >= 0.0
                                && button.x() + button.width() <= dialog.dialog.width() as f32 + 1.0
                                && button.y() + button.height()
                                    <= dialog.dialog.height() as f32 + 1.0
                        });
                    if inside {
                        let since = inside_since.get_or_insert_with(Instant::now);
                        if since.elapsed() >= Duration::from_millis(150) {
                            break;
                        }
                    } else {
                        inside_since = None;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "WebKit dialog geometry did not settle: theme={theme} size={size} host={}x{} dialog={}x{} webview={}x{} cancel={button:?} loaded={} ready={} mapped={} maximized={}",
                        window.width(),
                        window.height(),
                        dialog.dialog.width(),
                        dialog.dialog.height(),
                        view.width(),
                        view.height(),
                        loaded.get(),
                        dialog.presentation_ready(),
                        dialog.cancel.is_mapped(),
                        window.is_maximized()
                    );
                    if context.pending() {
                        context.iteration(false);
                    } else {
                        thread::sleep(Duration::from_millis(5));
                    }
                }
                assert!(
                    failed.get().is_none(),
                    "local page failed in the native dialog"
                );
                assert!(
                    !dialog.spinner.is_spinning() && !dialog.spinner.is_visible(),
                    "page loading spinner remained active: {theme}/{size}"
                );
                assert!(dialog.cancel.is_mapped());
                let button = dialog
                    .cancel
                    .compute_bounds(&dialog.dialog)
                    .expect("cancel button allocation");
                assert!(
                    button.x() >= 0.0 && button.y() >= 0.0,
                    "cancel before dialog origin: {theme}/{size} {button:?}"
                );
                assert!(
                    button.x() + button.width() <= dialog.dialog.width() as f32 + 1.0,
                    "cancel outside dialog width: {theme}/{size} {button:?} dialog_width={}",
                    dialog.dialog.width()
                );
                assert!(
                    button.y() + button.height() <= dialog.dialog.height() as f32 + 1.0,
                    "cancel scrolled outside the dialog: {theme}/{size} {button:?} dialog_height={}",
                    dialog.dialog.height()
                );
                assert!(view.grab_focus(), "WebKit cannot receive keyboard focus");
                assert!(
                    dialog.cancel.grab_focus() && dialog.cancel.has_focus(),
                    "cancel cannot receive keyboard focus"
                );
                let origin = dialog
                    .origin
                    .compute_bounds(&dialog.dialog)
                    .expect("active origin allocation");
                assert!(
                    dialog.origin.is_mapped(),
                    "active origin disappeared after page focus: {theme}/{size}"
                );
                assert!(
                    origin.x() >= 0.0
                        && origin.y() >= 0.0
                        && origin.x() + origin.width() <= dialog.dialog.width() as f32 + 1.0
                        && origin.y() + origin.height() <= dialog.dialog.height() as f32 + 1.0,
                    "active origin scrolled outside dialog chrome: {theme}/{size} {origin:?}"
                );
                // This pixel inspection is test-only and reads a synthetic page.
                // A completed load does not establish that the first WebKit
                // frame has painted. Require actual nonblank rendered content,
                // without JavaScript, DOM access or arbitrary timing guesses.
                let rendered = Rc::new(RefCell::new(None));
                let rendered_page = rendered.clone();
                let rendered_view = view.clone();
                gtk::glib::spawn_future_local(async move {
                    for _ in 0..20 {
                        if let Ok(texture) = rendered_view
                            .snapshot_future(
                                webkit6::SnapshotRegion::Visible,
                                webkit6::SnapshotOptions::NONE,
                            )
                            .await
                        {
                            let stride = texture.width() as usize * 4;
                            let length = stride.checked_mul(texture.height() as usize).unwrap();
                            assert!(
                                length > 0 && length <= 16 * 1024 * 1024,
                                "unexpected local-page snapshot dimensions"
                            );
                            let mut pixels = vec![0; length];
                            texture.download(&mut pixels, stride);
                            let first = &pixels[..4];
                            if pixels
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .filter(|pixel| pixel.as_slice() != first)
                                .take(129)
                                .count()
                                > 128
                            {
                                *rendered_page.borrow_mut() = Some(Some(texture));
                                return;
                            }
                        }
                        gtk::glib::timeout_future(Duration::from_millis(50)).await;
                    }
                    *rendered_page.borrow_mut() = Some(None);
                });
                pump_until(|| rendered.borrow().is_some());
                let texture = rendered
                    .borrow_mut()
                    .take()
                    .flatten()
                    .expect("local WebKit page remained blank");
                let frames = Rc::new(Cell::new(0));
                let painted_frames = frames.clone();
                view.add_tick_callback(move |_, _| {
                    painted_frames.set(painted_frames.get() + 1);
                    if painted_frames.get() >= 3 {
                        gtk::glib::ControlFlow::Break
                    } else {
                        gtk::glib::ControlFlow::Continue
                    }
                });
                view.queue_draw();
                pump_until(|| frames.get() >= 3);
                if let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    let result = std::process::Command::new("xdotool")
                        .args([
                            "search",
                            "--onlyvisible",
                            "--name",
                            "^Karin WebKit synthetic fixture$",
                        ])
                        .output()
                        .unwrap();
                    assert!(result.status.success());
                    let id = String::from_utf8(result.stdout).unwrap();
                    let id = id.lines().next().unwrap();
                    assert!(id.bytes().all(|byte| byte.is_ascii_digit()));
                    texture
                        .save_to_png(directory.join(format!("auth-webkit-page-{size}-{theme}.png")))
                        .expect("save synthetic WebKit snapshot");
                    assert!(
                        std::process::Command::new("import")
                            .args(["-screen", "-window", id])
                            .arg(directory.join(format!("auth-webkit-{size}-{theme}.png")))
                            .status()
                            .unwrap()
                            .success()
                    );
                }
                attempt.cancel();
                dialog.request_close();
                pump_until(|| dialog.dialog.parent().is_none());
                window.close();
            }
        }
        stop.store(true, Ordering::Release);
        server.join().unwrap();
    }

    /// Real WebKit cookies, the real session-grant adapter and the existing
    /// MusicService use a numeric-loopback server only. This proves plumbing,
    /// never real service authorization or account access.
    #[cfg(feature = "cassette-auth")]
    fn session_grant_and_music_fixtures() {
        use crate::{
            auth::{AuthProvider, cassette::CassetteProvider},
            music::{MusicService, SearchKind},
        };
        use std::sync::atomic::AtomicUsize;

        for scenario in [
            "success",
            "missing-state",
            "wrong-state",
            "missing-cookie",
            "cancel",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let stop = Arc::new(AtomicBool::new(false));
            let server_stop = stop.clone();
            let started = Arc::new(AtomicBool::new(false));
            let server_started = started.clone();
            let token_posts = Arc::new(AtomicUsize::new(0));
            let server_posts = token_posts.clone();
            let metadata = Arc::new(AtomicUsize::new(0));
            let server_metadata = metadata.clone();
            let track_batches = Arc::new(AtomicUsize::new(0));
            let server_track_batches = track_batches.clone();
            let server_origin = origin.clone();
            let server = thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(12);
                while !server_stop.load(Ordering::Acquire) {
                    assert!(Instant::now() < deadline, "session fixture did not finish");
                    let mut stream = match listener.accept() {
                        Ok((stream, _)) => stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                            continue;
                        }
                        Err(_) => panic!("session fixture accept failed"),
                    };
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut byte = [0];
                    while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
                        if stream.read(&mut byte).unwrap_or(0) != 1 {
                            break;
                        }
                        request.push(byte[0]);
                    }
                    if !request.ends_with(b"\r\n\r\n") {
                        continue;
                    }
                    let headers = String::from_utf8(request).unwrap();
                    let target = headers.split_whitespace().nth(1).unwrap();
                    let url = reqwest::Url::parse(&format!("{server_origin}{target}")).unwrap();
                    let body = match url.path() {
                        "/authorize" => {
                            assert!(
                                !headers
                                    .lines()
                                    .any(|line| line.to_ascii_lowercase().starts_with("cookie:")),
                                "fresh view imported another session"
                            );
                            server_started.store(true, Ordering::Release);
                            if scenario == "cancel" {
                                thread::sleep(Duration::from_millis(250));
                            }
                            let state = url
                                .query_pairs()
                                .find(|(key, _)| key == "state")
                                .unwrap()
                                .1
                                .into_owned();
                            let fragment = match scenario {
                                "missing-state" => "access_token=synthetic-unused-implicit-token".to_owned(),
                                "wrong-state" => "state=synthetic-wrong-state&access_token=synthetic-unused-implicit-token".to_owned(),
                                _ => format!("state={state}&access_token=synthetic-unused-implicit-token"),
                            };
                            let cookie = if scenario == "missing-cookie" {
                                ""
                            } else {
                                "Set-Cookie: Session_id=synthetic-browser-session; Path=/; HttpOnly; SameSite=Lax\r\n"
                            };
                            let response = format!(
                                "HTTP/1.1 302 Found\r\nLocation: {server_origin}/complete#{fragment}\r\n{cookie}Content-Length: 0\r\nConnection: close\r\n\r\n"
                            );
                            let _ = stream.write_all(response.as_bytes());
                            continue;
                        }
                        "/complete" => "<html><body>synthetic return</body></html>".to_owned(),
                        "/token" => {
                            assert!(headers.starts_with("POST /token "));
                            assert!(
                                headers.lines().any(|line| line.trim_end()
                                    == "cookie: Session_id=synthetic-browser-session"),
                                "session grant did not receive this view's cookie"
                            );
                            let length: usize = headers
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|value| value.trim().parse().unwrap())
                                })
                                .unwrap();
                            assert!(length < 4096);
                            let mut body = vec![0; length];
                            stream.read_exact(&mut body).unwrap();
                            let form = reqwest::Url::parse(&format!(
                                "http://localhost/?{}",
                                String::from_utf8(body).unwrap()
                            ))
                            .unwrap();
                            assert!(form.query_pairs().any(|(key, value)| key == "grant_type" && value == "sessionid"));
                            server_posts.fetch_add(1, Ordering::AcqRel);
                            r#"{"access_token":"synthetic-session-token","token_type":"bearer","expires_in":60}"#.to_owned()
                        }
                        "/account/status" => {
                            server_metadata.fetch_add(1, Ordering::AcqRel);
                            r#"{"result":{"account":{"uid":1234,"displayName":"Synthetic session account"}}}"#.to_owned()
                        }
                        "/users/1234/likes/tracks" => {
                            server_metadata.fetch_add(1, Ordering::AcqRel);
                            r#"{"result":{"library":{"tracks":[]}}}"#.to_owned()
                        }
                        "/users/1234/playlists/list" => {
                            server_metadata.fetch_add(1, Ordering::AcqRel);
                            r#"{"result":[]}"#.to_owned()
                        }
                        "/search" => {
                            assert!(headers.starts_with("GET /search?"));
                            server_metadata.fetch_add(1, Ordering::AcqRel);
                            // Search supplies identity, not a canonical Track.
                            // Only the subsequent bounded lookup supplies UI data.
                            r#"{"result":{"text":"Synthetic","tracks":{"total":1,"perPage":20,"page":0,"results":[{"id":42,"title":{"ignored":true},"artists":null,"albums":false,"available":[true],"explicit":{"ignored":true}}]}}}"#.to_owned()
                        }
                        "/tracks" => {
                            assert!(headers.starts_with("POST /tracks HTTP/1.1\r\n"));
                            assert!(url.query().is_none());
                            assert!(headers.lines().any(|line| line.trim_end()
                                == "content-type: application/x-www-form-urlencoded"));
                            let length: usize = headers
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|value| value.trim().parse().unwrap())
                                })
                                .unwrap();
                            assert!(length > 0 && length < 4096);
                            let mut body = vec![0; length];
                            stream.read_exact(&mut body).unwrap();
                            let form = reqwest::Url::parse(&format!(
                                "http://localhost/?{}",
                                String::from_utf8(body).unwrap()
                            ))
                            .unwrap();
                            let pairs = form.query_pairs().collect::<Vec<_>>();
                            assert_eq!(pairs.len(), 1);
                            assert_eq!(pairs[0].0, "track-ids");
                            assert_eq!(pairs[0].1, "42");
                            server_track_batches.fetch_add(1, Ordering::AcqRel);
                            server_metadata.fetch_add(1, Ordering::AcqRel);
                            r#"{"result":[{"id":42,"title":"Normalized synthetic session track","artists":[],"albums":[],"available":true}]}"#.to_owned()
                        }
                        _ => panic!("unexpected session fixture route"),
                    };
                    if !matches!(url.path(), "/complete" | "/token") {
                        assert!(headers.lines().any(|line| line.trim_end()
                            == "authorization: OAuth synthetic-session-token"));
                        assert!(
                            !headers
                                .lines()
                                .any(|line| line.to_ascii_lowercase().starts_with("cookie:")),
                            "Music received browser cookies"
                        );
                    }
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        if url.path() == "/complete" {
                            "text/html"
                        } else {
                            "application/json"
                        },
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            let attempt = AuthAttempt::for_test_session(
                &format!("{origin}/authorize"),
                &format!("{origin}/complete"),
            )
            .unwrap();
            let main_loop = gtk::glib::MainLoop::new(None, false);
            let success = Rc::new(Cell::new(false));
            let finished = success.clone();
            let session_loop = main_loop.clone();
            let error_loop = main_loop.clone();
            let error = Rc::new(Cell::new(None));
            let reported_error = error.clone();
            let session_attempt = attempt.clone();
            let token_endpoint = format!("{origin}/token");
            let music_origin = origin.clone();
            let browser = Rc::new(AuthWebView::new(
                attempt.clone(),
                Rc::new(|_| panic!("session flow returned an authorization code")),
                Rc::new(move |cookies| {
                    let provider = CassetteProvider::for_test(&token_endpoint);
                    let attempt = session_attempt.clone();
                    let origin = music_origin.clone();
                    let finished = finished.clone();
                    let main_loop = session_loop.clone();
                    gtk::glib::spawn_future_local(async move {
                        gtk::gio::spawn_blocking(move || {
                            let token = provider.exchange_session(&attempt, cookies).unwrap();
                            let service = MusicService::for_test(&format!("{origin}/"));
                            service.account(&token).unwrap();
                            assert!(service.liked_tracks(&token, 0).unwrap().items.is_empty());
                            assert!(service.playlists(&token, 0).unwrap().items.is_empty());
                            let search = service
                                .search(&token, "Synthetic", SearchKind::Tracks, 0)
                                .unwrap();
                            assert_eq!(search.tracks.len(), 1);
                            assert_eq!(search.tracks[0].id, "42");
                            assert_eq!(
                                search.tracks[0].title,
                                "Normalized synthetic session track"
                            );
                            assert!(search.tracks[0].available);
                            assert_eq!(search.omitted_tracks, 0);
                        })
                        .await
                        .unwrap();
                        finished.set(true);
                        main_loop.quit();
                    });
                }),
                Rc::new(move |failure| {
                    reported_error.set(Some(failure));
                    error_loop.quit();
                }),
                Rc::new(|_| {}),
                Rc::new(|_| {}),
            ));
            let window = gtk::Window::builder().child(browser.widget()).build();
            window.present();
            let timed_out = Rc::new(Cell::new(false));
            let timeout_fired = timed_out.clone();
            let timeout_loop = main_loop.clone();
            let timeout = gtk::glib::timeout_add_local_once(Duration::from_secs(8), move || {
                timeout_fired.set(true);
                timeout_loop.quit();
            });
            let cancel = if scenario == "cancel" {
                let cancelled_browser = browser.clone();
                let cancelled_attempt = attempt.clone();
                let cancelled_loop = main_loop.clone();
                Some(gtk::glib::timeout_add_local(
                    Duration::from_millis(5),
                    move || {
                        if !started.load(Ordering::Acquire) {
                            return gtk::glib::ControlFlow::Continue;
                        }
                        cancelled_attempt.cancel();
                        cancelled_browser.shutdown();
                        let cancelled_loop = cancelled_loop.clone();
                        gtk::glib::timeout_add_local_once(Duration::from_millis(400), move || {
                            cancelled_loop.quit()
                        });
                        gtk::glib::ControlFlow::Break
                    },
                ))
            } else {
                None
            };
            browser.load(&attempt.authorization_url());
            main_loop.run();
            if !timed_out.get() {
                timeout.remove();
            } else if let Some(cancel) = cancel {
                cancel.remove();
            }
            browser.shutdown();
            window.set_child(gtk::Widget::NONE);
            window.close();
            stop.store(true, Ordering::Release);
            server.join().unwrap();
            assert!(!timed_out.get(), "session scenario timed out");
            match scenario {
                "success" => {
                    assert!(
                        success.get(),
                        "session exchange or Music fixture did not finish"
                    );
                    assert!(error.get().is_none());
                    assert_eq!(token_posts.load(Ordering::Acquire), 1);
                    assert_eq!(metadata.load(Ordering::Acquire), 5);
                    assert_eq!(track_batches.load(Ordering::Acquire), 1);
                }
                "missing-state" => assert_eq!(error.get(), Some(AuthError::SessionStateMissing)),
                "wrong-state" => assert_eq!(error.get(), Some(AuthError::SessionStateMismatch)),
                "missing-cookie" => assert_eq!(error.get(), Some(AuthError::BrowserSessionMissing)),
                "cancel" => {
                    assert!(error.get().is_none());
                    assert!(!success.get());
                }
                _ => unreachable!(),
            }
            if scenario != "success" {
                assert_eq!(token_posts.load(Ordering::Acquire), 0);
                assert_eq!(metadata.load(Ordering::Acquire), 0);
                assert_eq!(track_batches.load(Ordering::Acquire), 0);
            }
        }
    }
}
