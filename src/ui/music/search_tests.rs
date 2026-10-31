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
    searches: usize,
    batches: Vec<Vec<String>>,
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

fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref();
    pump_until(|| widget.is_mapped() && widget.width() > 0 && widget.height() > 0);
    let bounds = widget
        .compute_bounds(window)
        .expect("native input allocation");
    // For a list row, target its title rather than a right-hand nested action.
    let x = (bounds.x() + bounds.width().min(160.0) / 2.0) as i32;
    let y = (bounds.y() + bounds.height() / 2.0) as i32;
    assert!(x >= 0 && x < window.width() && y >= 0 && y < window.height());
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
    let bounds = expander
        .compute_bounds(window)
        .expect("diagnostic disclosure allocation");
    // The expanded child changes the expander's total height. Click its header
    // line both times, rather than selecting text in the expanded support code.
    let x = (bounds.x() + bounds.width().min(160.0) / 2.0) as i32;
    let y = (bounds.y() + 10.0) as i32;
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
                    assert_eq!(stream.read(&mut byte).unwrap(), 1);
                    bytes.push(byte[0]);
                    assert!(bytes.len() < 16 * 1024);
                }
                let request = String::from_utf8(bytes).unwrap();
                assert!(request.contains("authorization: OAuth synthetic-search-ux-token"));
                assert!(
                    !request
                        .to_ascii_lowercase()
                        .contains("x-yandex-music-client:")
                );
                let length = request
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse::<usize>().unwrap())
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
                        let number = {
                            let mut counts = counts.lock().unwrap();
                            counts.searches += 1;
                            counts.searches
                        };
                        if number == 1 {
                            (
                                503,
                                json!({"error":{"name":"synthetic-private-search-name","message":"synthetic-private-search-response"}}),
                            )
                        } else {
                            let page = url
                                .query_pairs()
                                .find(|(name, _)| name == "page")
                                .unwrap()
                                .1
                                .parse::<u32>()
                                .unwrap();
                            let ids: &[u64] = if page == 0 {
                                &[50, 42, 43, 44, 45, 46, 47, 48, 49, 51, 52, 53, 42, 54]
                            } else {
                                &[60]
                            };
                            let hits = ids.iter().enumerate().map(|(index,id)| json!({
                                "id":if index % 2 == 0 { json!(id) } else { json!(id.to_string()) },
                                "title":{"ignored":"synthetic-search-display"},
                                "available":[false,null],
                                "artists":[null,false,17,{"name":["odd","display"],"id":{"nested":"display"}}],
                                "albums":false,
                                "durationMs":{"not":"duration"},
                                "explicit":[true,false],
                                "coverUri":{"not":"image"}
                            })).collect::<Vec<_>>();
                            (
                                200,
                                json!({"result":{"tracks":{"page":page,"perPage":20,"total":40,"results":hits}}}),
                            )
                        }
                    }
                    "/tracks" => {
                        assert!(request.starts_with("POST /tracks "));
                        let form = String::from_utf8(form).unwrap();
                        let encoded =
                            reqwest::Url::parse(&format!("http://127.0.0.1/?{form}")).unwrap();
                        let ids = encoded
                            .query_pairs()
                            .filter(|(name, _)| name == "track-ids")
                            .map(|(_, value)| value.into_owned())
                            .collect::<Vec<_>>();
                        assert!(!ids.is_empty() && ids.len() <= 50);
                        assert_eq!(
                            ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
                            ids.len()
                        );
                        counts.lock().unwrap().batches.push(ids.clone());
                        // Deliberately reverse canonical response order; a
                        // missing track stays a truthful partial result.
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
                    "/users/1234/likes/tracks" => (
                        200,
                        json!({"result":{"library":{"uid":1234,"revision":1,"tracks":[]}}}),
                    ),
                    _ => panic!("unexpected synthetic search fixture route"),
                };
                let body = body.to_string();
                write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
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
    assert!(id.bytes().all(|byte| byte.is_ascii_digit()));
    input(&["windowfocus", "--sync", id]);
    click(window, id, &this.widgets.search);
    input(&["type", "--clearmodifiers", "synthetic-query"]);
    input(&["key", "--clearmodifiers", "Return"]);
    pump_until(|| counts.lock().unwrap().searches == 1 && !this.model.borrow().busy);
    let retry = named_button(
        this.widgets.search_feedback.upcast_ref(),
        "music-search-retry",
    );
    pump_until(|| retry.is_mapped());
    assert_eq!(this.widgets.search.text(), "synthetic-query");
    assert!(this.widgets.search_rows.first_child().is_none());
    assert!(!this.widgets.status_revealer.reveals_child());
    assert_eq!(
        descendants(this.widgets.search_feedback.upcast_ref())
            .into_iter()
            .filter(|widget| widget.widget_name() == "music-search-error")
            .count(),
        1
    );
    let details = descendants(this.widgets.search_feedback.upcast_ref())
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk::Expander>().ok())
        .unwrap();
    assert!(!details.is_expanded());
    let plain = mapped_text(this.widgets.search_feedback.upcast_ref());
    assert!(plain.contains("Не удалось выполнить поиск"));
    assert!(
        !plain.contains("HTTP")
            && !plain.contains("http-rejected")
            && !plain.contains("track-wire")
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
    pump_until(|| details.is_expanded());
    assert!(contains_text(details.upcast_ref(), "HTTP: 503"));
    assert!(!contains_text(details.upcast_ref(), "synthetic-private"));
    click_expander_heading(window, id, &details);
    pump_until(|| !details.is_expanded());
    click(window, id, &retry);
    pump_until(|| counts.lock().unwrap().searches == 2 && !this.model.borrow().busy);
    let result_rows = rows(&this.widgets.search_rows);
    assert_eq!(result_rows.len(), 13);
    let expected = [50, 42, 43, 44, 45, 46, 47, 48, 49, 51, 52, 53, 42];
    for (row, id) in result_rows.iter().zip(expected) {
        assert!(contains_text(
            row.upcast_ref(),
            &format!("Normalized synthetic track {id}")
        ));
        assert!(!contains_text(row.upcast_ref(), "synthetic-search-display"));
        let play = named_button(row.upcast_ref(), "music-track-play");
        assert_eq!(play.is_sensitive(), id != 47);
    }
    {
        let counts = counts.lock().unwrap();
        assert_eq!(counts.accounts, 1);
        assert_eq!(
            counts.batches.len(),
            1,
            "one normalized batch, never per-row requests"
        );
        assert_eq!(
            counts.batches[0].len(),
            13,
            "duplicate positions share one canonical lookup"
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
    let visible = result_rows
        .iter()
        .filter(|row| {
            let bounds = row.compute_bounds(window).unwrap();
            bounds.y() >= 0.0 && bounds.y() + bounds.height() <= footer.y()
        })
        .count();
    assert!(
        visible >= 6,
        "900x650 should expose at least six complete track rows, got {visible}"
    );
    for row in result_rows.iter().take(visible) {
        assert!(row.height() >= 48);
        let play = named_button(row.upcast_ref(), "music-track-play");
        // GtkWidget's content width excludes its themed padding/border. Input
        // targets use the complete native allocation, as the mouse helper does.
        let target = play
            .compute_bounds(window)
            .expect("allocated Play hit target");
        if row.index() == 0 {
            eprintln!(
                "Karin synthetic density: complete-rows={visible} Play-content={}x{} Play-target={}x{}",
                play.width(),
                play.height(),
                target.width(),
                target.height(),
            );
        }
        assert!(
            target.width() >= 32.0 && target.height() >= 32.0,
            "Play target is too small: content={}x{}, allocated={}x{}",
            play.width(),
            play.height(),
            target.width(),
            target.height(),
        );
    }
    capture_native("search-normalized-dense-medium");
    click(window, id, &result_rows[0]);
    pump_until(|| {
        this.widgets.stack.visible_child_name().as_deref() == Some("detail")
            && !this.model.borrow().busy
    });
    assert!(contains_text(
        this.widgets.detail.upcast_ref(),
        "Normalized synthetic track 50"
    ));
    assert!(named_button(this.widgets.detail.upcast_ref(), "music-track-play").is_sensitive());
    assert_eq!(
        counts.lock().unwrap().batches.len(),
        2,
        "opening one detail is one independent lookup"
    );
    capture_native("search-normalized-track-detail");
    input(&["key", "--clearmodifiers", "alt+Left"]);
    pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("search"));
    assert_eq!(rows(&this.widgets.search_rows).len(), 13);
    for (width, height, name) in [(420, 720, "compact"), (1440, 900, "wide")] {
        window.set_default_size(width, height);
        if width == 420 {
            pump_until(|| window.width() <= 430);
        } else {
            pump_until(|| window.width() >= 1400);
        }
        this.widgets.search.grab_focus();
        settle();
        capture_native(&format!("search-normalized-{name}"));
    }
    // The original search pager survives omitted canonical records; navigating
    // forward is still one search + one bounded normalization batch.
    window.set_default_size(900, 650);
    pump_until(|| window.width() >= 880 && window.width() <= 920);
    this.widgets.search.grab_focus();
    settle();
    let next = this.widgets.search_next.clone();
    // Page controls follow the results in the scroller; keyboard focus reveals
    // the real button before the physical click, as it does in normal GTK.
    next.grab_focus();
    settle();
    click(window, id, &next);
    pump_until(|| counts.lock().unwrap().searches == 3 && !this.model.borrow().busy);
    assert_eq!(this.model.borrow().search_page, 1);
    assert!(this.widgets.search_previous.is_visible());
    assert!(!this.widgets.search_next.is_visible());
    assert_eq!(rows(&this.widgets.search_rows).len(), 1);
    assert!(contains_text(
        this.widgets.search_rows.upcast_ref(),
        "Normalized synthetic track 60"
    ));
    assert_eq!(counts.lock().unwrap().batches.len(), 3);
    assert!(!this.widgets.search_feedback.is_visible());
    stop.store(true, Ordering::Release);
    server.join().unwrap();
    pane.close();
    window.close();
}
