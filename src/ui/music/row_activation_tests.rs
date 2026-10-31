// SPDX-License-Identifier: GPL-3.0-or-later
//! Real X11 track playback activation and explicit detail/navigation actions.

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
    let (x, y) = super::test_input::pointer(window, widget, super::test_input::Target::Leading);
    input(&[
        "mousemove",
        "--sync",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
    ]);
    input(&["click", "1"]);
    // XTest completion is not GTK dispatch; observe the actual resulting action.
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

fn displayed_cover(widget: &gtk::Widget) -> bool {
    descendants(widget).into_iter().any(|widget| {
        let Some(overlay) = widget.downcast_ref::<gtk::Overlay>() else {
            return false;
        };
        let decoded = descendants(overlay.upcast_ref()).into_iter().any(|widget| {
            widget
                .downcast_ref::<gtk::Picture>()
                .is_some_and(|picture| {
                    picture.is_visible()
                        && picture
                            .paintable()
                            .is_some_and(|value| value.is::<gtk::gdk::Texture>())
                })
        });
        decoded
            && overlay
                .child()
                .is_some_and(|fallback| !fallback.is_visible())
    })
}

fn first(list: &gtk::ListBox) -> gtk::ListBoxRow {
    list.first_child().unwrap().downcast().unwrap()
}

