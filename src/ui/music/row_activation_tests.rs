// SPDX-License-Identifier: GPL-3.0-or-later
//! Real X11 input through rendered GTK rows, not direct detail calls/signals.

use super::tests::{capture_native, contains_text, pump_until};
use super::*;
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

fn input(arguments: &[&str]) {
    assert!(
        std::process::Command::new("xdotool")
            .args(arguments)
            .status()
            .expect("xdotool is required for the input-driven fixture")
            .success(),
        "synthetic X11 input failed"
    );
}

fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref();
    pump_until(|| widget.is_mapped() && widget.width() > 0 && widget.height() > 0);
    let bounds = widget
        .compute_bounds(window)
        .expect("rendered input target");
    // Stay away from a row's nested right-hand menu/add buttons.
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
    // XSendEvent/XTest completion is not GTK event dispatch. In particular,
    // testing !busy immediately could pass before the click starts its job.
    settle();
}

fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut widgets = vec![widget.clone()];
    let mut child = widget.first_child();
    while let Some(current) = child {
        widgets.extend(descendants(&current));
        child = current.next_sibling();
    }
    widgets
}

fn named_button(widget: &gtk::Widget, name: &str) -> Option<gtk::Button> {
    descendants(widget)
        .into_iter()
        .find(|widget| widget.widget_name() == name)
        .and_then(|widget| widget.downcast().ok())
}

fn first(list: &gtk::ListBox) -> gtk::ListBoxRow {
    list.first_child().unwrap().downcast().unwrap()
}

fn settle() {
    let deadline = Instant::now() + Duration::from_millis(120);
    pump_until(|| Instant::now() >= deadline);
}

