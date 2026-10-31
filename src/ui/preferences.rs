// SPDX-License-Identifier: GPL-3.0-or-later
//! Native persistent preferences. The owner performs all storage and session work.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;

use crate::{
    cache::CacheStats,
    settings::{Appearance, Preferences},
};

const MIB: u64 = 1024 * 1024;

#[derive(Clone)]
pub(super) struct PreferencesSnapshot {
    pub values: Preferences,
    pub settings_ready: bool,
    pub storage_supported: bool,
    pub session_status: String,
    pub session_busy: bool,
    pub can_retry_session: bool,
    pub cache_stats: Option<CacheStats>,
    pub cache_busy: bool,
    /// Friendly, non-secret status supplied by the application coordinator.
    pub message: Option<String>,
}

#[derive(Clone)]
pub(super) struct PreferencesCallbacks {
    pub changed: Rc<dyn Fn(Preferences)>,
    pub clear_cache: Rc<dyn Fn()>,
    pub retry_session: Rc<dyn Fn()>,
    pub reload_stats: Rc<dyn Fn()>,
}

struct State {
    values: RefCell<Preferences>,
    updating: Cell<bool>,
    ready: Cell<bool>,
    storage_supported: Cell<bool>,
    session_busy: Cell<bool>,
    cache_busy: Cell<bool>,
    can_retry_session: Cell<bool>,
    callbacks: PreferencesCallbacks,
}

#[derive(Default)]
struct MessageReveal {
    serial: Cell<u64>,
    pending: Cell<bool>,
}

impl State {
    fn change(&self, update: impl FnOnce(&mut Preferences)) {
        if self.updating.get() || !self.ready.get() {
            return;
        }
        // Release the borrow before dispatch: a synchronous owner update is
        // allowed to render a newer snapshot or report a rejected operation.
        let mut next = self.values.borrow().clone();
        update(&mut next);
        self.values.replace(next.clone());
        (self.callbacks.changed)(next);
    }
}

pub(super) struct PreferencesView {
    pub dialog: adw::PreferencesDialog,
    state: Rc<State>,
    page: adw::PreferencesPage,
    scheme: adw::ComboRow,
    remember: adw::SwitchRow,
    session: adw::ActionRow,
    session_retry: gtk::Button,
    session_spinner: gtk::Spinner,
    cache_enabled: adw::SwitchRow,
    cache_limit: adw::SpinRow,
    cache_usage: adw::ActionRow,
    cache_refresh: gtk::Button,
    cache_clear: gtk::Button,
    cache_spinner: gtk::Spinner,
    operation_status: adw::ActionRow,
    last_message: RefCell<Option<String>>,
    message_reveal: Rc<MessageReveal>,
}

