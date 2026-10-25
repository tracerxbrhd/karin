// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit local-media developer diagnostics; no service stream or account UI.

use std::{cell::RefCell, rc::Rc, time::Duration};

use adw::prelude::*;
use gtk::{gio, glib};

use crate::playback::{
    DiagnosticMedia, PlaybackController, PlaybackError, PlaybackOutput, PlaybackSnapshot,
    PlaybackState,
};

pub(super) struct DeveloperPlayback {
    pub group: adw::PreferencesGroup,
    pub prepare: gtk::Button,
    play: gtk::Button,
    state: Rc<RefCell<PanelState>>,
}

struct Session {
    controller: PlaybackController,
    output: PlaybackOutput,
    // The generated file lives for the entire session, including cleanup.
    _media: DiagnosticMedia,
}

#[derive(Default)]
struct PanelState {
    session: Option<Session>,
    preparing: bool,
    closed: bool,
    command_error: Option<PlaybackError>,
}

#[derive(Clone)]
struct Widgets {
    status: gtk::Label,
    position: gtk::Label,
    output: gtk::DropDown,
    prepare: gtk::Button,
    play: gtk::Button,
    stop: gtk::Button,
    seek: gtk::Scale,
    volume: gtk::Scale,
}

impl DeveloperPlayback {
    pub fn new(app: &adw::Application, window: &adw::ApplicationWindow) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title("Проигрыватель · для разработчиков")
            .description("Локальный синтетический WAV, 8 секунд. Не музыка из Яндекс Музыки.")
            .build();
        let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
        card.add_css_class("card");
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        super::margins(&content, 16);
        card.append(&content);
        group.add(&card);
        let source = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let output_label = gtk::Label::new(Some("Вывод"));
        let output = gtk::DropDown::from_strings(&["Без звука", "Система"]);
        output.set_tooltip_text(Some("Вывод для следующей подготовки файла"));
        output.update_property(&[gtk::accessible::Property::Label("Аудиовыход")]);
        let prepare = gtk::Button::with_label("Подготовить");
        prepare.set_hexpand(true);
        prepare.set_halign(gtk::Align::End);
        prepare.set_tooltip_text(Some("Создать локальный тестовый WAV без запуска звука"));
        source.append(&output_label);
        source.append(&output);
        source.append(&prepare);
        content.append(&source);
        let status = super::label("Подготовьте локальный тестовый файл", "");
        status.set_widget_name("playback-status");
        content.append(&status);
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let play = gtk::Button::with_label("Воспроизвести");
        play.add_css_class("suggested-action");
        let stop = gtk::Button::with_label("Остановить");
        controls.append(&play);
        controls.append(&stop);
        content.append(&controls);
        let position = super::label("0:00 / —", "dim-label");
        let seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 8.0, 0.1);
        seek.set_draw_value(false);
        seek.update_property(&[gtk::accessible::Property::Label("Позиция воспроизведения")]);
        seek.set_tooltip_text(Some("Позиция в локальном тестовом WAV"));
        content.append(&position);
        content.append(&seek);
        let volume_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let volume_label = gtk::Label::new(Some("Громкость"));
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.05);
        volume.set_hexpand(true);
        volume.set_draw_value(false);
        volume.set_value(0.5);
        volume.update_property(&[gtk::accessible::Property::Label("Громкость")]);
        volume_row.append(&volume_label);
        volume_row.append(&volume);
        content.append(&volume_row);

        let widgets = Widgets {
            status,
            position,
            output,
            prepare: prepare.clone(),
            play,
            stop,
            seek,
            volume,
        };
        let state = Rc::new(RefCell::new(PanelState::default()));
        render(&state.borrow(), &widgets);
        {
            let state = state.clone();
            let app = app.downgrade();
            let weak_group = group.downgrade();
            let output = widgets.output.downgrade();
            prepare.connect_clicked(move |_| {
                let (Some(app), Some(output)) = (app.upgrade(), output.upgrade()) else {
                    return;
                };
                let mut model = state.borrow_mut();
                if model.preparing || model.closed {
                    return;
                }
                model.preparing = true;
                model.command_error = None;
                let old = model.session.take();
                drop(model);
                let output = if output.selected() == 1 {
                    PlaybackOutput::System
                } else {
                    PlaybackOutput::Fakesink
                };
                let hold = app.hold();
                let state = state.clone();
                let weak_group = weak_group.clone();
                glib::spawn_future_local(async move {
                    let _hold_until_cleanup = hold;
                    if let Some(old) = old {
                        retire(old).await;
                    }
                    if state.borrow().closed || weak_group.upgrade().is_none() {
                        return;
                    }
                    let created = gio::spawn_blocking(move || {
                        let media = DiagnosticMedia::create(Duration::from_secs(8))?;
                        let controller = PlaybackController::new(output)?;
                        controller.prepare(media.source())?;
                        Ok::<_, PlaybackError>(Session {
                            controller,
                            output,
                            _media: media,
                        })
                    })
                    .await
                    .unwrap_or(Err(PlaybackError::WorkerUnavailable));
                    let closed = state.borrow().closed || weak_group.upgrade().is_none();
                    if closed {
                        if let Ok(session) = created {
                            retire(session).await;
                        }
                        return;
                    }
                    let mut state = state.borrow_mut();
                    state.preparing = false;
                    match created {
                        Ok(session) => state.session = Some(session),
                        Err(error) => state.command_error = Some(error),
                    }
                });
            });
        }
        {
            let state = state.clone();
            widgets.play.connect_clicked(move |_| {
                command(&state, |controller| match controller.snapshot().state {
                    PlaybackState::Playing => controller.pause(),
                    PlaybackState::Paused => controller.resume(),
                    _ => controller.play(),
                });
            });
        }
        {
            let state = state.clone();
            widgets
                .stop
                .connect_clicked(move |_| command(&state, PlaybackController::stop));
        }
        {
            let state = state.clone();
            widgets.volume.connect_change_value(move |_, _, value| {
                command(&state, |controller| controller.set_volume(value));
                glib::Propagation::Proceed
            });
        }
        {
            let state = state.clone();
            widgets.seek.connect_change_value(move |_, _, value| {
                if value.is_finite() && value >= 0.0 {
                    command(&state, |controller| {
                        controller.seek(Duration::from_secs_f64(value))
                    });
                }
                glib::Propagation::Proceed
            });
        }
        {
            // The timer holds no GTK objects strongly. Destroying the window
            // cancels polling and leaves only the bounded worker cleanup hold.
            let weak_group = group.downgrade();
            let state = state.clone();
            let widgets = widgets.downgrade();
            glib::timeout_add_local(Duration::from_millis(80), move || {
                if state.borrow().closed || weak_group.upgrade().is_none() {
                    return glib::ControlFlow::Break;
                }
                if let Some(widgets) = widgets.upgrade() {
                    render(&state.borrow(), &widgets);
                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                }
            });
        }
        {
            let state = state.clone();
            let app = app.downgrade();
            window.connect_close_request(move |_| {
                let mut state = state.borrow_mut();
                state.closed = true;
                if let Some(session) = state.session.take() {
                    // Request cancellation before the window disappears. Keep
                    // the main context alive until the worker releases playbin.
                    session.controller.shutdown();
                    if let Some(app) = app.upgrade() {
                        let hold = app.hold();
                        glib::spawn_future_local(async move {
                            let _hold_until_cleanup = hold;
                            retire(session).await;
                        });
                    }
                }
                glib::Propagation::Proceed
            });
        }
        Self {
            group,
            prepare,
            play: widgets.play,
            state,
        }
    }

    /// Used by the structural smoke test to exercise the real GTK entry point.
    pub fn prepare_for_smoke(&self) {
        self.prepare.emit_clicked();
    }

    pub fn snapshot(&self) -> Option<PlaybackSnapshot> {
        self.state
            .borrow()
            .session
            .as_ref()
            .map(|s| s.controller.snapshot())
    }

    pub fn play_for_smoke(&self) -> Result<(), PlaybackError> {
        if self.state.borrow().session.is_none() {
            return Err(PlaybackError::InvalidState);
        }
        self.play.emit_clicked();
        Ok(())
    }

    pub fn cleanup_observer(&self) -> Option<PlaybackController> {
        self.state
            .borrow()
            .session
            .as_ref()
            .map(|s| s.controller.clone())
    }
}

