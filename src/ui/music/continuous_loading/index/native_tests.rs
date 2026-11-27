// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual native input over 2,403 synthetic collection positions. This proves
//! application behavior, not a live Music account or audible service playback.
use super::*;
use crate::ui::music::{
    collection_filter::CollectionSort,
    test_input,
    tests::{capture_native, contains_text, pump_until},
};
use std::{
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

struct ReleaseHeld(Arc<AtomicBool>);
impl Drop for ReleaseHeld {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

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
    // Keep motion and button input ordered on one X connection. Waiting
    // for a motion event hangs when a repeated click uses the same point.
    input(&[
        "mousemove",
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
    input(&["key", "--clearmodifiers", "ctrl+a"]);
    if value.is_empty() {
        input(&["key", "--clearmodifiers", "BackSpace"]);
    } else if entry.text().is_empty() {
        input(&["type", "--clearmodifiers", value]);
    } else {
        // Native replace-selection is one GtkEditable change, unlike Clear
        // followed by typing. This exercises editing an active search.
        entry.clipboard().set_text(value);
        input(&["key", "--clearmodifiers", "ctrl+v"]);
    }
    pump_until(|| entry.text().as_str() == value);
    settle();
}
fn sort_option(widget: &gtk::Widget, label: &str) -> Option<gtk::Widget> {
    if widget.is_mapped()
        && widget
            .downcast_ref::<gtk::Label>()
            .is_some_and(|name| name.text() == label)
    {
        let mut parent = widget.parent();
        while let Some(option) = parent {
            if option.is::<gtk::Actionable>() {
                return Some(option);
            }
            parent = option.parent();
        }
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Some(option) = sort_option(&widget, label) {
            return Some(option);
        }
    }
    None
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
fn native_liked_index_loads_without_scroll_and_keeps_list_navigation() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let fixture = crate::music::library_index::tests::fixture_native(2_403);
    let token = crate::music::library_index::tests::token();
    fixture.service.account(&token).unwrap();
    fixture.hold_after.store(100_050, Ordering::Release);
    let _release_held = ReleaseHeld(fixture.released.clone());
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.AutoLikedFixture")
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
    window.set_title(Some("Karin synthetic automatic liked collection"));
    window.set_default_size(900, 750);
    window.present();
    let this = &ui.music.controller;
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token);
        model.account_valid = true;
    }
    this.render_controls();
    this.set_music_shuffle(true);
    this.widgets.stack.set_visible_child_name("library");
    this.load_library(0);
    let feed = this
        .track_feeds
        .borrow()
        .iter()
        .find(|feed| feed.auto_index.get())
        .unwrap()
        .clone();
    pump_until(|| fixture.held.load(Ordering::Acquire));
    let first = feed.rows.row_at_index(0).unwrap();
    assert!(first.is_mapped());
    assert_eq!(row_count(&feed.rows), 50);
    assert!(feed.data.borrow().has_more);
    assert!(feed.filter_ui.as_ref().unwrap().search.text().is_empty());
    let play = feed.play_button.borrow().upgrade().unwrap();
    assert!(!play.is_sensitive());
    play.emit_clicked();
    assert!(this.model.borrow().queue.items().is_empty());
    let selected = feed.data.borrow().context(12).unwrap();
    let single = this.limit_unfinished_liked_shuffle(selected).unwrap();
    assert_eq!(single.items().len(), 1);
    assert_eq!(single.source_position(0), Some(12));
    assert_eq!(single.items()[0].id, "100012");
    fixture.released.store(true, Ordering::Release);
    pump_until(|| feed.index.borrow().complete);
    assert_eq!(feed.index.borrow().cursor, 2_403);
    assert_eq!(feed.data.borrow().items.len(), 2_402);
    assert_eq!(row_count(&feed.rows), LIKED_RESULT_WINDOW);
    assert_eq!(feed.rows.row_at_index(0), Some(first));
    assert!(play.is_sensitive());
    assert_eq!(fixture.membership.load(Ordering::Acquire), 3);
    assert_eq!(fixture.batches.load(Ordering::Acquire), 49);
    assert_eq!(fixture.offers.load(Ordering::Acquire), 0);

    let scroll = this.content_scroll.borrow().upgrade().unwrap();
    let adjustment = scroll.vadjustment();
    pump_until(|| adjustment.upper() > adjustment.page_size());
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    let ids = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic automatic liked collection$",
        ])
        .output()
        .unwrap();
    assert!(ids.status.success());
    let ids = String::from_utf8(ids.stdout).unwrap();
    let id = ids.lines().next().unwrap();
    let (x, y) = test_input::pointer(window, &scroll, test_input::Target::Center);
    for (wheel, expected) in [("5", LIKED_RESULT_WINDOW), ("4", 0)] {
        input(&[
            "mousemove",
            "--window",
            id,
            &x.to_string(),
            &y.to_string(),
            "click",
            wheel,
        ]);
        pump_until(|| feed.window_start.get() == expected && !feed.window_turning.get());
        assert_eq!(row_count(&feed.rows), LIKED_RESULT_WINDOW);
    }
    adjustment.set_value(0.0);
    query(window, id, &feed, "Original 102402");
    let position = feed.data.borrow().filter.visible()[0];
    assert_eq!(feed.data.borrow().items[position].id, "102402");
    assert_eq!(row_count(&feed.rows), 1);
    assert_eq!(fixture.batches.load(Ordering::Acquire), 49);
    window.close();
    pump_until(|| !window.is_visible());
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
    this.set_fixture_resolver(crate::stream::StreamResolver::for_test(
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
    pump_until(|| bar.search.is_mapped() && bar.sort.is_mapped());
    assert!(!bar.index_action.is_visible());
    assert!(!bar.status.is_mapped());
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
    let memberships = fixture.membership.load(Ordering::Acquire);
    // Quick input followed by Clear must not admit a background manifest.
    query(window, id, &feed, "Original 10240");
    input(&["key", "--clearmodifiers", "ctrl+a", "BackSpace"]);
    pump_until(|| bar.search.text().is_empty());
    let quiet = Instant::now() + COLLECTION_SEARCH_DEBOUNCE;
    pump_until(|| Instant::now() >= quiet);
    assert_eq!(fixture.batches.load(Ordering::Acquire), before);
    assert_eq!(fixture.membership.load(Ordering::Acquire), memberships);
    assert_eq!(row_count(&feed.rows), 50);
    assert!(!bar.status.is_mapped());

    let _release_held = ReleaseHeld(fixture.released.clone());
    fixture.hold_after.store(100_200, Ordering::Release);
    query(window, id, &feed, "Original 102402");
    assert!(feed.data.borrow().filter.visible().is_empty());
    assert_eq!(
        fixture.batches.load(Ordering::Acquire),
        before,
        "local keyboard filtering does not wait for or fetch during debounce"
    );
    pump_until(|| fixture.held.load(Ordering::Acquire));
    let cursor = feed.index.borrow().cursor;
    assert!((200..2_000).contains(&cursor));
    assert_eq!(bar.index_action.label().as_deref(), Some("Пауза"));
    let active_manifest = fixture.membership.load(Ordering::Acquire);
    let active_serial = feed.index.borrow().serial;
    // The entry already owns native focus. Edit its last word in place
    // with physical key input while the next service batch is held.
    for selector in ["100042", "102402"] {
        input(&["key", "--clearmodifiers", "End", "ctrl+BackSpace"]);
        input(&["type", "--clearmodifiers", selector]);
        pump_until(|| bar.search.text().as_str() == format!("Original {selector}"));
        assert_eq!(
            feed.data.borrow().filter.visible().len(),
            usize::from(selector == "100042")
        );
    }
    assert_eq!(feed.index.borrow().serial, active_serial);
    assert!(feed.index.borrow().running);
    assert_eq!(
        fixture.membership.load(Ordering::Acquire),
        active_manifest,
        "the same in-flight batch serves the latest query without restarting the manifest"
    );
    click(window, id, &bar.index_action);
    let paused = !feed.index.borrow().running;
    fixture.released.store(true, Ordering::Release);
    assert!(paused);
    pump_until(|| !this.model.borrow().busy);
    assert_eq!(
        feed.index.borrow().cursor,
        cursor,
        "late cancelled normalization is discarded"
    );
    assert_eq!(bar.index_action.label().as_deref(), Some("Продолжить"));
    assert_eq!(this.model.borrow().queue.context(), Some(&initial));
    assert!(this.model.borrow().queue.shuffle());
    assert_eq!(
        this.model.borrow().queue.repeat_mode(),
        crate::queue::RepeatMode::Queue
    );
    // Clear restores the ordinary collection and retains a way to browse
    // beyond the prefix without restarting automatic indexing.
    query(window, id, &feed, "");
    assert!(!bar.status.is_mapped());
    let before_browse = fixture.batches.load(Ordering::Acquire);
    let browse = feed
        .status
        .last_child()
        .and_downcast::<gtk::Button>()
        .unwrap();
    assert_eq!(browse.widget_name(), "music-collection-browse-more");
    assert_eq!(browse.label().as_deref(), Some("Загрузить ещё"));
    let scroll = this.content_scroll.borrow().upgrade().unwrap();
    let adjustment = scroll.vadjustment();
    pump_until(|| adjustment.upper() > adjustment.page_size());
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    test_input::wait_for_layout(window, &browse);
    click(window, id, &browse);
    pump_until(|| feed.index.borrow().cursor == cursor + 50 && !feed.index.borrow().running);
    let quiet = Instant::now() + COLLECTION_SEARCH_DEBOUNCE;
    pump_until(|| Instant::now() >= quiet);
    assert_eq!(fixture.batches.load(Ordering::Acquire), before_browse + 1);
    adjustment.set_value(0.0);
    query(window, id, &feed, "Original 102402");
    fixture.changed.store(true, Ordering::Release);
    bar.search.grab_focus();
    input(&["key", "--clearmodifiers", "Return"]);
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
    click(window, id, &bar.index_action);
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
        bar.sort.menu_model().unwrap().n_items(),
        6,
        "actual timestamps enable date sorting"
    );
    let after_index = fixture.batches.load(Ordering::Acquire);
    query(window, id, &feed, "Original");
    assert_eq!(row_count(&feed.rows), RESULT_WINDOW);
    let popover = bar.sort.popover().unwrap();
    bar.search.grab_focus();
    input(&["key", "--clearmodifiers", "Tab", "space"]);
    pump_until(|| popover.is_mapped());
    let title = sort_option(popover.upcast_ref(), "По названию").unwrap();
    test_input::wait_for_layout(window, &title);
    title.grab_focus();
    pump_until(|| title.has_focus());
    input(&["key", "--clearmodifiers", "Return"]);
    pump_until(|| bar.selected_sort() == Some(CollectionSort::Title) && !popover.is_mapped());
    assert!(bar.sort.has_css_class("music-collection-sort-active"));
    click(window, id, &bar.sort);
    let newest = sort_option(popover.upcast_ref(), "Сначала недавно добавленные").unwrap();
    click(window, id, &newest);
    pump_until(|| bar.selected_sort() == Some(CollectionSort::AddedNewest) && !popover.is_mapped());
    assert_eq!(
        feed.data.borrow().filter.sort(),
        CollectionSort::AddedNewest
    );
    click(window, id, &bar.sort);
    let title = sort_option(popover.upcast_ref(), "По названию").unwrap();
    click(window, id, &title);
    pump_until(|| bar.selected_sort() == Some(CollectionSort::Title) && !popover.is_mapped());
    assert_eq!(this.model.borrow().queue.context(), Some(&initial));
    assert_eq!(fixture.batches.load(Ordering::Acquire), after_index);
    assert_eq!(row_count(&feed.rows), RESULT_WINDOW);
    click(window, id, &bar.next);
    assert_eq!(feed.window_start.get(), RESULT_WINDOW);
    assert_eq!(row_count(&feed.rows), RESULT_WINDOW);
    let expected = feed.rendered_positions.borrow()[0];
    assert_eq!(
        feed.data.borrow().context(expected).unwrap().items().len(),
        feed.data.borrow().filter.visible().len()
    );
    let row = feed.rows.row_at_index(0).unwrap();
    let (x, y) = test_input::pointer(window, &row, test_input::Target::Leading);
    input(&[
        "mousemove",
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
    assert_eq!(bar.selected_sort(), Some(CollectionSort::Title));
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
    let before_logout = fixture.batches.load(Ordering::Acquire);
    let before_logout_manifest = fixture.membership.load(Ordering::Acquire);
    this.logout();
    assert!(this.track_feeds.borrow().is_empty());
    bar.index_action.emit_clicked();
    settle();
    assert_eq!(fixture.batches.load(Ordering::Acquire), before_logout);
    assert_eq!(
        fixture.membership.load(Ordering::Acquire),
        before_logout_manifest
    );
    assert!(!this.model.borrow().account_valid && this.model.borrow().queue.items().is_empty());
    window.close();
    pump_until(|| !window.is_visible());
}