#[test]
#[ignore = "requires an isolated GTK/X11 display and xdotool; run explicitly under Xvfb"]
fn native_catalog_rows_mouse_keyboard_details_and_delivery() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let counts = Arc::new(Mutex::new(BTreeMap::<String, usize>::new()));
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
                    Err(_) => panic!("synthetic input fixture listener failed"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    assert_eq!(stream.read(&mut byte).unwrap(), 1);
                    bytes.push(byte[0]);
                    assert!(bytes.len() < 16 * 1024);
                }
                let request = String::from_utf8(bytes).unwrap();
                assert!(request.contains("authorization: OAuth synthetic-row-input-token"));
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
                stream.read_exact(&mut vec![0; length]).unwrap();
                let target = request
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).unwrap();
                *counts.lock().unwrap().entry(url.path().into()).or_default() += 1;
                let track = json!({"id":42,"title":"Synthetic input track","durationMs":123000,
                    "available":true,"artists":[{"id":11,"name":"Synthetic input artist"}],
                    "albums":[{"id":7,"title":"Synthetic input album"}]});
                let body = match url.path() {
                    "/account/status" => json!({"result":{"account":{"uid":1234,"displayName":"Synthetic input account"}}}),
                    "/users/1234/likes/tracks" => json!({"result":{"library":{"uid":1234,"revision":1,"tracks":[{"id":42,"track":track}]}}}),
                    "/users/1234/playlists/list" => json!({"result":[{"kind":3,"title":"Synthetic owned playlist","trackCount":1,"owner":{"uid":1234}}]}),
                    "/users/1234/likes/playlists" => json!({"result":[{"playlist":{"kind":9,"title":"Synthetic liked playlist","trackCount":1,"owner":{"uid":4321}}}]}),
                    "/users/1234/playlists/3" | "/users/4321/playlists/9" => json!({"result":{"kind":3,"trackCount":1,"tracks":[{"id":42,"track":track}]}}),
                    "/tracks" => json!({"result":[track]}),
                    "/search" => {
                        let kind = url.query_pairs().find(|(key,_)| key == "type").unwrap().1.into_owned();
                        match kind.as_str() {
                            // Real upstream converters permit partial/mixed
                            // embedded display credits. Own track identity and
                            // availability remain well formed and authoritative.
                            "track" => json!({"result":{"tracks":{"page":0,"perPage":20,"total":1,"results":[{
                                "id":42,"title":"Synthetic input track","available":true,
                                "artists":[null,false,{"id":{"optional":"credit"},"name":"Synthetic input artist"},{"name":22}],
                                "albums":[null,true,{"id":7,"title":"Synthetic input album","artists":null}]
                            }]}}}),
                            "album" => json!({"result":{"albums":{"page":0,"perPage":20,"total":1,"results":[{"id":7,"title":"Synthetic input album","artists":[]}]}}}),
                            "artist" => json!({"result":{"artists":{"page":0,"perPage":20,"total":1,"results":[{"id":11,"name":"Synthetic input artist"}]}}}),
                            _ => panic!("unexpected synthetic search category"),
                        }
                    }
                    "/albums/7/with-tracks" => json!({"result":{"id":7,"title":"Synthetic input album","artists":[],"volumes":[[track]]}}),
                    "/artists/11/tracks" => json!({"result":{"tracks":[track],"pager":{"page":0,"perPage":20,"total":1}}}),
                    _ => panic!("unexpected synthetic input fixture route"),
                }.to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        })
    };
    let count = |path: &str| counts.lock().unwrap().get(path).copied().unwrap_or(0);
    let service = MusicService::for_test(&format!("http://{address}/"));
    let token = AccessToken::for_test("synthetic-row-input-token", Duration::from_secs(300));
    service.account(&token).unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.RowInputFixture")
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
    pane.enable_delivery_diagnostic();
    this.render_controls();
    this.widgets.stack.set_visible_child_name("library");
    this.load_library(0);
    pump_until(|| !this.model.borrow().busy && first(&this.widgets.library_rows).is_activatable());
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
    settle();

    // Nested controls consume their own actual clicks rather than opening the
    // parent row. No direct call to open_track or emitted activation is used.
    let row = first(&this.widgets.library_rows);
    let add = descendants(row.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.tooltip_text().as_deref() == Some("Добавить трек в очередь"))
        .unwrap();
    click(window, id, &add);
    pump_until(|| this.model.borrow().queue.items().len() == 1);
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("library")
    );
    let menu = descendants(row.upcast_ref())
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk::MenuButton>().ok())
        .unwrap();
    click(window, id, &menu);
    pump_until(|| menu.popover().unwrap().is_visible());
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("library")
    );
    input(&["key", "--clearmodifiers", "Escape"]);
    pump_until(|| !menu.popover().unwrap().is_visible());

    for key in [None, Some("Return"), Some("space")] {
        eprintln!("Karin synthetic input: target=library-track method={key:?}");
        let before = count("/tracks");
        let row = first(&this.widgets.library_rows);
        if let Some(key) = key {
            assert!(row.grab_focus());
            input(&["key", "--clearmodifiers", key]);
        } else {
            click(window, id, &row);
        }
        pump_until(|| {
            this.widgets.stack.visible_child_name().as_deref() == Some("detail")
                && !this.model.borrow().busy
        });
        assert_eq!(
            count("/tracks"),
            before + 1,
            "one real row input sends one detail request"
        );
        assert!(contains_text(
            this.widgets.detail.upcast_ref(),
            "Synthetic input track"
        ));
        let delivery = named_button(this.widgets.detail.upcast_ref(), "music-delivery-probe")
            .expect("actual row navigation must reach the opt-in delivery action");
        assert!(delivery.is_sensitive());
        pump_until(|| delivery.is_mapped());
        // Do not activate it: its production network client is intentionally
        // not supplied real credentials or a real track in Cloud/CI.
        capture_native("input-track-detail-delivery");
        let back = this.widgets.detail.first_child().unwrap();
        click(window, id, &back);
        pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("library"));
    }

    // Refresh replaces rows; the one parent dispatcher must not accumulate
    // handlers. Both owned and liked playlists keep their actual owner path.
    for (mode, title, path) in [
        (1, "Synthetic owned playlist", "/users/1234/playlists/3"),
        (2, "Synthetic liked playlist", "/users/4321/playlists/9"),
    ] {
        eprintln!("Karin synthetic input: target=playlist mode={mode}");
        this.widgets.library_mode.set_selected(mode);
        pump_until(|| {
            !this.model.borrow().busy
                && contains_text(this.widgets.library_rows.upcast_ref(), title)
        });
        for _ in 0..2 {
            let refresh_path = if mode == 1 {
                "/users/1234/playlists/list"
            } else {
                "/users/1234/likes/playlists"
            };
            let before_refresh = count(refresh_path);
            click(window, id, &this.widgets.library_refresh);
            pump_until(|| count(refresh_path) > before_refresh && !this.model.borrow().busy);
        }
        let before = count(path);
        click(window, id, &first(&this.widgets.library_rows));
        pump_until(|| {
            this.widgets.stack.visible_child_name().as_deref() == Some("detail")
                && !this.model.borrow().busy
        });
        settle();
        assert_eq!(
            count(path),
            before + 1,
            "refresh must not duplicate row dispatch"
        );
        assert!(contains_text(
            this.widgets.detail.upcast_ref(),
            "Synthetic input track"
        ));
        click(window, id, &this.widgets.detail.first_child().unwrap());
        pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("library"));
    }
    for (kind, enter, path, title) in [
        (0, true, "/tracks", "Synthetic input track"),
        (0, false, "/tracks", "Synthetic input track"),
        (1, false, "/albums/7/with-tracks", "Synthetic input album"),
        (2, false, "/artists/11/tracks", "Synthetic input artist"),
    ] {
        this.widgets.stack.set_visible_child_name("search");
        this.widgets.search_kind.set_selected(kind);
        // Test actual SearchEntry Return as well as the actual Find button;
        // neither path invokes Controller::search or emits activate directly.
        this.widgets.search.set_text(if enter {
            "Синтетический ввод"
        } else {
            "Synthetic input"
        });
        if enter {
            assert!(this.widgets.search.grab_focus());
            input(&["key", "--clearmodifiers", "Return"]);
            settle();
        } else {
            click(window, id, &this.widgets.search_button);
        }
        pump_until(|| {
            !this.model.borrow().busy && contains_text(this.widgets.search_rows.upcast_ref(), title)
        });
        let before = count(path);
        click(window, id, &first(&this.widgets.search_rows));
        pump_until(|| {
            this.widgets.stack.visible_child_name().as_deref() == Some("detail")
                && !this.model.borrow().busy
        });
        assert_eq!(count(path), before + 1);
        assert!(contains_text(
            this.widgets.detail.upcast_ref(),
            "Synthetic input track"
        ));
        click(window, id, &this.widgets.detail.first_child().unwrap());
        pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("search"));
    }

    // Actual click on an empty/nonactivatable row must send no request and
    // leave the current destination intact.
    set_empty(&this.widgets.search_rows, "Synthetic empty results");
    let before = counts.lock().unwrap().clone();
    click(window, id, &first(&this.widgets.search_rows));
    settle();
    assert_eq!(*counts.lock().unwrap(), before);
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("search")
    );
    assert!(!first(&this.widgets.search_rows).is_activatable());

    // Queue rows use the same parent dispatch without starting audio.
    this.enqueue(Track {
        id: "43".into(),
        title: "Synthetic second queued track".into(),
        artists: vec![],
        album_id: None,
        album_title: None,
        duration_ms: None,
        available: true,
        cover: None,
        explicit: false,
    });
    this.widgets.stack.set_visible_child_name("queue");
    let second = this.widgets.queue_rows.row_at_index(1).unwrap();
    click(window, id, &second);
    pump_until(|| this.model.borrow().queue.current_index() == Some(1));
    assert_eq!(this.widgets.current.text(), "Synthetic second queued track");
    let first_row = first(&this.widgets.queue_rows);
    assert!(first_row.grab_focus());
    input(&["key", "--clearmodifiers", "Return"]);
    pump_until(|| this.model.borrow().queue.current_index() == Some(0));
    assert_eq!(this.widgets.current.text(), "Synthetic input track");
    assert!(!this.widgets.position.text().contains("0:00"));
    let second = this.widgets.queue_rows.row_at_index(1).unwrap();
    let remove = descendants(second.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.tooltip_text().as_deref() == Some("Удалить трек из очереди"))
        .unwrap();
    click(window, id, &remove);
    pump_until(|| this.model.borrow().queue.items().len() == 1);
    assert_eq!(this.model.borrow().queue.current_index(), Some(0));

    pane.close();
    window.close();
    stop.store(true, Ordering::Release);
    server.join().unwrap();
}