// All asynchronous UI work upgrades these weak references only for rendering.
struct WeakWidgets {
    status: glib::WeakRef<gtk::Label>,
    position: glib::WeakRef<gtk::Label>,
    output: glib::WeakRef<gtk::DropDown>,
    prepare: glib::WeakRef<gtk::Button>,
    play: glib::WeakRef<gtk::Button>,
    stop: glib::WeakRef<gtk::Button>,
    seek: glib::WeakRef<gtk::Scale>,
    volume: glib::WeakRef<gtk::Scale>,
}

impl Widgets {
    fn downgrade(&self) -> WeakWidgets {
        WeakWidgets {
            status: self.status.downgrade(),
            position: self.position.downgrade(),
            output: self.output.downgrade(),
            prepare: self.prepare.downgrade(),
            play: self.play.downgrade(),
            stop: self.stop.downgrade(),
            seek: self.seek.downgrade(),
            volume: self.volume.downgrade(),
        }
    }
}

impl WeakWidgets {
    fn upgrade(&self) -> Option<Widgets> {
        Some(Widgets {
            status: self.status.upgrade()?,
            position: self.position.upgrade()?,
            output: self.output.upgrade()?,
            prepare: self.prepare.upgrade()?,
            play: self.play.upgrade()?,
            stop: self.stop.upgrade()?,
            seek: self.seek.upgrade()?,
            volume: self.volume.upgrade()?,
        })
    }
}

fn command(
    state: &Rc<RefCell<PanelState>>,
    action: impl FnOnce(&PlaybackController) -> Result<(), PlaybackError>,
) {
    let mut state = state.borrow_mut();
    state.command_error = state
        .session
        .as_ref()
        .and_then(|session| action(&session.controller).err());
}

