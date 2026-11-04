// SPDX-License-Identifier: GPL-3.0-or-later
//! Native presentation evidence only. Safe injected states do not establish a
//! radio session, provider acknowledgement, actual Playing or audible Music.

use super::super::tests::{contains_text, pump_until};
use super::*;
use serde_json::json;
use std::{
    fs::OpenOptions,
    io::Write,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const TITLE: &str = "Karin synthetic Wave fixture";

struct WindowManager(Option<Child>);

impl WindowManager {
    fn new() -> Self {
        let present = Command::new("wmctrl")
            .arg("-m")
            .output()
            .is_ok_and(|output| output.status.success());
        let owned = (!present).then(|| {
            Command::new("openbox")
                .arg("--sm-disable")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("Wave Home maximize evidence requires a window manager")
        });
        let manager = Self(owned);
        pump_until(|| {
            Command::new("wmctrl")
                .arg("-m")
                .output()
                .is_ok_and(|output| output.status.success())
        });
        manager
    }
}

impl Drop for WindowManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn settle() {
    let deadline = Instant::now() + Duration::from_millis(180);
    pump_until(|| Instant::now() >= deadline);
}

fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut result = vec![widget.clone()];
    let mut child = widget.first_child();
    while let Some(current) = child {
        result.extend(descendants(&current));
        child = current.next_sibling();
    }
    result
}

fn window_id() -> String {
    let output = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", &format!("^{TITLE}$")])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();
    let ids: Vec<_> = output.lines().collect();
    assert_eq!(
        ids.len(),
        1,
        "the isolated fixture has exactly one main window"
    );
    assert!(ids[0].bytes().all(|byte| byte.is_ascii_digit()));
    ids[0].to_owned()
}

fn status(phase: RadioPhase, playback: Option<PlaybackState>) -> RadioStatus {
    RadioStatus {
        phase,
        playback,
        authenticated: true,
    }
}

fn reveal_wave(ui: &crate::ui::MainWindow, wave: &WaveParts) {
    settle();
    let scroll = wave
        .widget
        .ancestor(gtk::ScrolledWindow::static_type())
        .unwrap()
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
    let rect = wave.widget.compute_bounds(&scroll).unwrap();
    let adjustment = scroll.vadjustment();
    let target = (adjustment.value() + f64::from(rect.y()) - 12.0).clamp(
        adjustment.lower(),
        (adjustment.upper() - adjustment.page_size()).max(0.0),
    );
    adjustment.set_value(target);
    settle();
    pump_until(|| ui.verify_layout().is_ok());
    ui.verify_layout().unwrap();
}

