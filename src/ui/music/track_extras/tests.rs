// SPDX-License-Identifier: GPL-3.0-or-later
//! Ordinary native UI, local HTTP and original CC0 audio; no real account/text.

use super::super::{
    MusicPane, OfflineProvider, launch_system_browser, test_input,
    tests::{capture_native, pump_until},
};
use super::*;
use crate::{
    music::MusicService,
    queue::{ContextKind, PlaybackContext},
    session::AccessToken,
};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::Command,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const TOKEN: &str = "synthetic-v08-extras-token";

#[derive(Default)]
struct Counts {
    external: usize,
    similar: usize,
    batches: usize,
    posts: usize,
    liked: Vec<u64>,
    disliked: Vec<u64>,
    held: Option<u64>,
    held_started: Option<u64>,
}

struct Fixture {
    origin: String,
    counts: Arc<Mutex<Counts>>,
    gate: Arc<(Mutex<bool>, Condvar)>,
    stop: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<()>>,
}

impl Fixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        let counts = Arc::new(Mutex::new(Counts::default()));
        let gate = Arc::new((Mutex::new(true), Condvar::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let media = Arc::new(super::super::player::tests::generated_mp3());
        let server = {
            let (counts, gate, stop, origin) =
                (counts.clone(), gate.clone(), stop.clone(), origin.clone());
            thread::spawn(move || {
                let mut children = Vec::new();
                while !stop.load(Ordering::Acquire) {
                    let (stream, _) = match listener.accept() {
                        Ok(stream) => stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => panic!("local extras listener"),
                    };
                    assert!(children.len() < 160, "bounded fixture requests");
                    let (counts, gate, origin, media) =
                        (counts.clone(), gate.clone(), origin.clone(), media.clone());
                    children.push(thread::spawn(move || {
                        serve(stream, &origin, &counts, &gate, &media)
                    }));
                }
                for child in children {
                    child.join().unwrap();
                }
            })
        };
        Self {
            origin,
            counts,
            gate,
            stop,
            server: Some(server),
        }
    }

    fn hold(&self, id: u64) {
        *self.gate.0.lock().unwrap() = false;
        self.counts.lock().unwrap().held = Some(id);
        self.counts.lock().unwrap().held_started = None;
    }

    fn release(&self) {
        *self.gate.0.lock().unwrap() = true;
        self.gate.1.notify_all();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.release();
        self.stop.store(true, Ordering::Release);
        if let Some(server) = self.server.take() {
            server.join().unwrap();
        }
    }
}

fn canonical(id: u64) -> serde_json::Value {
    json!({"id":id,"title":format!("Original fixture {id}"),"available":true,"durationMs":4000,"artists":[{"id":11,"name":"Karin original fixture"}],"albums":[{"id":7,"title":"Original fixture collection"}]})
}

fn track(id: u64) -> Track {
    Track {
        id: id.to_string(),
        title: format!("Original fixture {id}"),
        artists: vec!["Karin original fixture".into()],
        artist_links: Vec::new(),
        album_id: Some("7".into()),
        album_title: Some("Original fixture collection".into()),
        duration_ms: Some(4000),
        cover: None,
        explicit: false,
        available: true,
    }
}

