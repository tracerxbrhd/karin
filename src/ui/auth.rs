// SPDX-License-Identifier: GPL-3.0-or-later
//! Native presentation for one authorization attempt. Provider details stay outside GTK.

use adw::prelude::*;
use std::{cell::Cell, rc::Rc};

#[cfg(feature = "auth-webkit")]
pub(super) mod webkit;

#[derive(Clone)]
pub(super) struct LoginDialog {
    pub dialog: adw::Dialog,
    pub retry: gtk::Button,
    pub cancel: gtk::Button,
    message: adw::ActionRow,
    spinner: gtk::Spinner,
    origin: gtk::Label,
    presentation_ready: Rc<Cell<bool>>,
    close_requested: Rc<Cell<bool>>,
    #[cfg(feature = "auth-webkit")]
    web_content: gtk::Box,
    #[cfg(feature = "auth-webkit")]
    webview: std::rc::Rc<std::cell::RefCell<Option<webkit::AuthWebView>>>,
}

impl LoginDialog {
    pub fn new() -> Self {
        let dialog = adw::Dialog::builder()
            .title("Вход в Яндекс Музыку")
            .can_close(false)
            .content_width(440)
            .content_height(340)
            .build();
        dialog.set_widget_name("music-login-dialog");
        let presentation_ready = Rc::new(Cell::new(false));
        let close_requested = Rc::new(Cell::new(false));
        let ready = presentation_ready.clone();
        let closing = close_requested.clone();
        let weak = dialog.downgrade();
        let frames = Cell::new(0);
        // Libadwaita 1.5 opens its sheet on the second map tick. Closing an
        // already-false sheet before then does nothing, and its pending tick
        // would subsequently reopen it. Our third tick runs after that setup,
        // regardless of the order in which GTK invokes the tick callbacks.
        // Only a weak widget is captured, so an unmapped dialog can be dropped.
        dialog.add_tick_callback(move |_, _| {
            frames.set(frames.get() + 1);
            if frames.get() < 3 {
                return gtk::glib::ControlFlow::Continue;
            }
            ready.set(true);
            if let Some(dialog) = weak.upgrade() {
                if closing.get() {
                    dialog.force_close();
                } else {
                    dialog.set_can_close(true);
                }
            }
            gtk::glib::ControlFlow::Break
        });
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
        super::margins(&content, 18);
        let heading = gtk::Label::new(Some("Ваш аккаунт Яндекса"));
        heading.add_css_class("title-2");
        heading.set_wrap(true);
        heading.set_xalign(0.0);
        content.append(&heading);
        let origin = gtk::Label::new(None);
        origin.set_widget_name("music-login-origin");
        origin.set_xalign(0.0);
        origin.set_wrap(true);
        origin.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        origin.add_css_class("dim-label");
        origin.set_margin_start(18);
        origin.set_margin_end(18);
        origin.set_margin_top(6);
        origin.set_margin_bottom(6);
        origin.set_visible(false);
        // The verified active origin is browser chrome. It must remain visible
        // when focus scrolls the page or its surrounding status into view.
        toolbar.add_top_bar(&origin);
        let group = adw::PreferencesGroup::new();
        let message = adw::ActionRow::builder()
            .title("Подготовка входа…")
            .title_lines(0)
            .subtitle_lines(0)
            .build();
        message.set_widget_name("music-login-message");
        let spinner = gtk::Spinner::new();
        spinner.set_valign(gtk::Align::Center);
        message.add_suffix(&spinner);
        group.add(&message);
        content.append(&group);
        #[cfg(feature = "auth-webkit")]
        let web_content = {
            let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
            container.set_vexpand(true);
            container.set_hexpand(true);
            // The outer scroller propagates natural height. A concrete minimum
            // keeps a live WebView usable instead of allocating an empty strip.
            container.set_size_request(-1, 360);
            container.set_visible(false);
            content.append(&container);
            container
        };
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::End);
        let retry = gtk::Button::with_label("Повторить");
        retry.set_visible(false);
        let cancel = gtk::Button::with_label("Отменить");
        actions.append(&retry);
        actions.append(&cancel);
        // A provider's page may scroll independently. Keep cancellation outside
        // both scrollers so it remains reachable in compact/short windows.
        super::margins(&actions, 12);
        toolbar.add_bottom_bar(&actions);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .child(&content)
            .build();
        toolbar.set_content(Some(&scroll));
        dialog.set_child(Some(&toolbar));
        Self {
            dialog,
            retry,
            cancel,
            message,
            spinner,
            origin,
            presentation_ready,
            close_requested,
            #[cfg(feature = "auth-webkit")]
            web_content,
            #[cfg(feature = "auth-webkit")]
            webview: Default::default(),
        }
    }

    pub fn loading(&self, title: &str) {
        self.message.set_title(title);
        self.message
            .set_subtitle("Не закрывайте это окно до проверки доступа к Яндекс Музыке.");
        self.spinner.set_visible(true);
        self.spinner.set_spinning(true);
        self.retry.set_visible(false);
        self.cancel.set_label("Отменить");
    }

    pub fn presentation_ready(&self) -> bool {
        self.presentation_ready.get()
    }

    pub fn request_close(&self) {
        self.shutdown();
        self.close_requested.set(true);
        if self.presentation_ready() {
            self.dialog.force_close();
        }
    }

    pub fn unavailable(&self, title: &str, explanation: &str) {
        self.shutdown();
        self.set_origin("");
        self.message.set_title(title);
        self.message.set_subtitle(explanation);
        self.spinner.set_spinning(false);
        self.spinner.set_visible(false);
        self.retry.set_visible(true);
        self.cancel.set_label("Закрыть");
    }

    /// Receives only a provider-verified origin, never a URL path or query.
    pub fn set_origin(&self, origin: &str) {
        self.origin.set_text(origin);
        self.origin.set_visible(!origin.is_empty());
    }

    pub fn system_browser(&self) {
        self.loading("Подтвердите вход в системном браузере");
        self.set_origin("https://oauth.yandex.ru");
        self.message.set_subtitle("Вход проходит на странице Яндекса в системном браузере. После подтверждения Karin автоматически продолжит проверку доступа к Музыке.");
    }

    #[cfg(feature = "auth-webkit")]
    pub fn browser_loading(&self, loading: bool) {
        self.loading(if loading {
            "Загрузка страницы Яндекса…"
        } else {
            "Выполните вход на странице Яндекса"
        });
        self.message.set_subtitle("Прототип использует совместимый с Cassette способ авторизации. Разрешения показывает Яндекс; доступ к Музыке проверяется после входа.");
        self.spinner.set_visible(loading);
        self.spinner.set_spinning(loading);
    }

    #[cfg(feature = "auth-webkit")]
    pub fn set_webview(&self, webview: webkit::AuthWebView) {
        let origin = self.origin.text();
        self.shutdown();
        self.dialog.set_content_width(520);
        self.dialog.set_content_height(640);
        self.message.set_subtitle("Вход выполняется на настоящей странице Яндекса. Прототип использует совместимый с Cassette способ авторизации; доступ к Музыке проверяется отдельно.");
        self.web_content.set_visible(true);
        self.web_content.append(webview.widget());
        *self.webview.borrow_mut() = Some(webview);
        self.set_origin(&origin);
    }

    pub fn shutdown(&self) {
        self.set_origin("");
        self.dialog.set_content_width(440);
        self.dialog.set_content_height(340);
        #[cfg(feature = "auth-webkit")]
        self.web_content.set_visible(false);
        #[cfg(feature = "auth-webkit")]
        if let Some(webview) = self.webview.borrow_mut().take() {
            webview.shutdown();
            self.web_content.remove(webview.widget());
        }
    }
}
