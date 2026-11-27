// SPDX-License-Identifier: GPL-3.0-or-later
//! Sign-in dialog lifecycle and Music account validation.

use std::{rc::Rc, sync::Arc};

use adw::prelude::*;
use gtk::{gio, glib};

use super::{Controller, Operation, music_error_message};
#[cfg(feature = "auth-webkit")]
use crate::auth::SessionCookies;
use crate::{
    auth::{AuthAttempt, AuthError, AuthorizationCode, BrowserMode},
    music::MusicError,
    session::AccessToken,
    ui::auth::LoginDialog,
};

impl Controller {
    pub(super) fn open_login(self: &Rc<Self>) {
        if self.model.borrow().closed || self.model.borrow().account_valid {
            return;
        }
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            if dialog.dialog.parent().is_some()
                && let Some(window) = self.window.upgrade()
            {
                window.present();
            }
            return;
        }
        if !self.prepare_interactive_login() {
            return;
        }
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let dialog = LoginDialog::new();
        let weak = Rc::downgrade(self);
        dialog.dialog.connect_closed(move |closed| {
            if let Some(this) = weak.upgrade() {
                this.cancel_dialog(closed);
            }
        });
        let weak = Rc::downgrade(self);
        dialog.dialog.connect_close_attempt(move |closing| {
            if let Some(this) = weak.upgrade() {
                this.cancel_dialog(closing);
            }
        });
        let weak = Rc::downgrade(self);
        let owner = dialog.dialog.downgrade();
        dialog.cancel.connect_clicked(move |_| {
            if let (Some(this), Some(owner)) = (weak.upgrade(), owner.upgrade()) {
                this.cancel_dialog(&owner);
            }
        });
        let weak = Rc::downgrade(self);
        let owner = dialog.dialog.downgrade();
        dialog.retry.connect_clicked(move |_| {
            if let (Some(this), Some(owner)) = (weak.upgrade(), owner.upgrade()) {
                let current = this
                    .login_dialog
                    .borrow()
                    .as_ref()
                    .is_some_and(|current| current.dialog == owner);
                if current {
                    this.start_login();
                }
            }
        });
        *self.login_dialog.borrow_mut() = Some(dialog.clone());
        let generation = {
            let mut model = self.model.borrow_mut();
            model.auth_busy = true;
            model.busy = true;
            model.generation
        };
        self.render_controls();
        if self.login_host_ready(&window) {
            dialog.dialog.present(Some(&window));
            dialog.cancel.grab_focus();
            self.start_login();
            return;
        }
        // Libadwaita 1.5 keeps a closing sheet parented after logical closure.
        // Presenting another sheet then shadows the retiring dialog and can
        // register a native focus weak pointer whose owner is being disposed.
        // Observe host removal and unparenting; never overlap our login sheets.
        let weak = Rc::downgrade(self);
        let pending = dialog.dialog.downgrade();
        window.add_tick_callback(move |window, _| {
            let (Some(this), Some(dialog)) = (weak.upgrade(), pending.upgrade()) else {
                return glib::ControlFlow::Break;
            };
            let current = {
                let model = this.model.borrow();
                !model.closed
                    && model.generation == generation
                    && this
                        .login_dialog
                        .borrow()
                        .as_ref()
                        .is_some_and(|current| current.dialog == dialog)
            };
            if !current {
                return glib::ControlFlow::Break;
            }
            if !this.login_host_ready(window) {
                return glib::ControlFlow::Continue;
            }
            dialog.present(Some(window));
            if let Some(current) = this.login_dialog.borrow().as_ref() {
                current.cancel.grab_focus();
            }
            this.start_login();
            glib::ControlFlow::Break
        });
    }

    pub(super) fn login_host_ready(&self, window: &adw::ApplicationWindow) -> bool {
        let mut retiring = self.retiring_logins.borrow_mut();
        retiring.retain(|dialog| {
            dialog
                .upgrade()
                .is_some_and(|dialog| dialog.parent().is_some())
        });
        retiring.is_empty() && window.dialogs().n_items() == 0
    }

    pub(super) fn dismiss_login(&self) {
        let dialog = self.login_dialog.borrow_mut().take();
        if let Some(dialog) = dialog {
            self.retiring_logins
                .borrow_mut()
                .push(dialog.dialog.downgrade());
            dialog.request_close();
        }
    }

    fn cancel_dialog(&self, dialog: &adw::Dialog) {
        let current = self
            .login_dialog
            .borrow()
            .as_ref()
            .is_some_and(|current| current.dialog == *dialog);
        if current {
            self.cancel_login();
        }
    }

    pub(super) fn cancel_login(&self) {
        if self.login_dialog.borrow().is_none() {
            return;
        }
        self.cancel_radio();
        self.stop_music();
        self.cancel_search_input();
        self.requests.borrow_mut().invalidate_all();
        self.cancel_delivery();
        if let Some(artwork) = &self.artwork {
            artwork.clear();
        }
        {
            let mut model = self.model.borrow_mut();
            model.generation += 1;
            model.view_generation += 1;
            model.auth_busy = false;
            model.busy = false;
            if let Some(attempt) = model.attempt.take() {
                attempt.cancel();
            }
            if let Some(token) = model.token.take() {
                token.invalidate();
            }
            model.account_valid = false;
        }
        if let Some(service) = &self.service {
            service.clear_session();
        }
        self.dismiss_login();
        self.reset_status();
        self.render_controls();
        self.widgets.login.grab_focus();
    }

    fn start_login(self: &Rc<Self>) {
        if self
            .login_dialog
            .borrow()
            .as_ref()
            .is_none_or(|dialog| dialog.dialog.parent().is_none())
        {
            return;
        }
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let generation = {
            let mut model = self.model.borrow_mut();
            if model.closed {
                return;
            }
            if let Some(attempt) = model.attempt.take() {
                attempt.cancel();
            }
            if let Some(token) = model.token.take() {
                token.invalidate();
            }
            model.generation += 1;
            model.view_generation += 1;
            model.account_valid = false;
            model.auth_busy = true;
            model.busy = true;
            model.generation
        };
        if let Some(service) = &self.service {
            service.clear_session();
        }
        self.reset_status();
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            dialog.shutdown();
            dialog.loading("Подготовка входа…");
        }
        self.render_controls();
        let provider = self.provider.clone();
        let weak = Rc::downgrade(self);
        let hold = app.hold();
        glib::spawn_future_local(async move {
            let _hold = hold;
            let result = gio::spawn_blocking(move || provider.begin()).await;
            let Some(this) = weak.upgrade() else {
                if let Ok(Ok(attempt)) = result {
                    attempt.cancel();
                }
                return;
            };
            if !this.current_attempt(generation) {
                if let Ok(Ok(attempt)) = result {
                    attempt.cancel();
                }
                return;
            }
            match result {
                Ok(Ok(attempt)) => this.present_attempt(attempt, generation),
                Ok(Err(error)) => this.auth_failed(error),
                Err(_) => this.auth_failed(AuthError::Configuration),
            }
        });
    }

    fn current_attempt(&self, generation: u64) -> bool {
        let model = self.model.borrow();
        !model.closed && model.generation == generation && self.login_dialog.borrow().is_some()
    }

    fn present_attempt(self: &Rc<Self>, attempt: Arc<AuthAttempt>, generation: u64) {
        self.model.borrow_mut().attempt = Some(attempt.clone());
        let url = attempt.authorization_url();
        match attempt.browser_mode() {
            BrowserMode::System => {
                if (self.launch_browser)(&url).is_err() {
                    self.auth_failed(AuthError::BrowserLoad);
                    return;
                }
                if let Some(dialog) = self.login_dialog.borrow().as_ref() {
                    dialog.system_browser();
                }
                self.wait_for_callback(attempt, generation);
            }
            BrowserMode::Embedded => {
                #[cfg(feature = "auth-webkit")]
                self.present_embedded(attempt, generation, &url);
                #[cfg(not(feature = "auth-webkit"))]
                self.auth_failed(AuthError::Configuration);
            }
        }
    }

    #[cfg(feature = "auth-webkit")]
    fn present_embedded(self: &Rc<Self>, attempt: Arc<AuthAttempt>, generation: u64, url: &str) {
        let expiry_attempt = Arc::downgrade(&attempt);
        let weak = Rc::downgrade(self);
        let callback_attempt = attempt.clone();
        let on_redirect = Rc::new(move |code| {
            if let Some(this) = weak.upgrade()
                && this.current_attempt(generation)
            {
                this.exchange_code(callback_attempt.clone(), code, generation);
            }
        });
        let weak = Rc::downgrade(self);
        let session_attempt = attempt.clone();
        let on_session = Rc::new(move |cookies| {
            if let Some(this) = weak.upgrade()
                && this.current_attempt(generation)
            {
                this.exchange_session(session_attempt.clone(), cookies, generation);
            }
        });
        let weak = Rc::downgrade(self);
        let on_error = Rc::new(move |error| {
            if let Some(this) = weak.upgrade()
                && this.current_attempt(generation)
            {
                this.auth_failed(error);
            }
        });
        let weak = Rc::downgrade(self);
        let on_loading = Rc::new(move |loading| {
            if let Some(this) = weak.upgrade()
                && this.current_attempt(generation)
                && let Some(dialog) = this.login_dialog.borrow().as_ref()
            {
                dialog.browser_loading(loading);
            }
        });
        let weak = Rc::downgrade(self);
        let on_origin = Rc::new(move |origin: &str| {
            if let Some(this) = weak.upgrade()
                && this.current_attempt(generation)
                && let Some(dialog) = this.login_dialog.borrow().as_ref()
            {
                dialog.set_origin(origin);
            }
        });
        let webview = crate::ui::auth::webkit::AuthWebView::new(
            attempt,
            on_redirect,
            on_session,
            on_error,
            on_loading,
            on_origin,
        );
        webview.load(url);
        if self.current_attempt(generation) {
            if let Some(dialog) = self.login_dialog.borrow().as_ref() {
                dialog.set_webview(webview);
            }
        } else {
            webview.shutdown();
        }
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            loop {
                let delay = {
                    let Some(attempt) = expiry_attempt.upgrade() else {
                        return;
                    };
                    if attempt.is_cancelled() {
                        return;
                    }
                    attempt
                        .time_remaining()
                        .min(std::time::Duration::from_secs(1))
                };
                // Do not retain PKCE material or the callback listener during
                // the timer wait after cancellation, failure or dialog closure.
                glib::timeout_future(delay).await;
                let Some(this) = weak.upgrade() else {
                    return;
                };
                if !this.current_attempt(generation) {
                    return;
                }
                let Some(attempt) = expiry_attempt.upgrade() else {
                    return;
                };
                if attempt.is_cancelled() {
                    return;
                }
                if attempt.expired() {
                    this.auth_failed(AuthError::Expired);
                    return;
                }
            }
        });
    }

    fn wait_for_callback(self: &Rc<Self>, attempt: Arc<AuthAttempt>, generation: u64) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let hold = app.hold();
        let weak = Rc::downgrade(self);
        let waiting = attempt.clone();
        glib::spawn_future_local(async move {
            let _hold = hold;
            let result = gio::spawn_blocking(move || waiting.receive_callback()).await;
            let Some(this) = weak.upgrade() else {
                return;
            };
            if !this.current_attempt(generation) {
                return;
            }
            match result {
                Ok(Ok(code)) => this.exchange_code(attempt, code, generation),
                Ok(Err(error)) => this.auth_failed(error),
                Err(_) => this.auth_failed(AuthError::Configuration),
            }
        });
    }

    fn exchange_code(
        self: &Rc<Self>,
        attempt: Arc<AuthAttempt>,
        code: AuthorizationCode,
        generation: u64,
    ) {
        if let Some(window) = self.window.upgrade() {
            window.present();
        }
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            dialog.set_origin("");
            dialog.loading("Проверка разрешения приложения…");
        }
        let provider = self.provider.clone();
        self.run_auth_worker(generation, move || provider.exchange(&attempt, code));
    }

    #[cfg(feature = "auth-webkit")]
    fn exchange_session(
        self: &Rc<Self>,
        attempt: Arc<AuthAttempt>,
        cookies: SessionCookies,
        generation: u64,
    ) {
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            dialog.set_origin("");
            dialog.loading("Получение разрешения Яндекс Музыки…");
        }
        let provider = self.provider.clone();
        self.run_auth_worker(generation, move || {
            provider.exchange_session(&attempt, cookies)
        });
    }

    fn run_auth_worker(
        self: &Rc<Self>,
        generation: u64,
        work: impl FnOnce() -> Result<AccessToken, AuthError> + Send + 'static,
    ) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let hold = app.hold();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let _hold = hold;
            let result = gio::spawn_blocking(work).await;
            let Some(this) = weak.upgrade() else {
                if let Ok(Ok(token)) = result {
                    token.invalidate();
                }
                return;
            };
            if !this.current_attempt(generation) {
                if let Ok(Ok(token)) = result {
                    token.invalidate();
                }
                return;
            }
            match result {
                Ok(Ok(token)) => this.authenticate(token),
                Ok(Err(error)) => this.auth_failed(error),
                Err(_) => this.auth_failed(AuthError::Configuration),
            }
        });
    }

    fn auth_failed(&self, error: AuthError) {
        self.clear_cache_presence();
        self.cancel_radio();
        self.cancel_music_cache(crate::audio_cache::diagnostic::Cause::AccountChanged);
        self.stop_music_engine();
        {
            let mut model = self.model.borrow_mut();
            model.generation += 1;
            if let Some(attempt) = model.attempt.take() {
                attempt.cancel();
            }
            if let Some(token) = model.token.take() {
                token.invalidate();
            }
            model.account_valid = false;
            model.auth_busy = false;
            model.busy = false;
        }
        if let Some(service) = &self.service {
            service.clear_session();
        }
        self.reset_status();
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            dialog.unavailable("Не удалось выполнить вход", &error.to_string());
        }
        self.render_controls();
    }

    pub(super) fn authenticate(self: &Rc<Self>, token: AccessToken) {
        if self.model.borrow().closed {
            token.invalidate();
            return;
        }
        self.retire_playback_restore();
        self.retire_playlist_management();
        self.clear_cache_presence();
        self.cancel_radio();
        self.cancel_music_cache(crate::audio_cache::diagnostic::Cause::AccountChanged);
        self.stop_music_engine();
        {
            let mut model = self.model.borrow_mut();
            if model.closed {
                token.invalidate();
                return;
            }
            if let Some(previous) = model.token.replace(token) {
                previous.invalidate();
            }
            model.account_valid = false;
            model.auth_busy = false;
            model.busy = false;
            model.queue.clear();
            model.queue.reset_modes();
        }
        if self.service.is_none() {
            self.session_validation_failed(MusicError::Configuration);
            self.auth_failed(AuthError::Configuration);
            return;
        }
        if let Some(dialog) = self.login_dialog.borrow().as_ref() {
            dialog.loading("Проверка доступа к Яндекс Музыке…");
        }
        self.run(
            Operation::Account,
            |service, token| {
                let account = service.account(&token)?;
                let scope = service.cache_scope(&token)?;
                Ok((account, scope))
            },
            |this, result| match result {
                Ok((account, scope)) => {
                    {
                        let mut model = this.model.borrow_mut();
                        model.generation += 1;
                        model.account_valid = true;
                        if let Some(attempt) = model.attempt.take() {
                            attempt.cancel();
                        }
                    }
                    this.widgets.account.set_text(&account.display_name);
                    this.widgets.notice.set_text(
                        "Аккаунт Яндекс Музыки проверен. Сохранением входа можно управлять в настройках.",
                    );
                    this.can_create_playlist.set(true);
                    this.widgets.stack.set_visible_child_name("home");
                    this.session_validated(scope);
                    this.dismiss_login();
                    this.render_controls();
                    this.load_initial_library();
                }
                Err(error) => {
                    this.session_validation_failed(error);
                    let (title, message) = music_error_message(error);
                    {
                        let mut model = this.model.borrow_mut();
                        model.generation += 1;
                        if let Some(token) = model.token.take() {
                            token.invalidate();
                        }
                        if let Some(attempt) = model.attempt.take() {
                            attempt.cancel();
                        }
                        model.account_valid = false;
                        model.auth_busy = false;
                        model.busy = false;
                    }
                    if let Some(service) = &this.service {
                        service.clear_session();
                    }
                    this.reset_status();
                    if let Some(dialog) = this.login_dialog.borrow().as_ref() {
                        let status = error
                            .http_status()
                            .map(|status| format!(" · HTTP {status}"))
                            .unwrap_or_default();
                        dialog.unavailable(
                            title,
                            &format!("Шаг: проверка аккаунта Яндекс Музыки. {message}{status}"),
                        );
                    }
                    this.render_controls();
                }
            },
        );
    }
}
