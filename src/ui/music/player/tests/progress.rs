// SPDX-License-Identifier: GPL-3.0-or-later
//! Timeline input transitions on the production footer's actual GTK adjustment.

use super::*;

#[test]
fn progress_projection_uses_positive_duration_and_preserves_position() {
    let mut snapshot = PlaybackSnapshot {
        state: PlaybackState::Playing,
        position: Some(Duration::from_millis(150)),
        ..PlaybackSnapshot::default()
    };
    let progress = ProgressPresentation::new(Some(&snapshot), Some(141_000), None);
    assert_eq!(progress.duration, Some(Duration::from_secs(141)));
    assert_eq!(progress.position, snapshot.position);
    snapshot.duration = Some(Duration::ZERO);
    assert_eq!(
        ProgressPresentation::new(Some(&snapshot), Some(141_000), None),
        progress
    );
    snapshot.duration = Some(Duration::from_secs(140));
    assert_eq!(
        ProgressPresentation::new(Some(&snapshot), Some(141_000), None).duration,
        Some(Duration::from_secs(140))
    );
    snapshot.position = Some(Duration::from_secs(142));
    assert_eq!(
        ProgressPresentation::new(Some(&snapshot), Some(141_000), None).position,
        Some(Duration::from_secs(140))
    );
    let restored = ProgressPresentation::new(None, Some(141_000), Some(Duration::from_secs(40)));
    assert_eq!(restored.position, Some(Duration::from_secs(40)));
    let rejected = ProgressPresentation::new(None, Some(141_000), None);
    assert_eq!(rejected.position, None);
    assert_eq!(rejected.duration, Some(Duration::from_secs(141)));
    snapshot.duration = None;
    assert_eq!(
        ProgressPresentation::new(Some(&snapshot), Some(0), None).duration,
        None
    );
}

fn css_child(root: &gtk::Widget, name: &str) -> gtk::Widget {
    if root.css_name() == name {
        return root.clone();
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Some(found) = find_css(&widget, name) {
            return found;
        }
    }
    panic!("mapped scale CSS child {name} is missing");
}

fn find_css(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.css_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Some(found) = find_css(&widget, name) {
            return Some(found);
        }
    }
    None
}

