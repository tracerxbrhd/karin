// SPDX-License-Identifier: GPL-3.0-or-later
//! Matched native work-count/elapsed observation, not a renderer latency claim.

use super::*;
use std::cell::Cell;
use std::time::Instant;

#[test]
#[ignore = "isolated Xvfb native catalog work observer; original metadata, no network or audio"]
fn native_v08_incremental_marker_updates_and_stable_ticks() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.NativeCatalogWork")
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
    let this = &ui.music.controller;
    let window = &ui.window;
    window.set_title(Some("Karin synthetic catalog work fixture"));
    window.set_default_size(900, 650);
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(AccessToken::for_test(
            "synthetic-native-catalog-work",
            Duration::from_secs(600),
        ));
        model.account_valid = true;
    }
    this.render_controls();
    this.set_info_open(false);
    this.widgets.stack.set_visible_child_name("search");
    window.present();
    pump_until(|| this.widgets.search_rows.is_mapped());

    for count in [500, 2000] {
        clear_list(&this.widgets.search_rows);
        this.sync_catalog_rows();
        let tracks: Vec<_> = (0..count)
            .map(|index| {
                track(
                    &format!("{}", 40_000 + index),
                    "Original catalog work fixture",
                )
            })
            .collect();
        // Each row resolves its actual bounded playback window. The source
        // binding distinguishes later windows; no fake large queue is created.
        let windows: Vec<_> = tracks
            .chunks(499)
            .enumerate()
            .map(|(index, items)| {
                PlaybackContext::new(
                    ContextKind::Search,
                    Some(format!("catalog-work-window:{index}")),
                    items.to_vec(),
                    0,
                    true,
                )
                .unwrap()
            })
            .collect();
        this.model
            .borrow_mut()
            .queue
            .replace_context(windows[0].clone());
        let calls = Rc::new(Cell::new(0_u64));
        let started = Instant::now();
        for (index, track) in tracks.iter().enumerate() {
            let context = windows[index / 499]
                .with_current_index(index % 499)
                .unwrap();
            let observed = Rc::clone(&calls);
            this.track_row_bound(
                &this.widgets.search_rows,
                track.clone(),
                Rc::new(move || {
                    observed.set(observed.get() + 1);
                    Some(context.clone())
                }),
                None,
            );
        }
        let append_elapsed_us = started.elapsed().as_micros();
        let append_markers = calls.get();
        assert!(
            this.widgets
                .search_rows
                .row_at_index(0)
                .unwrap()
                .has_css_class("music-current")
        );
        assert!(
            !this
                .widgets
                .search_rows
                .row_at_index(499)
                .unwrap()
                .has_css_class("music-current"),
            "an equal local index in a different window is a distinct source"
        );

        // An explicit filter/feed presentation always performs a complete
        // admission, even if the playback occurrence itself did not change.
        calls.set(0);
        this.sync_catalog_rows();
        assert_eq!(calls.get(), count as u64);
        calls.set(0);
        let started = Instant::now();
        for _ in 0..24 {
            this.render_music_player();
        }
        let stable_elapsed_us = started.elapsed().as_micros();
        let stable_markers = calls.get();

        calls.set(0);
        this.model.borrow_mut().queue.select(1);
        this.render_music_player();
        let selection_markers = calls.get();
        assert_eq!(selection_markers, count as u64);
        assert!(
            this.widgets
                .search_rows
                .row_at_index(1)
                .unwrap()
                .has_css_class("music-current")
        );
        assert!(
            !this
                .widgets
                .search_rows
                .row_at_index(0)
                .unwrap()
                .has_css_class("music-current")
        );
        let playing_occurrence = this.model.borrow().queue.current_occurrence_id().cloned();
        calls.set(0);
        this.model
            .borrow_mut()
            .queue
            .append(track("90", "Original manual append"))
            .unwrap();
        this.render_music_player();
        assert_eq!(calls.get(), count as u64);
        assert_eq!(
            this.model.borrow().queue.current_occurrence_id(),
            playing_occurrence.as_ref(),
            "editing future items must not replace the current occurrence"
        );

        // The same fixture is injected verbatim into the accepted baseline;
        // its report is compared externally, without renderer/timing changes.
        eprintln!(
            "KARIN_MARKER_PERF {}",
            serde_json::json!({
                "rows": count,
                "polls": 24,
                "append_marker_evaluations": append_markers,
                "append_elapsed_us": append_elapsed_us,
                "stable_marker_evaluations": stable_markers,
                "stable_elapsed_us": stable_elapsed_us,
                "changed_selection_marker_evaluations": selection_markers,
                "future_edit_marker_evaluations": calls.get(),
                "source_window_identity_preserved": true,
                "current_occurrence_preserved": true,
                "network_requests": 0,
                "player_created": this.music_snapshot().is_some(),
                "physical_gpu_or_frame_latency_measurement": false,
            })
        );
        assert!(this.music_snapshot().is_none());
        clear_list(&this.widgets.search_rows);
        this.sync_catalog_rows();
    }
    ui.music.close();
    window.close();
}
