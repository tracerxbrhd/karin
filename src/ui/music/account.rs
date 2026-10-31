// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit account actions; opening or dismissing the menu never ends a session.

use super::*;

#[cfg(test)]
mod tests;

// Account management is intentionally a fixed external destination. Never add
// an account ID, token or session cookie to this URI.
const ACCOUNT_URI: &str = "https://id.yandex.ru/";

pub(super) struct AccountMenu {
    pub menu: gtk::MenuButton,
    pub name: gtk::Label,
    pub open_account: gtk::Button,
    pub launch_error: gtk::Label,
    pub logout: gtk::Button,
}

impl AccountMenu {
    pub fn new() -> Self {
        let menu = gtk::MenuButton::new();
        menu.set_widget_name("music-account-menu");
        let avatar = adw::Avatar::new(28, None, true);
        avatar.set_widget_name("music-account-avatar");
        menu.set_child(Some(&avatar));
        menu.set_always_show_arrow(false);
        menu.set_has_frame(false);
        menu.set_size_request(36, 36);
        menu.add_css_class("circular");
        menu.set_tooltip_text(Some("Меню аккаунта"));
        menu.update_property(&[gtk::accessible::Property::Label("Меню аккаунта")]);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        super::super::margins(&content, 12);
        let name = text("Ваш аккаунт Яндекса", "heading");
        name.set_widget_name("music-account");
        name.set_max_width_chars(28);
        name.set_wrap(true);
        // Only the Music account's already validated display name supplies the
        // initials. No unverified avatar endpoint, extra HTTP or browser cookies.
        name.bind_property("label", &avatar, "text")
            .sync_create()
            .build();
        content.append(&name);
        let info = text("Яндекс Музыка · вход выполнен", "dim-label");
        info.set_widget_name("music-account-summary");
        content.append(&info);
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let open_account = gtk::Button::with_label("Открыть аккаунт в браузере");
        open_account.set_widget_name("music-account-open-browser");
        open_account.add_css_class("flat");
        if let Some(label) = open_account.child().and_downcast::<gtk::Label>() {
            label.set_wrap(true);
            label.set_xalign(0.0);
        }
        content.append(&open_account);
        let launch_error = text("Не удалось открыть браузер. Попробуйте ещё раз.", "error");
        launch_error.set_widget_name("music-account-browser-error");
        launch_error.set_max_width_chars(28);
        launch_error.set_wrap(true);
        launch_error.set_visible(false);
        content.append(&launch_error);
        let logout = gtk::Button::with_label("Выйти из аккаунта");
        logout.set_widget_name("music-account-logout");
        logout.add_css_class("flat");
        if let Some(label) = logout.child().and_downcast::<gtk::Label>() {
            label.set_xalign(0.0);
        }
        content.append(&logout);
        let popover = gtk::Popover::new();
        popover.set_child(Some(&content));
        let error = launch_error.downgrade();
        popover.connect_closed(move |_| {
            if let Some(error) = error.upgrade() {
                error.set_visible(false);
            }
        });
        menu.set_popover(Some(&popover));
        Self {
            menu,
            name,
            open_account,
            launch_error,
            logout,
        }
    }
}

/// A close requested before Libadwaita 1.5's native sheet opens must also be
/// honoured after its map ticks. Keep weak native ownership, as for sign-in.
#[derive(Clone)]
pub(super) struct LogoutConfirmation {
    pub dialog: adw::AlertDialog,
    ready: Rc<Cell<bool>>,
    close_requested: Rc<Cell<bool>>,
}

impl LogoutConfirmation {
    fn new() -> Self {
        let dialog = adw::AlertDialog::new(
            Some("Выйти из аккаунта?"),
            Some(
                "Воспроизведение остановится, а очередь будет очищена. Чтобы снова открыть свою музыку, потребуется войти в аккаунт.",
            ),
        );
        dialog.set_widget_name("music-logout-confirmation");
        dialog.add_responses(&[("cancel", "Отмена"), ("logout", "Выйти из аккаунта")]);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("logout", adw::ResponseAppearance::Destructive);
        dialog.set_can_close(false);
        let ready = Rc::new(Cell::new(false));
        let close_requested = Rc::new(Cell::new(false));
        let weak = dialog.downgrade();
        let closing = close_requested.clone();
        let prepared = ready.clone();
        let frames = Cell::new(0);
        dialog.add_tick_callback(move |_, _| {
            frames.set(frames.get() + 1);
            if frames.get() < 3 {
                return glib::ControlFlow::Continue;
            }
            prepared.set(true);
            if let Some(dialog) = weak.upgrade() {
                if closing.get() {
                    if dialog.parent().is_some() {
                        dialog.force_close();
                    }
                } else {
                    dialog.set_can_close(true);
                }
            }
            glib::ControlFlow::Break
        });
        Self {
            dialog,
            ready,
            close_requested,
        }
    }

    fn request_close(&self) {
        self.close_requested.set(true);
        if self.ready.get() && self.dialog.parent().is_some() {
            self.dialog.force_close();
        }
    }
}

