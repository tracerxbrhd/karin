// SPDX-License-Identifier: GPL-3.0-or-later
//! Rendered search retry, normalized results and useful list density.

use super::tests::{capture_native, contains_text, pump_until};
use super::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Counts {
    searches: Vec<(String, u32)>,
    batches: Vec<Vec<String>>,
    offers: std::collections::BTreeMap<String, usize>,
    accounts: usize,
}

fn input(arguments: &[&str]) {
    assert!(
        std::process::Command::new("xdotool")
            .args(arguments)
            .status()
            .expect("xdotool is required for native search input")
            .success()
    );
}

fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut found = vec![widget.clone()];
    let mut child = widget.first_child();
    while let Some(widget) = child {
        found.extend(descendants(&widget));
        child = widget.next_sibling();
    }
    found
}

fn named_button(widget: &gtk::Widget, name: &str) -> gtk::Button {
    descendants(widget)
        .into_iter()
        .find(|widget| widget.widget_name() == name)
        .and_then(|widget| widget.downcast().ok())
        .expect("rendered native search action")
}

fn settle() {
    let deadline = Instant::now() + Duration::from_millis(120);
    pump_until(|| Instant::now() >= deadline);
}

fn wait_for_step(step: &'static str, condition: impl FnMut() -> bool) {
    eprintln!("Karin synthetic search step={step} state=waiting");
    pump_until(condition);
    eprintln!("Karin synthetic search step={step} state=complete");
}

/// A mouse user scrolls a long unified page before clicking a lower section.
/// A direct grab_focus succeeds even when a lower section is still outside the
/// viewport; it is not evidence that a mouse target has become visible.
/// Use actual wheel events and observed geometry, never adjustment teleporting.
fn reveal_by_wheel(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref();
    if widget.native().and_then(|native| native.surface()) != window.surface() {
        // Native popups are positioned by GTK; scrolling their parent while a
        // popover is open would test a different action and coordinate frame.
        return;
    }
    let mut parent = widget.parent();
    let mut scroller = None;
    while let Some(current) = parent {
        if let Some(found) = current.downcast_ref::<gtk::ScrolledWindow>() {
            scroller = Some(found.clone());
            break;
        }
        parent = current.parent();
    }
    let Some(scroller) = scroller else { return };
    super::test_input::wait_for_layout(window, widget);
    for _ in 0..24 {
        let bounds = widget.compute_bounds(window).expect("mapped scroll target");
        let viewport = scroller.compute_bounds(window).expect("mapped scroller");
        if bounds.y() >= viewport.y() + 4.0
            && bounds.y() + bounds.height() <= viewport.y() + viewport.height() - 4.0
        {
            return;
        }
        let direction = if bounds.y() < viewport.y() + 4.0 {
            "4"
        } else {
            "5"
        };
        let (x, y) =
            super::test_input::pointer(window, &scroller, super::test_input::Target::Center);
        // One X connection preserves move-before-wheel order. --sync would
        // wait unnecessarily when another notch uses the same pointer point.
        input(&[
            "mousemove",
            "--window",
            id,
            &x.to_string(),
            &y.to_string(),
            "click",
            direction,
        ]);
        settle();
    }
    panic!(
        "synthetic wheel could not expose type={} name={} target={:?} viewport={:?}",
        widget.type_().name(),
        widget.widget_name(),
        widget.compute_bounds(window),
        scroller.compute_bounds(window),
    );
}

fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    reveal_by_wheel(window, id, widget);
    let (x, y) = super::test_input::pointer(window, widget, super::test_input::Target::Leading);
    let widget = widget.as_ref();
    if widget.native().and_then(|native| native.surface()) == window.surface() {
        // Allocation alone is insufficient: a pinned nonmodal pane can still
        // cover a catalog action. Check GTK's real event target before sending
        // XTest input; popup targets have their own native coordinate system.
        let scale = f64::from(window.scale_factor());
        let (dx, dy) = window.surface_transform();
        let picked = window.pick(
            f64::from(x) / scale - dx,
            f64::from(y) / scale - dy,
            gtk::PickFlags::DEFAULT,
        );
        let reaches_target = picked
            .as_ref()
            .is_some_and(|picked| picked == widget || picked.is_ancestor(widget));
        if !reaches_target {
            capture_native("search-native-action-occluded");
        }
        assert!(
            reaches_target,
            "native click target {} ({}) is covered by {:?}",
            widget.widget_name(),
            widget.type_().name(),
            picked
                .as_ref()
                .map(|picked| (picked.widget_name(), picked.type_().name())),
        );
    }
    input(&[
        "mousemove",
        "--sync",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
    ]);
    input(&["click", "1"]);
    settle();
}

