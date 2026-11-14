// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual native input over 2,403 synthetic collection positions. This proves
//! application behavior, not a live Music account or audible service playback.
use super::*;
use crate::ui::music::{
    test_input,
    tests::{capture_native, contains_text, pump_until},
};
use std::{process::Command, sync::atomic::Ordering, time::Instant};

fn input(arguments: &[&str]) {
    assert!(
        Command::new("xdotool")
            .args(arguments)
            .status()
            .unwrap()
            .success()
    );
}
fn settle() {
    let until = Instant::now() + Duration::from_millis(100);
    pump_until(|| Instant::now() >= until);
}
fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    let (x, y) = test_input::pointer(window, widget, test_input::Target::Leading);
    input(&[
        "mousemove",
        "--sync",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "1",
    ]);
    settle();
}
fn query(window: &adw::ApplicationWindow, id: &str, feed: &Feed, value: &str) {
    let entry = &feed.filter_ui.as_ref().unwrap().search;
    click(window, id, entry);
    input(&["key", "--clearmodifiers", "ctrl+a", "BackSpace"]);
    if !value.is_empty() {
        input(&["type", "--clearmodifiers", value]);
    }
    pump_until(|| entry.text().as_str() == value);
    settle();
}
fn row_count(rows: &gtk::ListBox) -> usize {
    let mut count = 0;
    let mut child = rows.first_child();
    while let Some(widget) = child {
        count += 1;
        child = widget.next_sibling();
    }
    count
}

