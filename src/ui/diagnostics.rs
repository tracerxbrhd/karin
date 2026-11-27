// SPDX-License-Identifier: GPL-3.0-or-later
//! Native developer tools, constructed only by an explicit diagnostic or test mode.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::{gio, glib};

use super::{label, margins, playback};
use crate::{
    application::ConnectionService,
    audio::{self, AudioOutput},
    capabilities::{AuthenticationCapability, AuthenticationUnavailable},
    domain::{AccountAccess, ConnectionState, ServiceError},
    platform,
};

pub(super) struct DiagnosticsPage {
    pub scroll: gtk::ScrolledWindow,
    pub columns: gtk::Box,
    pub back: gtk::Button,
    pub player: playback::DeveloperPlayback,
    pub lifetime_refs: Vec<glib::WeakRef<gtk::Widget>>,
}

impl DiagnosticsPage {
    pub fn new(
        app: &adw::Application,
        window: &adw::ApplicationWindow,
        service: ConnectionService,
    ) -> Self {
        let (diagnostics, diagnostics_heading) = page(
            "Диагностика",
            "Проверки работают независимо от авторизации.",
        );
        let connection_group = adw::PreferencesGroup::builder()
            .title("Подключение к сервису")
            .build();
        let connection_row = row("Яндекс Музыка", "Доступность ещё не проверена");
        let check_diagnostics = gtk::Button::builder()
            .label("Проверить")
            .valign(gtk::Align::Center)
            .build();
        connection_row.add_suffix(&check_diagnostics);
        connection_group.add(&connection_row);
        let AuthenticationCapability::AuthenticationUnavailable(
            AuthenticationUnavailable::MusicGrantNotEstablished,
        ) = service.capabilities().authentication;
        let (login_title, login_description) = if cfg!(feature = "browser-session-auth") {
            (
                "Вход через Яндекс",
                "На стартовом экране нажмите «Войти через Яндекс». Доступ к аккаунту, библиотеке и поиску проверяется после входа. Воспроизведение проверяется отдельно для выбранного трека.",
            )
        } else {
            (
                "Минимальная сборка без входа",
                "В этой сборке нет WebKit и провайдера авторизации. Для проверки входа нужна обычная сборка Karin. Музыкальное аудио в этой сборке недоступно.",
            )
        };
        connection_group.add(&row(login_title, login_description));
        diagnostics.append(&connection_group);
        let player = playback::DeveloperPlayback::new(app, window);
        diagnostics.append(&player.group);

        let diagnostic_columns = gtk::Box::new(gtk::Orientation::Horizontal, 24);
        diagnostic_columns.set_homogeneous(true);
        diagnostic_columns.set_valign(gtk::Align::Start);
        let audio_group = adw::PreferencesGroup::builder()
            .title("Аудиодиагностика")
            .description("Синтетический сигнал, не музыка из сервиса.")
            .hexpand(true)
            .build();
        let audio_row = row("Результат", "Проверка ещё не выполнялась");
        audio_group.add(&audio_row);
        let silent = gtk::Button::builder()
            .label("Проверить")
            .valign(gtk::Align::Center)
            .build();
        let silent_row = row("Обработка аудио", "Без устройства вывода");
        silent_row.add_suffix(&silent);
        audio_group.add(&silent_row);
        let audible = gtk::Button::builder()
            .label("Прослушать")
            .valign(gtk::Align::Center)
            .build();
        let audible_row = row("Тестовый сигнал", "Негромкий синтетический звук");
        audible_row.add_suffix(&audible);
        audio_group.add(&audible_row);
        diagnostic_columns.append(&audio_group);

        let desktop_group = adw::PreferencesGroup::builder()
            .title("Рабочий стол")
            .description("Проверка без чтения сохранённых секретов.")
            .hexpand(true)
            .build();
        let desktop_row = row("D-Bus и Secret Service", "Проверка ещё не выполнялась");
        let inspect = gtk::Button::builder()
            .label("Проверить")
            .valign(gtk::Align::Center)
            .build();
        desktop_row.add_suffix(&inspect);
        desktop_group.add(&desktop_row);
        desktop_group.add(&row(
            "Интеграция",
            "Состояние защищённого хранения и запоминание входа доступны в настройках",
        ));
        diagnostic_columns.append(&desktop_group);
        diagnostics.append(&diagnostic_columns);
        let runtime_group = adw::PreferencesGroup::builder().title("Окружение").build();
        runtime_group.add(&row(
            "Нативные библиотеки",
            &format!(
                "GTK {}.{}.{} · Libadwaita {}.{}.{} · {}",
                gtk::major_version(),
                gtk::minor_version(),
                gtk::micro_version(),
                adw::major_version(),
                adw::minor_version(),
                adw::micro_version(),
                gstreamer::version_string()
            ),
        ));
        diagnostics.append(&runtime_group);

        // Keep the anonymous availability request in the technical diagnostics.
        let status = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let status_title = label("", "heading");
        let status_description = label("", "");
        let check = gtk::Button::with_label("Проверить доступность");
        check.set_halign(gtk::Align::Start);
        let spinner = gtk::Spinner::builder().visible(false).build();
        status.append(&status_title);
        status.append(&status_description);
        status.append(&check);
        status.append(&spinner);
        diagnostics.append(&status);
        // Both real controls trigger the same state machine and bounded request.
        check
            .bind_property("sensitive", &check_diagnostics, "sensitive")
            .sync_create()
            .build();
        let weak_check = check.downgrade();
        check_diagnostics.connect_clicked(move |_| {
            if let Some(check) = weak_check.upgrade() {
                check.emit_clicked();
            }
        });
        let state = Rc::new(RefCell::new(ConnectionState::NotChecked));
        render_connection(
            &state.borrow(),
            &status_title,
            &status_description,
            &connection_row,
            &spinner,
            &check,
        );
        {
            // Permanent signal handlers must not retain their own widget tree.
            // Workers may temporarily keep widgets alive until bounded cleanup.
            let app = app.downgrade();
            let status_title = status_title.downgrade();
            let status_description = status_description.downgrade();
            let connection_row = connection_row.downgrade();
            let spinner = spinner.downgrade();
            check.connect_clicked(move |button| {
                let (
                    Some(app),
                    Some(status_title),
                    Some(status_description),
                    Some(connection_row),
                    Some(spinner),
                ) = (
                    app.upgrade(),
                    status_title.upgrade(),
                    status_description.upgrade(),
                    connection_row.upgrade(),
                    spinner.upgrade(),
                )
                else {
                    return;
                };
                if !state.borrow_mut().begin_check() {
                    return;
                }
                render_connection(
                    &state.borrow(),
                    &status_title,
                    &status_description,
                    &connection_row,
                    &spinner,
                    button,
                );
                let service = service.clone();
                let state = state.clone();
                let status_title = status_title.clone();
                let status_description = status_description.clone();
                let connection_row = connection_row.clone();
                let spinner = spinner.clone();
                let button = button.clone();
                let hold = app.hold();
                glib::spawn_future_local(async move {
                    let _hold_until_cleanup = hold;
                    let result = gio::spawn_blocking(move || service.check_connection())
                        .await
                        .unwrap_or(Err(ServiceError::Network));
                    state.borrow_mut().complete(result);
                    render_connection(
                        &state.borrow(),
                        &status_title,
                        &status_description,
                        &connection_row,
                        &spinner,
                        &button,
                    );
                });
            });
        }
        connect_audio(app, &silent, &audible, &audio_row, AudioOutput::Fakesink);
        connect_audio(app, &audible, &silent, &audio_row, AudioOutput::System);
        {
            let app = app.downgrade();
            let desktop_row = desktop_row.downgrade();
            inspect.connect_clicked(move |button| {
                let (Some(app), Some(desktop_row)) = (app.upgrade(), desktop_row.upgrade()) else {
                    return;
                };
                button.set_sensitive(false);
                desktop_row.set_subtitle("Проверяем окружение…");
                let button = button.clone();
                let desktop_row = desktop_row.clone();
                let hold = app.hold();
                glib::spawn_future_local(async move {
                    let _hold_until_cleanup = hold;
                    match gio::spawn_blocking(platform::inspect_session).await {
                        Ok(report) => desktop_row.set_subtitle(&format!(
                            "Сеансовая шина: {} · Secret Service: {}. Секреты не читались.",
                            if report.session_bus {
                                "доступна"
                            } else {
                                "недоступна"
                            },
                            if report.secret_service_running {
                                "запущен"
                            } else {
                                "не обнаружен"
                            }
                        )),
                        Err(_) => desktop_row.set_subtitle("Не удалось проверить окружение"),
                    }
                    button.set_sensitive(true);
                });
            });
        }

        diagnostics_heading.set_widget_name("diagnostics-heading");
        let lifetime_refs = [
            status.upcast_ref::<gtk::Widget>(),
            check.upcast_ref::<gtk::Widget>(),
            silent.upcast_ref::<gtk::Widget>(),
            audible.upcast_ref::<gtk::Widget>(),
            inspect.upcast_ref::<gtk::Widget>(),
            player.group.upcast_ref::<gtk::Widget>(),
            player.prepare.upcast_ref::<gtk::Widget>(),
        ]
        .into_iter()
        .map(|widget| widget.downgrade())
        .collect();
        let back = gtk::Button::builder()
            .label("Назад")
            .tooltip_text("Вернуться в Karin")
            .action_name("win.overview")
            .visible(false)
            .build();
        Self {
            back,
            scroll: scroll_page(&diagnostics),
            columns: diagnostic_columns,
            player,
            lifetime_refs,
        }
    }
}