fn click_expander_heading(window: &adw::ApplicationWindow, id: &str, expander: &gtk::Expander) {
    let (x, y) = super::test_input::pointer(
        window,
        expander,
        super::test_input::Target::DisclosureHeading,
    );
    input(&[
        "mousemove",
        "--sync",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
    ]);
    input(&["click", "1"]);
    settle();
}

fn rows(list: &gtk::ListBox) -> Vec<gtk::ListBoxRow> {
    let mut found = Vec::new();
    let mut child = list.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        found.push(widget.downcast().expect("catalog result row"));
    }
    found
}

fn mapped_text(widget: &gtk::Widget) -> String {
    descendants(widget)
        .into_iter()
        .filter(|widget| widget.is_mapped())
        .filter_map(|widget| widget.downcast::<gtk::Label>().ok())
        .map(|label| label.text().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalized_track(id: &str) -> serde_json::Value {
    json!({
        "id":id,
        "title":format!("Normalized synthetic track {id}"),
        "available":id != "47",
        "durationMs":123000,
        "artists":[{"id":11,"name":"Synthetic artist"}]
    })
}

fn section(this: &Rc<Controller>, name: &str) -> gtk::Box {
    descendants(this.widgets.search_sections.upcast_ref())
        .into_iter()
        .find(|widget| widget.widget_name() == name)
        .and_then(|widget| widget.downcast().ok())
        .expect("rendered unified section")
}

fn labelled_button(widget: &gtk::Widget, label: &str) -> gtk::Button {
    descendants(widget)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some(label))
        .expect("explicit section action")
}

fn open_details(window: &adw::ApplicationWindow, id: &str, row: &gtk::ListBoxRow) {
    assert!(row.grab_focus());
    settle();
    reveal_by_wheel(window, id, row);
    let menu = descendants(row.upcast_ref())
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk::MenuButton>().ok())
        .unwrap();
    click(window, id, &menu);
    wait_for_step("track-actions-open", || {
        menu.popover().unwrap().is_visible()
    });
    let details = labelled_button(
        menu.popover().unwrap().upcast_ref(),
        "Открыть сведения о треке",
    );
    click(window, id, &details);
}

fn fixture_hits(page: u32) -> Vec<serde_json::Value> {
    let ids: &[u64] = if page == 0 {
        &[50, 42, 43, 44, 45, 46, 47, 48, 49, 51, 52, 53, 42, 54]
    } else {
        &[60]
    };
    ids.iter().enumerate().map(|(index,id)| json!({
        "id":if index%2==0 {json!(id)}else{json!(id.to_string())},
        "title":{"ignored":"synthetic-search-display"},"available":[false,null],
        "artists":[null,false,17,{"name":["odd","display"],"id":{"nested":"display"}}],
        "albums":false,"durationMs":{"not":"duration"},"explicit":[true,false],"coverUri":{"not":"image"}
    })).collect()
}

