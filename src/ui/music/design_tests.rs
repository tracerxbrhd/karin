// SPDX-License-Identifier: GPL-3.0-or-later
//! Matched native a70bbf7/current refinement evidence. All account/catalog/artwork data below is
//! original synthetic loopback data; no fixture is compiled into production.

use super::*;
use gtk::gdk;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const TOKEN: &str = "synthetic-native-design-token";
const TITLES: [&str; 8] = [
    "Горизонт",
    "Тихие улицы",
    "После заката",
    "Океан сигналов",
    "Пульс",
    "Северный свет",
    "Медленный рассвет",
    "Долгая дорога домой",
];
const ARTISTS: [&str; 4] = ["Aster Field", "Mira Vale", "Новая глава", "Elian"];

struct WindowManager(Option<Child>);
impl WindowManager {
    fn start() -> Self {
        let present = Command::new("wmctrl")
            .arg("-m")
            .output()
            .is_ok_and(|result| result.status.success());
        let owned = (!present).then(|| {
            Command::new("openbox")
                .arg("--sm-disable")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("native design evidence requires Openbox")
        });
        let manager = Self(owned);
        pump(|| {
            Command::new("wmctrl")
                .arg("-m")
                .output()
                .is_ok_and(|result| result.status.success())
        });
        manager
    }
}
impl Drop for WindowManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[track_caller]
fn pump(mut condition: impl FnMut() -> bool) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !condition() {
        assert!(Instant::now() < deadline, "native design fixture timed out");
        if context.pending() {
            context.iteration(false);
        } else {
            thread::sleep(Duration::from_millis(5));
        }
    }
}
fn settle() {
    let deadline = Instant::now() + Duration::from_millis(220);
    pump(|| Instant::now() >= deadline);
}
fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut found = vec![widget.clone()];
    let mut child = widget.first_child();
    while let Some(current) = child {
        found.extend(descendants(&current));
        child = current.next_sibling();
    }
    found
}
fn textures(widget: &impl IsA<gtk::Widget>) -> usize {
    descendants(widget.as_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Picture>().ok())
        .filter(|picture| {
            picture.is_visible()
                && picture.is_mapped()
                && picture
                    .paintable()
                    .is_some_and(|paintable| paintable.is::<gdk::Texture>())
        })
        .count()
}
fn assert_decoded_artwork(widget: &impl IsA<gtk::Widget>) {
    let texture = descendants(widget.as_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Picture>().ok())
        .find_map(|picture| {
            picture
                .paintable()
                .and_then(|paintable| paintable.downcast::<gdk::Texture>().ok())
        })
        .expect("synthetic artwork must reach a native decoded texture");
    assert!(texture.width() >= 128 && texture.height() >= 128);
    let stride = texture.width() as usize * 4;
    let mut pixels = vec![0; stride * texture.height() as usize];
    texture.download(&mut pixels, stride);
    let first = &pixels[..3];
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| &pixel[..3] != first),
        "HTTP-decoded original raster bands must be visible, not a symbolic fallback"
    );
}
fn cover(index: usize) -> String {
    format!(
        "avatars.yandex.net/get-music-content/karin-design/cover-{}/%%",
        index % 5
    )
}
fn track(index: usize) -> Value {
    json!({"id":index+1,"title":TITLES[index],"available":true,
        "durationMs":217000+index*7000,"coverUri":cover(index),
        "artists":[{"id":11+index%4,"name":ARTISTS[index%4]}],
        "albums":[{"id":7,"title":"Тихие улицы","coverUri":cover(1)}]})
}
fn playlist(index: usize) -> Value {
    json!({"owner":{"uid":1234},"kind":index+3,
        "title":(["Вечерний маршрут","Звуки города","Новые горизонты"][index]),
        "trackCount":8,"cover":{"itemsUri":[cover(index+2)]}})
}
fn reply(stream: &mut TcpStream, status: u16, mime: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status} OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

// The same original four-second encoding used by the playback regressions.
// This fixture traverses StreamResolver and the normal GStreamer worker before
// pausing for static evidence; queue selection alone never makes the footer active.
pub(super) fn serve_original_media(stream: &mut TcpStream, headers: &str, media: &[u8]) {
    let range = headers.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("range").then_some(value.trim())
    });
    let range = range.map(|value| {
        let (start, end) = value
            .strip_prefix("bytes=")
            .unwrap()
            .split_once('-')
            .unwrap();
        let start = start.parse::<usize>().unwrap();
        let end = if end.is_empty() {
            media.len() - 1
        } else {
            end.parse::<usize>().unwrap().min(media.len() - 1)
        };
        assert!(start <= end && end < media.len());
        (start, end)
    });
    let (start, end) = range.unwrap_or((0, media.len() - 1));
    let status = if range.is_some() {
        "206 Partial Content"
    } else {
        "200 OK"
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: audio/mpeg\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\nConnection: close\r\n",
        end - start + 1
    );
    if range.is_some() {
        let _ = write!(
            stream,
            "Content-Range: bytes {start}-{end}/{}\r\n",
            media.len()
        );
    }
    let _ = stream.write_all(b"\r\n");
    if !headers.starts_with("HEAD ") {
        let _ = stream.write_all(&media[start..=end]);
    }
}

struct Fixture {
    origin: String,
    stop: Arc<AtomicBool>,
    images: Arc<AtomicUsize>,
    media_requests: Arc<AtomicUsize>,
    search_error: Arc<AtomicBool>,
    search_hold: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}