fn json_response(stream: &mut TcpStream, status: u16, body: serde_json::Value) {
    let body = body.to_string();
    let _ = write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn serve(
    mut stream: TcpStream,
    origin: &str,
    counts: &Mutex<Counts>,
    gate: &(Mutex<bool>, Condvar),
    media: &[u8],
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    let mut head = Vec::new();
    let mut byte = [0];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            return;
        }
        head.push(byte[0]);
        assert!(head.len() < 16 * 1024);
    }
    let head = String::from_utf8(head).unwrap();
    let first = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    let header = |wanted: &str| {
        head.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(wanted).then_some(value.trim())
        })
    };
    let length = header("content-length")
        .unwrap_or("0")
        .parse::<usize>()
        .unwrap();
    assert!(length < 16 * 1024);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    let url = reqwest::Url::parse(&format!("http://fixture{}", first[1])).unwrap();
    let api = url.path() != "/external-lyrics"
        && !url.path().starts_with("/download-info/")
        && !url.path().starts_with("/get-mp3/");
    if api {
        assert_eq!(
            header("authorization"),
            Some("OAuth synthetic-v08-extras-token")
        );
    } else {
        for field in ["authorization", "cookie", "origin", "referer"] {
            assert!(header(field).is_none());
        }
    }
    let parts = url.path().split('/').collect::<Vec<_>>();
    if url.path() == "/account/status" {
        json_response(
            &mut stream,
            200,
            json!({"result":{"account":{"uid":1234,"displayName":"Synthetic extras account"}}}),
        );
    } else if url.path() == "/external-lyrics" {
        counts.lock().unwrap().external += 1;
        let query = url
            .query_pairs()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            query.get("track_name").map(|value| value.as_ref()),
            Some("Original fixture 42")
        );
        assert!(query.keys().all(|key| matches!(
            key.as_ref(),
            "track_name" | "artist_name" | "album_name" | "duration"
        )));
        let lrc = (0..100)
            .map(|i| {
                format!(
                    "[00:{:02}.{:03}]Original synthetic line {i}",
                    i * 40 / 1000,
                    i * 40 % 1000
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        json_response(
            &mut stream,
            200,
            json!({"trackName":"Original fixture 42","artistName":"Karin original fixture","albumName":"Original fixture collection","duration":4,"instrumental":false,"plainLyrics":"Original synthetic plain text","syncedLyrics":lrc}),
        );
    } else if parts.len() == 4 && parts[1] == "tracks" && parts[3] == "supplement" {
        let id = parts[2].parse::<u64>().unwrap();
        if counts.lock().unwrap().held == Some(id) {
            counts.lock().unwrap().held_started = Some(id);
            let released = gate.0.lock().unwrap();
            let (released, timeout) = gate
                .1
                .wait_timeout_while(released, Duration::from_secs(8), |released| !*released)
                .unwrap();
            assert!(
                *released && !timeout.timed_out(),
                "bounded synthetic reply gate"
            );
        }
        json_response(
            &mut stream,
            200,
            json!({"result":{"lyrics":{"hasRights":true,"fullLyrics":format!("Original full synthetic text {id}\nOnly original permitted fixture content")}}}),
        );
    } else if parts.len() == 4 && parts[1] == "tracks" && parts[3] == "similar" {
        let attempt = {
            let mut counts = counts.lock().unwrap();
            counts.similar += 1;
            counts.similar
        };
        if attempt == 1 {
            json_response(&mut stream, 503, json!({"error":"synthetic-retry"}));
        } else {
            json_response(
                &mut stream,
                200,
                json!({"result":{"track":{"id":parts[2]},"similarTracks":(43..55).map(|id| json!({"id":id,"title":false,"artists":{"display":"ignored"}})).collect::<Vec<_>>()}}),
            );
        }
    } else if url.path() == "/tracks" {
        counts.lock().unwrap().batches += 1;
        let form = reqwest::Url::parse(&format!(
            "http://fixture/?{}",
            String::from_utf8(body).unwrap()
        ))
        .unwrap();
        let tracks = form
            .query_pairs()
            .filter(|(key, _)| key == "track-ids")
            .map(|(_, id)| canonical(id.parse().unwrap()))
            .collect::<Vec<_>>();
        assert!(tracks.len() <= 50);
        json_response(&mut stream, 200, json!({"result":tracks}));
    } else if url.path().starts_with("/users/1234/") && parts.len() >= 5 && parts[4] == "tracks" {
        let dislike = parts[3] == "dislikes";
        let mut counts = counts.lock().unwrap();
        if first[0] == "POST" {
            assert_eq!(parts.last(), Some(&"add-multiple"));
            let form = reqwest::Url::parse(&format!(
                "http://fixture/?{}",
                String::from_utf8(body).unwrap()
            ))
            .unwrap();
            let id = form
                .query_pairs()
                .find(|(key, _)| key == "track-ids")
                .unwrap()
                .1
                .parse::<u64>()
                .unwrap();
            counts.posts += 1;
            if dislike {
                counts.liked.retain(|old| *old != id);
                counts.disliked.push(id);
            } else {
                counts.disliked.retain(|old| *old != id);
                counts.liked.push(id);
            }
            json_response(
                &mut stream,
                200,
                json!({"result":{"revision":counts.posts+1}}),
            );
        } else {
            let entries = if dislike {
                &counts.disliked
            } else {
                &counts.liked
            };
            json_response(
                &mut stream,
                200,
                json!({"result":{"library":{"uid":1234,"revision":counts.posts+1,"tracks":entries.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()}}}),
            );
        }
    } else if parts.len() == 4 && parts[1] == "tracks" && parts[3] == "download-info" {
        json_response(
            &mut stream,
            200,
            json!({"result":[{"codec":"mp3","bitrateInKbps":192,"preview":false,"direct":false,"downloadInfoUrl":format!("{origin}download-info/{}",parts[2])}]}),
        );
    } else if let Some(id) = url.path().strip_prefix("/download-info/") {
        let xml = format!(
            "<download-info><host>s268myt.storage.yandex.net</host><path>/fixture-{id}.mp3</path><ts>1700000000</ts><s>synthetic-server-input</s></download-info>"
        );
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{xml}",
            xml.len()
        );
    } else if url.path().starts_with("/get-mp3/") {
        let range = header("range").map(|range| {
            let (start, end) = range
                .strip_prefix("bytes=")
                .unwrap()
                .split_once('-')
                .unwrap();
            (
                start.parse::<usize>().unwrap(),
                if end.is_empty() {
                    media.len() - 1
                } else {
                    end.parse::<usize>().unwrap().min(media.len() - 1)
                },
            )
        });
        let (start, end) = range.unwrap_or((0, media.len() - 1));
        assert!(start <= end && end < media.len());
        let status = if range.is_some() {
            "206 Partial Content"
        } else {
            "200 OK"
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: audio/mpeg\r\nCache-Control: no-store\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\n",
            end - start + 1
        );
        if range.is_some() {
            let _ = write!(
                stream,
                "Content-Range: bytes {start}-{end}/{}\r\n",
                media.len()
            );
        }
        let _ = write!(stream, "Connection: close\r\n\r\n");
        if first[0] != "HEAD" {
            let _ = stream.write_all(&media[start..=end]);
        }
    } else {
        json_response(&mut stream, 404, json!({"error":"synthetic-unsupported"}));
    }
}

