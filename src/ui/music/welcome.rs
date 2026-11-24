// SPDX-License-Identifier: GPL-3.0-or-later
//! The unauthenticated surface of the existing music controller.

use adw::prelude::*;

pub(super) const LOGIN_LABEL: &str = "Войти через Яндекс";
const TITLE: &str = "Ваша музыка — в Karin";
const DESCRIPTION: &str = "Войдите в аккаунт Яндекса, чтобы открыть свою музыкальную библиотеку";

#[derive(Clone)]
pub(super) struct Welcome {
    pub page: adw::StatusPage,
    pub login: gtk::Button,
    waiting: gtk::Spinner,
}

impl Welcome {
    pub fn new() -> Self {
        super::super::icons::install();
        let login = gtk::Button::with_label(LOGIN_LABEL);
        login.set_widget_name("music-login");
        login.add_css_class("suggested-action");
        login.add_css_class("pill");
        login.set_halign(gtk::Align::Center);

        let waiting = gtk::Spinner::new();
        waiting.set_widget_name("music-welcome-progress");
        waiting.set_halign(gtk::Align::Center);
        let independent = gtk::Label::new(Some("Karin — независимый клиент для Linux"));
        independent.set_wrap(true);
        independent.set_justify(gtk::Justification::Center);
        independent.add_css_class("dim-label");
        independent.add_css_class("caption");

        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        content.append(&login);
        content.append(&waiting);
        content.append(&independent);
        let page = adw::StatusPage::builder()
            .icon_name(crate::config::APP_ID)
            .title(TITLE)
            .description(DESCRIPTION)
            .child(&content)
            .vexpand(true)
            .build();
        page.set_widget_name("music-welcome");
        Self {
            page,
            login,
            waiting,
        }
    }

    pub fn render(&self, ready: bool, restoring: bool, signing_in: bool, can_sign_in: bool) {
        self.page.set_title(if restoring {
            "Восстанавливаем вход"
        } else {
            TITLE
        });
        self.page.set_description(Some(if restoring {
            "Проверяем сохранённую сессию Яндекс Музыки…"
        } else if signing_in {
            "Вход через Яндекс выполняется…"
        } else {
            DESCRIPTION
        }));
        self.login.set_visible(!ready && !restoring);
        self.login.set_sensitive(can_sign_in);
        self.waiting.set_visible(restoring);
        self.waiting.set_spinning(restoring);
        self.page.set_visible(!ready);
    }
}