impl Fixture {
    fn new() -> Self {
        let mut art = BTreeMap::new();
        for size in [200, 400, 1000] {
            for (index, color) in [
                0x8b59_aaff,
                0x2697_a5ff,
                0xd583_66ff,
                0x556a_9eff,
                0x608a_62ff,
            ]
            .into_iter()
            .enumerate()
            {
                let pixbuf = gtk::gdk_pixbuf::Pixbuf::new(
                    gtk::gdk_pixbuf::Colorspace::Rgb,
                    false,
                    8,
                    size,
                    size,
                )
                .unwrap();
                pixbuf.fill(color);
                // Original raster bands give visibly decoded covers without
                // borrowing any music artwork or the approved mockup assets.
                let band = gtk::gdk_pixbuf::Pixbuf::new(
                    gtk::gdk_pixbuf::Colorspace::Rgb,
                    false,
                    8,
                    size,
                    size / 8,
                )
                .unwrap();
                band.fill(color ^ 0x2525_2500);
                for y in [size / 4, size * 3 / 4] {
                    band.copy_area(0, 0, size, size / 8, &pixbuf, 0, y);
                }
                let format = if index % 2 == 0 { "png" } else { "jpeg" };
                art.insert(
                    (index, size),
                    (format, pixbuf.save_to_bufferv(format, &[]).unwrap()),
                );
            }
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let images = Arc::new(AtomicUsize::new(0));
        let search_error = Arc::new(AtomicBool::new(false));
        let search_hold = Arc::new(AtomicBool::new(false));
        let media_requests = Arc::new(AtomicUsize::new(0));
        let media = player::tests::generated_mp3();
        let server = {
            let stop = stop.clone();
            let images = images.clone();
            let search_error = search_error.clone();
            let search_hold = search_hold.clone();
            let media_requests = media_requests.clone();
            let origin = origin.clone();
            thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(value) => value,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => panic!("synthetic design listener failed"),
                    };
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    let mut byte = [0];
                    while !bytes.ends_with(b"\r\n\r\n") {
                        if stream.read(&mut byte).unwrap_or(0) == 0 {
                            break;
                        }
                        bytes.push(byte[0]);
                        assert!(bytes.len() < 16 * 1024);
                    }
                    if !bytes.ends_with(b"\r\n\r\n") {
                        continue;
                    }
                    let headers = String::from_utf8(bytes).unwrap();
                    let lower = headers.to_ascii_lowercase();
                    let length = lower
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length:")
                                .map(|value| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    assert!(length < 16 * 1024);
                    let mut body = vec![0; length];
                    stream.read_exact(&mut body).unwrap();
                    let target = headers
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap();
                    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).unwrap();
                    if url.path().starts_with("/get-mp3/")
                        || url.path().starts_with("/download-info/")
                    {
                        assert!(
                            !lower.contains("authorization:")
                                && !lower.contains("cookie:")
                                && !lower.contains("referer:")
                                && !lower.contains("origin:")
                        );
                        if url.path().starts_with("/download-info/") {
                            reply(&mut stream, 200, "application/xml", b"<download-info><host>s268myt.storage.yandex.net</host><path>/karin-original-design.mp3</path><ts>1700000000</ts><s>original-synthetic-input</s></download-info>");
                        } else {
                            media_requests.fetch_add(1, Ordering::AcqRel);
                            serve_original_media(&mut stream, &headers, &media);
                        }
                        continue;
                    }
                    if url.path().starts_with("/get-music-content/") {
                        assert!(
                            !lower.contains("authorization:")
                                && !lower.contains("cookie:")
                                && !lower.contains("referer:")
                        );
                        let parts: Vec<_> = url.path().split('/').collect();
                        let index = parts[3]
                            .strip_prefix("cover-")
                            .unwrap()
                            .parse::<usize>()
                            .unwrap();
                        let size = parts[4].split('x').next().unwrap().parse::<i32>().unwrap();
                        let (format, bytes) = art
                            .get(&(index, size))
                            .expect("bounded native artwork preset");
                        images.fetch_add(1, Ordering::AcqRel);
                        reply(
                            &mut stream,
                            200,
                            if *format == "jpeg" {
                                "image/jpeg"
                            } else {
                                "image/png"
                            },
                            bytes,
                        );
                        continue;
                    }
                    assert!(lower.contains(&format!("authorization: oauth {TOKEN}")));
                    assert!(
                        !lower.contains("cookie:") && !lower.contains("x-yandex-music-client:")
                    );
                    if url.path() == "/search" {
                        while search_hold.load(Ordering::Acquire) && !stop.load(Ordering::Acquire) {
                            thread::sleep(Duration::from_millis(5));
                        }
                    }
                    if url.path() == "/search" && search_error.load(Ordering::Acquire) {
                        reply(&mut stream, 503, "application/json", b"{}");
                        continue;
                    }
                    let tracks: Vec<_> = (0..8).map(track).collect();
                    let references: Vec<_> = tracks
                        .iter()
                        .enumerate()
                        .map(|(index, track)| json!({"id":index+1,"track":track}))
                        .collect();
                    let result = match url.path() {
                        path if path.starts_with("/tracks/")
                            && path.ends_with("/download-info") =>
                        {
                            json!({"result":[{"codec":"mp3","bitrateInKbps":192,"preview":false,"direct":false,
                                "downloadInfoUrl":format!("{origin}download-info/1")} ]})
                        }
                        "/account/status" => {
                            json!({"result":{"account":{"uid":1234,"displayName":"Synthetic design account"}}})
                        }
                        "/users/1234/likes/tracks" => {
                            json!({"result":{"library":{"uid":1234,"revision":1,"tracks":references}}})
                        }
                        "/users/1234/playlists/list" => {
                            json!({"result":(0..3).map(playlist).collect::<Vec<_>>()})
                        }
                        "/users/1234/likes/playlists" => {
                            json!({"result":[{"playlist":{"owner":{"uid":4321},"kind":8,
                                "title":"Открытый горизонт","trackCount":8,
                                "cover":{"itemsUri":[cover(4)]}}}]})
                        }
                        "/users/1234/playlists/3"
                        | "/users/1234/playlists/4"
                        | "/users/1234/playlists/5" => json!({"result":{"tracks":references}}),
                        "/albums/7/with-tracks" => {
                            json!({"result":{"id":7,"title":"Тихие улицы","coverUri":cover(1),"artists":[{"id":11,"name":"Aster Field"}],"volumes":[tracks]}})
                        }
                        "/tracks" => {
                            let form = reqwest::Url::parse(&format!(
                                "http://127.0.0.1/?{}",
                                String::from_utf8(body).unwrap()
                            ))
                            .unwrap();
                            let ids: Vec<_> = form
                                .query_pairs()
                                .filter(|(key, _)| key == "track-ids")
                                .flat_map(|(_, value)| {
                                    value.split(',').map(str::to_owned).collect::<Vec<_>>()
                                })
                                .collect();
                            assert!(!ids.is_empty());
                            json!({"result":ids.iter().map(|id| track(id.split(':').next().unwrap().parse::<usize>().unwrap()-1)).collect::<Vec<_>>()})
                        }
                        "/search" => json!({"result":{"text":"Синтетическая музыка",
                            "tracks":{"total":8,"perPage":20,"page":0,"results":(1..=8).map(|id| json!({"id":id})).collect::<Vec<_>>()},
                            "albums":{"total":1,"perPage":20,"page":0,"results":[{"id":7,"title":"Тихие улицы","coverUri":cover(1),"artists":[{"id":11,"name":"Aster Field"}]}]},
                            "artists":{"total":1,"perPage":20,"page":0,"results":[{"id":11,"name":"Aster Field","cover":{"uri":cover(0)}}]},
                            "playlists":{"total":3,"perPage":20,"page":0,"results":(0..3).map(playlist).collect::<Vec<_>>()}}}),
                        _ => panic!("unexpected synthetic design API route"),
                    };
                    reply(
                        &mut stream,
                        200,
                        "application/json",
                        result.to_string().as_bytes(),
                    );
                }
            })
        };
        Self {
            origin,
            stop,
            images,
            media_requests,
            search_error,
            search_hold,
            server: Some(server),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(server) = self.server.take()
            && !thread::panicking()
        {
            server.join().unwrap();
        }
    }
}

// Native actions are the only mode entry points used by the visual matrix.
// Mode intent is explicit application state, never inferred from an animation.
fn pane_open(this: &Controller) -> bool {
    this.panel_mode.get() != workspace::PanelMode::Closed
}
fn set_panel(ui: &crate::ui::MainWindow, mode: workspace::PanelMode) {
    use workspace::PanelMode;
    let this = &ui.music.controller;
    let previous = this.panel_mode.get();
    if previous != mode {
        let action = match (previous, mode) {
            (PanelMode::Queue, PanelMode::Closed) => "win.music-queue",
            (_, PanelMode::Queue) => "win.music-queue",
            _ => "win.music-info",
        };
        gtk::prelude::WidgetExt::activate_action(&ui.window, action, None).unwrap();
    }
    pump(|| this.panel_mode.get() == mode);
    pump(|| {
        this.info_host
            .borrow()
            .as_ref()
            .and_then(|host| host.revealer())
            .is_some_and(|panel| {
                if mode == PanelMode::Closed {
                    panel.progress() <= f64::EPSILON
                } else {
                    panel.is_child_revealed()
                }
            })
    });
    settle();
    if std::env::var_os("KARIN_V04_ASSERT_DENSITY").is_some() {
        settle_geometry(ui, "panel-mode");
    }
}
fn set_pane(ui: &crate::ui::MainWindow, open: bool) {
    set_panel(
        ui,
        if open {
            workspace::PanelMode::TrackInfo
        } else {
            workspace::PanelMode::Closed
        },
    );
}
fn primary_destination(ui: &crate::ui::MainWindow, route: &str) {
    let action = match route {
        "home" => "win.music-home",
        "library" => "win.music-library",
        "playlists" => "win.music-playlists",
        _ => panic!("unknown fixture primary destination"),
    };
    gtk::prelude::WidgetExt::activate_action(&ui.window, action, None).unwrap();
    let this = &ui.music.controller;
    pump(|| this.widgets.stack.visible_child_name().as_deref() == Some(route));
    settle();
    assert!(
        descendants(ui.music.widget.upcast_ref())
            .into_iter()
            .all(|widget| !widget.is::<gtk::DropDown>()),
        "primary navigation must not retain parented technical dropdowns"
    );
}
fn resize(ui: &crate::ui::MainWindow, width: i32, height: i32) {
    ui.window.set_default_size(width, height);
    pump(|| (ui.window.width() - width).abs() <= 32 && (ui.window.height() - height).abs() <= 32);
    settle_geometry(ui, &format!("resize-{width}x{height}"));
    settle();
}
fn settle_geometry(ui: &crate::ui::MainWindow, stage: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    let last = RefCell::new(None);
    pump(|| match ui.verify_layout() {
        Ok(()) => true,
        Err(error) => {
            if last.replace(Some(error.clone())).as_ref() != Some(&error) {
                eprintln!("Synthetic design allocation {stage}: {error}");
            }
            Instant::now() >= deadline
        }
    });
    if let Err(error) = ui.verify_layout() {
        super::tests::capture_native(&format!(
            "failure-allocation-{stage}-scale{}",
            ui.window.scale_factor()
        ));
        panic!("native design allocation {stage} failed: {error}");
    }
}
fn row_metrics(ui: &crate::ui::MainWindow) -> (Vec<i32>, usize) {
    let list = &ui.music.controller.widgets.library_rows;
    let clip = list
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_then(|view| view.compute_bounds(&ui.window));
    let player = ui.music.player_bar.compute_bounds(&ui.window);
    let mut heights = Vec::new();
    let mut visible = 0;
    for row in descendants(list.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::ListBoxRow>().ok())
    {
        heights.push(row.height());
        if row.is_mapped()
            && let Some(bounds) = row.compute_bounds(&ui.window)
            && let Some(clip) = clip
        {
            let bottom = player.map_or(clip.y() + clip.height(), |bar| {
                bar.y().min(clip.y() + clip.height())
            });
            if bounds.y() >= clip.y() - 1.0 && bounds.y() + bounds.height() <= bottom + 1.0 {
                visible += 1;
            }
        }
    }
    (heights, visible)
}
fn assert_footer_geometry(ui: &crate::ui::MainWindow) {
    let this = &ui.music.controller;
    assert!(
        !this.widgets.stop.is_mapped(),
        "Stop is system/internal-only"
    );
    if !this.widgets.player_revealer.reveals_child() {
        assert!(!this.widgets.player_revealer.is_child_revealed());
        for control in [
            this.widgets.play.upcast_ref::<gtk::Widget>(),
            this.widgets.current_art_button.upcast_ref(),
            this.widgets.queue.upcast_ref(),
        ] {
            assert!(
                !control.is_mapped(),
                "an inactive footer must not retain a painted/focusable control"
            );
        }
        return;
    }
    assert!(this.widgets.player_revealer.is_child_revealed());
    let metadata = this
        .widgets
        .current_art_button
        .compute_bounds(&ui.window)
        .unwrap();
    for content in [
        this.widgets.current_art.upcast_ref::<gtk::Widget>(),
        this.widgets.current.upcast_ref(),
        this.widgets.current_artist.upcast_ref(),
    ] {
        assert!(
            content.is_ancestor(&this.widgets.current_art_button),
            "cover, title and artist belong to the single native info button"
        );
        let bounds = content.compute_bounds(&ui.window).unwrap();
        assert!(
            bounds.x() >= metadata.x() - 1.0
                && bounds.y() >= metadata.y() - 1.0
                && bounds.x() + bounds.width() <= metadata.x() + metadata.width() + 1.0
                && bounds.y() + bounds.height() <= metadata.y() + metadata.height() + 1.0,
            "whole current-track activation must include its visible metadata: {bounds:?}/{metadata:?}"
        );
    }
    assert!(
        !this
            .widgets
            .current_like_slot
            .is_ancestor(&this.widgets.current_art_button),
        "Like stays outside the metadata button, preserving its independent input/focus"
    );
    let badge = player_bar::repeat_one_badge(&this.widgets.repeat)
        .expect("stable native literal-one badge");
    assert_eq!(badge.text(), "1");
    let one = this.model.borrow().queue.repeat_mode() == crate::queue::RepeatMode::One;
    assert_eq!(
        badge.is_visible(),
        one,
        "literal1 must reflect the shared repeat policy"
    );
    if one && this.widgets.repeat.is_mapped() {
        assert!(badge.is_mapped(), "literal1 must actually be painted");
    }
    let footer = ui.music.player_bar.compute_bounds(&ui.window).unwrap();
    for name in [
        "music-current-info",
        "music-current-like",
        "music-play",
        "music-next",
        "music-queue-toggle",
        "music-volume-menu",
        "music-progress",
    ] {
        let control = named_widget(&ui.music.player_bar, name);
        assert!(
            control.is_mapped(),
            "persistent footer control is missing: {name}"
        );
        let rect = control.compute_bounds(&ui.window).unwrap();
        assert!(rect.width() > 10.0 && rect.height() > 10.0);
        if name == "music-current-info" || name == "music-current-like" {
            assert!(
                rect.width() >= 44.0 && rect.height() >= 44.0,
                "metadata and confirmed Like remain usable pointer targets"
            );
        }
        assert!(
            rect.x() >= footer.x() - 1.0
                && rect.y() >= footer.y() - 1.0
                && rect.x() + rect.width() <= footer.x() + footer.width() + 1.0
                && rect.y() + rect.height() <= footer.y() + footer.height() + 1.0,
            "footer control escaped its allocated surface: {name} {rect:?} / {footer:?}"
        );
    }
    let modes = named_widget(&ui.music.player_bar, "music-playback-modes");
    if ui.music.player_bar.width() < 720 {
        assert!(
            modes.is_mapped(),
            "compact shuffle/repeat popover entry must remain reachable"
        );
    } else {
        assert!(!modes.is_visible());
        assert!(this.widgets.shuffle.is_mapped() && this.widgets.repeat.is_mapped());
        let transport = named_widget(&ui.music.player_bar, "music-transport")
            .compute_bounds(&ui.window)
            .unwrap();
        let center = this.widgets.player_main.compute_bounds(&ui.window).unwrap();
        assert!(
            (transport.x() + transport.width() / 2.0 - center.x() - center.width() / 2.0).abs()
                <= 2.0,
            "wide transport must stay visually centered, independent of metadata: transport={transport:?} main={center:?} footer={footer:?}"
        );
    }
}