fn assert_structure(ui: &crate::ui::MainWindow, wave: &WaveParts, state: RadioStatus) {
    assert_eq!(wave.widget.widget_name(), "music-wave-hero");
    assert!(wave.widget.has_css_class("card"));
    assert!(contains_text(wave.widget.upcast_ref(), "Моя волна"));
    assert!(contains_text(
        wave.widget.upcast_ref(),
        "Персональная музыка, которая подстраивается под ваш вкус."
    ));
    assert!(wave.primary.is_visible());
    assert_eq!(
        wave.cancel.is_visible(),
        state.phase == RadioPhase::Starting
    );
    assert_eq!(
        wave.cancel.is_sensitive(),
        state.phase == RadioPhase::Starting
    );
    let disabled = state.phase == RadioPhase::Disabled;
    assert_eq!(
        wave.experiment.text(),
        if disabled {
            "Функция проходит проверку"
        } else {
            "Предварительная версия"
        }
    );
    let transport = matches!(state.phase, RadioPhase::Active | RadioPhase::LoadingNext);
    let playing = state.playback == Some(PlaybackState::Playing);
    let resumable = matches!(
        state.playback,
        Some(PlaybackState::Paused | PlaybackState::Ready)
    );
    let busy = matches!(state.phase, RadioPhase::Starting | RadioPhase::Finishing)
        || (transport && !playing && !resumable);
    assert_eq!(wave.spinner.is_spinning(), busy);
    assert_eq!(wave.spinner.is_visible(), busy);
    let failed = matches!(state.phase, RadioPhase::Failed { .. });
    assert_eq!(wave.message.has_css_class("error"), failed);
    assert_eq!(wave.disclosure.is_visible(), failed);
    if failed {
        assert!(wave.details.text().starts_with("Этап: "));
        assert!(wave.details.text().contains(" · Причина: "));
        assert_eq!(wave.primary_content.label(), "Начать новую волну");
        assert!(
            !wave.cancel.is_visible(),
            "failure never reintroduces Finish"
        );
    } else {
        assert_eq!(wave.details.text(), "");
        assert!(!wave.disclosure.is_expanded());
        if transport && playing {
            assert_eq!(wave.primary_content.label(), "Пауза");
        } else if transport && resumable {
            assert_eq!(wave.primary_content.label(), "Продолжить");
        } else if !busy {
            assert_eq!(wave.primary_content.label(), "Слушать");
        }
    }
    assert_eq!(
        wave.primary.is_sensitive(),
        !disabled
            && state.authenticated
            && !busy
            && (matches!(state.phase, RadioPhase::Ready | RadioPhase::Failed { .. })
                || (transport && (playing || resumable)))
    );
    for widget in descendants(wave.widget.upcast_ref()) {
        if let Some(button) = widget.downcast_ref::<gtk::Button>() {
            assert!(
                !button
                    .tooltip_text()
                    .is_some_and(|t| t.contains("Завершить"))
            );
            assert!(!button.label().is_some_and(|t| t.contains("Завершить")));
        }
    }
    let scroll = wave
        .widget
        .ancestor(gtk::ScrolledWindow::static_type())
        .unwrap();
    let viewport = scroll.compute_bounds(&ui.window).unwrap();
    let area = wave.widget.compute_bounds(&ui.window).unwrap();
    assert!(area.width() >= 200.0 && area.height() >= 140.0);
    assert!(
        area.height() <= 320.0,
        "musical Hero must stay compact even for error/loading: {area:?}"
    );
    assert!(
        area.x() >= viewport.x() - 1.0
            && area.x() + area.width() <= viewport.x() + viewport.width() + 1.0
    );
    assert!(
        area.y() >= viewport.y() - 1.0
            && area.y() + area.height() <= viewport.y() + viewport.height() + 1.0,
        "the complete Hero must fit the actual viewport: {area:?}/{viewport:?}"
    );
    for widget in descendants(wave.widget.upcast_ref()) {
        assert!(!widget.is::<gtk::Entry>() && !widget.is::<gtk::PasswordEntry>());
        if !widget.is_visible() || !widget.is_mapped() {
            continue;
        }
        let bounds = widget.compute_bounds(&ui.window).unwrap();
        assert!(
            bounds.x() >= area.x() - 1.0
                && bounds.x() + bounds.width() <= area.x() + area.width() + 1.0,
            "Hero child fits horizontally: {} {bounds:?}/{area:?}",
            widget.widget_name()
        );
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            let layout = label.layout();
            let (width, height) = layout.pixel_size();
            assert!(
                !layout.is_ellipsized(),
                "guidance/actions cannot be ellipsized"
            );
            assert!(
                width <= label.width() + 2 && height <= label.height() + 2,
                "fixed text wraps fully: {} layout={width}x{height} allocation={}x{}",
                label.widget_name(),
                label.width(),
                label.height()
            );
        }
    }
    for button in [&wave.primary, &wave.cancel] {
        if button.is_visible() {
            let bounds = button.compute_bounds(&ui.window).unwrap();
            assert!(
                button.is_mapped() && bounds.width() >= 44.0 && bounds.height() >= 30.0,
                "native action remains usable: {} {bounds:?}",
                button.widget_name()
            );
        }
    }
}

