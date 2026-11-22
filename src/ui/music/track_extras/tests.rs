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

#[path = "tests/playing_frames.rs"]
mod playing_frames;
#[path = "tests/provider_flow.rs"]
mod provider_flow;

const TOKEN: &str = "synthetic-v08-extras-token";

#[derive(Default)]
struct Counts {
    external: usize,
    external_status: Option<u16>,
    external_plain_only: bool,
    external_held: bool,
    external_entered: bool,
    similar: usize,
    batches: usize,
    posts: usize,
    liked: Vec<u64>,
    disliked: Vec<u64>,
    held: Option<u64>,
    held_started: Option<u64>,
}

pub(in crate::ui::music) struct Fixture {
    origin: String,
    counts: Arc<Mutex<Counts>>,
    gate: Arc<(Mutex<bool>, Condvar)>,
    stop: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<()>>,
}

impl Fixture {
    pub(in crate::ui::music) fn new() -> Self {
        Self::with_karaoke_audio(false)
    }

    fn with_karaoke_audio(karaoke: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        let counts = Arc::new(Mutex::new(Counts::default()));
        let gate = Arc::new((Mutex::new(true), Condvar::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let media = Arc::new((
            super::super::player::tests::generated_mp3(),
            karaoke.then(|| super::super::player::tests::generated_mp3_for_seconds(7)),
        ));
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

fn duration_ms(id: u64) -> u64 {
    if id == 46 { 7000 } else { 4000 }
}

fn canonical(id: u64) -> serde_json::Value {
    json!({"id":id,"title":format!("Original fixture {id}"),"available":true,"durationMs":duration_ms(id),"artists":[{"id":11,"name":"Karin original fixture"}],"albums":[{"id":7,"title":"Original fixture collection"}]})
}

pub(in crate::ui::music) fn track(id: u64) -> Track {
    Track {
        id: id.to_string(),
        title: format!("Original fixture {id}"),
        artists: vec!["Karin original fixture".into()],
        artist_links: Vec::new(),
        album_id: Some("7".into()),
        album_title: Some("Original fixture collection".into()),
        duration_ms: Some(duration_ms(id)),
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
    media: &(Vec<u8>, Option<Vec<u8>>),
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
        let (status, plain_only, held) = {
            let mut c = counts.lock().unwrap();
            c.external += 1;
            c.external_entered = true;
            (c.external_status, c.external_plain_only, c.external_held)
        };
        if held {
            let released = gate.0.lock().unwrap();
            let (released, timeout) = gate
                .1
                .wait_timeout_while(released, Duration::from_secs(8), |r| !*r)
                .unwrap();
            assert!(!timeout.timed_out(), "release the held external fixture");
            drop(released);
        }
        if let Some(status) = status {
            json_response(&mut stream, status, json!({}));
            return;
        }
        let query = url
            .query_pairs()
            .collect::<std::collections::BTreeMap<_, _>>();
        let name = query.get("track_name").unwrap().as_ref();
        assert!(matches!(
            name,
            "Original fixture 42" | "Original fixture 44" | "Original fixture 46"
        ));
        assert!(query.keys().all(|key| matches!(
            key.as_ref(),
            "track_name" | "artist_name" | "album_name" | "duration"
        )));
        // The long fixture exercises scrolling and virtualization. Track 46
        // fits the pinned pane and includes both an ordinary adjacent change
        // and simultaneous LRC timestamps (the last line in a group wins).
        let timestamps = if name == "Original fixture 46" {
            let mut times = vec![0, 1400, 3300, 3300, 3300, 3300];
            times.extend([5000; 6]);
            times.push(5800);
            times.extend((1..20).map(|i| 7000 + i * 100));
            times
        } else {
            (0..100).map(|i| i * 40).collect()
        };
        let lrc = timestamps
            .into_iter()
            .enumerate()
            .map(|(i, ms)| {
                let text = if name == "Original fixture 46" {
                    match i {
                        // The em space separates the measured first word on the
                        // long line even when the rest of the sentence rewraps.
                        0 => "Журавли",
                        1 => "Фонари",
                        2 => "Ночной ветер",
                        3 => "Следы у реки",
                        4 => "Стихает дождь",
                        5 => "Берёзы отражаются в воде. За рекой ещё горят огни, а вдоль тропы цветёт иван-чай — до са́мой зари.",
                        _ => ["Над водой — тишина", "Дальний берег спит", "Ветви шепчут", "Свет уходит за холмы", "Снова утро", "Тает синий лёд"][i % 6],
                    }
                    .to_owned()
                } else {
                    format!("Original synthetic line {i} · строка")
                };
                format!("[00:{:02}.{:03}]{text}", ms / 1000, ms % 1000)
            })
            .collect::<Vec<_>>()
            .join("\n");
        json_response(
            &mut stream,
            200,
            json!({"trackName":name,"artistName":"Karin original fixture","albumName":"Original fixture collection","duration":if name == "Original fixture 46" {7} else {4},"instrumental":false,"plainLyrics":"Original synthetic plain text","syncedLyrics":if plain_only {None} else {Some(lrc)}}),
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
            json!({"result":{"lyrics":{"hasRights":true,"fullLyrics":format!("Original full synthetic text {id}\nПервая строка: ёж и звёзды\n\nВторая строфа — только оригинальный тестовый текст")}}}),
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
        let media = if url.path().ends_with("fixture-46.mp3") {
            media.1.as_ref().expect("seven-second karaoke recording")
        } else {
            &media.0
        };
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
                // Use real viewport wheel input when a normal Info action
                // is outside its single scroll viewport.
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
        // GtkStack may already allocate the destination at its final bounds
        // while the previous pane still covers it during the native transition.
        pump_until(|| {
            window
                .pick(
                    f64::from(x) / scale - dx,
                    f64::from(y) / scale - dy,
                    gtk::PickFlags::DEFAULT,
                )
                .is_some_and(|picked| &picked == widget || picked.is_ancestor(widget))
        });
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

/// GTK 4.14 can place a native popover below a bottom-edge anchor and outside
/// the parent surface. Keep this fixture's actions inside the surface checked
/// by the unchanged shared pointer helper: a user scrolls the anchor upward
/// before opening the menu. This changes neither production popup placement
/// nor the global geometry/native-input contract.
fn center_menu_anchor(window: &adw::ApplicationWindow, id: &str, menu: &gtk::MenuButton) {
    let mut parent = menu.parent();
    let scroll = loop {
        let current = parent.expect("related action belongs to the native pane scroller");
        if let Ok(scroll) = current.clone().downcast::<gtk::ScrolledWindow>() {
            break scroll;
        }
        parent = current.parent();
    };
    test_input::wait_for_layout(window, menu);
    for _ in 0..32 {
        let bounds = menu.compute_bounds(window).unwrap();
        let viewport = scroll.compute_bounds(window).unwrap();
        let lower = (viewport.y() + viewport.height() / 2.0).min(window.height() as f32 / 2.0);
        if bounds.y() >= viewport.y() + 4.0 && bounds.y() + bounds.height() <= lower {
            return;
        }
        let direction = if bounds.y() < viewport.y() + 4.0 {
            "4"
        } else {
            "5"
        };
        let (dx, dy) = window.surface_transform();
        let scale = f64::from(window.scale_factor());
        let x = ((f64::from(viewport.x()) + 3.0 + dx) * scale).round() as i32;
        let y = ((f64::from(viewport.y() + viewport.height() / 2.0) + dy) * scale).round() as i32;
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
        "actual wheel input must place the related menu in the upper half of its parent surface"
    );
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
    center_menu_anchor(window, id, &menu);
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
            pump_until(|| !popover.is_mapped());
            return;
        }
    }
    panic!("native recommendation action missing");
}

fn clipboard_text(widget: &impl IsA<gtk::Widget>) -> String {
    let result = Rc::new(RefCell::new(None));
    let copy = Rc::clone(&result);
    let clipboard = widget.as_ref().clipboard();
    // xdotool returning only confirms queued X events. Begin a Unicode read
    // after GTK has actually acquired the selection and published its provider.
    pump_until(|| clipboard.is_local() && clipboard.content().is_some());
    glib::spawn_future_local(async move {
        copy.replace(Some(
            clipboard
                .read_text_future()
                .await
                .unwrap()
                .unwrap()
                .to_string(),
        ));
    });
    pump_until(|| result.borrow().is_some());
    result.borrow_mut().take().unwrap()
}

fn find_button(root: &gtk::Widget, label: &str) -> Option<gtk::Button> {
    if let Some(button) = root.downcast_ref::<gtk::Button>()
        && button.label().as_deref() == Some(label)
    {
        return Some(button.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_button(&widget, label) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

fn external_consent_ready(window: &adw::ApplicationWindow, view: &ExtrasView) -> adw::AlertDialog {
    // Presenting creates the object before its modal focus and native layout
    // are ready. Sending Enter at object creation can still reach the opener.
    pump_until(|| {
        view.inner
            .external_consent
            .borrow()
            .as_ref()
            .is_some_and(|dialog| {
                dialog.is_mapped()
                    && find_button(dialog.upcast_ref(), "Отмена")
                        .is_some_and(|cancel| cancel.is_mapped() && cancel.has_focus())
            })
    });
    let dialog = view
        .inner
        .external_consent
        .borrow()
        .as_ref()
        .unwrap()
        .clone();
    let cancel = find_button(dialog.upcast_ref(), "Отмена").unwrap();
    test_input::wait_for_layout(window, &cancel);
    assert!(
        cancel.has_focus(),
        "the safe Cancel response owns native focus"
    );
    dialog
}

fn external_consent_closed(view: &ExtrasView, dialog: &adw::AlertDialog) {
    pump_until(|| {
        view.inner.external_consent.borrow().is_none()
            && !dialog.is_mapped()
            && dialog.parent().is_none()
    });
}

fn copy_lyrics_menu(window: &adw::ApplicationWindow, id: &str, view: &ExtrasView) {
    let clipboard = view.lyrics_widget.clipboard();
    let changed = Rc::new(Cell::new(false));
    let observed = Rc::clone(&changed);
    let handler = clipboard.connect_changed(move |_| observed.set(true));
    click(window, id, &view.inner.lyrics_actions, false);
    let popover = view.inner.lyrics_actions.popover().unwrap();
    pump_until(|| popover.is_mapped());
    test_input::wait_for_layout(window, &popover);
    input(&["key", "--clearmodifiers", "Home"]);
    input(&["key", "--clearmodifiers", "Return"]);
    pump_until(|| !popover.is_mapped() && changed.get());
    clipboard.disconnect(handler);
}

pub(in crate::ui::music) fn fixture_window(
    fixture: &Fixture,
) -> (adw::Application, crate::ui::MainWindow) {
    let app = fixture_app();
    let ui = window_for(&app, fixture);
    (app, ui)
}

fn fixture_app() -> adw::Application {
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.TrackExtrasFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    app
}

fn window_for(app: &adw::Application, fixture: &Fixture) -> crate::ui::MainWindow {
    let service = MusicService::for_native_input_test(&fixture.origin);
    let token = AccessToken::for_test(TOKEN, Duration::from_secs(300));
    service.account(&token).unwrap();
    let ui = super::super::super::build_with_music_pane(
        app,
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
    ui.window.set_title(Some("Karin synthetic HTTP fixture"));
    ui.window.set_default_size(1440, 900);
    let this = &ui.music.controller;
    if let Ok(style) = std::env::var("KARIN_TEST_THEME_STYLE") {
        let style = serde_json::from_value(serde_json::Value::String(style)).unwrap();
        let mut values = this.persistence.borrow().values.clone();
        values.ui_style = style;
        values.appearance = match adw::StyleManager::default().color_scheme() {
            adw::ColorScheme::ForceLight => crate::settings::Appearance::Light,
            adw::ColorScheme::ForceDark => crate::settings::Appearance::Dark,
            _ => values.appearance,
        };
        this.persistence.borrow_mut().values = values.clone();
        crate::ui::style::apply(&values, &ui.window);
    }
    ui.window.present();
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
    ui
}

#[test]
#[ignore = "requires isolated GTK/Xvfb, xdotool and actual GStreamer; original synthetic text/audio only"]
fn native_v08_lyrics_similar_seek_and_stale_selection() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let app = fixture_app();
    if std::env::var("KARIN_TEST_LYRIC_PLAYING_ONLY").as_deref() == Ok("1") {
        playing_frames::ordinary_lyrics_playback(&app);
        return;
    }
    if std::env::var_os("KARIN_TEST_LYRIC_HIGH_CONTRAST").is_some() {
        assert!(adw::StyleManager::default().is_high_contrast());
        playing_frames::ordinary_lyrics_playback(&app);
        return;
    }
    // Adwaita reads the desktop contrast setting once at initialization.
    let contrast = Command::new("timeout")
        .arg("90")
        .arg(std::env::current_exe().unwrap())
        .args([
            "native_v08_lyrics_similar_seek_and_stale_selection",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("ADW_DEBUG_HIGH_CONTRAST", "1")
        .env("KARIN_TEST_LYRIC_HIGH_CONTRAST", "1")
        .status()
        .unwrap();
    assert!(contrast.success(), "high contrast lyric playback");
    provider_flow::run(&app);
    let fixture = Fixture::new();
    // Static layout capture deliberately switches the test process to Reduced
    // Motion. Exercise ordinary playback first, on the untouched GTK settings.
    playing_frames::ordinary_lyrics_playback(&app);
    let ui = window_for(&app, &fixture);
    let window = &ui.window;
    let this = &ui.music.controller;
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
    pump_until(|| this.track_pane.extras.widget.is_mapped());
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
    assert!(!view.lyrics_widget.is_mapped());
    assert!(
        view.inner.lyrics.borrow().is_none(),
        "normal Track Info never preloads lyrics"
    );
    click(window, id, &this.widgets.lyrics, false);
    pump_until(|| view.lyrics_widget.is_mapped() && view.inner.lyrics.borrow().is_some());
    assert_eq!(view.inner.source.text(), "Яндекс Музыка");
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
    // Ordinary TEXT is divided into selectable Unicode stanzas, not an
    // editable TextView. Native Ctrl+C can copy a selected stanza fragment.
    assert_eq!(view.inner.lyrics_status.text(), "Без синхронизации");
    assert_eq!(view.inner.lyrics_model.n_items(), 2);
    pump_until(|| {
        view.inner
            .visible_lines
            .borrow()
            .iter()
            .any(|(position, label)| {
                *position == 0
                    && label
                        .upgrade()
                        .is_some_and(|label| label.is_mapped() && label.height() > 0)
            })
    });
    let paragraph = view
        .inner
        .visible_lines
        .borrow()
        .get(&0)
        .and_then(glib::WeakRef::upgrade)
        .unwrap();
    assert!(paragraph.is_selectable());
    assert_eq!(paragraph.text(), view.inner.lyrics_model.string(0).unwrap());
    assert!(paragraph.grab_focus());
    input(&["key", "--clearmodifiers", "ctrl+a"]);
    input(&["key", "--clearmodifiers", "ctrl+c"]);
    let clipboard = view.lyrics_widget.clipboard();
    pump_until(|| clipboard.is_local() && clipboard.content().is_some());
    let provider_text = clipboard
        .content()
        .and_then(|content| content.value(String::static_type()).ok())
        .and_then(|value| value.get::<String>().ok());
    assert_eq!(
        provider_text.as_deref(),
        view.inner.lyrics_model.string(0).as_deref()
    );
    assert_eq!(
        clipboard_text(&view.lyrics_widget),
        view.inner.lyrics_model.string(0).unwrap()
    );
    drop(paragraph);
    copy_lyrics_menu(window, id, view);
    assert_eq!(
        clipboard_text(&view.lyrics_widget),
        view.inner.lyrics.borrow().as_ref().unwrap().plain
    );

    assert!(view.inner.external.grab_focus());
    settle();
    input(&["key", "space"]);
    let dialog = external_consent_ready(window, view);
    assert_eq!(dialog.default_response().as_deref(), Some("cancel"));
    assert_eq!(fixture.counts.lock().unwrap().external, 0);
    // The safe default genuinely cancels instead of sharing metadata.
    input(&["key", "--clearmodifiers", "Return"]);
    external_consent_closed(view, &dialog);
    drop(dialog);
    assert_eq!(fixture.counts.lock().unwrap().external, 0);
    click(window, id, &view.inner.external, false);
    let dialog = external_consent_ready(window, view);
    let accept = find_button(dialog.upcast_ref(), "Искать").unwrap();
    click(window, id, &accept, false);
    external_consent_closed(view, &dialog);
    drop(accept);
    drop(dialog);
    pump_until(|| {
        view.inner
            .lyrics
            .borrow()
            .as_ref()
            .is_some_and(|lyrics| !lyrics.synchronized.is_empty())
    });
    assert_eq!(fixture.counts.lock().unwrap().external, 1);
    copy_lyrics_menu(window, id, view);
    let copied = clipboard_text(&view.lyrics_widget);
    assert_eq!(copied, view.inner.lyrics.borrow().as_ref().unwrap().plain);
    assert!(
        !copied.contains("[00:"),
        "copy exports displayed text, not LRC timestamp metadata"
    );
    assert_eq!(view.inner.source.text(), "LRCLIB");
    assert_eq!(
        view.inner.source.tooltip_text().as_deref(),
        Some("LRCLIB · внешний источник")
    );
    assert!(view.inner.lyrics_status.text().is_empty());
    assert!(!view.inner.lyrics_status.is_visible());
    assert!(
        !view.inner.lyrics_status.parent().unwrap().is_visible(),
        "synchronized lyrics reserve no status row"
    );
    assert!(
        view.inner.following.get(),
        "Copy menu keyboard input must not suspend lyric following"
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
    pump_until(|| {
        let active = view.inner.active.get();
        view.inner
            .visible_lines
            .borrow()
            .iter()
            .any(|(index, label)| {
                if Some(*index) != active {
                    return false;
                }
                label.upgrade().is_some_and(|label| {
                    label.has_css_class("lyrics-active")
                        && label
                            .compute_bounds(&view.inner.lyrics_scroll)
                            .is_some_and(|bounds| {
                                ((bounds.y() + bounds.height() / 2.0)
                                    - view.inner.lyrics_scroll.height() as f32 / 2.0)
                                    .abs()
                                    < 40.0
                            })
                })
            })
    });
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
    pump_until(|| view.inner.following.get());
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
    click(window, id, &this.widgets.lyrics, false);
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
    pump_until(|| view.inner.following.get());
    let settings = gtk::Settings::default().unwrap();
    let animations = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);
    let seek = this.music_snapshot().unwrap().seek_serial;
    this.seek_music(Duration::from_millis(2760));
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.seek_serial > seek)
    });
    settle();
    assert!(view.inner.scroll_transition.get().is_none());
    assert!(view.inner.following.get());
    settings.set_gtk_enable_animations(animations);
    for (width, height, name) in [
        (420, 720, "v08-lyrics-compact"),
        (900, 650, "v08-lyrics-medium"),
        (1440, 900, "v08-lyrics-wide"),
    ] {
        super::super::player::tests::resize_player_window(&ui, width, height);
        let (minimum, _, _, _) = view.lyrics_widget.measure(gtk::Orientation::Horizontal, -1);
        assert!(minimum <= 300, "lyrics cannot widen the native panel");
        capture_native(name);
    }
    // The header Back action switches modes inside the same pinned container.
    let before_modes = this.music_serial();
    click(window, id, &this.track_pane.back_info, false);
    pump_until(|| view.widget.is_mapped() && !view.lyrics_widget.is_mapped());
    assert_eq!(this.music_serial(), before_modes);
    pump_until(|| view.inner.similar_retry.is_visible() && !view.inner.similar_loading.get());
    assert!(!view.inner.similar_status.text().contains("503"));
    click(window, id, &view.inner.similar_retry, false);
    pump_until(|| view.inner.related.borrow().len() == 12);
    assert!(view.inner.similar_status.text().is_empty());
    assert!(!view.inner.similar_status.is_visible());
    assert!(
        !view.inner.similar_status.parent().unwrap().is_visible(),
        "successful recommendations reserve no status row"
    );
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
    click(window, id, &this.widgets.lyrics, false);
    pump_until(|| {
        view.inner.lyrics_loading.get() && fixture.counts.lock().unwrap().held_started == Some(43)
    });
    assert!(view.inner.lyrics_status.text().is_empty());
    assert!(!view.inner.lyrics_status.is_visible());
    assert!(view.inner.lyrics_spinner.is_visible());
    assert!(view.inner.lyrics_spinner.is_spinning());
    assert!(view.inner.lyrics_status.parent().unwrap().is_visible());
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
    click(window, id, &this.track_pane.back_info, false);
    pump_until(|| view.widget.is_mapped());
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
    });
    click(window, id, &this.widgets.lyrics, false);
    pump_until(|| {
        view.inner.lyrics_loading.get() && fixture.counts.lock().unwrap().held_started == Some(45)
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
    native_line_transitions();
}

// Deterministic confirmed positions isolate GTK presentation. The ordinary
// application scenario above verifies that real GStreamer seek/position,
// occurrence changes, and logout feed that same presentation.
fn native_line_transitions() {
    native_feedback_states();
    check_line_transition_timing();
    let settings = gtk::Settings::default().unwrap();
    let animations = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);
    let manager = adw::StyleManager::default();
    let scheme = manager.color_scheme();
    manager.set_color_scheme(adw::ColorScheme::ForceDark);
    let view = ExtrasView::new(Rc::new(|_| {}));
    view.set_track(Some(&track(42)), None);
    view.set_modes(false, true);
    let window = gtk::Window::builder()
        .title("Karin synthetic lyric motion")
        .default_width(360)
        .default_height(640)
        .child(&view.lyrics_widget)
        .build();
    window.add_css_class("karin-style-system");
    window.present();
    pump_until(|| view.lyrics_widget.is_mapped() && view.inner.lyrics_loading.get());
    let ticket = Ticket {
        selection: view.inner.epoch.get(),
        request: view.inner.lyrics_serial.get(),
    };
    let source = crate::lyrics::LyricsSource::Lrclib;
    let lrc = (0..200)
        .map(|i| {
            format!(
                "[{:02}:{:02}.000]Строка {i} · original Unicode",
                i / 60,
                i % 60
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    view.show_lyrics(ticket, Ok(Lyrics::from_lrc(source, &lrc).unwrap()));
    view.inner.suspend_following();
    let mut snapshot = PlaybackSnapshot {
        state: PlaybackState::Playing,
        position: Some(Duration::from_secs(2)),
        ..PlaybackSnapshot::default()
    };
    view.update_position(Some(&snapshot), 1);

    let mapped_line = |index| {
        view.inner
            .visible_lines
            .borrow()
            .get(&index)
            .and_then(glib::WeakRef::upgrade)
            .filter(|label| label.is_mapped() && label.height() > 0)
    };
    pump_until(|| mapped_line(2).is_some() && mapped_line(3).is_some());
    let old = mapped_line(2).unwrap();
    let next = mapped_line(3).unwrap();
    let heights = (old.height(), next.height());
    let sizes = || (label_scale(&old), label_scale(&next));
    let endpoints = sizes();
    assert!(endpoints.0 > 1.2 && endpoints.1 == 1.0);
    let samples = Rc::new(RefCell::new(Vec::new()));
    let clock = window.frame_clock().unwrap();
    let handler = clock.connect_after_paint({
        let (old, next, samples) = (old.clone(), next.clone(), samples.clone());
        move |_| {
            samples
                .borrow_mut()
                .push((label_scale(&old), label_scale(&next)))
        }
    });
    snapshot.position = Some(Duration::from_secs(3));
    view.update_position(Some(&snapshot), 1);
    pump_until(|| {
        // Repeated snapshots must not restart the transition or rebind rows.
        view.update_position(Some(&snapshot), 1);
        sizes() == (endpoints.1, endpoints.0)
    });
    clock.disconnect(handler);
    assert!(samples.borrow().iter().any(|&(a, b)| {
        (endpoints.1..endpoints.0).contains(&a)
            && a > endpoints.1
            && (endpoints.1..endpoints.0).contains(&b)
            && b > endpoints.1
    }));
    assert_eq!(heights, (old.height(), next.height()));
    snapshot.state = PlaybackState::Paused;
    view.update_position(Some(&snapshot), 1);
    assert_eq!(view.inner.active.get(), Some(3));
    snapshot.seek_serial += 1;
    snapshot.position = Some(Duration::from_secs(2));
    view.update_position(Some(&snapshot), 1);
    assert!(old.has_css_class("lyrics-active"));
    assert_eq!(sizes(), endpoints);
    settings.set_gtk_enable_animations(false);
    snapshot.state = PlaybackState::Playing;
    snapshot.position = Some(Duration::from_secs(3));
    view.update_position(Some(&snapshot), 1);
    assert!(next.has_css_class("lyrics-active"));
    assert_eq!(sizes(), (endpoints.1, endpoints.0));

    // A third quick boundary retargets the still-growing fourth row.
    // The first row keeps shrinking and the new target grows from its rest size.
    settings.set_gtk_enable_animations(true);
    let fourth = mapped_line(4).unwrap();
    let fifth = mapped_line(5).unwrap();
    snapshot.position = Some(Duration::from_secs(4));
    view.update_position(Some(&snapshot), 1);
    pump_until(|| label_scale(&fourth) > 1.04 && label_scale(&fourth) < endpoints.0);
    let reversing = label_scale(&fourth);
    let shrinking = label_scale(&next);
    snapshot.position = Some(Duration::from_secs(5));
    view.update_position(Some(&snapshot), 1);
    assert_eq!(label_scale(&fourth), reversing);
    assert_eq!(label_scale(&next), shrinking);
    pump_until(|| label_scale(&fourth) < reversing && label_scale(&fifth) > 1.0);
    settings.set_gtk_enable_animations(false);
    pump_until(|| label_scale(&fifth) == endpoints.0);
    assert_eq!((label_scale(&fourth), label_scale(&next)), (1.0, 1.0));

    // A different native source/seek target is styled from the model after
    // virtualization brings the new row into view, without a stale fade.
    window.add_css_class("karin-high-contrast");
    snapshot.seek_serial += 1;
    snapshot.position = Some(Duration::from_secs(151));
    view.update_position(Some(&snapshot), 1);
    view.inner
        .lyrics_list
        .scroll_to(152, gtk::ListScrollFlags::NONE, None);
    pump_until(|| mapped_line(151).is_some() && mapped_line(152).is_some());
    let active = mapped_line(151).unwrap();
    assert!(active.has_css_class("lyrics-active"));
    assert_eq!(label_scale(&active), endpoints.0);
    assert!(
        view.inner
            .visible_lines
            .borrow()
            .iter()
            .all(|(index, label)| {
                label.upgrade().is_none_or(|label| {
                    label.has_css_class("lyrics-active") == (*index == 151)
                        && label_scale(&label) == if *index == 151 { endpoints.0 } else { 1.0 }
                })
            })
    );
    view.show_lyrics(
        ticket,
        Ok(Lyrics::from_text(source, "Later plain-only result").unwrap()),
    );
    assert_eq!(view.inner.active.get(), Some(151));
    assert!(
        view.inner.synchronized.get(),
        "plain refresh cannot downgrade an admitted LRC"
    );
    view.set_track(None, None);
    assert_eq!(view.inner.lyrics_model.n_items(), 0);
    assert!(view.inner.active.get().is_none());
    assert!(!view.lyrics_is_current(ticket));
    window.close();
    settings.set_gtk_enable_animations(animations);
    manager.set_color_scheme(scheme);
}

fn label_scale(label: &gtk::Label) -> f64 {
    label
        .attributes()
        .and_then(|attrs| {
            attrs
                .iterator()
                .get(gtk::pango::AttrType::Scale)
                .and_then(|attr| attr.downcast::<gtk::pango::AttrFloat>().ok())
                .map(|attr| attr.value())
        })
        .unwrap_or(1.0)
}

fn native_feedback_states() {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let (status, spinner, retry) = feedback(&root, "", "synthetic-feedback");
    let row = status.parent().unwrap();
    assert!(!row.is_visible());
    assert!(!status.is_visible());
    assert_eq!(root.measure(gtk::Orientation::Vertical, -1).0, 0);
    spinner.set_visible(true);
    assert!(row.is_visible());
    assert!(
        !status.is_visible(),
        "loading uses only its compact spinner"
    );
    status.set_text("Не удалось загрузить.");
    spinner.set_visible(false);
    retry.set_visible(true);
    assert!(row.is_visible() && status.is_visible());
    status.set_text("");
    retry.set_visible(false);
    assert!(!row.is_visible());
    assert_eq!(root.measure(gtk::Orientation::Vertical, -1).0, 0);

    let view = ExtrasView::new(Rc::new(|_| {}));
    view.set_track(Some(&track(42)), None);
    let ticket = Ticket {
        selection: view.inner.epoch.get(),
        request: view.inner.similar_serial.get(),
    };
    view.show_similar(
        ticket,
        Ok(SimilarTracks {
            tracks: Vec::new(),
            omitted: 0,
        }),
    );
    assert_eq!(view.inner.similar_status.text(), "Похожих треков нет");
    assert!(view.inner.similar_status.parent().unwrap().is_visible());
    assert!(!view.inner.similar_rows.is_visible());
    view.show_similar(
        ticket,
        Ok(SimilarTracks {
            tracks: vec![track(43)],
            omitted: 1,
        }),
    );
    assert_eq!(view.inner.related.borrow().len(), 1);
    assert!(view.inner.similar_rows.is_visible());
    assert!(view.inner.similar_status.is_visible());
    view.show_similar(
        ticket,
        Ok(SimilarTracks {
            tracks: vec![track(43)],
            omitted: 0,
        }),
    );
    assert!(!view.inner.similar_status.parent().unwrap().is_visible());
    assert_eq!(view.inner.related.borrow().len(), 1);
}
