// SPDX-License-Identifier: GPL-3.0-or-later
//! Matched native v0.3/v0.4 evidence. All account/catalog/artwork data below is
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
        let server = {
            let stop = stop.clone();
            let images = images.clone();
            let search_error = search_error.clone();
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

// Both revisions are driven by the same window action. Runtime GTK ancestry
// observes the presentation without coupling the fixture to either controller's
// private old OverlaySplitView/new nonmodal Revealer implementation.
fn pane_open(this: &Controller) -> bool {
    let mut current = this.track_pane.widget.parent();
    while let Some(widget) = current {
        if let Some(reveal) = widget.downcast_ref::<gtk::Revealer>()
            && widget.widget_name() == "music-track-info-revealer"
        {
            return reveal.reveals_child();
        }
        if let Some(split) = widget.downcast_ref::<adw::OverlaySplitView>() {
            return split.shows_sidebar();
        }
        current = widget.parent();
    }
    panic!("native information host is not attached");
}
fn set_pane(ui: &crate::ui::MainWindow, open: bool) {
    if pane_open(&ui.music.controller) != open {
        gtk::prelude::WidgetExt::activate_action(&ui.window, "win.music-info", None).unwrap();
    }
    pump(|| pane_open(&ui.music.controller) == open);
    settle();
    if std::env::var_os("KARIN_V04_ASSERT_DENSITY").is_some() {
        settle_geometry(ui, if open { "pane-open" } else { "pane-closed" });
    }
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
fn evidence(ui: &crate::ui::MainWindow, name: &str) {
    let scale = ui.window.scale_factor();
    let name = format!("{name}-scale{scale}");
    super::tests::capture_native(&name);
    let (heights, visible) = row_metrics(ui);
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
            writeln!(file,"{}",json!({"capture":format!("{name}-{theme}.png"),"logical_width":ui.window.width(),"logical_height":ui.window.height(),"scale":scale,"maximized":ui.window.is_maximized(),"pane_open":pane_open(&ui.music.controller),"rows":heights,"fully_visible_rows":visible,"source":"synthetic-loopback-musicservice-http-decoded-artwork","tracks":8})).unwrap();
        }
    }
}

#[test]
#[ignore = "isolated GTK/X11 design evidence; run under Xvfb with GDK_SCALE=1 and2"]
fn native_design_matched_matrix_and_populated_surfaces() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let _manager = WindowManager::start();
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
    // Four geometries × open/closed × both native themes =16 captures here;
    // the separate scale2 process makes32 per revision. Baseline and current
    // receive identical canonical tracks, order, covers and logical geometry.
    for (width, height, name) in [
        (420, 720, "compact"),
        (900, 650, "medium"),
        (1440, 900, "wide"),
    ] {
        set_pane(&ui, false);
        resize(&ui, width, height);
        for open in [false, true] {
            set_pane(&ui, open);
            if open {
                pump(|| textures(&this.track_pane.widget) > 0);
            } else {
                pump(|| textures(&this.widgets.library_rows) > 0);
            }
            evidence(
                &ui,
                &format!(
                    "matrix-{name}-pane-{}",
                    if open { "open" } else { "closed" }
                ),
            );
            if name == "medium" && !open && std::env::var_os("KARIN_V04_ASSERT_DENSITY").is_some() {
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
    for open in [false, true] {
        set_pane(&ui, open);
        evidence(
            &ui,
            &format!(
                "matrix-maximized-pane-{}",
                if open { "open" } else { "closed" }
            ),
        );
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
    this.widgets.stack.set_visible_child_name("queue");
    pump(|| textures(&this.widgets.queue_rows) >= 6);
    evidence(&ui, "surface-queue");

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
    ui.music.close();
    ui.window.close();
    settle();
}