impl Controller {
    pub(super) fn connect_account_menu(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.widgets.account_open.connect_clicked(move |_| {
            let Some(this) = weak.upgrade() else { return };
            {
                let model = this.model.borrow();
                if model.closed
                    || !model.account_valid
                    || model.token.as_ref().is_none_or(AccessToken::is_expired)
                {
                    return;
                }
            }
            this.widgets.account_launch_error.set_visible(false);
            // GIO launches the system default browser, including the desktop
            // portal when sandboxed. This never enters the sign-in WebView.
            // The existing injected launcher lets fixtures avoid opening a
            // real browser while exercising the same account action/guards.
            if (this.launch_browser)(ACCOUNT_URI).is_ok() {
                this.widgets.account_menu.popdown();
            } else {
                this.widgets.account_launch_error.set_visible(true);
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.logout.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.request_logout();
            }
        });
    }

    fn retire_logout_confirmation(&self, confirmation: LogoutConfirmation) {
        self.retiring_logins.borrow_mut().push(
            confirmation
                .dialog
                .clone()
                .upcast::<adw::Dialog>()
                .downgrade(),
        );
        confirmation.request_close();
    }

    pub(super) fn dismiss_logout_confirmation(&self) {
        let confirmation = self.logout_confirmation.borrow_mut().take();
        if let Some(confirmation) = confirmation {
            self.retire_logout_confirmation(confirmation);
        }
        self.widgets.account_menu.popdown();
    }

    fn request_logout(self: &Rc<Self>) {
        self.widgets.account_menu.popdown();
        let generation = {
            let model = self.model.borrow();
            if model.closed || !model.account_valid || model.token.is_none() {
                return;
            }
            model.generation
        };
        if self.logout_confirmation.borrow().is_some() {
            return;
        }
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let confirmation = LogoutConfirmation::new();
        let weak = Rc::downgrade(self);
        confirmation
            .dialog
            .connect_response(None, move |dialog, response| {
                let Some(this) = weak.upgrade() else { return };
                let current = this
                    .logout_confirmation
                    .borrow()
                    .as_ref()
                    .is_some_and(|current| current.dialog == *dialog);
                if !current {
                    return;
                }
                let current = this.logout_confirmation.borrow_mut().take();
                if let Some(current) = current {
                    this.retire_logout_confirmation(current);
                }
                let confirmed_current_session = {
                    let model = this.model.borrow();
                    response == "logout"
                        && !model.closed
                        && model.account_valid
                        && model.generation == generation
                        && model.token.is_some()
                };
                if confirmed_current_session {
                    this.logout();
                }
            });
        let weak = Rc::downgrade(self);
        confirmation.dialog.connect_close_attempt(move |dialog| {
            let Some(this) = weak.upgrade() else { return };
            let current = this
                .logout_confirmation
                .borrow()
                .as_ref()
                .is_some_and(|current| current.dialog == *dialog);
            if current {
                this.dismiss_logout_confirmation();
            }
        });
        *self.logout_confirmation.borrow_mut() = Some(confirmation.clone());
        if self.login_host_ready(&window) {
            self.widgets.account_menu.grab_focus();
            confirmation.dialog.present(Some(&window));
            return;
        }
        // Keep a pending confirmation logical only until every retiring sheet
        // is actually unparented. No shadowed focus owner can be disposed below it.
        let weak = Rc::downgrade(self);
        let pending = confirmation.dialog.downgrade();
        window.add_tick_callback(move |window, _| {
            let (Some(this), Some(dialog)) = (weak.upgrade(), pending.upgrade()) else {
                return glib::ControlFlow::Break;
            };
            let current = this
                .logout_confirmation
                .borrow()
                .as_ref()
                .is_some_and(|current| current.dialog == dialog);
            if !current {
                return glib::ControlFlow::Break;
            }
            let valid = {
                let model = this.model.borrow();
                !model.closed
                    && model.account_valid
                    && model.generation == generation
                    && model.token.is_some()
            };
            if !valid {
                this.dismiss_logout_confirmation();
                return glib::ControlFlow::Break;
            }
            if !this.login_host_ready(window) {
                return glib::ControlFlow::Continue;
            }
            this.widgets.account_menu.grab_focus();
            dialog.present(Some(window));
            glib::ControlFlow::Break
        });
    }

    #[cfg(test)]
    pub(super) fn respond_logout_for_test(&self, response: &str) {
        let confirmation = self.logout_confirmation.borrow().as_ref().unwrap().clone();
        let label = match response {
            "cancel" => "Отмена",
            "logout" => "Выйти из аккаунта",
            _ => panic!("unknown synthetic account response"),
        };
        let mut pending = vec![confirmation.dialog.clone().upcast::<gtk::Widget>()];
        while let Some(widget) = pending.pop() {
            if let Some(button) = widget.downcast_ref::<gtk::Button>()
                && button.label().as_deref() == Some(label)
            {
                button.emit_clicked();
                return;
            }
            let mut child = widget.first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                pending.push(widget);
            }
        }
        panic!("native account response button missing");
    }
}