#[test]
#[ignore = "requires an isolated GTK display; run the registered native fixture"]
fn native_music_player_progress_duration_transitions() {
    gtk::init().unwrap();
    adw::init().unwrap();
    crate::ui::style::install();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.ProgressFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let parts = crate::ui::music::player_bar::PlayerBarParts::new();
    let window = adw::ApplicationWindow::builder()
        .application(&app)
        .title("Karin progress fixture")
        .default_width(900)
        .default_height(240)
        .content(&parts.player_bar)
        .build();
    crate::ui::style::attach(&window);
    parts.player_revealer.set_reveal_child(true);
    window.present();
    pump_until(|| parts.progress.is_mapped() && parts.progress.width() > 100);
    let changes = Rc::new(std::cell::Cell::new(0));
    parts.progress.connect_change_value({
        let changes = changes.clone();
        move |_, _, _| {
            changes.set(changes.get() + 1);
            glib::Propagation::Proceed
        }
    });
    let mut records = Vec::new();
    let settings = gtk::Settings::default().unwrap();
    let animations_before = settings.is_gtk_enable_animations();
    for (width, reduced_motion) in [(900, false), (420, true)] {
        window.set_default_size(width, 240);
        // Default size addresses the GDK surface; GtkWindow's content excludes
        // client-side shadow margins, just as in resize_player_window.
        pump_until(|| {
            window
                .surface()
                .is_some_and(|surface| surface.width() == width)
        });
        settings.set_gtk_enable_animations(!reduced_motion);
        for appearance in [
            crate::settings::ColorScheme::Light,
            crate::settings::ColorScheme::Dark,
            crate::settings::ColorScheme::System,
        ] {
            let values = crate::settings::Preferences {
                appearance,
                ..Default::default()
            };
            crate::ui::style::apply(&values, &window);
            let mut check = |stage: &str,
                             snapshot: Option<&PlaybackSnapshot>,
                             metadata,
                             initial,
                             can_seek,
                             expected_upper,
                             expected_value,
                             expected_label: &str| {
                ProgressPresentation::new(snapshot, metadata, initial).apply(
                    &parts.progress,
                    &parts.position,
                    can_seek,
                );
                let painted = Rc::new(std::cell::Cell::new(false));
                let clock = window.frame_clock().unwrap();
                let handler = clock.connect_after_paint({
                    let painted = painted.clone();
                    move |_| painted.set(true)
                });
                window.queue_draw();
                pump_until(|| painted.get());
                clock.disconnect(handler);
                let adjustment = parts.progress.adjustment();
                assert_eq!(adjustment.upper(), expected_upper, "{stage}: real range");
                assert_eq!(adjustment.value(), expected_value, "{stage}: real position");
                assert_eq!(adjustment.page_size(), 0.0);
                assert_eq!(
                    parts.progress.is_sensitive(),
                    can_seek && !expected_label.ends_with(" / —"),
                    "{stage}: only a known duration may offer the slider seek target"
                );
                assert_eq!(
                    parts.position.text(),
                    expected_label,
                    "{stage}: label agrees"
                );
                let trough = css_child(parts.progress.upcast_ref(), "trough");
                let slider = css_child(parts.progress.upcast_ref(), "slider");
                let highlight = css_child(parts.progress.upcast_ref(), "highlight");
                let slider_bounds = slider.compute_bounds(&trough).unwrap();
                let fraction = adjustment.value() / adjustment.upper();
                if fraction < 0.002 {
                    assert!(
                        highlight.width() < trough.width() / 20,
                        "{stage}: mapped highlight must not show a false fifteen percent"
                    );
                    assert!(
                        slider_bounds.center().x() < trough.width() as f32 / 20.0,
                        "{stage}: mapped thumb remains at the start"
                    );
                }
                records.push(
                    json!({"stage":stage,"upper":adjustment.upper(),"value":adjustment.value(),
                    "fraction":fraction,"label":parts.position.text().as_str(),
                    "slider_x":slider_bounds.x(),"highlight_width":highlight.width(),
                    "trough_width":trough.width(),"sensitive":parts.progress.is_sensitive(),
                    "logical_content_width":window.width(),"surface_width":window.surface().unwrap().width(),"dark":adw::StyleManager::default().is_dark(),
                    "reduced_motion":!gtk::Settings::default().unwrap().is_gtk_enable_animations()}),
                );
            };
            let zero = Some(Duration::ZERO);
            check(
                "initial-selection",
                None,
                Some(141_000),
                zero,
                false,
                141.0,
                0.0,
                "0:00 / 2:21",
            );
            let mut snapshot = PlaybackSnapshot {
                state: PlaybackState::Loading,
                ..PlaybackSnapshot::default()
            };
            check(
                "loading-before-timing",
                Some(&snapshot),
                Some(141_000),
                zero,
                false,
                141.0,
                0.0,
                "0:00 / 2:21",
            );
            snapshot.state = PlaybackState::Ready;
            snapshot.position = Some(Duration::from_millis(150));
            check(
                "position-before-duration",
                Some(&snapshot),
                Some(141_000),
                zero,
                true,
                141.0,
                0.15,
                "0:00 / 2:21",
            );
            snapshot.state = PlaybackState::Playing;
            snapshot.duration = Some(Duration::from_secs(141));
            check(
                "native-duration-arrives",
                Some(&snapshot),
                Some(141_000),
                zero,
                true,
                141.0,
                0.15,
                "0:00 / 2:21",
            );
            snapshot.position = Some(Duration::from_secs(35));
            check(
                "confirmed-playing-position",
                Some(&snapshot),
                Some(141_000),
                zero,
                true,
                141.0,
                35.0,
                "0:35 / 2:21",
            );
            snapshot.state = PlaybackState::Paused;
            check(
                "paused",
                Some(&snapshot),
                Some(141_000),
                zero,
                true,
                141.0,
                35.0,
                "0:35 / 2:21",
            );
            check(
                "next-track-clears-previous-widget",
                None,
                Some(200_000),
                zero,
                false,
                200.0,
                0.0,
                "0:00 / 3:20",
            );
            check(
                "rejected-before-playback",
                None,
                Some(141_000),
                None,
                false,
                141.0,
                0.0,
                "— / 2:21",
            );
            snapshot.duration = None;
            check(
                "both-durations-unknown",
                Some(&snapshot),
                None,
                zero,
                true,
                1.0,
                0.0,
                "0:35 / —",
            );
            assert!(
                !parts.progress.is_sensitive(),
                "unknown fraction cannot offer a slider seek"
            );
            check(
                "restored-position",
                None,
                Some(141_000),
                Some(Duration::from_secs(40)),
                false,
                141.0,
                40.0,
                "0:40 / 2:21",
            );
            snapshot.state = PlaybackState::Loading;
            snapshot.duration = None;
            snapshot.position = Some(Duration::from_secs(40));
            check(
                "renewal-loading-preserves-confirmed-position",
                Some(&snapshot),
                Some(141_000),
                zero,
                false,
                141.0,
                40.0,
                "0:40 / 2:21",
            );
            assert_eq!(
                changes.get(),
                0,
                "programmatic timeline updates must not dispatch user Seek"
            );
        }
    }
    settings.set_gtk_enable_animations(animations_before);
    if let Some(directory) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(PathBuf::from(directory).join("progress-transitions.json"),
            serde_json::to_vec_pretty(&json!({"scope":"synthetic snapshot timing on mapped production footer widgets",
                "scale":window.scale_factor(),"change_value_signals":changes.get(),"records":records})).unwrap()).unwrap();
    }
    window.close();
}