fn capture(ui: &crate::ui::MainWindow, id: &str, shape: &str, name: &str, theme: &str) {
    let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") else {
        return;
    };
    std::fs::create_dir_all(&directory).unwrap();
    let scale = ui.window.scale_factor();
    let filename = format!("wave-home-{shape}-{name}-scale{scale}-{theme}.png");
    assert!(
        Command::new("import")
            .args(["-silent", "-window", id])
            .arg(std::path::Path::new(&directory).join(&filename))
            .status()
            .unwrap()
            .success()
    );
    let mut manifest = OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::path::Path::new(&directory).join(format!("wave-home-layout-scale{scale}.jsonl")))
        .unwrap();
    writeln!(
        manifest,
        "{}",
        json!({"capture":filename,"logical_width":ui.window.width(),
        "logical_height":ui.window.height(),"maximized":ui.window.is_maximized(),"theme":theme,
        "scale":scale,"renderer":ui.window.renderer().unwrap().type_().name(),
        "animations":false,"source":"safe-state-presentation-only-no-account-network-or-audio",
        "provider_acceptance":false,"audible_acceptance":false})
    )
    .unwrap();
}

fn native_input(ui: &crate::ui::MainWindow, id: &str, button: &gtk::Button) {
    let count = Rc::new(Cell::new(0));
    let delivered = count.clone();
    let handler = button.connect_clicked(move |_| delivered.set(delivered.get() + 1));
    assert!(
        Command::new("xdotool")
            .args(["windowfocus", "--sync", id])
            .status()
            .unwrap()
            .success()
    );
    let (x, y) = super::super::test_input::pointer(
        &ui.window,
        button,
        super::super::test_input::Target::Center,
    );
    assert!(
        Command::new("xdotool")
            .args([
                "mousemove",
                "--window",
                id,
                &x.to_string(),
                &y.to_string(),
                "click",
                "1"
            ])
            .status()
            .unwrap()
            .success()
    );
    pump_until(|| count.get() == 1);
    settle();
    assert_eq!(
        count.get(),
        1,
        "one actual mouse click delivers exactly one action"
    );
    assert!(button.grab_focus());
    assert!(
        Command::new("xdotool")
            .args(["key", "--clearmodifiers", "Tab", "shift+Tab"])
            .status()
            .unwrap()
            .success()
    );
    pump_until(|| button.has_focus() && ui.window.gets_focus_visible());
    for (index, key) in ["Return", "space"].into_iter().enumerate() {
        assert!(
            Command::new("xdotool")
                .args(["key", "--clearmodifiers", key])
                .status()
                .unwrap()
                .success()
        );
        pump_until(|| count.get() == index + 2);
        settle();
        assert_eq!(
            count.get(),
            index + 2,
            "one {key} delivers exactly one action"
        );
    }
    button.disconnect(handler);
}