// The normal matrix contains one short title, one artist and one album.
// Scrolling remains appropriate for the separate long-metadata state, but the
// ordinary musical cluster must not leave a half Play button behind the footer.
fn assert_short_info_actions_visible(ui: &crate::ui::MainWindow) {
    let pane = &ui.music.controller.track_pane;
    let scroll = named_widget(&pane.widget, "music-track-pane-scroll");
    let viewport = scroll.compute_bounds(&ui.window).unwrap();
    for action in [&pane.play, &pane.enqueue, &pane.details] {
        assert!(
            action.is_mapped(),
            "normal TrackInfo action must actually map"
        );
        let rect = action.compute_bounds(&ui.window).unwrap();
        assert!(
            rect.x() >= viewport.x() - 1.0
                && rect.y() >= viewport.y() - 1.0
                && rect.x() + rect.width() <= viewport.x() + viewport.width() + 1.0
                && rect.y() + rect.height() <= viewport.y() + viewport.height() + 1.0,
            "normal short TrackInfo actions must fit the actual viewport: {} {rect:?}/{viewport:?}",
            action.widget_name()
        );
    }
}

fn evidence(ui: &crate::ui::MainWindow, name: &str) {
    assert_footer_geometry(ui);
    assert!(
        descendants(ui.music.widget.upcast_ref())
            .into_iter()
            .all(|widget| widget.widget_name() != "music-feed-continue"),
        "continuous track feeds must not reintroduce a manual pagination button"
    );
    let scale = ui.window.scale_factor();
    let name = format!("{name}-scale{scale}");
    super::tests::capture_native(&name);
    if std::env::var_os("KARIN_TEST_COMPARE_RENDER_READBACK").is_some()
        && (name.starts_with("surface-wave-placeholder-scale")
            || name.starts_with("matrix-wide-panel-track-scale"))
    {
        compare_render_readback(ui, &name);
    }
    let (heights, visible) = row_metrics(ui);
    eprintln!(
        "Native design renderer: {}",
        ui.window.renderer().unwrap().type_().name()
    );
    eprintln!(
        "Synthetic design evidence: {name} logical={}x{} rows={heights:?} fully-visible={visible}",
        ui.window.width(),
        ui.window.height()
    );
    if let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") {
        let path =
            std::path::Path::new(&directory).join(format!("native-layout-scale{scale}.jsonl"));
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        for theme in ["light", "dark"] {
            writeln!(file,"{}",json!({"capture":format!("{name}-{theme}.png"),"logical_width":ui.window.width(),"logical_height":ui.window.height(),"scale":scale,"maximized":ui.window.is_maximized(),"pane_open":pane_open(&ui.music.controller),"panel_mode":format!("{:?}",ui.music.controller.panel_mode.get()),"renderer":ui.window.renderer().unwrap().type_().name(),"rows":heights,"fully_visible_rows":visible,"source":"synthetic-loopback-musicservice-http-decoded-artwork","tracks":8,"footer_art_hit":[ui.music.controller.widgets.current_art_button.width(),ui.music.controller.widgets.current_art_button.height()],"footer_track_hit":[ui.music.controller.widgets.current_art_button.width(),ui.music.controller.widgets.current_art_button.height()],"footer_cover_size":[ui.music.controller.widgets.current_art.width(),ui.music.controller.widgets.current_art.height()],"footer_like_mapped":ui.music.controller.widgets.current_like_slot.is_mapped(),"footer_title_width":ui.music.controller.widgets.current.width(),"player_requested_visible":ui.music.controller.widgets.player_revealer.reveals_child(),"player_child_revealed":ui.music.controller.widgets.player_revealer.is_child_revealed(),"player_height":ui.music.player_bar.height(),"player_state":format!("{:?}",ui.music.controller.music_snapshot().map(|snapshot|snapshot.state))})).unwrap();
        }
    }
}

// Optional diagnosis of Cairo/X11 partial-damage readback. This does not queue
// redraws, change widget visibility, or force a production renderer. The same
// current GTK render node is saved beside a second unmodified X11 readback.
fn compare_render_readback(ui: &crate::ui::MainWindow, name: &str) {
    let directory = std::path::PathBuf::from(std::env::var("KARIN_TEST_CAPTURE_DIR").unwrap());
    let paintable = gtk::WidgetPaintable::new(Some(&ui.window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(
        &snapshot,
        f64::from(ui.window.width()),
        f64::from(ui.window.height()),
    );
    let node = snapshot
        .to_node()
        .expect("settled native window render node");
    let texture = ui.window.renderer().unwrap().render_texture(
        &node,
        Some(&gtk::graphene::Rect::new(
            0.0,
            0.0,
            ui.window.width() as f32,
            ui.window.height() as f32,
        )),
    );
    texture
        .save_to_png(directory.join(format!("diagnostic-gtk-render-{name}-dark.png")))
        .unwrap();
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
    let windows = String::from_utf8(output.stdout).unwrap();
    let id = windows.lines().next().unwrap();
    assert!(id.bytes().all(|byte| byte.is_ascii_digit()));
    assert!(
        Command::new("import")
            .args(["-silent", "-screen", "-window", id])
            .arg(directory.join(format!("diagnostic-x11-readback-{name}-dark.png")))
            .status()
            .unwrap()
            .success()
    );
}

fn populated_window(
    active: bool,
) -> (
    Fixture,
    adw::Application,
    crate::ui::MainWindow,
    Page<Track>,
    Album,
) {
    let fixture = Fixture::new();
    let service = MusicService::for_test(&fixture.origin);
    let token = AccessToken::for_test(TOKEN, Duration::from_secs(600));
    service.account(&token).unwrap();
    let liked = service.liked_tracks(&token, 0).unwrap();
    let playlists = service.playlists(&token, 0).unwrap();
    let album = service.album(&token, "7").unwrap();
    assert_eq!(liked.items.len(), 8);
    let context = PlaybackContext::new(
        ContextKind::LikedTracks,
        None,
        liked.items.clone(),
        0,
        false,
    )
    .unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.DesignFixture")
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
    let this = &ui.music.controller;
    this.artwork
        .as_ref()
        .unwrap()
        .set_test_origin(&fixture.origin)
        .unwrap();
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token);
        model.account_valid = true;
        model.liked = Some(Ok(liked.clone()));
        model.playlists = Some(Ok(playlists));
        model.queue.replace_context(context.clone());
    }
    this.widgets.account.set_text("Synthetic design account");
    this.render_controls();
    this.render_queue();
    this.render_home_preview();
    this.render_liked(Ok(liked.clone()), 0);
    this.select_track_info(liked.items[0].clone(), Some(context), true);
    this.widgets.stack.set_visible_child_name("library");
    this.widgets.nav_liked.set_active(true);
    this.set_fixture_resolver_for_test(crate::stream::StreamResolver::for_test(
        &fixture.origin,
        &fixture.origin,
        &fixture.origin,
    ));
    ui.window.set_title(Some("Karin synthetic HTTP fixture"));
    ui.window.set_default_size(1440, 900);
    ui.window.present();
    pump(|| textures(&this.widgets.library_rows) >= 6);
    assert_decoded_artwork(&this.widgets.library_rows);
    if active {
        activate_design_player(&ui, &fixture);
    }
    assert!(fixture.images.load(Ordering::Acquire) > 0);
    let actual = ui.window.renderer().unwrap().type_().name().to_string();
    eprintln!("Native design renderer: {actual}");
    if let Ok(requested) = std::env::var("GSK_RENDERER") {
        match requested.as_str() {
            "cairo" => assert_eq!(actual, "GskCairoRenderer"),
            "ngl" | "opengl" => assert!(
                actual.to_ascii_lowercase().contains("glrenderer"),
                "requested GL renderer must not silently fall back: {actual}"
            ),
            "gl" => {
                assert!(
                    gtk::major_version() == 4 && gtk::minor_version() < 18,
                    "legacy GL is removed in GTK 4.18; use ngl without silent fallback"
                );
                assert_eq!(actual, "GskGLRenderer");
            }
            _ => {}
        }
    }
    (fixture, app, ui, liked, album)
}