fn input(args: &[&str]) {
    assert!(
        Command::new("timeout")
            .args(["5", "xdotool"])
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}

fn settle() {
    let deadline = Instant::now() + Duration::from_millis(120);
    pump_until(|| Instant::now() >= deadline);
}

fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>, heading: bool) {
    let widget = widget.as_ref();
    widget.grab_focus();
    settle();
    if widget.native().and_then(|native| native.surface()) == window.surface() {
        let mut parent = widget.parent();
        let mut scroll = None;
        while let Some(current) = parent {
            if let Some(found) = current.downcast_ref::<gtk::ScrolledWindow>() {
                scroll = Some(found.clone());
                break;
            }
            parent = current.parent();
        }
        if let Some(scroll) = scroll {
            for _ in 0..32 {
                let bounds = widget.compute_bounds(window).unwrap();
                let viewport = scroll.compute_bounds(window).unwrap();
                let height = if heading {
                    bounds.height().min(20.0)
                } else {
                    bounds.height()
                };
                if bounds.y() >= viewport.y() + 2.0
                    && bounds.y() + height <= viewport.y() + viewport.height() - 2.0
                {
                    break;
                }
                let direction = if bounds.y() < viewport.y() + 2.0 {
                    "4"
                } else {
                    "5"
                };
                // Outer viewport padding avoids sending an intended outer
                // scroll into the nested virtual lyrics scroller.
                let (dx, dy) = window.surface_transform();
                let scale = f64::from(window.scale_factor());
                let x = ((f64::from(viewport.x()) + 3.0 + dx) * scale).round() as i32;
                let y = ((f64::from(viewport.y() + viewport.height() / 2.0) + dy) * scale).round()
                    as i32;
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
        }
    }
    let target = if heading {
        test_input::Target::DisclosureHeading
    } else {
        test_input::Target::Leading
    };
    let (x, y) = test_input::pointer(window, widget, target);
    if widget.native().and_then(|native| native.surface()) == window.surface() {
        let scale = f64::from(window.scale_factor());
        let (dx, dy) = window.surface_transform();
        let picked = window.pick(
            f64::from(x) / scale - dx,
            f64::from(y) / scale - dy,
            gtk::PickFlags::DEFAULT,
        );
        assert!(
            picked
                .as_ref()
                .is_some_and(|picked| picked == widget || picked.is_ancestor(widget)),
            "new native extras target must be visible and unobscured"
        );
    }
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

fn menu_action(window: &adw::ApplicationWindow, id: &str, view: &ExtrasView, label: &str) {
    menu_action_at(window, id, view, 0, label);
}

fn menu_action_at(
    window: &adw::ApplicationWindow,
    id: &str,
    view: &ExtrasView,
    index: i32,
    label: &str,
) {
    let row = view.inner.similar_rows.row_at_index(index).unwrap();
    let menu = row
        .child()
        .unwrap()
        .last_child()
        .unwrap()
        .downcast::<gtk::MenuButton>()
        .unwrap();
    click(window, id, &menu, false);
    let popover = menu.popover().unwrap();
    pump_until(|| popover.is_mapped());
    let parent = popover.child().unwrap().downcast::<gtk::Box>().unwrap();
    let mut child = parent.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(button) = widget.downcast::<gtk::Button>()
            && button.label().as_deref() == Some(label)
        {
            click(window, id, &button, false);
            return;
        }
    }
    panic!("native recommendation action missing");
}

#[test]
#[ignore = "requires isolated GTK/Xvfb, xdotool and actual GStreamer; original synthetic text/audio only"]
fn native_v08_lyrics_similar_seek_and_stale_selection() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let fixture = Fixture::new();
    let service = MusicService::for_native_input_test(&fixture.origin);
    let token = AccessToken::for_test(TOKEN, Duration::from_secs(300));
    service.account(&token).unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.TrackExtrasFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let ui = super::super::super::build_with_music_pane(
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
    window.set_default_size(1440, 900);
    window.present();
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
    let context = PlaybackContext::new(
        ContextKind::Queue,
        None,
        vec![track(42), track(44)],
        0,
        false,
    )
    .unwrap();
    this.play_music_context(context);
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Playing)
    });
    this.pause_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
    });
    this.set_info_open(true);
    pump_until(|| this.track_pane.extras.inner.lyrics_expander.is_mapped());
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
    input(&["windowfocus", id]);
    let view = &this.track_pane.extras;
    assert_eq!(fixture.counts.lock().unwrap().external, 0);
    click(window, id, &view.inner.lyrics_expander, true);
    pump_until(|| view.inner.lyrics.borrow().is_some());
    assert_eq!(view.inner.source.text(), "Источник: Яндекс Музыка");
    assert_eq!(
        fixture.counts.lock().unwrap().external,
        0,
        "opening native lyrics never shares metadata externally"
    );
    assert!(
        view.inner
            .lyrics
            .borrow()
            .as_ref()
            .unwrap()
            .synchronized
            .is_empty()
    );
    assert!(view.inner.external.grab_focus());
    settle();
    input(&["key", "space"]);
    pump_until(|| {
        view.inner
            .lyrics
            .borrow()
            .as_ref()
            .is_some_and(|lyrics| !lyrics.synchronized.is_empty())
    });
    assert_eq!(fixture.counts.lock().unwrap().external, 1);
    assert!(
        view.inner
            .source
            .text()
            .contains("LRCLIB · внешний источник")
    );
    this.seek_music(Duration::from_millis(1200));
    pump_until(|| {
        view.inner
            .active
            .get()
            .is_some_and(|line| (29..=31).contains(&line))
    });
    assert!(
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
    );
    let (x, y) = test_input::pointer(
        window,
        &view.inner.lyrics_scroll,
        test_input::Target::Center,
    );
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "--repeat",
        "3",
        "--delay",
        "25",
        "4",
    ]);
    pump_until(|| !view.inner.following.get());
    settle();
    let before = view.inner.lyrics_scroll.vadjustment().value();
    this.seek_music(Duration::from_millis(2400));
    pump_until(|| {
        view.inner
            .active
            .get()
            .is_some_and(|line| (59..=61).contains(&line))
    });
    assert!(
        !view.inner.following.get(),
        "seek updates the line without overriding deliberate manual scroll"
    );
    assert!((view.inner.lyrics_scroll.vadjustment().value() - before).abs() < 8.0);
    click(window, id, &view.inner.follow, false);
    assert!(view.inner.following.get());
    // A separately inspected occurrence with the same service ID is not the
    // queue's actual playing occurrence. It must not acquire that playhead.
    this.select_track_info(track(42), None, false);
    this.sync_track_extras_position();
    assert!(view.inner.active.get().is_none());
    this.follow_current_occurrence();
    this.sync_track_pane();
    this.sync_track_extras_position();
    assert!(view.inner.active.get().is_some());
    let old_scroll = view.inner.lyrics_scroll.vadjustment().value();
    click(window, id, &this.track_pane.close, false);
    pump_until(|| !view.inner.lyrics_list.is_mapped());
    this.seek_music(Duration::from_millis(400));
    pump_until(|| {
        view.inner
            .active
            .get()
            .is_some_and(|line| (9..=11).contains(&line))
    });
    assert!(
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
    );
    click(window, id, &this.widgets.current_art_button, false);
    pump_until(|| {
        view.inner.lyrics_list.is_mapped()
            && view.inner.lyrics_scroll.vadjustment().value() < old_scroll - 10.0
    });
    assert_eq!(
        fixture.counts.lock().unwrap().external,
        1,
        "paused reopen reuses the loaded text without another external request"
    );
    // A native scrollbar-trough click is manual navigation even without a
    // wheel event, keyboard event or drag-update signal.
    let scrollbar = view.inner.lyrics_scroll.vscrollbar();
    let (x, y) = test_input::pointer(window, &scrollbar, test_input::Target::Center);
    assert!(view.inner.following.get());
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "1",
    ]);
    pump_until(|| !view.inner.following.get());
    this.seek_music(Duration::from_millis(2800));
    pump_until(|| {
        view.inner
            .active
            .get()
            .is_some_and(|line| (69..=71).contains(&line))
    });
    assert!(!view.inner.following.get());
    click(window, id, &view.inner.follow, false);
    assert!(view.inner.following.get());
    for (width, height, name) in [
        (420, 720, "v08-lyrics-compact"),
        (900, 650, "v08-lyrics-medium"),
        (1440, 900, "v08-lyrics-wide"),
    ] {
        super::super::player::tests::resize_player_window(&ui, width, height);
        let (minimum, _, _, _) = view.widget.measure(gtk::Orientation::Horizontal, -1);
        assert!(minimum <= 300, "lyrics cannot widen the native panel");
        capture_native(name);
    }
    click(window, id, &view.inner.lyrics_expander, true);
    click(window, id, &view.inner.similar_expander, true);
    pump_until(|| view.inner.similar_retry.is_visible() && !view.inner.similar_loading.get());
    assert!(!view.inner.similar_status.text().contains("503"));
    click(window, id, &view.inner.similar_retry, false);
    pump_until(|| view.inner.related.borrow().len() == 12);
    assert_eq!(
        fixture.counts.lock().unwrap().batches,
        1,
        "similar normalization uses one batch, not N+1 lookups"
    );
    assert_eq!(view.inner.shown.get(), 10);
    click(window, id, &view.inner.similar_more, false);
    assert_eq!(view.inner.shown.get(), 12);
    let serial = this.music_serial();
    menu_action(window, id, view, "Воспроизвести следующим");
    assert_eq!(this.music_serial(), serial);
    assert_eq!(
        this.model
            .borrow()
            .queue
            .items()
            .iter()
            .map(|track| track.id.as_str())
            .collect::<Vec<_>>(),
        ["42", "43", "44"]
    );
    menu_action(window, id, view, "Отметить: нравится");
    pump_until(|| !this.mutation_busy.get() && fixture.counts.lock().unwrap().posts == 1);
    menu_action(window, id, view, "Отметить: не нравится");
    pump_until(|| !this.mutation_busy.get() && fixture.counts.lock().unwrap().posts == 2);
    assert_eq!(
        this.music_serial(),
        serial,
        "recommendation marks do not restart or skip the paused current track"
    );
    assert_eq!(this.model.borrow().queue.current().unwrap().id, "42");
    capture_native("v08-similar-native-actions");
    // Actual recommended-row mouse activation reuses the current queue/player.
    let row = view.inner.similar_rows.row_at_index(0).unwrap();
    click(window, id, &row, false);
    pump_until(|| {
        this.model
            .borrow()
            .queue
            .current()
            .is_some_and(|track| track.id == "43")
            && this
                .music_snapshot()
                .is_some_and(|snapshot| snapshot.state == PlaybackState::Playing)
    });
    this.pause_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Paused)
    });
    fixture.hold(43);
    view.inner.lyrics_expander.set_expanded(true);
    pump_until(|| {
        view.inner.lyrics_loading.get() && fixture.counts.lock().unwrap().held_started == Some(43)
    });
    let old = Ticket {
        selection: view.inner.epoch.get(),
        request: view.inner.lyrics_serial.get(),
    };
    // A genuine transport action changes the following pane while an older
    // lyric HTTP response is pending; the reply cannot become track44's text.
    eprintln!("Karin synthetic extras: stage=held-reply-next");
    this.skip_music(true, false);
    fixture.release();
    pump_until(|| {
        this.model
            .borrow()
            .queue
            .current()
            .is_some_and(|track| track.id == "44")
            && view
                .inner
                .lyrics
                .borrow()
                .as_ref()
                .is_some_and(|lyrics| lyrics.plain.contains("text 44"))
    });
    assert!(!view.lyrics_is_current(old));
    // The accepted transport preserves a paused intent across Next: the new
    // source is prepared Ready without autoplay. Lyrics arriving independently
    // must not be interpreted as a command to resume the audio engine.
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Ready)
            && !this.music_requested()
    });
    let snapshot = this.music_snapshot().unwrap();
    eprintln!(
        "Karin synthetic extras: stage=next-prepared-without-autoplay requested={} state={:?} error={:?}",
        this.music_requested(),
        snapshot.state,
        snapshot.last_error
    );
    assert!(snapshot.last_error.is_none());
    this.select_track_info(track(42), None, false);
    pump_until(|| {
        view.inner
            .lyrics
            .borrow()
            .as_ref()
            .is_some_and(|lyrics| lyrics.plain.contains("text 42"))
    });
    this.sync_track_extras_position();
    assert!(
        view.inner.active.get().is_none(),
        "inspecting another track cannot reuse the current track's clock"
    );
    pump_until(|| {
        view.inner
            .related
            .borrow()
            .get(1)
            .is_some_and(|track| track.id == "44")
    });
    this.enqueue(track(45));
    fixture.hold(45);
    eprintln!("Karin synthetic extras: stage=current-related-dislike");
    menu_action_at(window, id, view, 1, "Отметить: не нравится");
    pump_until(|| {
        this.model
            .borrow()
            .queue
            .current()
            .is_some_and(|track| track.id == "45")
            && view.inner.lyrics_loading.get()
            && fixture.counts.lock().unwrap().held_started == Some(45)
    });
    assert_eq!(
        fixture.counts.lock().unwrap().posts,
        3,
        "confirmed dislike of the actual current recommended track reuses guarded Next exactly once"
    );
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == PlaybackState::Ready)
            && !this.music_requested()
    });
    assert_eq!(this.model.borrow().queue.current().unwrap().id, "45");
    assert_eq!(
        this.music_snapshot().unwrap().state,
        PlaybackState::Ready,
        "confirmed dislike advances the occurrence without unpausing a paused context"
    );
    eprintln!("Karin synthetic extras: stage=logout-with-pending-lyrics");
    this.logout();
    fixture.release();
    pump_until(|| !this.model.borrow().busy);
    assert!(view.inner.selected.borrow().is_none());
    assert!(view.inner.lyrics.borrow().is_none());
    assert!(view.inner.related.borrow().is_empty());
    assert_eq!(fixture.counts.lock().unwrap().external, 1);
    let weak = view.widget.downgrade();
    ui.close_for_smoke()(true);
    pump_until(|| weak.upgrade().is_none());
}
