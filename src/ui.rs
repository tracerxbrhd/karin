//! Native presentation layer. Network transport details never enter this module.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::{gio, glib};

use crate::{
    application::ConnectionService,
    audio::{self, AudioOutput},
    config,
    domain::{AccountAccess, ConnectionState, ServiceError},
    platform,
};

pub struct MainWindow {
    pub window: adw::ApplicationWindow,
    stack: adw::ViewStack,
    lifetime_refs: Vec<glib::WeakRef<gtk::Widget>>,
}

impl MainWindow {
    /// Verify the real widget tree and navigation without a network request.
    pub fn smoke_check(&self) -> bool {
        self.stack.set_visible_child_name("diagnostics");
        self.window.is_visible()
            && self.window.is_realized()
            && self.stack.visible_child_name().as_deref() == Some("diagnostics")
    }

    /// Close the actual window and return a deferred lifetime check. Checking
    /// after the main loop processes closure avoids retaining signal cycles.
    pub fn close_for_smoke(self) -> impl FnOnce() -> bool {
        let Self {
            window,
            stack,
            lifetime_refs,
        } = self;
        window.close();
        drop(window);
        drop(stack);
        move || lifetime_refs.iter().all(|weak| weak.upgrade().is_none())
    }
}

pub fn build(app: &adw::Application, service: ConnectionService) -> MainWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(config::APP_NAME)
        .default_width(920)
        .default_height(700)
        .width_request(360)
        .height_request(480)
        .build();
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Karin", "Яндекс Музыка")));
    let menu = gio::Menu::new();
    menu.append(Some("О Karin"), Some("app.about"));
    menu.append(Some("Выйти"), Some("app.quit"));
    header.pack_end(
        &gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Меню приложения")
            .menu_model(&menu)
            .build(),
    );
    toolbar.add_top_bar(&header);

    let stack = adw::ViewStack::new();
    let navigation = adw::ViewSwitcherBar::builder()
        .stack(&stack)
        .reveal(true)
        .build();
    toolbar.add_bottom_bar(&navigation);
    toolbar.set_content(Some(&stack));
    window.set_content(Some(&toolbar));

    let home = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let status = adw::StatusPage::builder()
        .icon_name("audio-x-generic-symbolic")
        .vexpand(true)
        .build();
    let check = gtk::Button::with_label("Проверить доступность");
    check.add_css_class("suggested-action");
    check.set_halign(gtk::Align::Center);
    let spinner = gtk::Spinner::builder()
        .halign(gtk::Align::Center)
        .visible(false)
        .build();
    let controls = gtk::Box::new(gtk::Orientation::Vertical, 12);
    controls.append(&spinner);
    controls.append(&check);
    status.set_child(Some(&controls));
    home.append(&status);
    stack
        .add_titled(&home, Some("home"), "Обзор")
        .set_icon_name(Some("go-home-symbolic"));

    let diagnostics = gtk::Box::new(gtk::Orientation::Vertical, 24);
    diagnostics.set_margin_top(24);
    diagnostics.set_margin_bottom(24);
    diagnostics.set_margin_start(18);
    diagnostics.set_margin_end(18);
    let title = gtk::Label::new(Some("Диагностика"));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    diagnostics.append(&title);

    let connection_group = adw::PreferencesGroup::builder()
        .title("Подключение к сервису")
        .description("Проверка без входа в аккаунт. Токены и пароли не запрашиваются.")
        .build();
    let connection_row = row("Яндекс Музыка", "Доступность ещё не проверена");
    connection_group.add(&connection_row);
    diagnostics.append(&connection_group);

    let audio_group = adw::PreferencesGroup::builder()
        .title("Проверка аудио")
        .description("Короткий синтетический сигнал. Это не музыка из Яндекс Музыки.")
        .build();
    let audio_row = row("Аудиодвижок", "Проверка ещё не выполнялась");
    audio_group.add(&audio_row);
    let silent = gtk::Button::with_label("Проверить без звука");
    let audible = gtk::Button::with_label("Прослушать тестовый сигнал");
    // A vertical layout remains usable on narrow windows and with large text.
    let audio_controls = gtk::Box::new(gtk::Orientation::Vertical, 8);
    audio_controls.append(&silent);
    audio_controls.append(&audible);
    audio_group.add(&audio_controls);
    diagnostics.append(&audio_group);

    let desktop_group = adw::PreferencesGroup::builder()
        .title("Рабочий стол")
        .description("Хранилище токенов и управление MPRIS ещё не подключены.")
        .build();
    let desktop_row = row("D-Bus и Secret Service", "Проверка ещё не выполнялась");
    let inspect = gtk::Button::with_label("Проверить окружение");
    desktop_group.add(&desktop_row);
    desktop_group.add(&inspect);
    desktop_group.add(&row(
        "Нативные библиотеки",
        &format!(
            "GTK {}.{}.{} · Libadwaita {}.{}.{} · GStreamer {}",
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version(),
            adw::major_version(),
            adw::minor_version(),
            adw::micro_version(),
            gstreamer::version_string()
        ),
    ));
    diagnostics.append(&desktop_group);
    let clamp = adw::Clamp::builder()
        .maximum_size(720)
        .child(&diagnostics)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
        .build();
    stack
        .add_titled(&scroll, Some("diagnostics"), "Диагностика")
        .set_icon_name(Some("applications-engineering-symbolic"));

    let state = Rc::new(RefCell::new(ConnectionState::NotChecked));
    render_connection(&state.borrow(), &status, &connection_row, &spinner, &check);
    {
        // Permanent signal handlers must not retain their own widget tree.
        // Workers may temporarily keep widgets alive until bounded cleanup.
        let app = app.downgrade();
        let status = status.downgrade();
        let connection_row = connection_row.downgrade();
        let spinner = spinner.downgrade();
        check.connect_clicked(move |button| {
            let (Some(app), Some(status), Some(connection_row), Some(spinner)) = (
                app.upgrade(),
                status.upgrade(),
                connection_row.upgrade(),
                spinner.upgrade(),
            ) else {
                return;
            };
            if !state.borrow_mut().begin_check() {
                return;
            }
            render_connection(&state.borrow(), &status, &connection_row, &spinner, button);
            let service = service.clone();
            let state = state.clone();
            let status = status.clone();
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
                render_connection(&state.borrow(), &status, &connection_row, &spinner, &button);
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
    let lifetime_refs = [
        window.upcast_ref::<gtk::Widget>(),
        stack.upcast_ref::<gtk::Widget>(),
        status.upcast_ref::<gtk::Widget>(),
        check.upcast_ref::<gtk::Widget>(),
        silent.upcast_ref::<gtk::Widget>(),
        audible.upcast_ref::<gtk::Widget>(),
        inspect.upcast_ref::<gtk::Widget>(),
    ]
    .into_iter()
    .map(|widget| widget.downgrade())
    .collect();
    MainWindow {
        window,
        stack,
        lifetime_refs,
    }
}

fn row(title: &str, subtitle: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
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
            "Karin ещё не авторизован. Можно проверить доступность сервиса без входа в аккаунт."
                .into(),
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
                AccountAccess::Anonymous => "API отвечает · вход не выполнен",
            };
            (
                title,
                format!(
                    "Получен ответ HTTP {}. Авторизация и воспроизведение музыки пока не подключены.{}",
                    report.http_status,
                    if report.service_available == Some(false) {
                        " Сервис сообщил о недоступности для текущего подключения."
                    } else {
                        ""
                    }
                ),
            )
        }
        ConnectionState::Failed(error) => (
            "Не удалось проверить подключение",
            format!("{error}. Можно повторить попытку."),
        ),
    }
}

fn render_connection(
    state: &ConnectionState,
    status: &adw::StatusPage,
    row: &adw::ActionRow,
    spinner: &gtk::Spinner,
    button: &gtk::Button,
) {
    let (title, description) = presentation(state);
    status.set_title(title);
    status.set_description(Some(&description));
    row.set_subtitle(&format!("{title}. {description}"));
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
        assert!(text.contains("пока не подключены"));
    }

    #[test]
    fn failure_offers_retry_and_loading_is_distinct() {
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
