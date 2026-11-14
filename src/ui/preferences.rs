// SPDX-License-Identifier: GPL-3.0-or-later
//! Native persistent preferences. The owner performs all storage and session work.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;

use crate::{
    cache::CacheStats,
    settings::{Appearance, CacheLimit, Preferences, UiStyle},
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
    pub can_retry_cache: bool,
    /// Friendly, non-secret status supplied by the application coordinator.
    pub message: Option<String>,
}

#[derive(Clone)]
pub(super) struct PreferencesCallbacks {
    pub changed: Rc<dyn Fn(Preferences)>,
    pub clear_cache: Rc<dyn Fn()>,
    pub retry_session: Rc<dyn Fn()>,
    pub reload_stats: Rc<dyn Fn()>,
    pub retry_cache: Rc<dyn Fn()>,
}

struct State {
    values: RefCell<Preferences>,
    updating: Cell<bool>,
    ready: Cell<bool>,
    storage_supported: Cell<bool>,
    session_busy: Cell<bool>,
    cache_busy: Cell<bool>,
    can_retry_session: Cell<bool>,
    can_retry_cache: Cell<bool>,
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
        let previous = self.values.borrow().clone();
        let mut next = previous.clone();
        update(&mut next);
        if next == previous {
            return;
        }
        self.values.replace(next.clone());
        (self.callbacks.changed)(next);
    }
}

