// SPDX-License-Identifier: GPL-3.0-or-later
//! Matched native a9eed45/current redesign evidence. All account/catalog/artwork data below is
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

struct Fixture {
    origin: String,
    stop: Arc<AtomicBool>,
    images: Arc<AtomicUsize>,
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
        let server = {
            let stop = stop.clone();
            let images = images.clone();
            let search_error = search_error.clone();
            let search_hold = search_hold.clone();
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
                        "/account/status" => {
                            json!({"result":{"account":{"uid":1234,"displayName":"Synthetic design account"}}})
                        }
                        "/users/1234/likes/tracks" => {
                            json!({"result":{"library":{"uid":1234,"revision":1,"tracks":references}}})
                        }
                        "/users/1234/playlists/list" => {
                            json!({"result":(0..3).map(playlist).collect::<Vec<_>>()})
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
        if name == "music-current-info" {
            assert!(
                rect.width() >= 44.0 && rect.height() >= 44.0,
                "compact cover remains a usable pointer target"
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
fn evidence(ui: &crate::ui::MainWindow, name: &str) {
    assert_footer_geometry(ui);
    let scale = ui.window.scale_factor();
    let name = format!("{name}-scale{scale}");
    super::tests::capture_native(&name);
    if std::env::var_os("KARIN_TEST_COMPARE_RENDER_READBACK").is_some()
        && (name.starts_with("surface-home-scale")
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
            writeln!(file,"{}",json!({"capture":format!("{name}-{theme}.png"),"logical_width":ui.window.width(),"logical_height":ui.window.height(),"scale":scale,"maximized":ui.window.is_maximized(),"pane_open":pane_open(&ui.music.controller),"panel_mode":format!("{:?}",ui.music.controller.panel_mode.get()),"renderer":ui.window.renderer().unwrap().type_().name(),"rows":heights,"fully_visible_rows":visible,"source":"synthetic-loopback-musicservice-http-decoded-artwork","tracks":8,"footer_art_hit":[ui.music.controller.widgets.current_art_button.width(),ui.music.controller.widgets.current_art_button.height()],"footer_title_width":ui.music.controller.widgets.current.width()})).unwrap();
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

fn populated_window() -> (
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
    ui.window.set_title(Some("Karin synthetic HTTP fixture"));
    ui.window.set_default_size(1440, 900);
    ui.window.present();
    pump(|| textures(&this.widgets.library_rows) >= 6 && textures(&this.widgets.current_art) > 0);
    assert_decoded_artwork(&this.widgets.library_rows);
    assert_decoded_artwork(&this.widgets.current_art);
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
            "gl" => panic!("removed GL alias cannot establish a verified visual renderer; use ngl"),
            _ => {}
        }
    }
    (fixture, app, ui, liked, album)
}

#[test]
#[ignore = "isolated GTK/X11 design evidence; run under Xvfb with GDK_SCALE=1 and2"]
fn native_design_matched_matrix_and_populated_surfaces() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let _manager = WindowManager::start();
    let (fixture, _app, ui, liked, album) = populated_window();
    let this = &ui.music.controller;
    // Eight geometries × three genuine panel modes × two themes × two
    // separate scale processes =96 current images, matched to the archived
    // a9 baseline's honest old representations at the same outer dimensions.
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
        evidence(&ui, &format!("matrix-maximized-panel-{label}"));
    }
    set_pane(&ui, false);
    ui.window.unmaximize();
    pump(|| !ui.window.is_maximized());
    resize(&ui, 1440, 900);

    this.widgets.stack.set_visible_child_name("home");
    settle();
    evidence(&ui, "surface-home");
    this.widgets.stack.set_visible_child_name("library");
    this.widgets.library_mode.set_selected(1);
    pump(|| !this.model.borrow().busy && textures(&this.playlist_grid.widget) >= 3);
    evidence(&ui, "surface-library-playlists");
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

    this.widgets.stack.set_visible_child_name("library");
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

    this.widgets.library_mode.set_selected(0);
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
    this.widgets.stack.set_visible_child_name("home");
    {
        let mut model = this.model.borrow_mut();
        model.liked = Some(Ok(liked.clone()));
        let mut page = model.playlists.as_ref().unwrap().as_ref().unwrap().clone();
        page.items.truncate(1);
        model.playlists = Some(Ok(page));
    }
    this.render_home_preview();
    settle();
    evidence(&ui, "surface-home-few-playlists");
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
    this.render_home_preview();
    settle();
    evidence(&ui, "surface-home-large-collection");
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
    this.render_home_preview();
    settle();
    evidence(&ui, "surface-home-empty");
    ui.music.close();
    ui.window.close();
    settle();
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

fn named_widget(root: &impl IsA<gtk::Widget>, name: &str) -> gtk::Widget {
    descendants(root.as_ref())
        .into_iter()
        .find(|widget| widget.widget_name() == name)
        .unwrap_or_else(|| panic!("missing real native widget: {name}"))
}

fn native_click(ui: &crate::ui::MainWindow, widget: &impl IsA<gtk::Widget>) {
    let (x, y) = super::test_input::pointer(&ui.window, widget, super::test_input::Target::Center);
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
            "frame_time":frame.time,"native_frame_index":index,"phase_convention":"openness","scale":scale,"renderer":ui.window.renderer().unwrap().type_().name(),"animations_enabled":true,"host":[frame.host.x(),frame.host.y(),frame.host.width(),frame.host.height()],
            "surface":frame.surface.map(|rect| [rect.x(),rect.y(),rect.width(),rect.height()]),"children":rectangles,"children_mapped":frame.mapped,
            "source":"native-after-paint-gtk-render-node-synthetic-fixture"})).unwrap();
    }
}

fn record_motion(
    ui: &crate::ui::MainWindow,
    panel: &super::sliding_panel::SlidingPanel,
    children: Vec<gtk::Widget>,
    opening: bool,
    name: &str,
    activate: impl FnOnce(),
) {
    let clock = ui.window.frame_clock().expect("mapped native frame clock");
    let frames = Rc::new(RefCell::new(Vec::new()));
    let captured = frames.clone();
    let window = ui.window.downgrade();
    let panel_weak = panel.downgrade();
    let signal = clock.connect_after_paint(move |clock| {
        let (Some(window), Some(panel)) = (window.upgrade(), panel_weak.upgrade()) else {
            return;
        };
        // A retained WidgetPaintable publishes pending frames through an idle.
        // A fresh observer synchronously reads the just-painted root render
        // node, so geometry and pixels refer to THIS native frame, without
        // idle starvation or a previous-frame cache under load.
        if let Some(frame) = snapshot_motion_frame(&window, &panel, &children, clock.frame_time()) {
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
    activate();
    let end = if opening { 1.0 } else { 0.0 };
    pump(|| {
        frames
            .borrow()
            .last()
            .is_some_and(|frame| (frame.progress - end).abs() <= f64::EPSILON)
    });
    clock.disconnect(signal);
    let frames = frames.borrow();
    let phases = assert_whole_surface(&frames, opening);
    save_motion_frames(ui, &frames, phases, name);
}

#[test]
#[ignore = "real ordinary GTK animation frames under isolated Xvfb; run at both scales"]
fn native_universal_panel_whole_surface_motion_and_mode_switch() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let _manager = WindowManager::start();
    let (_fixture, _app, ui, _liked, _album) = populated_window();
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