fn render(state: &PanelState, widgets: &Widgets) {
    let snapshot = state
        .session
        .as_ref()
        .map(|session| session.controller.snapshot());
    let current = snapshot
        .as_ref()
        .map_or(PlaybackState::Idle, |value| value.state);
    let status = if state.preparing {
        "Готовим локальный WAV…"
    } else {
        match current {
            PlaybackState::Idle => "Подготовьте локальный тестовый файл",
            PlaybackState::Loading => "Загружаем локальный WAV…",
            PlaybackState::Ready => "Файл готов · воспроизведение не запущено",
            PlaybackState::Playing => "Воспроизведение локального WAV",
            PlaybackState::Paused => "Пауза",
            PlaybackState::Stopped => "Остановлено · можно запустить снова",
            PlaybackState::Ended => "Файл воспроизведён до конца",
            PlaybackState::Error => "Ошибка локального воспроизведения",
        }
    };
    let engine_error = snapshot.as_ref().and_then(|s| s.last_error);
    let error = if current == PlaybackState::Error {
        engine_error.or(state.command_error)
    } else {
        state.command_error.or(engine_error)
    };
    let active_output = state.session.as_ref().map(|session| match session.output {
        PlaybackOutput::Fakesink => "Без звука",
        PlaybackOutput::System => "Системный аудиовыход",
    });
    widgets.status.set_text(&error.map_or_else(
        || {
            active_output.map_or_else(
                || status.to_owned(),
                |output| format!("{status} · {output}"),
            )
        },
        |error| format!("{status}. {}", error_text(error)),
    ));
    widgets
        .prepare
        .set_sensitive(!state.preparing && !state.closed);
    widgets.output.set_sensitive(
        !state.preparing
            && !matches!(
                current,
                PlaybackState::Loading | PlaybackState::Playing | PlaybackState::Paused
            ),
    );
    widgets.play.set_label(match current {
        PlaybackState::Playing => "Пауза",
        PlaybackState::Paused => "Продолжить",
        _ => "Воспроизвести",
    });
    widgets.play.set_sensitive(
        !state.preparing
            && matches!(
                current,
                PlaybackState::Ready
                    | PlaybackState::Playing
                    | PlaybackState::Paused
                    | PlaybackState::Stopped
                    | PlaybackState::Ended
            ),
    );
    widgets.stop.set_sensitive(
        !state.preparing
            && matches!(
                current,
                PlaybackState::Loading
                    | PlaybackState::Ready
                    | PlaybackState::Playing
                    | PlaybackState::Paused
            ),
    );
    widgets
        .volume
        .set_sensitive(snapshot.is_some() && !state.preparing);
    widgets.seek.set_sensitive(
        snapshot.as_ref().is_some_and(|s| s.seekable)
            && matches!(
                current,
                PlaybackState::Ready | PlaybackState::Playing | PlaybackState::Paused
            ),
    );
    let position = snapshot
        .as_ref()
        .and_then(|s| s.position)
        .unwrap_or_default();
    let duration = snapshot.as_ref().and_then(|s| s.duration);
    widgets.position.set_text(&format!(
        "{} / {}",
        time_text(position),
        duration.map_or_else(|| "—".into(), time_text)
    ));
    if let Some(duration) = duration {
        widgets.seek.set_range(0.0, duration.as_secs_f64().max(0.1));
    }
    widgets.seek.set_value(position.as_secs_f64());
    if let Some(snapshot) = snapshot {
        widgets.volume.set_value(snapshot.volume);
    }
}

fn time_text(value: Duration) -> String {
    format!("{}:{:02}", value.as_secs() / 60, value.as_secs() % 60)
}

fn error_text(error: PlaybackError) -> &'static str {
    match error {
        PlaybackError::QueueFull => "Слишком много команд; повторите действие.",
        PlaybackError::NotSeekable | PlaybackError::SeekFailed | PlaybackError::InvalidSeek => {
            "Перемотка сейчас недоступна."
        }
        PlaybackError::InvalidState => "Дождитесь готовности файла и повторите действие.",
        PlaybackError::InvalidVolume => "Громкость должна быть от 0 до 100%.",
        PlaybackError::SourceUnavailable | PlaybackError::DiagnosticMedia => {
            "Не удалось подготовить локальный тестовый файл."
        }
        PlaybackError::Pipeline | PlaybackError::Initialization => {
            "Проверьте установку GStreamer и аудиовыход."
        }
        PlaybackError::Playback => "Не удалось декодировать файл или открыть аудиовыход.",
        PlaybackError::Timeout | PlaybackError::StateChange => {
            "Аудиодвижок не завершил переход состояния."
        }
        PlaybackError::WorkerUnavailable | PlaybackError::ShuttingDown => {
            "Проигрыватель завершает работу; подготовьте файл снова."
        }
    }
}

async fn retire(session: Session) {
    session.controller.shutdown();
    let cleaned = gio::spawn_blocking(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(6);
        while session.controller.snapshot().worker_running && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        !session.controller.snapshot().worker_running
    })
    .await
    .unwrap_or(false);
    if !cleaned {
        eprintln!("Local playback cleanup was not confirmed before its deadline.");
    }
}