fn activate_design_player(ui: &crate::ui::MainWindow, fixture: &Fixture) {
    activate_design_player_observed(ui, fixture, false);
}

fn activate_design_player_observed(ui: &crate::ui::MainWindow, fixture: &Fixture, observe: bool) {
    use crate::playback::PlaybackState;
    let this = &ui.music.controller;
    let first = this
        .widgets
        .library_rows
        .row_at_index(0)
        .expect("first real loaded track row");
    if observe {
        observe_footer_transition(ui, "first-start", true, || native_click(ui, &first));
    } else {
        native_click(ui, &first);
    }
    pump(|| {
        this.music_snapshot()
            .is_some_and(|state| state.state == PlaybackState::Playing)
    });
    assert!(
        fixture.media_requests.load(Ordering::Acquire) > 0,
        "visible-state evidence must traverse the normal resolver and remote GStreamer source"
    );
    this.toggle_music();
    pump(|| {
        this.music_snapshot()
            .is_some_and(|state| state.state == PlaybackState::Paused)
    });
    pump(|| {
        this.widgets.player_revealer.is_child_revealed() && textures(&this.widgets.current_art) > 0
    });
    assert_decoded_artwork(&this.widgets.current_art);
    settle();
}

#[test]
#[ignore = "isolated GTK/X11 design evidence; run under Xvfb with GDK_SCALE=1 and2"]
fn native_design_matched_matrix_and_populated_surfaces() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let _manager = WindowManager::start();
    let (fixture, _app, ui, liked, album) = populated_window(false);
    let this = &ui.music.controller;
    set_panel(&ui, workspace::PanelMode::Closed);
    // Queue selection is deliberately idle. The footer must stay absent until
    // the real resolver/worker request below; hidden captures are not a forced
    // visibility facsimile or evidence of service audibility.
    for (width, height, shape) in [
        (420, 720, "compact"),
        (900, 650, "medium"),
        (1440, 900, "wide"),
    ] {
        resize(&ui, width, height);
        assert!(!this.widgets.player_revealer.reveals_child());
        evidence(&ui, &format!("surface-player-hidden-{shape}"));
    }
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(true);
    activate_design_player_observed(&ui, &fixture, true);
    evidence(&ui, "surface-player-visible-paused");
    // Eight geometries × three genuine panel modes × two themes × two
    // separate scale processes =96 current images, matched to the archived
    // a70 baseline's actual panel representations at the same outer dimensions.
    for (width, height, name) in [
        (420, 720, "compact"),
        (720, 650, "narrow"),
        (900, 650, "medium"),
        (1280, 800, "desktop"),
        (1440, 900, "wide"),
        (1650, 930, "owner-wide"),
        (1920, 1080, "large"),
    ] {
        set_pane(&ui, false);
        resize(&ui, width, height);
        for (mode, label) in [
            (workspace::PanelMode::Closed, "closed"),
            (workspace::PanelMode::TrackInfo, "track"),
            (workspace::PanelMode::Queue, "queue"),
        ] {
            set_panel(&ui, mode);
            if mode == workspace::PanelMode::TrackInfo {
                pump(|| textures(&this.track_pane.widget) > 0);
                assert_short_info_actions_visible(&ui);
            } else if mode == workspace::PanelMode::Queue {
                pump(|| textures(&this.widgets.queue_rows) > 0);
            } else {
                pump(|| textures(&this.widgets.library_rows) > 0);
            }
            evidence(&ui, &format!("matrix-{name}-panel-{label}"));
            if name == "medium"
                && mode == workspace::PanelMode::Closed
                && std::env::var_os("KARIN_V04_ASSERT_DENSITY").is_some()
            {
                let (_, visible) = row_metrics(&ui);
                assert!(
                    visible >= 6,
                    "900x650 library must show at least six whole track rows; observed {visible}"
                );
            }
        }
    }
    set_pane(&ui, false);
    ui.window.maximize();
    pump(|| ui.window.is_maximized());
    settle_geometry(&ui, "maximized");
    for (mode, label) in [
        (workspace::PanelMode::Closed, "closed"),
        (workspace::PanelMode::TrackInfo, "track"),
        (workspace::PanelMode::Queue, "queue"),
    ] {
        set_panel(&ui, mode);
        if mode == workspace::PanelMode::TrackInfo {
            assert_short_info_actions_visible(&ui);
        }
        evidence(&ui, &format!("matrix-maximized-panel-{label}"));
    }
    set_pane(&ui, false);
    ui.window.unmaximize();
    pump(|| !ui.window.is_maximized());
    resize(&ui, 1440, 900);

    primary_destination(&ui, "home");
    evidence(&ui, "surface-wave-placeholder");
    primary_destination(&ui, "library");
    evidence(&ui, "surface-liked-tab");
    primary_destination(&ui, "playlists");
    pump(|| {
        !this.model.borrow().busy
            && textures(&this.playlist_grid.widget) >= 3
            && textures(&this.liked_playlist_grid.widget) >= 1
    });
    evidence(&ui, "surface-playlists-tab");
    let playlist = this
        .model
        .borrow()
        .playlists
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .items[0]
        .clone();
    this.open_playlist(playlist, 0);
    pump(|| !this.model.borrow().busy && textures(&this.widgets.detail) >= 6);
    evidence(&ui, "surface-playlist");
    this.render_album(Rc::new(album), 0);
    pump(|| textures(&this.widgets.detail) >= 6);
    evidence(&ui, "surface-album");
    this.widgets.stack.set_visible_child_name("search");
    this.widgets.search.set_text("Синтетическая музыка");
    this.widgets.search.emit_by_name::<()>("activate", &[]);
    pump(|| !this.model.borrow().busy && textures(&this.widgets.search_sections) >= 5);
    evidence(&ui, "surface-unified-search");
    set_panel(&ui, workspace::PanelMode::Queue);
    pump(|| textures(&this.widgets.queue_rows) >= 6);
    evidence(&ui, "surface-queue");
    set_panel(&ui, workspace::PanelMode::TrackInfo);
    evidence(&ui, "surface-track-info");
    set_panel(&ui, workspace::PanelMode::Closed);

    // Real native window action/dialog rather than a fixture settings facsimile.
    gtk::prelude::WidgetExt::activate_action(&ui.window, "win.preferences", None).unwrap();
    pump(|| ui.window.visible_dialog().is_some());
    settle();
    evidence(&ui, "surface-preferences");
    ui.window.visible_dialog().unwrap().force_close();
    pump(|| ui.window.visible_dialog().is_none());

    this.widgets.stack.set_visible_child_name("search");
    fixture.search_error.store(true, Ordering::Release);
    this.widgets.search.emit_by_name::<()>("activate", &[]);
    pump(|| !this.model.borrow().busy && this.widgets.search_feedback.is_visible());
    settle();
    evidence(&ui, "surface-search-error-retry");
    fixture.search_error.store(false, Ordering::Release);
    this.widgets.search.emit_by_name::<()>("activate", &[]);
    pump(|| !this.model.borrow().busy && textures(&this.widgets.search_sections) >= 5);
    gtk::prelude::WidgetExt::activate_action(&ui.window, "win.music-search", None).unwrap();
    pump(|| gtk::prelude::GtkWindowExt::focus(&ui.window).is_some());
    let found = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic HTTP fixture$",
        ])
        .output()
        .unwrap();
    assert!(found.status.success());
    let ids = String::from_utf8(found.stdout).unwrap();
    let id = ids.lines().next().unwrap();
    assert!(id.bytes().all(|byte| byte.is_ascii_digit()));
    assert!(
        Command::new("xdotool")
            .args(["windowfocus", id, "key", "--clearmodifiers", "Tab"])
            .status()
            .unwrap()
            .success()
    );
    settle();
    evidence(&ui, "surface-keyboard-focus");
    this.widgets.search.set_text("Нет совпадений");
    this.model.borrow_mut().search_scope = Some(SearchKind::Tracks);
    // Empty state uses the same confirmed typed render boundary, with no
    // fabricated service feature or production demonstration catalog.
    clear_list(&this.widgets.search_rows);
    this.reset_search_sections();
    set_empty(
        &this.widgets.search_rows,
        "Ничего не найдено. Попробуйте другой запрос.",
    );
    this.widgets.search_feedback.set_visible(false);
    settle();
    evidence(&ui, "surface-search-empty");

    // The hold is a real pending loopback request, rather than a painted mock
    // spinner or a production demo state. Release before fixture cleanup.
    fixture.search_hold.store(true, Ordering::Release);
    this.model.borrow_mut().search_scope = None;
    this.widgets.search.set_text("Синтетическая задержка");
    this.widgets.search.emit_by_name::<()>("activate", &[]);
    pump(|| this.model.borrow().busy);
    evidence(&ui, "surface-search-loading");
    fixture.search_hold.store(false, Ordering::Release);
    pump(|| !this.model.borrow().busy);

    primary_destination(&ui, "library");
    this.set_music_shuffle(true);
    this.set_music_repeat(crate::queue::RepeatMode::Queue);
    set_panel(&ui, workspace::PanelMode::Queue);
    let repeat_geometry = this.widgets.repeat.compute_bounds(&ui.window).unwrap();
    let repeat_surface = this.widgets.repeat.child().unwrap();
    evidence(&ui, "surface-player-shuffle-repeat-queue");
    this.set_music_repeat(crate::queue::RepeatMode::One);
    settle();
    assert_eq!(
        repeat_surface,
        this.widgets.repeat.child().unwrap(),
        "mode changes reuse one native overlay"
    );
    assert_eq!(
        repeat_geometry,
        this.widgets.repeat.compute_bounds(&ui.window).unwrap(),
        "literal1 must not move transport"
    );
    evidence(&ui, "surface-player-shuffle-repeat-one");
    this.set_music_shuffle(false);
    this.set_music_repeat(crate::queue::RepeatMode::Off);
    set_panel(&ui, workspace::PanelMode::Closed);

    primary_destination(&ui, "library");
    pump(|| !this.model.borrow().busy);
    let mut unusual = liked.items.clone();
    unusual[0].title = "Длинное синтетическое название композиции — несколько частей ночного путешествия, продолжение и финал".into();
    unusual[0].artists =
        vec!["Очень длинное синтетическое имя исполнителя и приглашённые музыканты".into()];
    unusual[0].cover = None;
    unusual[1].available = false;
    let partial = Page {
        items: unusual.clone(),
        has_more: true,
    };
    let context = PlaybackContext::new(ContextKind::LikedTracks, None, unusual, 0, true).unwrap();
    this.model.borrow_mut().queue.replace_context(context);
    this.follow_current_occurrence();
    this.render_queue();
    this.render_liked(Ok(partial), 0);
    resize(&ui, 900, 650);
    evidence(&ui, "surface-long-metadata-missing-art-unavailable-partial");
    set_panel(&ui, workspace::PanelMode::TrackInfo);
    evidence(&ui, "surface-long-metadata-information");
    resize(&ui, 420, 720);
    evidence(&ui, "surface-long-metadata-compact-information");
    set_panel(&ui, workspace::PanelMode::Closed);
    this.render_liked(
        Ok(Page {
            items: Vec::new(),
            has_more: false,
        }),
        0,
    );
    evidence(&ui, "surface-empty-library");

    resize(&ui, 1440, 900);
    primary_destination(&ui, "playlists");
    {
        let mut model = this.model.borrow_mut();
        model.liked = Some(Ok(liked.clone()));
        let mut page = model.playlists.as_ref().unwrap().as_ref().unwrap().clone();
        page.items.truncate(1);
        model.playlists = Some(Ok(page));
    }
    let owned = this.model.borrow().playlists.as_ref().unwrap().clone();
    this.render_playlists(owned, 0);
    settle();
    evidence(&ui, "surface-playlists-few-owned");
    {
        let mut model = this.model.borrow_mut();
        let first = model.playlists.as_ref().unwrap().as_ref().unwrap().items[0].clone();
        let items = (0..24)
            .map(|index| {
                let mut playlist = first.clone();
                playlist.id.kind = (index + 3).to_string();
                playlist.title = format!("Синтетическая коллекция {}", index + 1);
                playlist
            })
            .collect();
        model.playlists = Some(Ok(Page {
            items,
            has_more: true,
        }));
    }
    let owned = this.model.borrow().playlists.as_ref().unwrap().clone();
    this.render_playlists(owned, 0);
    settle();
    evidence(&ui, "surface-playlists-large-collection");
    {
        let mut model = this.model.borrow_mut();
        model.liked = Some(Ok(Page {
            items: Vec::new(),
            has_more: false,
        }));
        model.playlists = Some(Ok(Page {
            items: Vec::new(),
            has_more: false,
        }));
    }
    let owned = this.model.borrow().playlists.as_ref().unwrap().clone();
    this.render_playlists(owned, 0);
    settle();
    evidence(&ui, "surface-playlists-empty-owned");
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(true);
    observe_footer_transition(&ui, "explicit-stop", false, || this.stop_music());
    assert!(!this.widgets.player_revealer.reveals_child());
    evidence(&ui, "surface-player-hidden-after-stop");
    ui.music.close();
    ui.window.close();
    settle();
}