impl PreferencesView {
    pub fn new(snapshot: &PreferencesSnapshot, callbacks: &PreferencesCallbacks) -> Self {
        let state = Rc::new(State {
            values: RefCell::new(snapshot.values.clone()),
            updating: Cell::new(true),
            ready: Cell::new(false),
            storage_supported: Cell::new(false),
            session_busy: Cell::new(false),
            cache_busy: Cell::new(false),
            can_retry_session: Cell::new(false),
            callbacks: callbacks.clone(),
        });
        let dialog = adw::PreferencesDialog::builder()
            .title("Настройки")
            .search_enabled(false)
            .content_width(620)
            .content_height(700)
            .build();
        dialog.set_widget_name("karin-preferences");
        let page = adw::PreferencesPage::new();
        let appearance = adw::PreferencesGroup::builder().title("Оформление").build();
        let scheme = adw::ComboRow::builder()
            .title("Цветовая схема")
            .model(&gtk::StringList::new(&["Системная", "Светлая", "Тёмная"]))
            .use_markup(false)
            .build();
        scheme.set_widget_name("preferences-appearance");
        scheme.connect_selected_notify({
            let state = Rc::clone(&state);
            move |row| {
                state.change(|values| values.appearance = appearance_from_index(row.selected()))
            }
        });
        appearance.add(&scheme);
        page.add(&appearance);

        let authorization = adw::PreferencesGroup::builder()
            .title("Авторизация")
            .description("Пароль и данные страницы входа не сохраняются.")
            .build();
        let remember = adw::SwitchRow::builder()
            .title("Запоминать вход")
            .title_lines(0)
            .subtitle_lines(0)
            .use_markup(false)
            .build();
        remember.set_widget_name("preferences-remember-login");
        remember.connect_active_notify({
            let state = Rc::clone(&state);
            move |row| {
                if state.updating.get() {
                    return;
                }
                let current = state.values.borrow().remember_login;
                let next = row.is_active();
                if state.ready.get()
                    && !state.session_busy.get()
                    && remember_change_allowed(current, next, state.storage_supported.get())
                {
                    state.change(|values| values.remember_login = next);
                    row.set_sensitive(
                        state.ready.get()
                            && !state.session_busy.get()
                            && (state.storage_supported.get()
                                || state.values.borrow().remember_login),
                    );
                } else if next != current {
                    // Programmatic activation cannot turn remembering on
                    // without protected storage or during critical work.
                    state.updating.set(true);
                    row.set_active(current);
                    state.updating.set(false);
                }
            }
        });
        authorization.add(&remember);
        let session = adw::ActionRow::builder()
            .title("Сохранённый вход")
            .title_lines(0)
            .subtitle_lines(0)
            .use_markup(false)
            .build();
        session.set_widget_name("preferences-session-status");
        let session_spinner = gtk::Spinner::new();
        session_spinner.set_valign(gtk::Align::Center);
        session.add_suffix(&session_spinner);
        let session_retry = gtk::Button::with_label("Повторить проверку");
        session_retry.set_widget_name("preferences-session-retry");
        session_retry.set_valign(gtk::Align::Center);
        session_retry.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                if !state.session_busy.get() && state.can_retry_session.get() {
                    (state.callbacks.retry_session)();
                }
            }
        });
        session.add_suffix(&session_retry);
        authorization.add(&session);
        page.add(&authorization);

        let storage = adw::PreferencesGroup::builder()
            .title("Хранилище")
            .description("Метаданные и обложки хранятся отдельно от защищённого входа. Аудио не сохраняется.")
            .build();
        let cache_enabled = adw::SwitchRow::builder()
            .title("Кеширование данных")
            .title_lines(0)
            .subtitle_lines(0)
            .use_markup(false)
            .build();
        cache_enabled.set_widget_name("preferences-cache-enabled");
        cache_enabled.connect_active_notify({
            let state = Rc::clone(&state);
            move |row| {
                if !state.cache_busy.get() {
                    state.change(|values| values.cache_enabled = row.is_active());
                }
            }
        });
        storage.add(&cache_enabled);
        let cache_limit = adw::SpinRow::builder()
            .title("Максимальный размер")
            .subtitle("МиБ · от 16 до 4096")
            .adjustment(&gtk::Adjustment::new(256.0, 16.0, 4096.0, 16.0, 64.0, 0.0))
            .digits(0)
            .numeric(true)
            .title_lines(0)
            .subtitle_lines(0)
            .use_markup(false)
            .build();
        cache_limit.set_widget_name("preferences-cache-limit");
        cache_limit.connect_value_notify({
            let state = Rc::clone(&state);
            move |row| {
                if state.cache_busy.get() {
                    return;
                }
                let value = row.value();
                if value.is_finite() {
                    let mib = value.round().clamp(16.0, 4096.0) as u64;
                    state.change(|values| values.cache_limit_bytes = mib * MIB);
                }
            }
        });
        storage.add(&cache_limit);
        let cache_usage = adw::ActionRow::builder()
            .title("Размер файлов кеша")
            .title_lines(0)
            .subtitle_lines(0)
            .use_markup(false)
            .build();
        cache_usage.set_widget_name("preferences-cache-usage");
        let cache_spinner = gtk::Spinner::new();
        cache_spinner.set_valign(gtk::Align::Center);
        cache_usage.add_suffix(&cache_spinner);
        let cache_refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        cache_refresh.set_widget_name("preferences-cache-refresh");
        cache_refresh.set_valign(gtk::Align::Center);
        cache_refresh.set_tooltip_text(Some("Обновить размер кеша"));
        cache_refresh.update_property(&[gtk::accessible::Property::Label("Обновить размер кеша")]);
        cache_refresh.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                if state.ready.get() && !state.cache_busy.get() {
                    (state.callbacks.reload_stats)();
                }
            }
        });
        cache_usage.add_suffix(&cache_refresh);
        storage.add(&cache_usage);
        let operation_status = adw::ActionRow::builder()
            .title("Состояние операции")
            .title_lines(0)
            .subtitle_lines(0)
            .use_markup(false)
            .visible(false)
            .build();
        operation_status.set_widget_name("preferences-operation-status");
        operation_status.set_focusable(true);
        operation_status.set_activatable(false);
        storage.add(&operation_status);
        let clear_row = adw::ActionRow::builder()
            .title("Очистка кеша")
            .subtitle("Удаляет только сохранённые данные и обложки. Вход и текущее воспроизведение сохраняются.")
            .title_lines(0)
            .subtitle_lines(0)
            .use_markup(false)
            .build();
        let cache_clear = gtk::Button::with_label("Очистить кеш");
        cache_clear.set_widget_name("preferences-cache-clear");
        cache_clear.set_valign(gtk::Align::Center);
        cache_clear.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                if state.ready.get() && !state.cache_busy.get() {
                    (state.callbacks.clear_cache)();
                }
            }
        });
        clear_row.add_suffix(&cache_clear);
        storage.add(&clear_row);
        page.add(&storage);
        dialog.add(&page);
        let message_reveal = Rc::new(MessageReveal::default());
        dialog.connect_map({
            let status = operation_status.downgrade();
            let reveal = Rc::clone(&message_reveal);
            move |_| {
                if reveal.pending.replace(false) {
                    reveal_message(status.clone(), Rc::clone(&reveal));
                }
            }
        });
        let result = Self {
            dialog,
            state,
            page,
            scheme,
            remember,
            session,
            session_retry,
            session_spinner,
            cache_enabled,
            cache_limit,
            cache_usage,
            cache_refresh,
            cache_clear,
            cache_spinner,
            operation_status,
            last_message: RefCell::new(None),
            message_reveal,
        };
        result.update(snapshot);
        result
    }

    /// A snapshot render never persists values or dispatches callbacks.
    pub fn update(&self, snapshot: &PreferencesSnapshot) {
        self.state.updating.set(true);
        self.state.values.replace(snapshot.values.clone());
        self.state.ready.set(snapshot.settings_ready);
        self.state.storage_supported.set(snapshot.storage_supported);
        self.state.session_busy.set(snapshot.session_busy);
        self.state.cache_busy.set(snapshot.cache_busy);
        self.state.can_retry_session.set(snapshot.can_retry_session);
        let ready = snapshot.settings_ready;
        self.scheme
            .set_selected(appearance_index(snapshot.values.appearance));
        self.scheme.set_sensitive(ready);
        self.remember.set_active(snapshot.values.remember_login);
        self.remember.set_sensitive(
            ready
                && !snapshot.session_busy
                && (snapshot.storage_supported || snapshot.values.remember_login),
        );
        self.remember.set_subtitle(if snapshot.storage_supported {
            "Защищённое хранилище восстанавливает вход после перезапуска. Отключение удаляет сохранённый вход, оставляя текущий сеанс открытым."
        } else if snapshot.values.remember_login {
            "Хранилище недоступно: текущий вход остаётся только в памяти. Можно отключить запоминание; завершение удаления будет показано отдельно."
        } else {
            "Хранилище недоступно. Запоминание новых входов выключено, текущий сеанс остаётся в памяти."
        });
        self.session.set_subtitle(&snapshot.session_status);
        self.session_spinner.set_spinning(snapshot.session_busy);
        self.session_spinner.set_visible(snapshot.session_busy);
        self.session_retry.set_visible(snapshot.can_retry_session);
        self.session_retry
            .set_sensitive(!snapshot.session_busy && snapshot.can_retry_session);
        self.cache_enabled.set_active(snapshot.values.cache_enabled);
        self.cache_enabled
            .set_sensitive(ready && !snapshot.cache_busy);
        self.cache_enabled.set_subtitle(if snapshot.values.cache_enabled {
            "Повторно использует загруженные метаданные и обложки в пределах указанного объёма."
        } else {
            "Чтение и новые записи выключены. Сохранённые данные остаются до явной очистки кеша."
        });
        self.cache_limit
            .set_value(snapshot.values.cache_limit_bytes as f64 / MIB as f64);
        self.cache_limit
            .set_sensitive(ready && !snapshot.cache_busy);
        self.cache_usage.set_subtitle(&match snapshot.cache_stats {
            Some(stats) => usage_text(stats),
            None if snapshot.cache_busy => "Обновляем сведения о кеше…".into(),
            None => "Размер пока не проверен.".into(),
        });
        self.cache_refresh
            .set_sensitive(ready && !snapshot.cache_busy);
        self.cache_clear
            .set_sensitive(ready && !snapshot.cache_busy);
        self.cache_clear.set_label(if snapshot.cache_busy {
            "Подождите…"
        } else {
            "Очистить кеш"
        });
        self.cache_spinner.set_spinning(snapshot.cache_busy);
        self.cache_spinner.set_visible(snapshot.cache_busy);
        let description = if snapshot.settings_ready {
            "Изменения сохраняются между запусками Karin."
        } else {
            "Настройки временно недоступны до завершения текущей операции."
        };
        self.page.set_description(description);
        let message = snapshot
            .message
            .as_deref()
            .filter(|message| !message.is_empty())
            .map(str::to_owned);
        let message_changed = self.last_message.borrow().as_ref() != message.as_ref();
        if message_changed {
            self.last_message.replace(message.clone());
            self.message_reveal
                .serial
                .set(self.message_reveal.serial.get().wrapping_add(1));
            self.message_reveal
                .pending
                .set(message.is_some() && !snapshot.cache_busy && !snapshot.session_busy);
        }
        self.operation_status
            .set_subtitle(message.as_deref().unwrap_or(""));
        self.operation_status.set_visible(message.is_some());
        self.state.updating.set(false);
        if self.dialog.is_mapped() && self.message_reveal.pending.replace(false) {
            reveal_message(
                self.operation_status.downgrade(),
                Rc::clone(&self.message_reveal),
            );
        }
    }

    pub fn present(&self, window: &adw::ApplicationWindow) {
        self.dialog.present(Some(window));
    }
}