#[test]
#[ignore = "isolated GTK/X11 Home presentation; run under Xvfb at GDK_SCALE=1 and 2"]
fn native_wave_home_experiment_layout_and_controls() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let _manager = WindowManager::new();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.WaveHomeFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let ui = crate::ui::build_with_music_pane(
        &app,
        crate::application::ConnectionService::new(crate::api::YandexApi::new().unwrap()),
        |app, window| {
            MusicPane::with_provider(
                app,
                window,
                None,
                Arc::new(OfflineProvider),
                launch_system_browser,
            )
        },
    );
    ui.window.set_title(Some(TITLE));
    ui.window.set_default_size(420, 720);
    ui.window.present();
    let this = &ui.music.controller;
    assert!(this.service.is_none() && this.model.borrow().token.is_none());
    this.model.borrow_mut().account_valid = true;
    this.widgets
        .account
        .set_text("Synthetic presentation account");
    this.render_controls();
    ui.show_page("music-home");
    let wave = WaveParts::new();
    let wave_refs: Vec<_> = descendants(wave.widget.upcast_ref())
        .into_iter()
        .map(|widget| widget.downgrade())
        .collect();
    let previous = this.widgets.home_wave.widget.prev_sibling();
    assert!(
        previous.is_none(),
        "Wave is the first signed-in Home element"
    );
    this.widgets
        .home_signedin
        .remove(&this.widgets.home_wave.widget);
    this.widgets
        .home_signedin
        .insert_child_after(&wave.widget, previous.as_ref());
    assert_eq!(
        this.widgets.home_signedin.first_child(),
        Some(wave.widget.clone().upcast())
    );
    assert_eq!(
        wave.widget.next_sibling().unwrap().widget_name(),
        "music-home-shortcuts"
    );
    pump_until(|| wave.widget.is_mapped() && ui.window.width() >= 400);
    let id = window_id();
    let unknown = RadioPhase::Failed {
        stage: RadioStage::Feedback,
        failure: RadioFailure::OutcomeUnknown,
    };
    for (width, height, shape) in [
        (420, 720, "compact"),
        (850, 750, "medium"),
        (1440, 900, "wide"),
        (1920, 1080, "full"),
        (0, 0, "maximized"),
    ] {
        if shape == "maximized" {
            ui.window.maximize();
            pump_until(|| ui.window.is_maximized());
            assert!(ui.window.width() > 900 && ui.window.height() > 650);
        } else {
            ui.window.set_default_size(width, height);
            pump_until(|| {
                (ui.window.width() - width).abs() <= 32 && (ui.window.height() - height).abs() <= 32
            });
        }
        for (theme, scheme) in [
            ("light", adw::ColorScheme::ForceLight),
            ("dark", adw::ColorScheme::ForceDark),
        ] {
            adw::StyleManager::default().set_color_scheme(scheme);
            for (name, state) in [
                ("ready", status(RadioPhase::Ready, None)),
                (
                    "playing",
                    status(RadioPhase::Active, Some(PlaybackState::Playing)),
                ),
                ("failed", status(unknown, Some(PlaybackState::Playing))),
            ] {
                wave.render(state);
                reveal_wave(&ui, &wave);
                assert_structure(&ui, &wave, state);
                if shape == "compact" && theme == "light" {
                    match name {
                        "ready" => native_input(&ui, &id, &wave.primary),
                        "playing" => native_input(&ui, &id, &wave.primary),
                        "failed" => native_input(&ui, &id, &wave.primary),
                        _ => unreachable!(),
                    }
                }
                // A compact, representative current set supplements all-state
                // structural checks without publishing an enormous image tree.
                if (shape == "compact" && name == "ready")
                    || (shape == "medium" && name == "playing")
                    || (shape == "wide" && name == "failed")
                    || (shape == "full" && name == "ready")
                {
                    capture(&ui, &id, shape, name, theme);
                }
            }
            if shape == "compact" {
                for (name, state) in [
                    ("disabled", status(RadioPhase::Disabled, None)),
                    ("starting", status(RadioPhase::Starting, None)),
                    (
                        "paused",
                        status(RadioPhase::Active, Some(PlaybackState::Paused)),
                    ),
                    (
                        "loading-next",
                        status(RadioPhase::LoadingNext, Some(PlaybackState::Playing)),
                    ),
                    (
                        "finishing",
                        status(RadioPhase::Finishing, Some(PlaybackState::Paused)),
                    ),
                    ("failed-before-audio", status(unknown, None)),
                ] {
                    wave.render(state);
                    reveal_wave(&ui, &wave);
                    assert_structure(&ui, &wave, state);
                    if name == "starting" && theme == "light" {
                        native_input(&ui, &id, &wave.cancel);
                    }
                    if (name == "disabled" || name == "paused") && theme == "dark" {
                        capture(&ui, &id, shape, name, theme);
                    }
                }
                let unauthenticated = RadioStatus {
                    authenticated: false,
                    ..status(RadioPhase::Ready, None)
                };
                wave.render(unauthenticated);
                reveal_wave(&ui, &wave);
                assert_structure(&ui, &wave, unauthenticated);
                assert!(contains_text(wave.widget.upcast_ref(), "Войдите в Яндекс"));
            }
        }
    }
    this.model.borrow_mut().account_valid = false;
    this.render_controls();
    settle();
    assert!(
        !wave.widget.is_mapped(),
        "signed-out Home must hide experimental personal controls"
    );
    this.widgets.home_signedin.remove(&wave.widget);
    this.widgets
        .home_signedin
        .insert_child_after(&this.widgets.home_wave.widget, previous.as_ref());
    drop(previous);
    drop(wave);
    pump_until(|| wave_refs.iter().all(|widget| widget.upgrade().is_none()));
    let cleaned = ui.close_for_smoke();
    pump_until(|| cleaned(false));
    assert!(cleaned(true));
}