fn playlist_tile(this: &Controller, title: &str) -> Option<gtk::Button> {
    descendants(this.playlist_grid.widget.upcast_ref())
        .into_iter()
        .filter(|widget| widget.widget_name() == "music-playlist-tile")
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| contains_text(button.upcast_ref(), title))
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
    let pixels =
        gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, false, 8, 512, 512).unwrap();
    pixels.fill(0xe653_39ff);
    let cover_png = pixels.save_to_bufferv("png", &[]).unwrap();
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
                if url
                    .path()
                    .starts_with("/get-music-content/fixture/input-cover/")
                {
                    let headers = request.to_ascii_lowercase();
                    assert!(!headers.contains("authorization:") && !headers.contains("cookie:"));
                    assert!(matches!(
                        url.path().rsplit('/').next(),
                        Some("200x200" | "400x400" | "1000x1000")
                    ));
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        cover_png.len()
                    );
                    let _ = stream.write_all(&cover_png);
                    continue;
                }
                assert!(request.contains("authorization: OAuth synthetic-row-input-token"));
                *counts.lock().unwrap().entry(url.path().into()).or_default() += 1;
                let track = json!({"id":42,"title":"Synthetic input track","durationMs":123000,
                    "coverUri":"avatars.yandex.net/get-music-content/fixture/input-cover/%%",
                    "available":true,"artists":[{"id":11,"name":"Synthetic input artist"}],
                    "albums":[{"id":7,"title":"Synthetic input album"}]});
                let body = match url.path() {
                    "/account/status" => json!({"result":{"account":{"uid":1234,"displayName":"Synthetic input account"}}}),
                    "/users/1234/likes/tracks" => json!({"result":{"library":{"uid":1234,"revision":1,"tracks":[{"id":42,"track":track}]}}}),
                    "/users/1234/playlists/list" => json!({"result":[{"kind":3,"title":"Synthetic owned playlist","trackCount":1,"owner":{"uid":1234}}]}),
                    "/users/1234/likes/playlists" => json!({"result":[{"playlist":{"kind":9,"title":"Synthetic liked playlist","trackCount":1,"owner":{"uid":4321}}}]}),
                    "/users/1234/playlists/3" | "/users/4321/playlists/9" => json!({"result":{"kind":3,"trackCount":1,"tracks":[{"id":42,"track":track}]}}),
                    "/tracks" => json!({"result":[track]}),
                    // Deliberately unavailable local delivery: these input tests
                    // prove the playback intent/context, never a live service sound.
                    "/tracks/42/download-info" | "/tracks/43/download-info" => json!({"result":[]}),
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
                if let Err(error) = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    let fixture_origin = format!("http://{address}/");
    this.artwork
        .as_ref()
        .unwrap()
        .set_test_origin(&fixture_origin)
        .unwrap();
    this.set_track_info_preference(false);
    this.sync_info_visibility();
    this.set_fixture_resolver_for_test(crate::stream::StreamResolver::for_test(
        &fixture_origin,
        &fixture_origin,
        &fixture_origin,
    ));
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
    let menu = descendants(row.upcast_ref())
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk::MenuButton>().ok())
        .unwrap();
    click(window, id, &menu);
    pump_until(|| menu.popover().unwrap().is_visible());
    let add = descendants(menu.popover().unwrap().upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some("Добавить в очередь"))
        .unwrap();
    click(window, id, &add);
    pump_until(|| this.model.borrow().queue.items().len() == 1);
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("library")
    );
    pump_until(|| !menu.popover().unwrap().is_visible());
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
        let before = count("/tracks/42/download-info");
        let detail_requests = count("/tracks");
        let row = first(&this.widgets.library_rows);
        if let Some(key) = key {
            assert!(row.grab_focus());
            input(&["key", "--clearmodifiers", key]);
        } else {
            click(window, id, &row);
        }
        pump_until(|| count("/tracks/42/download-info") == before + 1 && !this.music_requested());
        assert_eq!(
            this.widgets.stack.visible_child_name().as_deref(),
            Some("library")
        );
        assert_eq!(
            count("/tracks"),
            detail_requests,
            "playing a row must not navigate through a detail lookup"
        );
        let model = this.model.borrow();
        assert_eq!(model.queue.current_index(), Some(0));
        assert_eq!(model.queue.current().unwrap().id, "42");
        assert_eq!(
            model.queue.context().unwrap().source_kind(),
            ContextKind::LikedTracks
        );
        assert_eq!(this.widgets.current.text(), "Synthetic input track");
        drop(model);
        this.stop_music();
    }

    // Details remain an explicit independent action, not the track's primary
    // click. Exercise the real native menu and verify the diagnostic is reachable.
    let before = count("/tracks");
    let row = first(&this.widgets.library_rows);
    let menu = descendants(row.upcast_ref())
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk::MenuButton>().ok())
        .unwrap();
    click(window, id, &menu);
    pump_until(|| menu.popover().unwrap().is_visible());
    let details = descendants(menu.popover().unwrap().upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some("Открыть сведения о треке"))
        .unwrap();
    click(window, id, &details);
    pump_until(|| {
        this.widgets.stack.visible_child_name().as_deref() == Some("detail")
            && !this.model.borrow().busy
    });
    assert_eq!(count("/tracks"), before + 1);
    let delivery = named_button(this.widgets.detail.upcast_ref(), "music-delivery-probe")
        .expect("explicit track details retain the opt-in metadata diagnostic");
    assert!(delivery.is_sensitive());
    for _ in 0..32 {
        if delivery.has_focus() {
            break;
        }
        input(&["key", "--clearmodifiers", "Tab"]);
        settle();
    }
    assert!(
        delivery.has_focus(),
        "delivery diagnostic remains keyboard reachable"
    );
    pump_until(|| {
        delivery
            .compute_bounds(window)
            .zip(pane.player_bar.compute_bounds(window))
            .is_some_and(|(action, footer)| {
                action.y() >= 0.0 && action.y() + action.height() <= footer.y() + 1.0
            })
    });
    // Do not activate the diagnostic: no provider session/media is used here.
    capture_native("input-track-detail-delivery");
    input(&["key", "--clearmodifiers", "alt+Left"]);
    pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("library"));

    // Refresh replaces rows; the one parent dispatcher must not accumulate
    // handlers. Both owned and liked playlists keep their actual owner path.
    for (mode, title, path) in [
        (1, "Synthetic owned playlist", "/users/1234/playlists/3"),
        (2, "Synthetic liked playlist", "/users/4321/playlists/9"),
    ] {
        eprintln!("Karin synthetic input: target=playlist mode={mode}");
        this.widgets.library_mode.set_selected(mode);
        pump_until(|| !this.model.borrow().busy && playlist_tile(this, title).is_some());
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
        for method in [None, Some("Return"), Some("space")] {
            let before = count(path);
            // Select the actual playlist by title, not the leading functional
            // Create tile in the owned-playlists grid.
            let tile = playlist_tile(this, title).expect("real playlist tile");
            assert!(tile.is_visible() && tile.is_sensitive());
            if let Some(key) = method {
                assert!(tile.grab_focus());
                input(&["key", "--clearmodifiers", key]);
            } else {
                click(window, id, &tile);
            }
            pump_until(|| {
                this.widgets.stack.visible_child_name().as_deref() == Some("detail")
                    && !this.model.borrow().busy
            });
            settle();
            assert_eq!(
                count(path),
                before + 1,
                "refresh must not duplicate actual tile dispatch"
            );
            assert!(contains_text(
                this.widgets.detail.upcast_ref(),
                "Synthetic input track"
            ));
            click(window, id, &this.widgets.detail.first_child().unwrap());
            pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("library"));
        }
    }
    for (kind, enter, path, title) in [
        (SearchKind::Tracks, true, "/tracks", "Synthetic input track"),
        (
            SearchKind::Tracks,
            false,
            "/tracks",
            "Synthetic input track",
        ),
        (
            SearchKind::Albums,
            false,
            "/albums/7/with-tracks",
            "Synthetic input album",
        ),
        (
            SearchKind::Artists,
            false,
            "/artists/11/tracks",
            "Synthetic input artist",
        ),
    ] {
        this.widgets.stack.set_visible_child_name("search");
        this.widgets.search.set_text(if enter {
            "Синтетический ввод"
        } else {
            "Synthetic input"
        });
        this.model.borrow_mut().search_scope = Some(kind);
        // Exercise actual entry Return/Find. Section selection itself is covered
        // by the independent unified Show All mouse fixture.
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
        if kind == SearchKind::Tracks {
            let before = count("/tracks/42/download-info");
            click(window, id, &first(&this.widgets.search_rows));
            pump_until(|| {
                count("/tracks/42/download-info") == before + 1 && !this.music_requested()
            });
            assert_eq!(
                this.model.borrow().queue.context().unwrap().source_kind(),
                ContextKind::Search
            );
            assert_eq!(this.widgets.current.text(), "Synthetic input track");
            assert_eq!(
                this.widgets.stack.visible_child_name().as_deref(),
                Some("search")
            );
            this.stop_music();
            let row = first(&this.widgets.search_rows);
            let menu = descendants(row.upcast_ref())
                .into_iter()
                .find_map(|widget| widget.downcast::<gtk::MenuButton>().ok())
                .unwrap();
            click(window, id, &menu);
            pump_until(|| menu.popover().unwrap().is_visible());
            let details = descendants(menu.popover().unwrap().upcast_ref())
                .into_iter()
                .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
                .find(|button| button.label().as_deref() == Some("Открыть сведения о треке"))
                .unwrap();
            let before = count(path);
            click(window, id, &details);
            pump_until(|| {
                this.widgets.stack.visible_child_name().as_deref() == Some("detail")
                    && !this.model.borrow().busy
            });
            assert_eq!(count(path), before + 1);
        } else {
            let before = count(path);
            let name = if kind == SearchKind::Albums {
                "music-search-album-tile"
            } else {
                "music-search-artist-tile"
            };
            let tile = named_button(this.widgets.search_rows.upcast_ref(), name).unwrap();
            click(window, id, &tile);
            pump_until(|| {
                this.widgets.stack.visible_child_name().as_deref() == Some("detail")
                    && !this.model.borrow().busy
            });
            assert_eq!(count(path), before + 1);
        }
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

    // Queue rows start the existing playback intent at the selected occurrence.
    // The explicit loopback resolver returns no audio; no sound is claimed here.
    let mut unavailable = Track {
        id: "44".into(),
        title: "Synthetic unavailable queue track".into(),
        artists: vec!["Synthetic queue artist".into()],
        artist_links: Vec::new(),
        album_id: None,
        album_title: None,
        duration_ms: None,
        available: false,
        cover: CoverArt::parse("avatars.yandex.net/get-music-content/fixture/input-cover/%%"),
        explicit: false,
    };
    this.enqueue(unavailable.clone());
    this.enqueue(Track {
        id: "43".into(),
        title: "Synthetic second queued track".into(),
        artists: vec!["Synthetic queue artist".into()],
        artist_links: Vec::new(),
        album_id: None,
        album_title: None,
        duration_ms: None,
        available: true,
        cover: None,
        explicit: false,
    });
    unavailable.id = "42".into();
    unavailable.title = "Synthetic duplicate queued track".into();
    unavailable.available = true;
    this.enqueue(unavailable);
    this.widgets.stack.set_visible_child_name("queue");
    pump_until(|| this.widgets.queue_rows.is_mapped());
    assert_eq!(this.model.borrow().queue.items().len(), 4);
    assert_eq!(
        this.model
            .borrow()
            .queue
            .context()
            .unwrap()
            .manual_items_len(),
        3
    );
    let first_row = first(&this.widgets.queue_rows);
    pump_until(|| displayed_cover(first_row.upcast_ref()));
    let retired_remove = named_button(first_row.upcast_ref(), "music-queue-remove").unwrap();
    assert!(contains_text(first_row.upcast_ref(), "Сейчас"));
    let next = this.widgets.queue_rows.row_at_index(2).unwrap();
    assert!(contains_text(next.upcast_ref(), "Далее"));
    let unavailable = this.widgets.queue_rows.row_at_index(1).unwrap();
    assert!(!unavailable.is_activatable());
    assert!(
        !named_button(unavailable.upcast_ref(), "music-queue-play")
            .unwrap()
            .is_sensitive()
    );
    let before_unavailable = counts.lock().unwrap().clone();
    click(window, id, &unavailable);
    assert_eq!(*counts.lock().unwrap(), before_unavailable);
    assert_eq!(this.model.borrow().queue.current_index(), Some(0));
    for row in [first_row, next, unavailable] {
        pump_until(|| row.height() >= 60);
        assert!(
            row.height() <= 72,
            "queue rows retain compact music density"
        );
        for action in [
            "music-queue-play",
            "music-queue-details",
            "music-queue-remove",
        ] {
            let button = named_button(row.upcast_ref(), action).unwrap();
            assert!(button.label().is_none(), "queue actions use native icons");
            assert!(
                button.tooltip_text().is_some(),
                "icon actions retain understandable names"
            );
        }
    }
    capture_native("input-native-music-queue-current-next");
    // A duplicate ID at the final position must select that occurrence, not0.
    let duplicate = this.widgets.queue_rows.row_at_index(3).unwrap();
    let before = count("/tracks/42/download-info");
    assert!(duplicate.grab_focus());
    input(&["key", "--clearmodifiers", "space"]);
    pump_until(|| {
        this.model.borrow().queue.current_index() == Some(3)
            && count("/tracks/42/download-info") == before + 1
            && !this.music_requested()
    });
    assert_eq!(
        this.widgets.current.text(),
        "Synthetic duplicate queued track"
    );
    // A released gesture from a row replaced by render_queue must not remove
    // an occurrence from the newer presentation. Normal actions below still
    // use actual X11 mouse/keyboard input; this injects its late GTK signal.
    retired_remove.emit_clicked();
    assert_eq!(this.model.borrow().queue.items().len(), 4);
    assert_eq!(this.model.borrow().queue.current_index(), Some(3));
    assert!(contains_text(
        this.widgets
            .queue_rows
            .row_at_index(3)
            .unwrap()
            .upcast_ref(),
        "Сейчас"
    ));
    let second = this.widgets.queue_rows.row_at_index(2).unwrap();
    click(window, id, &second);
    pump_until(|| this.model.borrow().queue.current_index() == Some(2) && !this.music_requested());
    assert_eq!(this.widgets.current.text(), "Synthetic second queued track");
    let first_row = first(&this.widgets.queue_rows);
    assert!(first_row.grab_focus());
    input(&["key", "--clearmodifiers", "Return"]);
    pump_until(|| this.model.borrow().queue.current_index() == Some(0) && !this.music_requested());
    assert_eq!(this.widgets.current.text(), "Synthetic input track");
    assert!(!this.widgets.position.text().contains("0:00"));
    // Independent Details must preserve queue selection and emit no Play.
    let duplicate = this.widgets.queue_rows.row_at_index(3).unwrap();
    let details = named_button(duplicate.upcast_ref(), "music-queue-details").unwrap();
    let before_play = count("/tracks/42/download-info");
    click(window, id, &details);
    pump_until(|| {
        this.widgets.stack.visible_child_name().as_deref() == Some("detail")
            && !this.model.borrow().busy
    });
    assert_eq!(this.model.borrow().queue.current_index(), Some(0));
    assert_eq!(count("/tracks/42/download-info"), before_play);
    let back = named_button(this.widgets.detail.upcast_ref(), "music-detail-back").unwrap();
    click(window, id, &back);
    pump_until(|| this.widgets.stack.visible_child_name().as_deref() == Some("queue"));
    let duplicate = this.widgets.queue_rows.row_at_index(3).unwrap();
    assert!(duplicate.grab_focus());
    input(&["key", "--clearmodifiers", "Return"]);
    pump_until(|| this.model.borrow().queue.current_index() == Some(3) && !this.music_requested());
    let before_remove = count("/tracks/42/download-info");
    let old_first = first(&this.widgets.queue_rows);
    let remove = named_button(old_first.upcast_ref(), "music-queue-remove").unwrap();
    click(window, id, &remove);
    pump_until(|| this.model.borrow().queue.items().len() == 3);
    assert_eq!(this.model.borrow().queue.current_index(), Some(2));
    assert_eq!(
        this.model.borrow().queue.current().unwrap().title,
        "Synthetic duplicate queued track"
    );
    assert_eq!(
        count("/tracks/42/download-info"),
        before_remove,
        "removing another occurrence cannot restart the current track"
    );
    let unavailable = first(&this.widgets.queue_rows);
    let remove = named_button(unavailable.upcast_ref(), "music-queue-remove").unwrap();
    click(window, id, &remove);
    pump_until(|| this.model.borrow().queue.items().len() == 2);
    assert_eq!(this.model.borrow().queue.current_index(), Some(1));
    assert_eq!(
        this.model
            .borrow()
            .queue
            .context()
            .unwrap()
            .manual_items_len(),
        2
    );
    assert_eq!(count("/tracks/42/download-info"), before_remove);
    capture_native("input-native-music-queue-after-remove");
    assert!(this.widgets.clear_queue.grab_focus());
    input(&["key", "--clearmodifiers", "space"]);
    pump_until(|| this.model.borrow().queue.items().is_empty() && !this.music_requested());
    assert!(this.model.borrow().account_valid);
    assert!(this.model.borrow().queue.current().is_none());
    assert!(!this.widgets.clear_queue.is_sensitive());
    assert!(!first(&this.widgets.queue_rows).is_activatable());
    assert!(contains_text(
        this.widgets.queue_rows.upcast_ref(),
        "Очередь пуста"
    ));
    capture_native("input-native-music-queue-empty");

    pane.close();
    window.close();
    stop.store(true, Ordering::Release);
    server.join().unwrap();
}
