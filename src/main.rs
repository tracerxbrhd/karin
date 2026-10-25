// SPDX-License-Identifier: GPL-3.0-or-later
use std::{cell::Cell, rc::Rc, time::Duration};

use adw::prelude::*;
use gtk::{gio, glib};
use karin::{api::YandexApi, application::ConnectionService, audio, config, ui};

fn main() -> glib::ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag] if flag == "--help" || flag == "-h" => {
            println!(
                "Karin\n\nUsage: karin [--smoke-test | --audio-self-test | --probe-service | --version]\n\n--smoke-test       Start GTK and verify navigation without network or audible output\n--audio-self-test  Process a synthetic signal through GStreamer fakesink\n--probe-service    Make an anonymous service availability request (network required)"
            );
            return glib::ExitCode::SUCCESS;
        }
        [flag] if flag == "--version" => {
            println!("Karin {}", config::VERSION);
            return glib::ExitCode::SUCCESS;
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
        [] => {}
        [flag] if flag == "--smoke-test" || flag == "--probe-service" => {}
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
    let smoke_passed = Rc::new(Cell::new(false));
    let app = adw::Application::builder()
        .application_id(config::APP_ID)
        .flags(if smoke {
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
                .comments("Независимый нативный клиент Яндекс Музыки. Технический фундамент v0.0.")
                .build();
            dialog.present(app.active_window().as_ref());
        }
    });
    app.add_action(&about);
    let checked = smoke_passed.clone();
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let ui = ui::build(app, service.clone());
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
                        for page in ["home", "diagnostics"] {
                            ui.show_page(page);
                            glib::timeout_future(Duration::from_millis(60)).await;
                            if let Err(error) = ui.verify_layout() {
                                eprintln!("GTK layout failure at {width}×{height}/{page}: {error}");
                                layout_passed = false;
                            }
                        }
                    }
                }
                let navigation_passed = ui.smoke_check();
                let focus_passed = ui.focus_primary_action();
                let released = ui.close_for_smoke();
                glib::timeout_add_local_once(Duration::from_millis(60), move || {
                    let _hold_until_cleanup = hold;
                    let cleanup_passed = released();
                    checked.set(layout_passed && navigation_passed && focus_passed && cleanup_passed);
                    if checked.get() {
                        println!(
                            "GTK smoke test passed: responsive layouts in both themes, header/text allocations, focus, navigation and widget cleanup."
                        );
                    } else {
                        eprintln!(
                            "GTK smoke test failed: layout={layout_passed}, navigation={navigation_passed}, focus={focus_passed}, widget cleanup={cleanup_passed}."
                        );
                    }
                    app.quit();
                });
            });
        }
    });
    let code = app.run_with_args(&["karin"]);
    if smoke && !smoke_passed.get() {
        glib::ExitCode::FAILURE
    } else {
        code
    }
}
