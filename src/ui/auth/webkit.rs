// SPDX-License-Identifier: GPL-3.0-or-later

//! A short-lived browser for an explicitly approved authorization provider.
//!
//! Only redirects cross into the authentication service. This component never
//! reads provider DOM, passwords, cookies, storage or JavaScript results.
//! Navigation policy has no reliable main-frame flag in WebKitGTK's UI API.
//! Exact callback URI/state validation therefore does not establish that its
//! navigation came from the top frame. A real embedded provider must resolve
//! this limitation before activation. Navigation policy also does not restrict
//! every subresource request made by the provider's page.

use std::{cell::Cell, rc::Rc, sync::Arc};

use webkit6::prelude::*;

use crate::auth::{AuthAttempt, AuthError, AuthorizationCode, NavigationDecision};

pub(in crate::ui) struct AuthWebView {
    inner: Rc<Browser>,
}

struct Browser {
    view: webkit6::WebView,
    session: webkit6::NetworkSession,
    attempt: Arc<AuthAttempt>,
    closed: Cell<bool>,
    on_redirect: Rc<dyn Fn(AuthorizationCode)>,
    on_error: Rc<dyn Fn(AuthError)>,
    on_loading: Rc<dyn Fn(bool)>,
    on_origin: Rc<dyn Fn(&str)>,
}

impl AuthWebView {
    pub(in crate::ui) fn new(
        attempt: Arc<AuthAttempt>,
        on_redirect: Rc<dyn Fn(AuthorizationCode)>,
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
        let inner = Rc::new(Browser {
            view,
            session,
            attempt,
            closed: Cell::new(false),
            on_redirect,
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
        if self.inner.closed.get() {
            return;
        }
        match self.inner.attempt.navigation(authorization_url) {
            NavigationDecision::AllowTrusted => self.inner.view.load_uri(authorization_url),
            NavigationDecision::Callback(_) => self.inner.fail(AuthError::InvalidCallback),
            NavigationDecision::Reject(error) => self.inner.fail(error),
        }
    }

    /// Stop before detaching the widget from the dialog's owning container.
    pub(in crate::ui) fn shutdown(&self) {
        self.inner.stop();
    }

    fn connect(inner: &Rc<Browser>) {
        let weak = Rc::downgrade(inner);
        inner.view.connect_decide_policy(move |_, decision, kind| {
            let Some(browser) = weak.upgrade() else {
                decision.ignore();
                return true;
            };
            if browser.closed.get() || browser.attempt.is_cancelled() {
                decision.ignore();
                return true;
            }
            match kind {
                webkit6::PolicyDecisionType::NewWindowAction => {
                    // Popups and external protocol handlers never escape this dialog.
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
                        decision.ignore();
                        browser.fail(AuthError::UntrustedNavigation);
                        return true;
                    };
                    match browser.attempt.navigation(uri.as_str()) {
                        NavigationDecision::AllowTrusted => decision.use_(),
                        NavigationDecision::Callback(code) => {
                            // Never send the code to the callback URL or render it.
                            decision.ignore();
                            browser.stop();
                            (browser.on_redirect)(code);
                        }
                        NavigationDecision::Reject(error) => {
                            decision.ignore();
                            browser.fail(error);
                        }
                    }
                    true
                }
                webkit6::PolicyDecisionType::Response => {
                    let Some(response_policy) =
                        decision.downcast_ref::<webkit6::ResponsePolicyDecision>()
                    else {
                        decision.ignore();
                        browser.fail(AuthError::BrowserLoad);
                        return true;
                    };
                    let Some(response) = response_policy.response() else {
                        decision.ignore();
                        browser.fail(AuthError::BrowserLoad);
                        return true;
                    };
                    let attachment = response
                        .http_headers()
                        .and_then(|headers| headers.one("Content-Disposition"))
                        .is_some_and(|value| {
                            value
                                .split(';')
                                .next()
                                .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("attachment"))
                        });
                    let main_resource = response_policy.is_main_frame_main_resource();
                    if attachment
                        || !response_policy.is_mime_type_supported()
                        || (main_resource && response.status_code() >= 400)
                    {
                        decision.ignore();
                        if main_resource {
                            browser.fail(AuthError::BrowserLoad);
                        }
                    } else {
                        decision.use_();
                    }
                    true
                }
                _ => false,
            }
        });

        let weak = Rc::downgrade(inner);
        inner.view.connect_uri_notify(move |view| {
            let Some(browser) = weak.upgrade() else {
                return;
            };
            let origin = if browser.closed.get() || browser.attempt.is_cancelled() {
                None
            } else {
                // WebView's URI describes the main document. Subframe policy
                // decisions must never change the displayed provider domain.
                view.uri()
                    .and_then(|uri| browser.attempt.display_origin(uri.as_str()))
            };
            (browser.on_origin)(origin.as_deref().unwrap_or(""));
        });

        let weak = Rc::downgrade(inner);
        inner.view.connect_load_changed(move |_, event| {
            let Some(browser) = weak.upgrade() else {
                return;
            };
            if browser.closed.get() || browser.attempt.is_cancelled() {
                return;
            }
            match event {
                webkit6::LoadEvent::Started => (browser.on_loading)(true),
                webkit6::LoadEvent::Finished => (browser.on_loading)(false),
                _ => {}
            }
        });

        let weak = Rc::downgrade(inner);
        inner.view.connect_load_failed(move |_, _, _, _| {
            if let Some(browser) = weak.upgrade() {
                browser.fail(AuthError::BrowserLoad);
            }
            // Suppress WebKit's default URI-bearing error page.
            true
        });
        let weak = Rc::downgrade(inner);
        inner
            .view
            .connect_load_failed_with_tls_errors(move |_, _, _, _| {
                if let Some(browser) = weak.upgrade() {
                    browser.fail(AuthError::BrowserTls);
                }
                true
            });
        let weak = Rc::downgrade(inner);
        inner.view.connect_web_process_terminated(move |_, _| {
            if let Some(browser) = weak.upgrade() {
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

impl Browser {
    fn stop(&self) {
        if self.closed.replace(true) {
            return;
        }
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
        if self.closed.get() || self.attempt.is_cancelled() {
            return;
        }
        self.stop();
        (self.on_error)(error);
    }
}

impl Drop for AuthWebView {
    fn drop(&mut self) {
        self.inner.stop();
    }
}

#[cfg(test)]
mod tests {
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
                let response = format!(
                    "HTTP/1.1 302 Found\r\nLocation: {next}\r\n{set_cookie}Content-Length: 0\r\nConnection: close\r\n\r\n"
                );
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
    }
}