// Raw first-use evidence is intentionally separate from the initialized panel
// quality gate. Never turn a missing cold intermediate into a synthetic frame,
// alter the normal reveal duration, or describe this as a cold-latency PASS.
fn observe_footer_transition(
    ui: &crate::ui::MainWindow,
    name: &str,
    opening: bool,
    activate: impl FnOnce(),
) {
    struct Frame {
        time: i64,
        height: i32,
        requested: bool,
        revealed: bool,
        child: Option<gtk::graphene::Rect>,
        node: Option<gtk::gsk::RenderNode>,
    }
    let reveal = &ui.music.controller.widgets.player_revealer;
    assert_eq!(
        reveal.transition_type(),
        gtk::RevealerTransitionType::SlideUp
    );
    assert_eq!(reveal.transition_duration(), 240);
    assert!(gtk::Settings::default().unwrap().is_gtk_enable_animations());
    let frames = Rc::new(RefCell::new(Vec::new()));
    let captured = frames.clone();
    let window = ui.window.clone();
    let observed = reveal.clone();
    let clock = window.frame_clock().unwrap();
    let handler = clock.connect_after_paint(move |clock| {
        let paintable = gtk::WidgetPaintable::new(Some(&window));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(
            &snapshot,
            f64::from(window.width()),
            f64::from(window.height()),
        );
        captured.borrow_mut().push(Frame {
            time: clock.frame_time(),
            height: observed.height(),
            requested: observed.reveals_child(),
            revealed: observed.is_child_revealed(),
            child: observed
                .child()
                .and_then(|child| child.compute_bounds(&window)),
            node: snapshot.to_node(),
        });
    });
    ui.window.queue_draw();
    pump(|| !frames.borrow().is_empty());
    activate();
    pump(|| {
        reveal.reveals_child() == opening
            && if opening {
                reveal.is_child_revealed()
            } else {
                reveal.height() == 0 && !ui.music.controller.widgets.play.is_mapped()
            }
    });
    ui.window.queue_draw();
    let endpoint_count = frames.borrow().len();
    pump(|| frames.borrow().len() > endpoint_count);
    clock.disconnect(handler);
    let frames = frames.borrow();
    let maximum_height = frames.iter().map(|frame| frame.height).max().unwrap();
    let intermediate: Vec<_> = frames
        .iter()
        .enumerate()
        .filter(|(_, frame)| {
            frame.height > 0
                && frame.height < maximum_height
                && frame.requested == opening
                && (!opening || !frame.revealed)
        })
        .map(|(index, _)| index)
        .collect();
    let directory = motion_diagnostic_directory();
    std::fs::create_dir_all(&directory).unwrap();
    let scale = ui.window.scale_factor();
    let records = frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            json!({
                "native_frame_index":index,"frame_time":frame.time,"height":frame.height,
                "requested":frame.requested,"child_revealed":frame.revealed,
                "child":frame.child.map(|rect|[rect.x(),rect.y(),rect.width(),rect.height()]),
                "has_render_node":frame.node.is_some()
            })
        })
        .collect::<Vec<_>>();
    let mut selected = std::collections::BTreeSet::from([0, frames.len() - 1]);
    for percent in [25, 50, 75] {
        if let Some(index) = intermediate.iter().copied().min_by_key(|index| {
            (i64::from(frames[*index].height) * 100 - i64::from(maximum_height) * percent).abs()
        }) {
            selected.insert(index);
        }
    }
    let reversals = frames
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| {
            let reversed = if opening {
                pair[1].height < pair[0].height
            } else {
                pair[1].height > pair[0].height
            };
            (reversed && pair.iter().all(|frame| frame.requested == opening))
                .then_some([index, index + 1])
        })
        .collect::<Vec<_>>();
    let saved_reversals = reversals.iter().take(4).copied().collect::<Vec<_>>();
    for pair in &saved_reversals {
        selected.extend(pair.iter().copied());
    }
    selected.retain(|index| frames[*index].node.is_some());
    let natural_child_heights = frames
        .iter()
        .filter_map(|frame| frame.child.map(|rect| rect.height() as i32))
        .collect::<std::collections::BTreeSet<_>>();
    let summary = json!({"transition":name,"scope":"raw-first-start-or-stop-observation",
        "cold_first_use_latency_accepted":false,"synthetic_frames":false,
        "animations_enabled":gtk::Settings::default().unwrap().is_gtk_enable_animations(),
        "transition_type":"SlideUp","transition_duration_ms":240,
        "scale":scale,"renderer":ui.window.renderer().unwrap().type_().name(),
        "after_paint_count":frames.len(),"intermediate_frame_count":intermediate.len(),
        "node_none_count":frames.iter().filter(|frame|frame.node.is_none()).count(),
        "intermediate_evidence_available":!intermediate.is_empty(),"frames":records,
        "natural_child_heights":natural_child_heights,"saved_frame_indices":selected,
        "observed_height_reversal_count":reversals.len(),"reversal_capture_limit":4,
        "selected_reversal_frame_pairs":saved_reversals,
        "selection_coordinate":"nearest-observed-allocation-height-not-animation-time-or-progress"});
    std::fs::write(
        directory.join(format!("footer-{name}-scale{scale}.json")),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    eprintln!("Karin native footer observation: {summary}");
    // Save only actual native observations, never representative static poses.
    for index in selected {
        let Some(node) = &frames[index].node else {
            continue;
        };
        let snapshot = gtk::Snapshot::new();
        snapshot.scale(scale as f32, scale as f32);
        snapshot.append_node(node);
        let texture = ui.window.renderer().unwrap().render_texture(
            snapshot.to_node().unwrap(),
            Some(&gtk::graphene::Rect::new(
                0.0,
                0.0,
                (ui.window.width() * scale) as f32,
                (ui.window.height() * scale) as f32,
            )),
        );
        texture
            .save_to_png(directory.join(format!("footer-{name}-frame{index:03}-scale{scale}.png")))
            .unwrap();
    }
}