fn reveal_message(row: gtk::glib::WeakRef<adw::ActionRow>, reveal: Rc<MessageReveal>) {
    let serial = reveal.serial.get();
    gtk::glib::idle_add_local_once(move || {
        if reveal.serial.get() != serial {
            return;
        }
        if let Some(row) = row.upgrade() {
            if row.is_mapped() && row.is_visible() {
                row.grab_focus();
            } else if row.is_visible() {
                // A close between scheduling and dispatch must not consume a
                // message that has not yet been shown on the next presentation.
                reveal.pending.set(true);
            }
        }
    });
}

fn appearance_index(appearance: Appearance) -> u32 {
    match appearance {
        Appearance::System => 0,
        Appearance::Light => 1,
        Appearance::Dark => 2,
    }
}

fn remember_change_allowed(current: bool, next: bool, supported: bool) -> bool {
    current != next && (supported || !next)
}

fn appearance_from_index(index: u32) -> Appearance {
    match index {
        1 => Appearance::Light,
        2 => Appearance::Dark,
        _ => Appearance::System,
    }
}

fn usage_text(stats: CacheStats) -> String {
    let size = if stats.bytes >= 1024 * MIB {
        format!("{:.1} ГиБ", stats.bytes as f64 / (1024 * MIB) as f64)
    } else if stats.bytes >= MIB {
        format!("{:.1} МиБ", stats.bytes as f64 / MIB as f64)
    } else if stats.bytes >= 1024 {
        format!("{:.1} КиБ", stats.bytes as f64 / 1024.0)
    } else {
        format!("{} Б", stats.bytes)
    }
    .replace('.', ",");
    let word = if (11..=14).contains(&(stats.entries % 100)) {
        "записей"
    } else {
        match stats.entries % 10 {
            1 => "запись",
            2..=4 => "записи",
            _ => "записей",
        }
    };
    format!("{size} · {} {word}", stats.entries)
}

#[cfg(test)]
mod tests;