#[test]
#[ignore = "requires an isolated GTK/X11 display and xdotool; run explicitly under Xvfb"]
fn native_search_retry_normalized_results_and_density() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let counts = Arc::new(Mutex::new(Counts::default()));
    let server = {
        let stop = stop.clone();
        let counts = counts.clone();
        thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(stream) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(_) => panic!("synthetic search fixture listener failed"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap() == 0 {
                        break;
                    }
                    bytes.push(byte[0]);
                    assert!(bytes.len() < 16 * 1024);
                }
                if !bytes.ends_with(b"\r\n\r\n") {
                    continue;
                }
                let request = String::from_utf8(bytes).unwrap();
                assert!(request.contains("authorization: OAuth synthetic-search-ux-token"));
                assert!(
                    !request
                        .to_ascii_lowercase()
                        .contains("x-yandex-music-client:")
                );
                assert!(!request.to_ascii_lowercase().contains("cookie:"));
                let length = request
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                assert!(length < 16 * 1024);
                let mut form = vec![0; length];
                stream.read_exact(&mut form).unwrap();
                let target = request
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).unwrap();
                let (status, body) = match url.path() {
                    "/account/status" => {
                        counts.lock().unwrap().accounts += 1;
                        (
                            200,
                            json!({"result":{"account":{"uid":1234,"displayName":"Synthetic search account"}}}),
                        )
                    }
                    "/search" => {
                        assert!(request.starts_with("GET /search?"));
                        let kind = url
                            .query_pairs()
                            .find(|(k, _)| k == "type")
                            .unwrap()
                            .1
                            .into_owned();
                        let page = url
                            .query_pairs()
                            .find(|(k, _)| k == "page")
                            .unwrap()
                            .1
                            .parse::<u32>()
                            .unwrap();
                        let partial = url
                            .query_pairs()
                            .any(|(k, v)| k == "text" && v == "partial-query");
                        let first = {
                            let mut c = counts.lock().unwrap();
                            c.searches.push((kind.clone(), page));
                            c.searches.len() == 1
                        };
                        if first {
                            (
                                503,
                                json!({"error":{"name":"synthetic-private-search-name","message":"synthetic-private-search-response"}}),
                            )
                        } else {
                            let track_section = json!({"page":page,"perPage":20,"total":40,"results":fixture_hits(page)});
                            let album_section = json!({"page":page,"perPage":20,"total":1,"results":[{"id":7,"title":"Synthetic search album","artists":[]}]});
                            let artist_section = json!({"page":page,"perPage":20,"total":1,"results":[{"id":11,"name":"Synthetic search artist"}]});
                            // No trackCount is deliberately different from an empty playlist.
                            let playlist_section = json!({"page":page,"perPage":20,"total":1,"results":[{"kind":3,"title":"Synthetic search playlist","owner":{"uid":1234}}]});
                            let result = match kind.as_str() {
                                "all" => {
                                    json!({"text":"Synthetic", "tracks":track_section,"albums":if partial{json!({"results":"synthetic-private-section"})}else{album_section},"artists":artist_section,"playlists":playlist_section})
                                }
                                "track" => json!({"text":"Synthetic","tracks":track_section}),
                                "album" => json!({"text":"Synthetic","albums":album_section}),
                                "artist" => json!({"text":"Synthetic","artists":artist_section}),
                                "playlist" => {
                                    json!({"text":"Synthetic","playlists":playlist_section})
                                }
                                _ => panic!("unexpected synthetic search category"),
                            };
                            (200, json!({"result":result}))
                        }
                    }
                    "/tracks" => {
                        assert!(request.starts_with("POST /tracks "));
                        let form = String::from_utf8(form).unwrap();
                        let encoded =
                            reqwest::Url::parse(&format!("http://127.0.0.1/?{form}")).unwrap();
                        let ids = encoded
                            .query_pairs()
                            .filter(|(k, _)| k == "track-ids")
                            .map(|(_, v)| v.into_owned())
                            .collect::<Vec<_>>();
                        assert!(!ids.is_empty() && ids.len() <= 50);
                        assert_eq!(
                            ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
                            ids.len()
                        );
                        counts.lock().unwrap().batches.push(ids.clone());
                        let values = ids
                            .iter()
                            .rev()
                            .map(|id| {
                                if id == "54" {
                                    serde_json::Value::Null
                                } else {
                                    normalized_track(id)
                                }
                            })
                            .collect::<Vec<_>>();
                        (200, json!({"result":values}))
                    }
                    // An explicitly unavailable loopback offer proves activation
                    // and context only. It cannot fetch or claim real service audio.
                    path if path.starts_with("/tracks/") && path.ends_with("/download-info") => {
                        let id = path
                            .strip_prefix("/tracks/")
                            .unwrap()
                            .strip_suffix("/download-info")
                            .unwrap();
                        assert!(
                            [
                                "50", "42", "43", "44", "45", "46", "47", "48", "49", "51", "52",
                                "53", "60"
                            ]
                            .contains(&id)
                        );
                        *counts
                            .lock()
                            .unwrap()
                            .offers
                            .entry(id.to_owned())
                            .or_default() += 1;
                        (200, json!({"result":[]}))
                    }
                    "/users/1234/likes/tracks" => (
                        200,
                        json!({"result":{"library":{"uid":1234,"revision":1,"tracks":[]}}}),
                    ),
                    "/users/1234/playlists/3" => (
                        200,
                        json!({"result":{"kind":3,"trackCount":1,"tracks":[{"id":50,"track":normalized_track("50")}]}}),
                    ),
                    "/albums/7/with-tracks" => (
                        200,
                        json!({"result":{"id":7,"title":"Synthetic search album","artists":[],"volumes":[[normalized_track("50")]]}}),
                    ),
                    "/artists/11/tracks" => (
                        200,
                        json!({"result":{"tracks":[normalized_track("50")],"pager":{"page":0,"perPage":20,"total":1}}}),
                    ),
                    _ => panic!("unexpected synthetic search fixture route"),
                };
                let body = body.to_string();
                if let Err(error) = write!(
                    stream,
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                ) {
                    assert!(
                        matches!(
                            error.kind(),
                            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                        ),
                        "unexpected synthetic fixture write failure"
                    );
                }
            }
        })
    };
    let service = MusicService::for_test(&format!("http://{address}/"));
    let token = AccessToken::for_test("synthetic-search-ux-token", Duration::from_secs(300));
    service.account(&token).unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.SearchUXFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let ui = super::super::build_with_music_pane(
        &app,
        crate::application::ConnectionService::new(crate::api::YandexApi::new().unwrap()),
        |app, window| {
            MusicPane::with_provider(
                app,
                window,
                Some(service),
                Arc::new(OfflineProvider),
                launch_system_browser,
            )
        },
    );
    let window = &ui.window;
    window.set_title(Some("Karin synthetic HTTP fixture"));
    window.set_default_size(900, 650);
    window.present();
    let pane = &ui.music;
    let this = &pane.controller;
    let origin = format!("http://{address}/");
    this.set_fixture_resolver_for_test(crate::stream::StreamResolver::for_test(
        &origin, &origin, &origin,
    ));
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token);
        model.account_valid = true;
    }
    this.render_controls();
    this.widgets.stack.set_visible_child_name("search");
    pump_until(|| this.widgets.search.is_mapped());
    let found = std::process::Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic HTTP fixture$",
        ])
        .output()
        .unwrap();
    assert!(found.status.success());
    let found = String::from_utf8(found.stdout).unwrap();
    let id = found.lines().next().unwrap();
    assert!(id.bytes().all(|b| b.is_ascii_digit()));
    input(&["windowfocus", "--sync", id]);
    click(window, id, &this.widgets.search);
    input(&["type", "--clearmodifiers", "synthetic-query"]);
    input(&["key", "--clearmodifiers", "Return"]);
    wait_for_step("initial-search-error", || {
        counts.lock().unwrap().searches.len() == 1 && !this.model.borrow().busy
    });
    assert_eq!(counts.lock().unwrap().searches[0], ("all".into(), 0));
    let retry = named_button(
        this.widgets.search_feedback.upcast_ref(),
        "music-search-retry",
    );
    pump_until(|| retry.is_mapped());
    assert!(this.widgets.search_rows.first_child().is_none());
    assert!(!this.widgets.status_revealer.reveals_child());
    assert_eq!(
        descendants(this.widgets.search_feedback.upcast_ref())
            .into_iter()
            .filter(|w| w.widget_name() == "music-search-error")
            .count(),
        1
    );
    let details = descendants(this.widgets.search_feedback.upcast_ref())
        .into_iter()
        .find_map(|w| w.downcast::<gtk::Expander>().ok())
        .unwrap();
    assert!(!details.is_expanded());
    let plain = mapped_text(this.widgets.search_feedback.upcast_ref());
    assert!(plain.contains("Не удалось выполнить поиск"));
    assert!(
        !plain.contains("HTTP")
            && !plain.contains("track-wire")
            && !plain.contains("http-rejected")
    );
    for value in [
        "synthetic-private-search-name",
        "synthetic-private-search-response",
        "synthetic-search-ux-token",
    ] {
        assert!(!contains_text(window.upcast_ref(), value));
    }
    capture_native("search-error-retry-collapsed");
    click_expander_heading(window, id, &details);
    wait_for_step("expand-safe-diagnostics", || details.is_expanded());
    assert!(contains_text(details.upcast_ref(), "HTTP: 503"));
    click_expander_heading(window, id, &details);
    wait_for_step("collapse-safe-diagnostics", || !details.is_expanded());
    click(window, id, &retry);
    wait_for_step("unified-normalized-search", || {
        counts.lock().unwrap().searches.len() == 2 && !this.model.borrow().busy
    });
    assert!(this.model.borrow().search_scope.is_none());
    for name in [
        "music-results-tracks",
        "music-results-albums",
        "music-results-artists",
        "music-results-playlists",
    ] {
        let group = section(this, name);
        assert!(group.is_visible());
    }
    assert_eq!(
        rows(&this.widgets.search_rows).len(),
        5,
        "unified track preview has a bounded useful section"
    );
    let playlist_group = section(this, "music-results-playlists");
    assert!(contains_text(
        playlist_group.upcast_ref(),
        "Synthetic search playlist"
    ));
    assert!(!contains_text(playlist_group.upcast_ref(), "0 треков"));
    {
        let c = counts.lock().unwrap();
        assert_eq!(c.accounts, 1);
        assert_eq!(c.batches.len(), 1);
        assert_eq!(
            c.batches[0].len(),
            13,
            "duplicate positions share one batch, missing54 remains omitted"
        );
    }
    capture_native("search-unified-sections");
    let group = section(this, "music-results-tracks");
    let show_all = named_button(group.upcast_ref(), "music-search-show-all");
    assert!(show_all.grab_focus());
    settle();
    click(window, id, &show_all);
    wait_for_step("track-section-show-all", || {
        counts.lock().unwrap().searches.len() == 3 && !this.model.borrow().busy
    });
    assert_eq!(this.model.borrow().search_scope, Some(SearchKind::Tracks));
    assert!(this.widgets.search_all.is_visible());
    let result_rows = rows(&this.widgets.search_rows);
    assert_eq!(result_rows.len(), 13);
    let expected = [50, 42, 43, 44, 45, 46, 47, 48, 49, 51, 52, 53, 42];
    for (row, track_id) in result_rows.iter().zip(expected) {
        assert!(contains_text(
            row.upcast_ref(),
            &format!("Normalized synthetic track {track_id}")
        ));
        assert!(!contains_text(row.upcast_ref(), "synthetic-search-display"));
        assert_eq!(row.is_activatable(), track_id != 47);
        assert_eq!(
            named_button(row.upcast_ref(), "music-track-play").is_sensitive(),
            track_id != 47
        );
    }
    assert!(contains_text(
        this.widgets.search_feedback.upcast_ref(),
        "Часть найденных треков сейчас недоступна"
    ));
    assert!(this.widgets.search_next.is_visible());
    assert!(!this.widgets.search_previous.is_visible());
    this.widgets.search.grab_focus();
    settle();
    let footer = pane.player_bar.compute_bounds(window).unwrap();
    let viewport = {
        let mut parent = this.widgets.search_rows.parent();
        let mut viewport = None;
        while let Some(widget) = parent {
            if widget.is::<gtk::ScrolledWindow>() {
                viewport = widget.compute_bounds(window);
                break;
            }
            parent = widget.parent();
        }
        viewport.expect("search tracks are inside the ordinary scroll viewport")
    };
    assert!(!this.widgets.cache_notice.is_revealed());
    assert!(
        !this.widgets.cache_notice.is_visible(),
        "a hidden cache notice must not consume an extra GtkBox interval"
    );
    let visible = result_rows
        .iter()
        .filter(|row| {
            row.compute_bounds(window).is_some_and(|b| {
                b.y() >= viewport.y() - 1.0
                    && b.y() + b.height() <= footer.y().min(viewport.y() + viewport.height()) + 1.0
            })
        })
        .count();
    assert!(
        visible >= 6,
        "900x650 should expose at least six complete track rows, got {visible}"
    );
    for row in result_rows.iter().take(visible) {
        assert!(
            (60..=72).contains(&row.height()),
            "song row height must preserve the approved compact music rhythm"
        );
        let play = named_button(row.upcast_ref(), "music-track-play");
        let target = play.compute_bounds(window).unwrap();
        assert!(
            target.width() >= 32.0 && target.height() >= 32.0,
            "Play remains a native usable hit target"
        );
    }
    capture_native("search-normalized-dense-medium");
    // Mouse/Enter/Space are primary playback actions. In particular a duplicate
    // ID at occurrence12 must not silently select its first occurrence1.
    for (index, key) in [(0, None), (12, Some("Return")), (1, Some("space"))] {
        let track_id = expected[index].to_string();
        let before = *counts.lock().unwrap().offers.get(&track_id).unwrap_or(&0);
        let row = &result_rows[index];
        assert!(row.grab_focus());
        settle();
        reveal_by_wheel(window, id, row);
        if let Some(key) = key {
            input(&["key", "--clearmodifiers", key]);
        } else {
            click(window, id, row);
        }
        pump_until(|| {
            *counts.lock().unwrap().offers.get(&track_id).unwrap_or(&0) == before + 1
                && !this.music_requested()
        });
        let model = this.model.borrow();
        assert_eq!(model.queue.current_index(), Some(index));
        assert_eq!(model.queue.items().len(), 13);
        assert_eq!(model.queue.current().unwrap().id, track_id);
        assert_eq!(
            model.queue.context().unwrap().source_kind(),
            ContextKind::Search
        );
        assert!(model.queue.context().unwrap().is_partial());
        assert_eq!(
            this.widgets.current.text(),
            format!("Normalized synthetic track {track_id}")
        );
        drop(model);
        assert_eq!(
            this.widgets.stack.visible_child_name().as_deref(),
            Some("search")
        );
        this.stop_music();
    }
    assert!(
        this.info_is_open(),
        "the ordinary pinned medium pane remains open while catalog actions stay usable"
    );
    assert_eq!(
        counts.lock().unwrap().batches.len(),
        2,
        "row playback does not do per-row metadata lookup"
    );
    open_details(window, id, &result_rows[0]);
    pump_until(|| {
        this.widgets.stack.visible_child_name().as_deref() == Some("detail")
            && !this.model.borrow().busy
    });
    assert!(contains_text(
        this.widgets.detail.upcast_ref(),
        "Normalized synthetic track 50"
    ));
    assert!(named_button(this.widgets.detail.upcast_ref(), "music-track-play").is_sensitive());
    assert_eq!(counts.lock().unwrap().batches.len(), 3);
    capture_native("search-normalized-track-detail");
    input(&["key", "--clearmodifiers", "alt+Left"]);
    pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("search"));
    for (width, height, name) in [(420, 720, "compact"), (1440, 900, "wide")] {
        window.set_default_size(width, height);
        pump_until(|| {
            window
                .surface()
                .is_some_and(|s| s.width() == width && s.height() == height)
        });
        this.widgets.search.grab_focus();
        settle();
        capture_native(&format!("search-normalized-{name}"));
    }
    window.set_default_size(900, 650);
    pump_until(|| {
        window
            .surface()
            .is_some_and(|s| s.width() == 900 && s.height() == 650)
    });
    let next = this.widgets.search_next.clone();
    assert!(next.grab_focus());
    settle();
    click(window, id, &next);
    pump_until(|| counts.lock().unwrap().searches.len() == 4 && !this.model.borrow().busy);
    assert_eq!(this.model.borrow().search_page, 1);
    assert!(this.widgets.search_previous.is_visible());
    assert!(!this.widgets.search_next.is_visible());
    assert_eq!(rows(&this.widgets.search_rows).len(), 1);
    assert!(contains_text(
        this.widgets.search_rows.upcast_ref(),
        "Normalized synthetic track 60"
    ));
    assert_eq!(counts.lock().unwrap().batches.len(), 4);
    assert!(!this.widgets.search_feedback.is_visible());
    assert!(this.widgets.search_all.grab_focus());
    settle();
    click(window, id, &this.widgets.search_all);
    pump_until(|| counts.lock().unwrap().searches.len() == 5 && !this.model.borrow().busy);
    assert!(this.model.borrow().search_scope.is_none());
    assert_eq!(counts.lock().unwrap().batches.len(), 5);
    for (name, kind, source) in [
        ("music-results-albums", ContextKind::Album, "7"),
        ("music-results-artists", ContextKind::Artist, "11"),
        ("music-results-playlists", ContextKind::Playlist, "1234:3"),
    ] {
        let group = section(this, name);
        let tile_name = match kind {
            ContextKind::Album => "music-search-album-tile",
            ContextKind::Artist => "music-search-artist-tile",
            ContextKind::Playlist => "music-search-playlist-tile",
            _ => unreachable!("only collection result sections are exercised here"),
        };
        let tile = named_button(group.upcast_ref(), tile_name);
        assert!(tile.grab_focus());
        settle();
        eprintln!(
            "Karin synthetic section: phase={name} bounds={:?} focused={} focusable={}",
            tile.compute_bounds(window),
            tile.has_focus(),
            tile.is_focusable(),
        );
        click(window, id, &tile);
        pump_until(|| {
            this.widgets.stack.visible_child_name().as_deref() == Some("detail")
                && !this.model.borrow().busy
        });
        let track_rows = descendants(this.widgets.detail.upcast_ref())
            .into_iter()
            .find_map(|w| w.downcast::<gtk::ListBox>().ok())
            .unwrap();
        let selected = track_rows.row_at_index(0).unwrap();
        assert!(selected.grab_focus());
        settle();
        let before = *counts.lock().unwrap().offers.get("50").unwrap_or(&0);
        click(window, id, &selected);
        pump_until(|| {
            *counts.lock().unwrap().offers.get("50").unwrap_or(&0) == before + 1
                && !this.music_requested()
        });
        {
            let m = this.model.borrow();
            assert_eq!(m.queue.context().unwrap().source_kind(), kind);
            assert_eq!(m.queue.context().unwrap().source_id(), Some(source));
            assert_eq!(m.queue.current().unwrap().id, "50");
            assert_eq!(m.queue.items().len(), 1);
        }
        this.stop_music();
        input(&["key", "--clearmodifiers", "alt+Left"]);
        pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("search"));
    }
    // A section contract failure keeps successful sibling sections. Its actual
    // Retry opens only that category and must not refetch the track batch.
    click(window, id, &this.widgets.search);
    input(&["key", "--clearmodifiers", "ctrl+a"]);
    input(&["type", "--clearmodifiers", "partial-query"]);
    input(&["key", "--clearmodifiers", "Return"]);
    wait_for_step("partial-unified-search", || {
        counts.lock().unwrap().searches.len() == 6 && !this.model.borrow().busy
    });
    assert!(this.model.borrow().search_scope.is_none());
    assert_eq!(rows(&this.widgets.search_rows).len(), 5);
    assert!(!this.widgets.search_feedback.is_visible());
    for name in [
        "music-results-tracks",
        "music-results-artists",
        "music-results-playlists",
    ] {
        assert!(section(this, name).is_visible());
    }
    let failed = section(this, "music-results-albums");
    assert!(contains_text(
        failed.upcast_ref(),
        "Не удалось загрузить этот раздел"
    ));
    assert!(!contains_text(failed.upcast_ref(), "HTTP"));
    assert!(!contains_text(
        window.upcast_ref(),
        "synthetic-private-section"
    ));
    let retry = labelled_button(failed.upcast_ref(), "Повторить");
    assert!(retry.grab_focus());
    settle();
    capture_native("search-partial-section-retry");
    let batches = counts.lock().unwrap().batches.len();
    click(window, id, &retry);
    wait_for_step("partial-album-retry", || {
        counts.lock().unwrap().searches.len() == 7 && !this.model.borrow().busy
    });
    assert_eq!(this.model.borrow().search_scope, Some(SearchKind::Albums));
    assert!(contains_text(
        this.widgets.search_rows.upcast_ref(),
        "Synthetic search album"
    ));
    assert_eq!(counts.lock().unwrap().batches.len(), batches);
    assert_eq!(
        counts.lock().unwrap().searches.last().unwrap(),
        &("album".into(), 0)
    );
    // The compact collection card remains a real keyboard target after a
    // section-specific Retry; the inert wrapping row must never consume Play.
    let album = named_button(
        this.widgets.search_rows.upcast_ref(),
        "music-search-album-tile",
    );
    assert!(album.grab_focus());
    settle();
    input(&["key", "--clearmodifiers", "space"]);
    wait_for_step("retried-album-keyboard-open", || {
        this.widgets.stack.visible_child_name().as_deref() == Some("detail")
            && !this.model.borrow().busy
    });
    assert!(contains_text(
        this.widgets.detail.upcast_ref(),
        "Synthetic search album"
    ));
    assert_eq!(this.model.borrow().detail_origin, "search");
    let before_back = counts.lock().unwrap().searches.len();
    // Alt+Left is exercised after each earlier detail route. Here use the
    // visible native Back action after a keyboard-opened refreshed tile: its
    // old focus owner was removed while replacing loading/detail content.
    // This also proves the pinned pane does not cover collection navigation.
    let back = named_button(this.widgets.detail.upcast_ref(), "music-detail-back");
    click(window, id, &back);
    wait_for_step("retried-album-native-back", || {
        this.widgets.stack.visible_child_name().as_deref() == Some("search")
    });
    assert_eq!(this.model.borrow().search_scope, Some(SearchKind::Albums));
    assert_eq!(counts.lock().unwrap().searches.len(), before_back);
    pane.close();
    window.close();
    stop.store(true, Ordering::Release);
    server.join().unwrap();
}
