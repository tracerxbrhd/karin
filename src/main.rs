// SPDX-License-Identifier: GPL-3.0-or-later
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use adw::prelude::*;
use gtk::{gio, glib};
use karin::{api::YandexApi, application::ConnectionService, audio, config, playback, ui};

mod oauth_cli;

fn main() -> glib::ExitCode {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let delivery_diagnostic = args.iter().any(|arg| arg == "--delivery-diagnostic");
    if args
        .iter()
        .filter(|arg| *arg == "--delivery-diagnostic")
        .count()
        > 1
    {
        eprintln!("Duplicate delivery diagnostic option.");
        return glib::ExitCode::from(2);
    }
    args.retain(|arg| arg != "--delivery-diagnostic");
    if delivery_diagnostic && !args.is_empty() {
        eprintln!("Run --delivery-diagnostic alone with the ordinary local sign-in window.");
        return glib::ExitCode::from(2);
    }
    let experimental = args.iter().any(|arg| arg == "--experimental-music");
    if experimental && !cfg!(feature = "experimental-device-auth") {
        eprintln!("Experimental Music login is absent from this public build.");
        return glib::ExitCode::from(2);
    }
    if args
        .iter()
        .filter(|arg| *arg == "--experimental-music")
        .count()
        > 1
    {
        eprintln!("Duplicate experimental option.");
        return glib::ExitCode::from(2);
    }
    args.retain(|arg| arg != "--experimental-music");
    match args.as_slice() {
        [flag] if flag == "--help" || flag == "-h" => {
            println!(
                "Karin\n\nUsage: karin [--smoke-test | --ui-fixture | --audio-self-test | --playback-self-test | --probe-service | --oauth-diagnostic | --delivery-diagnostic | --version]\n\n--smoke-test          Verify GTK and local playback cleanup without network or sound\n--ui-fixture          Offline sign-in fixture for automated UI captures; never contacts OAuth\n--audio-self-test     Process a synthetic signal through GStreamer fakesink\n--playback-self-test  Decode a generated local WAV through playbin; test controls and cleanup\n--probe-service       Make an anonymous availability request (network required)\n--oauth-diagnostic    Opt-in local PKCE experiment; private terminal and system browser required\n--delivery-diagnostic Opt-in local delivery-info probe using this window's ordinary sign-in; no media request"
            );
            if cfg!(feature = "experimental-device-auth") {
                println!(
                    "--experimental-music  Development compatibility flag; ordinary sign-in is unchanged"
                );
            }
            return glib::ExitCode::SUCCESS;
        }
        [flag] if flag == "--version" => {
            println!("Karin {}", config::VERSION);
            return glib::ExitCode::SUCCESS;
        }
        [flag] if flag == "--oauth-diagnostic" => {
            return match oauth_cli::run() {
                Ok(true) => glib::ExitCode::SUCCESS,
                Ok(false) => glib::ExitCode::FAILURE,
                Err(error) => {
                    eprintln!("OAuth diagnostic: {error}");
                    glib::ExitCode::FAILURE
                }
            };
        }
        [flag] if flag == "--audio-self-test" => {
            return match audio::run_test_tone(audio::AudioOutput::Fakesink) {
                Ok(report) => {
                    println!("{report}");
                    glib::ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{error}");
                    glib::ExitCode::FAILURE
                }
            };
        }
        [flag] if flag == "--playback-self-test" => {
            return match playback::run_self_test() {
                Ok(report) => {
                    println!("{report}");
                    glib::ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{error}");
                    glib::ExitCode::FAILURE
                }
            };
        }
        [] => {}
        [flag]
            if matches!(
                flag.as_str(),
                "--smoke-test" | "--ui-fixture" | "--probe-service"
            ) => {}
        _ => {
            eprintln!("Unknown arguments. Run karin --help.");
            return glib::ExitCode::from(2);
        }
    }
    let adapter = match YandexApi::new() {
        Ok(adapter) => adapter,
        Err(error) => {
            eprintln!("Cannot initialize service adapter: {error}");
            return glib::ExitCode::FAILURE;
        }
    };
    let service = ConnectionService::new(adapter);
    if args.first().is_some_and(|flag| flag == "--probe-service") {
        return match service.check_connection() {
            Ok(report) => {
                println!(
                    "{report:?}\nThis anonymous response does not establish authentication or music playback."
                );
                glib::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                glib::ExitCode::FAILURE
            }
        };
    }
    let smoke = args.first().is_some_and(|flag| flag == "--smoke-test");
    let offline_auth = smoke || args.first().is_some_and(|flag| flag == "--ui-fixture");
    let smoke_passed = Rc::new(Cell::new(false));
    let app = adw::Application::builder()
        .application_id(config::APP_ID)
        .flags(if offline_auth {
            gio::ApplicationFlags::NON_UNIQUE
        } else {
            gio::ApplicationFlags::FLAGS_NONE
        })
        .build();
    let quit = gio::SimpleAction::new("quit", None);
    let weak_app = app.downgrade();
    quit.connect_activate(move |_, _| {
        if let Some(app) = weak_app.upgrade() {
            for window in app.windows() {
                window.close();
            }
        }
    });
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<Primary>q"]);
    app.set_accels_for_action("win.overview", &["<Alt>1"]);
    app.set_accels_for_action("win.diagnostics", &["<Alt>2"]);
    let about = gio::SimpleAction::new("about", None);
    let weak_app = app.downgrade();
    about.connect_activate(move |_, _| {
        if let Some(app) = weak_app.upgrade() {
            let dialog = adw::AboutDialog::builder()
                .application_name("Karin")
                .application_icon("audio-x-generic")
                .developer_name("Karin contributors")
                .version(config::VERSION)
                .license_type(gtk::License::Gpl30)
                .copyright("© 2026 Karin contributors")
                .website("https://github.com/tracerxbrhd/karin")
                .comments("v0.1: библиотека и поиск через MusicService. Прототип входа использует совместимую с Cassette сессию WebKit; реальный доступ требует проверки на Fedora. Воспроизведение Яндекс Музыки пока недоступно.")
                .build();
            dialog.present(app.active_window().as_ref());
        }
    });
    app.add_action(&about);
    let checked = smoke_passed.clone();
    // Music signal handlers keep weak controller references. Retain the normal
    // window/controller until close, just as the smoke future retains its UI.
    let normal_window = Rc::new(RefCell::new(None::<ui::MainWindow>));
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let ui = if offline_auth {
            ui::build_offline_ui_fixture(app, service.clone(), experimental)
        } else {
            ui::build_with_options(app, service.clone(), experimental)
        };
        if delivery_diagnostic {
            ui.enable_delivery_diagnostic();
        }
        ui.window.present();
        if smoke {
            let app = app.clone();
            let checked = checked.clone();
            glib::spawn_future_local(async move {
                let hold = app.hold();
                let mut layout_passed = true;
                for scheme in [adw::ColorScheme::ForceLight, adw::ColorScheme::ForceDark] {
                    adw::StyleManager::default().set_color_scheme(scheme);
                    for (width, height) in [(420, 720), (900, 650), (1440, 900)] {
                        let selected_page = ui.visible_page();
                        ui.window.set_default_size(width, height);
                        glib::timeout_future(Duration::from_millis(160)).await;
                        if ui.visible_page() != selected_page {
                            eprintln!("GTK layout failure: resizing changed the selected page");
                            layout_passed = false;
                        }
                        for page in ["music-library", "music-search", "music-queue", "diagnostics"] {
                            ui.show_page(page);
                            glib::timeout_future(Duration::from_millis(60)).await;
                            if let Err(error) = ui.wait_for_layout().await {
                                eprintln!("GTK layout failure at {width}×{height}/{page}: {error}");
                                layout_passed = false;
                            }
                        }
                        if let Err(error) = ui.exercise_login_for_smoke().await {
                            eprintln!("GTK login dialog failure at {width}×{height}: {error}");
                            layout_passed = false;
                        }
                    }
                }
                let navigation_passed = ui.smoke_check();
                let playback_passed = ui.exercise_playback_for_smoke().await;
                for (width, height) in [(420, 720), (900, 650), (1440, 900)] {
                    ui.window.set_default_size(width, height);
                    glib::timeout_future(Duration::from_millis(160)).await;
                    if let Err(error) = ui.wait_for_layout().await {
                        eprintln!("GTK playback layout failure at {width}×{height}: {error}");
                        layout_passed = false;
                    }
                }
                let focus_passed = ui.focus_primary_action();
                let released = ui.close_for_smoke();
                for _ in 0..120 {
                    if released(false) { break; }
                    glib::timeout_future(Duration::from_millis(50)).await;
                }
                {
                    let _hold_until_cleanup = hold;
                    let cleanup_passed = released(true);
                    checked.set(layout_passed && navigation_passed && focus_passed && playback_passed && cleanup_passed);
                    if checked.get() {
                        println!(
                            "GTK smoke test passed: responsive layouts in both themes, header/text allocations, focus, navigation, actual local playback and window/worker cleanup."
                        );
                    } else {
                        eprintln!(
                            "GTK smoke test failed: layout={layout_passed}, navigation={navigation_passed}, focus={focus_passed}, playback={playback_passed}, cleanup={cleanup_passed}."
                        );
                    }
                    app.quit();
                }
            });
        } else {
            let retained = Rc::downgrade(&normal_window);
            ui.window.connect_close_request(move |_| {
                if let Some(retained) = retained.upgrade() {
                    glib::idle_add_local_once(move || {
                        retained.borrow_mut().take();
                    });
                }
                glib::Propagation::Proceed
            });
            *normal_window.borrow_mut() = Some(ui);
        }
    });
    let code = app.run_with_args(&["karin"]);
    if smoke && !smoke_passed.get() {
        glib::ExitCode::FAILURE
    } else {
        code
    }
}