fn page(title: &str, subtitle: &str) -> (gtk::Box, gtk::Label) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 24);
    content.set_valign(gtk::Align::Start);
    margins(&content, 24);
    let heading = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let title = label(title, "title-1");
    heading.append(&title);
    heading.append(&label(subtitle, "dim-label"));
    content.append(&heading);
    (content, title)
}

fn scroll_page(content: &gtk::Box) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::builder()
        .maximum_size(1120)
        .tightening_threshold(760)
        .child(content)
        .build();
    let viewport = gtk::Viewport::builder()
        .scroll_to_focus(true)
        .child(&clamp)
        .build();
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&viewport)
        .build()
}

fn row(title: &str, subtitle: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .title_lines(0)
        .subtitle(subtitle)
        .subtitle_lines(0)
        .build()
}

fn connect_audio(
    app: &adw::Application,
    button: &gtk::Button,
    other: &gtk::Button,
    row: &adw::ActionRow,
    output: AudioOutput,
) {
    let app = app.downgrade();
    let other = other.downgrade();
    let row = row.downgrade();
    button.connect_clicked(move |button| {
        let (Some(app), Some(other), Some(row)) = (app.upgrade(), other.upgrade(), row.upgrade())
        else {
            return;
        };
        button.set_sensitive(false);
        other.set_sensitive(false);
        row.set_subtitle("Проверяем синтетический сигнал…");
        let button = button.clone();
        let other = other.clone();
        let row = row.clone();
        let hold = app.hold();
        glib::spawn_future_local(async move {
            let _hold_until_cleanup = hold;
            let message = match gio::spawn_blocking(move || audio::run_test_tone(output)).await {
                Ok(Ok(_)) => match output {
                    AudioOutput::Fakesink => {
                        "Проверка без звука завершена. Поток обработан до конца.".to_owned()
                    }
                    AudioOutput::System => {
                        "Сигнал завершён. Подтвердите слышимость самостоятельно.".to_owned()
                    }
                },
                Ok(Err(error)) => format!("Ошибка аудио: {error}"),
                Err(_) => "Не удалось завершить проверку аудио".to_owned(),
            };
            row.set_subtitle(&message);
            button.set_sensitive(true);
            other.set_sensitive(true);
        });
    });
}