#[derive(Clone)]
struct MotionFrame {
    progress: f64,
    time: i64,
    node: gtk::gsk::RenderNode,
    host: gtk::graphene::Rect,
    surface: Option<gtk::graphene::Rect>,
    children: Vec<Option<gtk::graphene::Rect>>,
    mapped: Vec<bool>,
}

struct MotionTick {
    time: i64,
    progress: f64,
    callback_enter_us: u128,
    elapsed_us: u128,
    has_render_node: bool,
}

fn named_widget(root: &impl IsA<gtk::Widget>, name: &str) -> gtk::Widget {
    descendants(root.as_ref())
        .into_iter()
        .find(|widget| widget.widget_name() == name)
        .unwrap_or_else(|| panic!("missing real native widget: {name}"))
}

fn native_click(ui: &crate::ui::MainWindow, widget: &impl IsA<gtk::Widget>) {
    let started = Instant::now();
    let clock = ui.window.frame_clock().unwrap();
    let initial_frame_time = clock.frame_time();
    let (x, y) = super::test_input::pointer(&ui.window, widget, super::test_input::Target::Center);
    let target_ready_us = started.elapsed().as_micros();
    let result = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic HTTP fixture$",
        ])
        .output()
        .unwrap();
    assert!(result.status.success());
    let windows = String::from_utf8(result.stdout).unwrap();
    let id = windows.lines().next().unwrap();
    assert!(id.bytes().all(|byte| byte.is_ascii_digit()));
    let search_completed_us = started.elapsed().as_micros();
    // Keep GTK's frame clock advancing while X11 delivers the real press and
    // release. Waiting synchronously for xdotool can make an animation start
    // from an old frame timestamp and skip its first quarter in the fixture.
    let mut child = Command::new("xdotool")
        .args([
            "mousemove",
            "--window",
            id,
            &x.to_string(),
            &y.to_string(),
            "click",
            "1",
        ])
        .spawn()
        .unwrap();
    let mut result = None;
    pump(|| {
        result = child.try_wait().unwrap();
        result.is_some()
    });
    assert!(result.unwrap().success());
    eprintln!(
        "Karin synthetic motion input: {}",
        json!({"widget":widget.as_ref().widget_name().as_str(),"target_ready_us":target_ready_us,"search_completed_us":search_completed_us,"command_completed_us":started.elapsed().as_micros(),"initial_frame_time":initial_frame_time,"returned_frame_time":clock.frame_time()})
    );
}

fn snapshot_motion_frame(
    window: &adw::ApplicationWindow,
    panel: &super::sliding_panel::SlidingPanel,
    children: &[gtk::Widget],
    time: i64,
) -> Option<MotionFrame> {
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(
        &snapshot,
        f64::from(window.width()),
        f64::from(window.height()),
    );
    Some(MotionFrame {
        progress: panel.progress(),
        time,
        node: snapshot.to_node()?,
        host: panel
            .compute_bounds(window)
            .unwrap_or_else(|| gtk::graphene::Rect::new(0.0, 0.0, 0.0, 0.0)),
        surface: panel
            .surface()
            .and_then(|surface| surface.compute_bounds(window)),
        children: children
            .iter()
            .map(|widget| widget.compute_bounds(window))
            .collect(),
        mapped: children.iter().map(|widget| widget.is_mapped()).collect(),
    })
}

// Choose a globally close, strictly distinct native frame for each phase.
// Independent nearest-neighbour picks can incorrectly reuse a 62.5% frame
// for both 50% and 75%; that is not five-stage motion evidence.
fn motion_phase_indices(frames: &[MotionFrame]) -> [usize; 5] {
    let mut order: Vec<_> = (0..frames.len()).collect();
    order.sort_by(|a, b| frames[*a].progress.total_cmp(&frames[*b].progress));
    assert!(
        order.len() >= 5,
        "five distinct completed native frames are required"
    );
    let first = order[0];
    let last = *order.last().unwrap();
    assert!(frames[first].progress <= 0.001 && frames[last].progress >= 0.999);
    let mut best: Option<(f64, [usize; 5])> = None;
    for a in 1..order.len() - 1 {
        let da = (frames[order[a]].progress - 0.25).abs();
        if da > 0.17 {
            continue;
        }
        for b in a + 1..order.len() - 1 {
            let db = (frames[order[b]].progress - 0.5).abs();
            if db > 0.17 {
                continue;
            }
            for c in b + 1..order.len() - 1 {
                let dc = (frames[order[c]].progress - 0.75).abs();
                if dc > 0.17 {
                    continue;
                }
                let indices = [first, order[a], order[b], order[c], last];
                let times: std::collections::BTreeSet<_> =
                    indices.iter().map(|index| frames[*index].time).collect();
                if times.len() != 5 {
                    continue;
                }
                let score = da + db + dc;
                if best.as_ref().is_none_or(|(previous, _)| score < *previous) {
                    best = Some((score, indices));
                }
            }
        }
    }
    best.unwrap_or_else(|| {
        panic!(
            "five DISTINCT native quarter frames unavailable; sampled={:?}",
            frames
                .iter()
                .map(|frame| (frame.progress, frame.time))
                .collect::<Vec<_>>()
        )
    })
    .1
}

fn assert_whole_surface(frames: &[MotionFrame], opening: bool) -> [usize; 5] {
    let moving: Vec<_> = frames
        .iter()
        .filter(|frame| frame.progress > 0.01 && frame.progress < 0.99)
        .collect();
    assert!(
        moving.len() >= 3,
        "ordinary native motion must expose intermediate frames"
    );
    let complete = frames
        .iter()
        .max_by(|a, b| a.progress.total_cmp(&b.progress))
        .unwrap();
    let surface = complete.surface.expect("fully open surface allocation");
    assert!(surface.width() >= 280.0);
    let child_offsets: Vec<_> = complete
        .children
        .iter()
        .map(|rect| {
            rect.map(|rect| {
                (
                    rect.x() - surface.x(),
                    rect.y() - surface.y(),
                    rect.width(),
                    rect.height(),
                )
            })
        })
        .collect();
    for frame in &moving {
        let current = frame.surface.expect("moving full surface allocation");
        assert!(
            frame.mapped.iter().all(|mapped| *mapped),
            "actual visible heading/art/credits/album/actions must stay mapped throughout motion: {:?}",
            frame.mapped
        );
        assert!(
            (current.width() - surface.width()).abs() <= 1.0,
            "surface must translate at full width, never reflow during reveal"
        );
        assert!((current.height() - surface.height()).abs() <= 1.0);
        let expected_x = frame.host.x() + frame.host.width() * (1.0 - frame.progress as f32);
        assert!(
            (current.x() - expected_x).abs() <= 1.5,
            "whole surface translation differs from native animation progress"
        );
        for (rect, expected) in frame.children.iter().zip(&child_offsets) {
            let rect = rect.expect("heading/artwork/text/actions remain allocated during motion");
            let expected = expected.unwrap();
            let actual = (
                rect.x() - current.x(),
                rect.y() - current.y(),
                rect.width(),
                rect.height(),
            );
            assert!(
                (actual.0 - expected.0).abs() <= 1.5
                    && (actual.1 - expected.1).abs() <= 1.5
                    && (actual.2 - expected.2).abs() <= 1.5
                    && (actual.3 - expected.3).abs() <= 1.5,
                "every child must move with the same fixed-size surface: {actual:?} vs {expected:?}"
            );
        }
    }
    for pair in frames.windows(2) {
        assert!(pair[1].time >= pair[0].time);
        assert!(
            if opening {
                pair[1].progress + 0.001 >= pair[0].progress
            } else {
                pair[1].progress <= pair[0].progress + 0.001
            },
            "motion direction reversed unexpectedly"
        );
    }
    motion_phase_indices(frames)
}