#[test]
#[ignore = "requires isolated GTK/X11, xdotool and counted local HTTP"]
fn native_v08_whole_collection_index_search_sort_and_cancellation() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let fixture = crate::music::library_index::tests::fixture_native(2_403);
    let token = crate::music::library_index::tests::token();
    fixture.service.account(&token).unwrap();
    let manifest = fixture
        .service
        .collection_snapshot(&token, &CollectionSource::Liked)
        .unwrap();
    let first = fixture
        .service
        .collection_index_batch(&token, &manifest, 0)
        .unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.WholeLibraryFixture")
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
                Some(fixture.service.clone()),
                Arc::new(OfflineProvider),
                launch_system_browser,
            )
        },
    );
    let window = &ui.window;
    window.set_title(Some("Karin synthetic HTTP fixture"));
    window.set_default_size(900, 750);
    let this = &ui.music.controller;
    this.set_fixture_resolver_for_test(crate::stream::StreamResolver::for_test(
        &fixture.origin,
        &fixture.origin,
        &fixture.origin,
    ));
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token);
        model.account_valid = true;
    }
    this.render_controls();
    this.widgets.stack.set_visible_child_name("library");
    let feed = this.new_track_feed(
        FeedSource::Liked,
        ContextKind::LikedTracks,
        None,
        &this.widgets.library_rows,
        &this.widgets.library_feedback,
    );
    feed.prefill_remaining.set(0);
    this.append_track_feed(
        &feed,
        0,
        Page {
            items: first.tracks.into_iter().map(|(_, track)| track).collect(),
            has_more: true,
        },
    );
    let initial = feed.data.borrow().context(42).unwrap();
    {
        let mut model = this.model.borrow_mut();
        model.queue.replace_context(initial.clone());
        model.queue.set_shuffle(true);
        model.queue.set_repeat_mode(crate::queue::RepeatMode::Queue);
    }
    window.present();
    let bar = feed.filter_ui.as_ref().unwrap();
    pump_until(|| bar.index_all.is_mapped());
    let output = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic HTTP fixture$",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();
    let id = output.lines().next().unwrap();
    input(&["windowfocus", "--sync", id]);
    let before = fixture.batches.load(Ordering::Acquire);
    query(window, id, &feed, "Original 102402");
    assert!(feed.data.borrow().filter.visible().is_empty());
    assert_eq!(
        fixture.batches.load(Ordering::Acquire),
        before,
        "typing does not fetch the collection"
    );
    fixture.hold_after.store(100_200, Ordering::Release);
    click(window, id, &bar.index_all);
    pump_until(|| fixture.held.load(Ordering::Acquire));
    let cursor = feed.index.borrow().cursor;
    assert!((200..2_000).contains(&cursor));
    click(window, id, &bar.cancel_index);
    assert!(!feed.index.borrow().running);
    fixture.released.store(true, Ordering::Release);
    pump_until(|| !this.model.borrow().busy);
    assert_eq!(
        feed.index.borrow().cursor,
        cursor,
        "late cancelled normalization is discarded"
    );
    assert_eq!(this.model.borrow().queue.context(), Some(&initial));
    assert!(this.model.borrow().queue.shuffle());
    assert_eq!(
        this.model.borrow().queue.repeat_mode(),
        crate::queue::RepeatMode::Queue
    );
    fixture.changed.store(true, Ordering::Release);
    click(window, id, &bar.index_all);
    pump_until(|| feed.index.borrow().changed && !this.model.borrow().busy);
    assert!(
        !feed.index.borrow().complete,
        "a changed final membership cannot publish complete coverage"
    );
    assert!(contains_text(
        bar.status.upcast_ref(),
        "Коллекция изменилась"
    ));
    assert_eq!(this.model.borrow().queue.context(), Some(&initial));
    click(window, id, &bar.index_all);
    pump_until(|| feed.index.borrow().complete && !this.model.borrow().busy);
    assert_eq!(feed.index.borrow().cursor, 2_403);
    assert_eq!(feed.data.borrow().items.len(), 2_402);
    assert!(contains_text(
        bar.status.upcast_ref(),
        "Вся коллекция проверена"
    ));
    assert_eq!(row_count(&feed.rows), 1);
    let index = feed.data.borrow().filter.visible()[0];
    let context = feed.data.borrow().context(index).unwrap();
    assert_eq!(context.source_position(0), Some(2_402));
    assert_eq!(
        bar.sort.model().unwrap().n_items(),
        6,
        "actual timestamps enable date sorting"
    );
    let after_index = fixture.batches.load(Ordering::Acquire);
    query(window, id, &feed, "Original");
    assert_eq!(row_count(&feed.rows), RESULT_WINDOW);
    click(window, id, &bar.sort);
    input(&["key", "Home", "Down", "Down", "Return"]);
    pump_until(|| bar.sort.selected() == 2);
    assert_eq!(this.model.borrow().queue.context(), Some(&initial));
    assert_eq!(fixture.batches.load(Ordering::Acquire), after_index);
    assert_eq!(row_count(&feed.rows), RESULT_WINDOW);
    click(window, id, &bar.next);
    assert_eq!(feed.window_start.get(), RESULT_WINDOW);
    assert_eq!(row_count(&feed.rows), RESULT_WINDOW);
    let expected = feed.rendered_positions.borrow()[0];
    assert_eq!(
        feed.data.borrow().context(expected).unwrap().items().len(),
        500
    );
    let row = feed.rows.row_at_index(0).unwrap();
    let (x, y) = test_input::pointer(window, &row, test_input::Target::Leading);
    input(&[
        "mousemove",
        "--sync",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "--repeat",
        "5",
        "--delay",
        "15",
        "5",
    ]);
    let scroll = this.content_scroll.borrow().upgrade().unwrap();
    pump_until(|| scroll.vadjustment().value() > 20.0);
    let saved_scroll = scroll.vadjustment().value();
    eprintln!(
        "Karin synthetic collection scroll: phase=depart value={saved_scroll} rows-height={} upper={} page={}",
        feed.rows.height(),
        scroll.vadjustment().upper(),
        scroll.vadjustment().page_size()
    );
    // Navigation preserves the explicit index/projection; no whole-library
    // startup, tab-return, query or sort request is introduced.
    let saved_query = bar.search.text();
    gtk::prelude::WidgetExt::activate_action(window, "win.music-home", None).unwrap();
    settle();
    gtk::prelude::WidgetExt::activate_action(window, "win.music-library", None).unwrap();
    pump_until(|| bar.search.is_mapped());
    let previous_scroll = Cell::new(f64::NAN);
    pump_until(|| {
        let value = scroll.vadjustment().value();
        if previous_scroll.replace(value).to_bits() != value.to_bits() {
            eprintln!(
                "Karin synthetic collection scroll: phase=return value={value} target={saved_scroll} saved={} rows-height={} upper={} page={}",
                feed.saved_scroll.get(),
                feed.rows.height(),
                scroll.vadjustment().upper(),
                scroll.vadjustment().page_size()
            );
        }
        (value - saved_scroll).abs() < 2.0
    });
    assert_eq!(bar.search.text(), saved_query);
    assert_eq!(bar.sort.selected(), 2);
    assert_eq!(feed.window_start.get(), RESULT_WINDOW);
    assert!(feed.index.borrow().complete);
    assert_eq!(fixture.batches.load(Ordering::Acquire), after_index);
    scroll.vadjustment().set_value(0.0);
    query(window, id, &feed, "Original 102402");
    // The accepted default remembers an open Track Info pane, and this fixture
    // deliberately seeded a selected queue occurrence. Compact therefore
    // correctly presents Info as a separate subview. Explicitly close it for
    // this *library* capture matrix, just as a user returning to the collection.
    if this.info_is_open() {
        click(window, id, &this.track_pane.close);
        pump_until(|| {
            !this.info_is_open()
                && this
                    .info_host
                    .borrow()
                    .as_ref()
                    .and_then(workspace::InfoHost::revealer)
                    .is_none_or(|view| view.progress() == 0.0)
        });
    }
    assert_eq!(this.panel_mode.get(), workspace::PanelMode::Closed);
    for dark in [false, true] {
        adw::StyleManager::default().set_color_scheme(if dark {
            adw::ColorScheme::ForceDark
        } else {
            adw::ColorScheme::ForceLight
        });
        for (label, width, height) in [
            ("compact", 420, 720),
            ("medium", 900, 650),
            ("wide", 1440, 900),
        ] {
            window.set_default_size(width, height);
            test_input::wait_for_layout(window, &bar.search);
            assert!(bar.search.width() > 100 && bar.search.height() > 20);
            assert!(bar.status.is_mapped());
            assert_eq!(row_count(&feed.rows), 1);
            capture_native(&format!(
                "v08-library-{label}-{}",
                if dark { "dark" } else { "light" }
            ));
        }
    }
    window.set_default_size(900, 750);
    let row = feed.rows.row_at_index(0).unwrap();
    click(window, id, &row);
    pump_until(|| fixture.offers.load(Ordering::Acquire) > 0 && !this.music_requested());
    assert_eq!(
        this.model
            .borrow()
            .queue
            .context()
            .unwrap()
            .source_position(0),
        Some(2_402)
    );
    assert_eq!(this.model.borrow().queue.current().unwrap().id, "102402");
    // A different complete membership must not publish 'complete' under the
    // old generation. A confirmed local Like invalidates this index as well.
    this.invalidate_liked_feeds();
    assert!(feed.index.borrow().changed && !feed.index.borrow().complete);
    let retired = feed.index.borrow().serial;
    this.logout();
    assert!(this.track_feeds.borrow().is_empty());
    bar.index_all.emit_clicked();
    settle();
    assert!(feed.index.borrow().serial > retired);
    assert!(!this.model.borrow().account_valid && this.model.borrow().queue.items().is_empty());
    window.close();
    pump_until(|| !window.is_visible());
}