fn presentation(state: &ConnectionState) -> (&'static str, String) {
    match state {
        ConnectionState::NotChecked => (
            "Подключение не проверено",
            "Эта проверка доступности сервиса не использует аккаунт.".into(),
        ),
        ConnectionState::Checking => (
            "Проверяем доступность…",
            "Окно остаётся доступным во время запроса.".into(),
        ),
        ConnectionState::Available(report) => {
            let title = match report.access {
                AccountAccess::AuthorizationRequired => "Требуется авторизация",
                AccountAccess::Forbidden => "Доступ запрещён",
                AccountAccess::Anonymous if report.service_available == Some(false) => {
                    "Сервис недоступен в этой среде"
                }
                AccountAccess::Anonymous => "API отвечает на анонимный запрос",
            };
            (
                title,
                format!(
                    "Получен ответ HTTP {}. Этот запрос выполнен без входа и не проверяет аккаунт или доступ к музыке.{}",
                    report.http_status,
                    if report.service_available == Some(false) {
                        " Сервис сообщил о недоступности для текущего подключения."
                    } else {
                        ""
                    }
                ),
            )
        }
        ConnectionState::Failed(error) => {
            let message = match error {
                ServiceError::Network => "Сервис недоступен. Проверьте сетевое подключение.".into(),
                ServiceError::Timeout => "Сервис не ответил вовремя.".into(),
                ServiceError::Http(status) => format!("Сервис вернул HTTP {status}."),
                ServiceError::InvalidResponse => "Сервис вернул неподдерживаемый ответ.".into(),
                ServiceError::ResponseTooLarge => {
                    "Ответ сервиса превышает допустимый размер.".into()
                }
                ServiceError::Configuration => {
                    "Не удалось подготовить подключение к сервису.".into()
                }
            };
            (
                "Не удалось проверить подключение",
                format!("{message} Можно повторить попытку."),
            )
        }
    }
}

fn render_connection(
    state: &ConnectionState,
    title: &gtk::Label,
    description: &gtk::Label,
    row: &adw::ActionRow,
    spinner: &gtk::Spinner,
    button: &gtk::Button,
) {
    let (heading, text) = presentation(state);
    title.set_label(heading);
    description.set_label(&text);
    row.set_subtitle(&format!("{heading}. {text}"));
    let loading = matches!(state, ConnectionState::Checking);
    spinner.set_visible(loading);
    spinner.set_spinning(loading);
    button.set_sensitive(!loading);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ConnectionReport;

    #[test]
    fn anonymous_response_is_never_presented_as_authenticated() {
        let state = ConnectionState::Available(ConnectionReport {
            access: AccountAccess::Anonymous,
            http_status: 200,
            service_available: Some(false),
        });
        let (title, text) = presentation(&state);
        assert_eq!(title, "Сервис недоступен в этой среде");
        assert!(text.contains("запрос выполнен без входа"));
    }

    #[test]
    fn failure_offers_retry_and_loading_is_distinct() {
        let error_text = presentation(&ConnectionState::Failed(ServiceError::Timeout)).1;
        assert!(error_text.contains("Сервис не ответил вовремя."));
        assert!(!error_text.contains(".."));
        assert!(
            presentation(&ConnectionState::Failed(ServiceError::Timeout))
                .1
                .contains("повторить")
        );
        assert_ne!(
            presentation(&ConnectionState::NotChecked).0,
            presentation(&ConnectionState::Checking).0
        );
    }
}