fn save_motion_frames(
    ui: &crate::ui::MainWindow,
    frames: &[MotionFrame],
    phases: [usize; 5],
    name: &str,
) {
    let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") else {
        return;
    };
    let directory = std::path::Path::new(&directory);
    std::fs::create_dir_all(directory).unwrap();
    let scale = ui.window.scale_factor();
    let renderer = ui.window.renderer().unwrap();
    let mut manifest = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join(format!("panel-motion-scale{scale}.jsonl")))
        .unwrap();
    for (percent, index) in [0, 25, 50, 75, 100].into_iter().zip(phases) {
        let target = f64::from(percent) / 100.0;
        let frame = &frames[index];
        // These are actual after-paint GTK render nodes. Render their original
        // vector scene at the native scale; no screenshot is resized or edited.
        let snapshot = gtk::Snapshot::new();
        snapshot.scale(scale as f32, scale as f32);
        snapshot.append_node(&frame.node);
        let node = snapshot.to_node().unwrap();
        let texture = renderer.render_texture(
            &node,
            Some(&gtk::graphene::Rect::new(
                0.0,
                0.0,
                (ui.window.width() * scale) as f32,
                (ui.window.height() * scale) as f32,
            )),
        );
        let file = format!("animation-{name}-{percent:03}-scale{scale}.png");
        texture.save_to_png(directory.join(&file)).unwrap();
        let rectangles = frame
            .children
            .iter()
            .map(|rect| rect.map(|rect| [rect.x(), rect.y(), rect.width(), rect.height()]))
            .collect::<Vec<_>>();
        writeln!(manifest,"{}",json!({"capture":file,"requested_progress":target,"actual_progress":frame.progress,
            "frame_time":frame.time,"native_frame_index":index,"phase_convention":"openness","scale":scale,"renderer":ui.window.renderer().unwrap().type_().name(),"animations_enabled":true,
            "measurement_scope":"graphics-initialized-normal-animation","cold_first_use_latency_accepted":false,
            "graphics_initialization":"one-normal-open-close-and-six-native-offscreen-priming-renders",
            "host":[frame.host.x(),frame.host.y(),frame.host.width(),frame.host.height()],
            "surface":frame.surface.map(|rect| [rect.x(),rect.y(),rect.width(),rect.height()]),"children":rectangles,"children_mapped":frame.mapped,
            "source":"native-after-paint-gtk-render-node-synthetic-fixture"})).unwrap();
    }
}

fn motion_diagnostic_directory() -> std::path::PathBuf {
    std::env::var_os("KARIN_TEST_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from("build/ui-captures/fixtures/native-music-motion-diagnostics")
        })
}

fn dump_motion_observations(
    ui: &crate::ui::MainWindow,
    panel: &super::sliding_panel::SlidingPanel,
    frames: &[MotionFrame],
    ticks: &[MotionTick],
    name: &str,
    activation: Value,
) {
    let scale = ui.window.scale_factor();
    let renderer = ui.window.renderer().unwrap().type_().name().to_string();
    let after_paint = ticks
        .iter()
        .enumerate()
        .map(|(index, tick)| {
            json!({"after_paint_index":index,"frame_time":tick.time,"progress":tick.progress,"elapsed_us":tick.elapsed_us,
                "callback_enter_us":tick.callback_enter_us,"snapshot_completed_us":tick.elapsed_us,
                "snapshot_duration_us":tick.elapsed_us-tick.callback_enter_us,
                "since_previous_callback_enter_us":index.checked_sub(1).map(|previous|tick.callback_enter_us-ticks[previous].callback_enter_us),
                "native_frame_gap_us":index.checked_sub(1).map(|previous|tick.time-ticks[previous].time),"has_render_node":tick.has_render_node})
        })
        .collect::<Vec<_>>();
    let graphics_initialization = name.starts_with("graphics-initialization-");
    let summary = json!({"trajectory":name,"renderer":renderer,"scale":scale,
        "scope":if graphics_initialization {"graphics-initialization"} else {"measured-normal-motion"},
        "quarter_phase_acceptance":if graphics_initialization {"not-assessed"} else {"strict-assertion-follows"},
        "window":[ui.window.width(),ui.window.height()],"native_surface":ui.window.surface().map(|surface|[surface.width(),surface.height()]),
        "animations_enabled":gtk::Settings::default().unwrap().is_gtk_enable_animations(),"transition_duration_ms":panel.transition_duration(),
        "after_paint_count":ticks.len(),"node_none_count":ticks.iter().filter(|tick|!tick.has_render_node).count(),
        "retained_frame_count":frames.len(),"retained_intermediate_count":frames.iter().filter(|frame|frame.progress>0.01&&frame.progress<0.99).count(),
        "activation":activation,"after_paint":after_paint});
    // Emit before every assertion: even the first failed moving-frame guard
    // must retain the actual callback observations, not just a generic panic.
    eprintln!("Karin native motion observation: {summary}");
    let directory = motion_diagnostic_directory();
    std::fs::create_dir_all(&directory).unwrap();
    let mut file =
        std::fs::File::create(directory.join(format!("observed-{name}-scale{scale}.jsonl")))
            .unwrap();
    writeln!(file, "{}", json!({"event":"summary","observation":summary})).unwrap();
    for (index, frame) in frames.iter().enumerate() {
        let rectangle =
            |rect: gtk::graphene::Rect| [rect.x(), rect.y(), rect.width(), rect.height()];
        writeln!(file,"{}",json!({"event":"renderable-frame","trajectory":name,"renderer":renderer,"scale":scale,
            "native_frame_index":index,"frame_time":frame.time,"progress":frame.progress,"host":rectangle(frame.host),
            "surface":frame.surface.map(rectangle),"children":frame.children.iter().map(|rect|rect.map(rectangle)).collect::<Vec<_>>(),"children_mapped":frame.mapped})).unwrap();
    }
}

fn observe_motion(
    ui: &crate::ui::MainWindow,
    panel: &super::sliding_panel::SlidingPanel,
    children: Vec<gtk::Widget>,
    opening: bool,
    name: &str,
    activate: impl FnOnce(),
) -> Vec<MotionFrame> {
    let clock = ui.window.frame_clock().expect("mapped native frame clock");
    let started = Instant::now();
    let frames = Rc::new(RefCell::new(Vec::new()));
    let captured = frames.clone();
    let ticks = Rc::new(RefCell::new(Vec::new()));
    let observed_ticks = ticks.clone();
    let window = ui.window.downgrade();
    let panel_weak = panel.downgrade();
    let signal = clock.connect_after_paint(move |clock| {
        let (Some(window), Some(panel)) = (window.upgrade(), panel_weak.upgrade()) else {
            return;
        };
        // Observe every callback, including an absent paintable render node.
        // This separates an observation failure on another GTK version from
        // missing animation progress; no callback is silently discarded.
        let callback_enter_us = started.elapsed().as_micros();
        let frame = snapshot_motion_frame(&window, &panel, &children, clock.frame_time());
        observed_ticks.borrow_mut().push(MotionTick {
            time: clock.frame_time(),
            progress: panel.progress(),
            callback_enter_us,
            elapsed_us: started.elapsed().as_micros(),
            has_render_node: frame.is_some(),
        });
        if let Some(frame) = frame {
            let mut frames = captured.borrow_mut();
            if frames
                .last()
                .is_none_or(|last: &MotionFrame| (last.progress - frame.progress).abs() > 0.00001)
            {
                frames.push(frame);
            }
        }
    });
    ui.window.queue_draw();
    pump(|| !frames.borrow().is_empty());
    let activation_started_us = started.elapsed().as_micros();
    let activation_frame_time = clock.frame_time();
    let activation_progress = panel.progress();
    activate();
    let activation_returned_us = started.elapsed().as_micros();
    let activation_returned_frame_time = clock.frame_time();
    let activation_returned_progress = panel.progress();
    let end = if opening { 1.0 } else { 0.0 };
    pump(|| {
        frames
            .borrow()
            .last()
            .is_some_and(|frame| (frame.progress - end).abs() <= f64::EPSILON)
    });
    clock.disconnect(signal);
    let frames = frames.borrow();
    dump_motion_observations(
        ui,
        panel,
        &frames,
        &ticks.borrow(),
        name,
        json!({"started_us":activation_started_us,"returned_us":activation_returned_us,
            "started_frame_time":activation_frame_time,"returned_frame_time":activation_returned_frame_time,
            "started_progress":activation_progress,"returned_progress":activation_returned_progress,
            "requested_reveal":panel.reveals_child(),"child_revealed":panel.is_child_revealed(),"mapped":panel.is_mapped()}),
    );
    frames.to_vec()
}