pub(super) struct PreferencesView {
    pub dialog: adw::PreferencesDialog,
    state: Rc<State>,
    page: adw::PreferencesPage,
    style_system: gtk::CheckButton,
    style_karin: gtk::CheckButton,
    system_style_row: adw::ActionRow,
    scheme: adw::ComboRow,
    remember: adw::SwitchRow,
    session: adw::ActionRow,
    session_retry: gtk::Button,
    session_spinner: gtk::Spinner,
    cache_enabled: adw::SwitchRow,
    cache_limit: gtk::Scale,
    cache_limit_label: gtk::Label,
    cache_usage: adw::ActionRow,
    cache_refresh: gtk::Button,
    cache_clear: gtk::Button,
    cache_spinner: gtk::Spinner,
    operation_status: adw::ActionRow,
    cache_retry: gtk::Button,
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
            can_retry_cache: Cell::new(false),
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
        let styles = adw::PreferencesGroup::builder()
            .title("Стиль интерфейса")
            .description("Оформление не изменяет музыкальную библиотеку и воспроизведение.")
            .build();
        let system_style_row = adw::ActionRow::builder()
            .title("Системный")
            .subtitle("Нативное оформление GTK и Libadwaita.")
            .title_lines(0)
            .subtitle_lines(0)
            .use_markup(false)
            .activatable(true)
            .build();
        system_style_row.set_widget_name("preferences-style-system-row");
        let style_system = gtk::CheckButton::new();
        style_system.set_widget_name("preferences-style-system");
        style_system.set_valign(gtk::Align::Center);
        style_system.update_property(&[gtk::accessible::Property::Label(
            "Стиль интерфейса: системный",
        )]);
        style_system.connect_toggled({
            let state = Rc::clone(&state);
            move |button| {
                if button.is_active() {
                    state.change(|values| values.ui_style = UiStyle::System);
                }
            }
        });
        system_style_row.add_prefix(&style_system);
        system_style_row.set_activatable_widget(Some(&style_system));
        // An older/future persisted Karin choice is rendered honestly as a
        // System fallback. Explicitly activating this row clears that request.
        system_style_row.connect_activated({
            let state = Rc::clone(&state);
            move |_| state.change(|values| values.ui_style = UiStyle::System)
        });
        styles.add(&system_style_row);
        let karin_style_row = adw::ActionRow::builder()
            .title("Karin")
            .subtitle("В разработке — новый дизайн ещё не интегрирован.")
            .title_lines(0)
            .subtitle_lines(0)
            .use_markup(false)
            .sensitive(false)
            .build();
        karin_style_row.set_widget_name("preferences-style-karin-row");
        let style_karin = gtk::CheckButton::new();
        style_karin.set_widget_name("preferences-style-karin");
        style_karin.set_group(Some(&style_system));
        style_karin.set_sensitive(false);
        style_karin.set_valign(gtk::Align::Center);
        style_karin.update_property(&[
            gtk::accessible::Property::Label("Стиль интерфейса: Karin"),
            gtk::accessible::Property::Description(
                "В разработке. Материалы дизайна ещё не интегрированы.",
            ),
        ]);
        karin_style_row.add_prefix(&style_karin);
        styles.add(&karin_style_row);
        page.add(&styles);
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
            .title("Кеширование")
            .description("Данные, обложки и аудио, допускаемое правилами HTTP-кеша. Доступ к музыке проверяется перед повторным воспроизведением.")
            .build();
        let cache_enabled = adw::SwitchRow::builder()
            .title("Использовать кеш")
            .title_lines(0)
            .subtitle_lines(0)
            .use_markup(false)
            .build();
        cache_enabled.set_widget_name("preferences-cache-enabled");
        cache_enabled.connect_active_notify({
            let state = Rc::clone(&state);
            move |row| {
                state.change(|values| values.cache_enabled = row.is_active());
            }
        });
        storage.add(&cache_enabled);
        let limit_row = adw::PreferencesRow::new();
        limit_row.set_title("Лимит кеша");
        let limit_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        limit_box.set_margin_start(12);
        limit_box.set_margin_end(12);
        limit_box.set_margin_top(12);
        limit_box.set_margin_bottom(8);
        let limit_heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let limit_title = gtk::Label::new(Some("Лимит кеша"));
        limit_title.set_xalign(0.0);
        limit_title.set_hexpand(true);
        let cache_limit_label = gtk::Label::new(Some(CacheLimit::default().label()));
        cache_limit_label.set_widget_name("preferences-cache-limit-value");
        cache_limit_label.add_css_class("dim-label");
        limit_heading.append(&limit_title);
        limit_heading.append(&cache_limit_label);
        limit_box.append(&limit_heading);
        let cache_limit = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 5.0, 1.0);
        cache_limit.set_widget_name("preferences-cache-limit");
        cache_limit.set_accessible_role(gtk::AccessibleRole::Slider);
        cache_limit.set_hexpand(true);
        cache_limit.set_draw_value(false);
        cache_limit.set_digits(0);
        cache_limit.set_round_digits(0);
        cache_limit.set_format_value_func(|_, value| {
            limit_from_scale(value).unwrap_or_default().label().into()
        });
        cache_limit.update_property(&[
            gtk::accessible::Property::Label("Лимит кеша"),
            gtk::accessible::Property::Description(
                "Общий размер кеша Karin. Шесть вариантов от 500 МБ до без ограничений.",
            ),
        ]);
        for limit in CacheLimit::ALL {
            cache_limit.add_mark(f64::from(limit.index()), gtk::PositionType::Bottom, None);
        }
        cache_limit.connect_value_changed({
            let state = Rc::clone(&state);
            let label = cache_limit_label.clone();
            move |scale| {
                if state.updating.get() {
                    return;
                }
                let limit =
                    limit_from_scale(scale.value()).unwrap_or(state.values.borrow().cache_limit);
                let value = f64::from(limit.index());
                if scale.value() != value {
                    scale.set_value(value);
                    return;
                }
                label.set_label(limit.label());
                scale.update_property(&[gtk::accessible::Property::ValueText(limit.label())]);
                state.change(|values| values.cache_limit = limit);
            }
        });
        limit_box.append(&cache_limit);
        limit_row.set_child(Some(&limit_box));
        storage.add(&limit_row);
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
        let cache_retry = gtk::Button::with_label("Повторить");
        cache_retry.set_widget_name("preferences-cache-retry");
        cache_retry.set_valign(gtk::Align::Center);
        cache_retry.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                if state.can_retry_cache.get() && !state.cache_busy.get() {
                    (state.callbacks.retry_cache)();
                }
            }
        });
        operation_status.add_suffix(&cache_retry);
        storage.add(&operation_status);
        let clear_row = adw::ActionRow::builder()
            .title("Очистка кеша")
            .subtitle("Освобождает занятое место. Вход и текущее воспроизведение сохраняются.")
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
            style_system,
            style_karin,
            system_style_row,
            scheme,
            remember,
            session,
            session_retry,
            session_spinner,
            cache_enabled,
            cache_limit,
            cache_limit_label,
            cache_usage,
            cache_refresh,
            cache_clear,
            cache_spinner,
            operation_status,
            cache_retry,
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
        self.state.can_retry_cache.set(snapshot.can_retry_cache);
        let ready = snapshot.settings_ready;
        self.style_system.set_active(true);
        self.style_system.set_sensitive(ready);
        self.system_style_row.set_sensitive(ready);
        self.system_style_row.set_subtitle(if snapshot.values.ui_style == UiStyle::Karin {
            "Karin пока недоступен. Используется системный стиль; выберите его, чтобы сохранить этот выбор."
        } else {
            "Нативное оформление GTK и Libadwaita."
        });
        self.style_karin.set_active(false);
        self.style_karin.set_sensitive(false);
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
        self.cache_enabled.set_sensitive(ready);
        self.cache_enabled.set_subtitle(if snapshot.values.cache_enabled {
            "Повторно использует загруженные данные в пределах общего лимита."
        } else {
            "Чтение и новые записи выключены. Сохранённые данные остаются до явной очистки кеша."
        });
        self.cache_limit
            .set_value(f64::from(snapshot.values.cache_limit.index()));
        self.cache_limit_label
            .set_label(snapshot.values.cache_limit.label());
        self.cache_limit
            .update_property(&[gtk::accessible::Property::ValueText(
                snapshot.values.cache_limit.label(),
            )]);
        self.cache_limit.set_sensitive(ready);
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
        self.cache_retry.set_visible(snapshot.can_retry_cache);
        self.cache_retry
            .set_sensitive(snapshot.can_retry_cache && !snapshot.cache_busy);
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

fn limit_from_scale(value: f64) -> Option<CacheLimit> {
    if !value.is_finite() {
        return None;
    }
    CacheLimit::from_index(value.round().clamp(0.0, 5.0) as u32)
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

/// Reuse the Preferences input fixture's viewport reveal and actual native hit
/// checks when an application-level test activates a control in this dialog.
#[cfg(test)]
pub(crate) fn click_native_for_test(
    window: &adw::ApplicationWindow,
    id: &str,
    widget: &impl IsA<gtk::Widget>,
) {
    tests::click(window, id, widget);
}