fn initialize_motion_graphics(
    ui: &crate::ui::MainWindow,
    panel: &super::sliding_panel::SlidingPanel,
    children: Vec<gtk::Widget>,
    name: &str,
    open: impl FnOnce(),
    close: impl FnOnce(),
) {
    // One fixed setup cycle, never a conditional retry. Software GPU pipeline
    // initialization can stall a first paint longer than the animation itself.
    // Retain that raw cold observation separately; it is NOT quarter-frame or
    // first-use responsiveness acceptance. Production performs no prewarming.
    assert_eq!(panel.transition_duration(), 240);
    assert!(gtk::Settings::default().unwrap().is_gtk_enable_animations());
    {
        let frames = observe_motion(
            ui,
            panel,
            children.clone(),
            true,
            &format!("graphics-initialization-{name}-opening"),
            open,
        );
        assert_eq!(frames.first().unwrap().progress, 0.0);
        assert_eq!(frames.last().unwrap().progress, 1.0);
    }
    prime_native_panel_graphics(ui, panel, name);
    {
        let frames = observe_motion(
            ui,
            panel,
            children,
            false,
            &format!("graphics-initialization-{name}-closing"),
            close,
        );
        assert_eq!(frames.first().unwrap().progress, 1.0);
        assert_eq!(frames.last().unwrap().progress, 0.0);
    }
    assert!(gtk::Settings::default().unwrap().is_gtk_enable_animations());
}

fn prime_native_panel_graphics(
    ui: &crate::ui::MainWindow,
    panel: &super::sliding_panel::SlidingPanel,
    name: &str,
) {
    // A cold paint can jump over partial-clip materials entirely. Initialize
    // those GPU resources with a finite offscreen pass over the REAL mapped
    // surface. These static renderer poses are never animation evidence: no
    // UI progress/clock changes, screenshots or quarter-frame claims occur.
    assert_eq!(panel.progress(), 1.0);
    let surface = panel.surface().unwrap();
    assert!(surface.is_mapped());
    let width = surface.width();
    let height = surface.height();
    assert!(width > 0 && height > 0);
    let paintable = gtk::WidgetPaintable::new(Some(&surface));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
    let node = snapshot
        .to_node()
        .expect("graphics initialization requires the actual mapped native surface node");
    let renderer = ui.window.renderer().unwrap();
    let scale = ui.window.scale_factor();
    let mut passes = Vec::new();
    for fraction in [0.137_f32, 0.317, 0.533, 0.787, 0.941, 1.0] {
        let started = Instant::now();
        let snapshot = gtk::Snapshot::new();
        snapshot.scale(scale as f32, scale as f32);
        snapshot.push_clip(&gtk::graphene::Rect::new(
            0.0,
            0.0,
            width as f32,
            height as f32,
        ));
        snapshot.translate(&gtk::graphene::Point::new(
            width as f32 * (1.0 - fraction),
            0.0,
        ));
        snapshot.append_node(&node);
        snapshot.pop();
        let clipped = snapshot.to_node().unwrap();
        let texture = renderer.render_texture(
            &clipped,
            Some(&gtk::graphene::Rect::new(
                0.0,
                0.0,
                (width * scale) as f32,
                (height * scale) as f32,
            )),
        );
        // Native readback completes resource initialization; no image is
        // saved, retained as evidence, or substituted for a measured frame.
        let (pixels, _) = gdk::TextureDownloader::new(&texture).download_bytes();
        assert!(!pixels.is_empty());
        passes.push(json!({"static_fraction":fraction,"elapsed_us":started.elapsed().as_micros(),"physical_pixels":[texture.width(),texture.height()]}));
    }
    assert_eq!(panel.progress(), 1.0);
    let record = json!({"scope":"graphics-initialization","stage":"renderer-priming","animation_proof":false,
        "quarter_phase_acceptance":"not-assessed","trajectory":name,"renderer":renderer.type_().name().to_string(),"scale":scale,"passes":passes});
    eprintln!("Karin native graphics initialization: {record}");
    let directory = motion_diagnostic_directory();
    std::fs::create_dir_all(&directory).unwrap();
    let mut file = std::fs::File::create(directory.join(format!(
        "graphics-initialization-renderer-priming-{name}-scale{scale}.jsonl"
    )))
    .unwrap();
    writeln!(file, "{record}").unwrap();
}

fn record_motion(
    ui: &crate::ui::MainWindow,
    panel: &super::sliding_panel::SlidingPanel,
    children: Vec<gtk::Widget>,
    opening: bool,
    name: &str,
    activate: impl FnOnce(),
) {
    let frames = observe_motion(ui, panel, children, opening, name, activate);
    let phases = assert_whole_surface(&frames, opening);
    save_motion_frames(ui, &frames, phases, name);
}

#[test]
#[ignore = "real ordinary GTK animation after recorded graphics initialization; run at both scales"]
fn native_universal_panel_whole_surface_motion_and_mode_switch() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let _manager = WindowManager::start();
    let (_fixture, _app, ui, _liked, _album) = populated_window(true);
    let this = &ui.music.controller;
    let settings = gtk::Settings::default().unwrap();
    settings.set_gtk_enable_animations(true);
    assert!(settings.is_gtk_enable_animations());
    let panel = this
        .info_host
        .borrow()
        .as_ref()
        .unwrap()
        .revealer()
        .unwrap();
    assert_eq!(panel.transition_duration(), 240);
    for (theme, scheme) in [
        ("light", adw::ColorScheme::ForceLight),
        ("dark", adw::ColorScheme::ForceDark),
    ] {
        adw::StyleManager::default().set_color_scheme(scheme);
        for (width, height, shape) in [
            (1440, 900, "wide"),
            (900, 650, "medium"),
            (420, 720, "compact"),
        ] {
            set_panel(&ui, workspace::PanelMode::Closed);
            resize(&ui, width, height);
            for (mode, label) in [
                (workspace::PanelMode::TrackInfo, "track"),
                (workspace::PanelMode::Queue, "queue"),
            ] {
                let surface = panel.surface().unwrap();
                let heading = named_widget(&surface, "music-panel-heading");
                let mut children = vec![
                    heading.parent().unwrap(),
                    this.track_pane.close.clone().upcast(),
                ];
                if heading.is_visible() {
                    children.push(heading);
                }
                if mode == workspace::PanelMode::TrackInfo {
                    children.extend(
                        [
                            "music-track-pane-cover",
                            "music-track-pane-title",
                            "music-track-pane-actions",
                        ]
                        .map(|name| named_widget(&surface, name)),
                    );
                    if this.track_pane.artists.is_visible() {
                        children.push(this.track_pane.artists.clone().upcast());
                    } else {
                        let credits = named_widget(&surface, "music-track-pane-artist");
                        if credits.is_visible() {
                            children.push(credits);
                        }
                    }
                    if this.track_pane.album.is_visible() {
                        children.push(this.track_pane.album.clone().upcast());
                    }
                } else {
                    children.extend([
                        this.widgets.queue_count.clone().upcast(),
                        this.widgets.clear_queue.clone().upcast(),
                        this.widgets.queue_rows.clone().upcast(),
                    ]);
                }
                initialize_motion_graphics(
                    &ui,
                    &panel,
                    children.clone(),
                    &format!("{shape}-{theme}-{label}"),
                    || {
                        if mode == workspace::PanelMode::TrackInfo {
                            native_click(&ui, &this.widgets.current_art_button);
                        } else {
                            native_click(
                                &ui,
                                &named_widget(&ui.music.player_bar, "music-queue-toggle"),
                            );
                        }
                    },
                    || native_click(&ui, &this.track_pane.close),
                );
                assert_eq!(this.panel_mode.get(), workspace::PanelMode::Closed);
                record_motion(
                    &ui,
                    &panel,
                    children.clone(),
                    true,
                    &format!("{shape}-{theme}-{label}-opening"),
                    || {
                        if mode == workspace::PanelMode::TrackInfo {
                            native_click(&ui, &this.widgets.current_art_button);
                        } else {
                            native_click(
                                &ui,
                                &named_widget(&ui.music.player_bar, "music-queue-toggle"),
                            );
                        }
                    },
                );
                assert_eq!(this.panel_mode.get(), mode);
                record_motion(
                    &ui,
                    &panel,
                    children,
                    false,
                    &format!("{shape}-{theme}-{label}-closing"),
                    || {
                        native_click(&ui, &this.track_pane.close);
                    },
                );
                assert_eq!(this.panel_mode.get(), workspace::PanelMode::Closed);
            }
            // Switching modes keeps one already-open surface at its endpoint.
            set_panel(&ui, workspace::PanelMode::TrackInfo);
            let surface = panel.surface().unwrap();
            let before = surface.compute_bounds(&ui.window).unwrap();
            let close = this.track_pane.close.compute_bounds(&ui.window).unwrap();
            native_click(
                &ui,
                &named_widget(&ui.music.player_bar, "music-queue-toggle"),
            );
            pump(|| this.panel_mode.get() == workspace::PanelMode::Queue);
            settle();
            assert!(panel.is_child_revealed());
            assert_eq!(surface, panel.surface().unwrap());
            assert_eq!(before, surface.compute_bounds(&ui.window).unwrap());
            assert_eq!(
                close,
                this.track_pane.close.compute_bounds(&ui.window).unwrap()
            );
            native_click(&ui, &this.widgets.current_art_button);
            pump(|| this.panel_mode.get() == workspace::PanelMode::TrackInfo);
            assert!(panel.is_child_revealed());
            assert_eq!(surface, panel.surface().unwrap());
            set_panel(&ui, workspace::PanelMode::Closed);
        }
    }
    settings.set_gtk_enable_animations(false);
    set_panel(&ui, workspace::PanelMode::Queue);
    pump(|| panel.is_child_revealed());
    set_panel(&ui, workspace::PanelMode::Closed);
    pump(|| panel.progress() == 0.0);
    ui.music.close();
    ui.window.close();
    settle();
}
