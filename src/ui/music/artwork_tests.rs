// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic Music metadata -> ordinary artwork request -> HTTP -> native pixels.
//! No RAM seeding, external service, account, credential store or service media.

use super::tests::{capture_native, contains_text};
use super::*;
use crate::cache::{AccountScope, Cache, CachePolicy};
use gtk::gdk;
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const TOKEN: &str = "synthetic-cover-lifecycle-token";
const TRACK_COVER: &str = "avatars.yandex.net/get-music-content/fixture/track-red/%%";
const ALBUM_COVER: &str = "https://avatars.yandex.net/get-music-content/fixture/album-blue/%%";
const PLAYLIST_COVER: &str =
    "//avatars.yandex.net/get-music-user-playlist/fixture/playlist-green/%%?1700000000000";

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
                .expect("the isolated maximized-cover fixture requires Openbox")
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
    let deadline = Instant::now() + Duration::from_secs(16);
    while !condition() {
        assert!(Instant::now() < deadline, "native cover fixture timed out");
        if context.pending() {
            context.iteration(false);
        } else {
            thread::sleep(Duration::from_millis(5));
        }
    }
}

fn settle() {
    let end = Instant::now() + Duration::from_millis(160);
    pump(|| Instant::now() >= end);
}

fn resize_cover_window(ui: &crate::ui::MainWindow, width: i32, height: i32) {
    ui.window.set_default_size(width, height);
    let previous = Cell::new(None);
    pump(|| {
        // GTK's logical CSD content allocation can differ from the requested
        // outer geometry by the compositor's shadow/frame, as in the native
        // workspace fixture. Keep the target breakpoint and actual layout gate.
        let current = (ui.window.width(), ui.window.height());
        if previous.replace(Some(current)) != Some(current) {
            eprintln!(
                "Synthetic cover window allocation: {}x{}",
                current.0, current.1
            );
        }
        (current.0 - width).abs() <= 32 && (current.1 - height).abs() <= 32
    });
    pump(|| ui.verify_layout().is_ok());
}

fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut result = vec![widget.clone()];
    let mut child = widget.first_child();
    while let Some(current) = child {
        result.extend(descendants(&current));
        child = current.next_sibling();
    }
    result
}

fn rendered_covers(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Overlay> {
    descendants(widget.as_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Overlay>().ok())
        .filter(|overlay| {
            if !overlay
                .child()
                .is_some_and(|child| child.is::<gtk::Image>())
            {
                return false;
            }
            descendants(overlay.upcast_ref()).into_iter().any(|widget| {
                widget
                    .downcast::<gtk::Picture>()
                    .ok()
                    .is_some_and(|picture| {
                        picture.is_visible()
                            && picture.is_mapped()
                            && picture
                                .paintable()
                                .is_some_and(|paintable| paintable.is::<gdk::Texture>())
                    })
            })
        })
        .collect()
}

fn assert_native_cover(overlay: &gtk::Overlay) {
    let picture = descendants(overlay.upcast_ref())
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk::Picture>().ok())
        .expect("native thumbnail picture");
    assert!(picture.is_visible() && picture.is_mapped());
    let texture = picture
        .paintable()
        .unwrap()
        .downcast::<gdk::Texture>()
        .unwrap();
    assert!(texture.width() >= 128 && texture.height() >= 128);
    let stride = texture.width() as usize * 4;
    let mut pixels = vec![0; stride * texture.height() as usize];
    texture.download(&mut pixels, stride);
    assert!(pixels[..3].iter().max().unwrap() - pixels[..3].iter().min().unwrap() > 40);
    let fallback = overlay.child().unwrap().downcast::<gtk::Image>().unwrap();
    assert!(
        !fallback.is_visible(),
        "decoded texture must replace the fallback"
    );
}

/// Downloading a Picture's own texture proves decode, but cannot prove that
/// another mapped widget has not painted over it. Snapshot the whole window,
/// including the information overlay, and inspect the known synthetic coral
/// image inside the actually visible library row.
#[track_caller]
fn assert_composited_library_cover(ui: &crate::ui::MainWindow, cover: &gtk::Overlay) {
    assert!(cover.is_mapped());
    let scroll = cover
        .ancestor(gtk::ScrolledWindow::static_type())
        .expect("the library cover belongs to the scrollable central workspace");
    let panel = ui
        .music
        .controller
        .info_host
        .borrow()
        .as_ref()
        .and_then(|host| host.revealer())
        .expect("the ordinary shell's native information surface");
    assert!(!panel.reveals_child(), "the fixture explicitly closed Info");
    let renderer = ui.window.renderer().expect("realized native renderer");

    struct ComposedFrame {
        time: i64,
        width: i32,
        height: i32,
        bounds: gtk::graphene::Rect,
        viewport: gtk::graphene::Rect,
        panel_closed: bool,
        node: Option<gtk::gsk::RenderNode>,
    }
    fn same_rect(left: &gtk::graphene::Rect, right: &gtk::graphene::Rect) -> bool {
        [left.x(), left.y(), left.width(), left.height()]
            .into_iter()
            .zip([right.x(), right.y(), right.width(), right.height()])
            .all(|(left, right)| (left - right).abs() < 0.01)
    }

    for scheme in [adw::ColorScheme::ForceLight, adw::ColorScheme::ForceDark] {
        adw::StyleManager::default().set_color_scheme(scheme);
        // A responsive hero changes its size request from a tick callback. A
        // mapped/decoded thumbnail or the new window width alone does not prove
        // that the resulting allocation has painted. Observe three completed
        // native frames with stable geometry, not a longer arbitrary sleep or
        // a loop that waits until a coral pixel happens to appear.
        let frames = Rc::new(RefCell::new((None::<ComposedFrame>, 0_u8)));
        let captured = frames.clone();
        let window = ui.window.clone();
        let observed_cover = cover.clone();
        let observed_scroll = scroll.clone();
        let observed_panel = panel.clone();
        let clock = ui.window.frame_clock().expect("mapped native frame clock");
        let handler = clock.connect_after_paint(move |clock| {
            let (width, height) = (window.width(), window.height());
            let bounds = observed_cover
                .compute_bounds(&window)
                .expect("allocated library cover after native paint");
            let viewport = observed_scroll
                .compute_bounds(&window)
                .expect("allocated library viewport after native paint");
            // As in the native motion observer, take a fresh root paintable in
            // after-paint. The retained node and its sample bounds then belong
            // to this same completed allocation, not pre-resize cached bounds.
            let paintable = gtk::WidgetPaintable::new(Some(&window));
            let snapshot = gtk::Snapshot::new();
            paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
            let current = ComposedFrame {
                time: clock.frame_time(),
                width,
                height,
                bounds,
                viewport,
                panel_closed: !observed_panel.reveals_child() && observed_panel.progress() == 0.0,
                node: snapshot.to_node(),
            };
            let mut frames = captured.borrow_mut();
            let stable = frames.0.as_ref().is_some_and(|previous| {
                current.time > previous.time
                    && current.width == previous.width
                    && current.height == previous.height
                    && same_rect(&current.bounds, &previous.bounds)
                    && same_rect(&current.viewport, &previous.viewport)
                    && current.panel_closed
                    && previous.panel_closed
            });
            frames.1 = if stable { frames.1 + 1 } else { 0 };
            frames.0 = Some(current);
        });
        let tick = ui.window.add_tick_callback(|window, _| {
            window.queue_draw();
            glib::ControlFlow::Continue
        });
        ui.window.queue_draw();
        pump(|| frames.borrow().1 >= 2);
        tick.remove();
        clock.disconnect(handler);
        let frame = frames.borrow_mut().0.take().unwrap();
        let (width, height) = (frame.width, frame.height);
        let (bounds, viewport) = (frame.bounds, frame.viewport);
        assert!(width > 0 && height > 0 && frame.panel_closed);
        assert!(cover.is_mapped());
        assert!(bounds.width() >= 32.0 && bounds.height() >= 32.0);
        assert!(bounds.x() >= viewport.x() - 1.0 && bounds.y() >= viewport.y() - 1.0);
        assert!(bounds.x() + bounds.width() <= viewport.x() + viewport.width() + 1.0);
        assert!(bounds.y() + bounds.height() <= viewport.y() + viewport.height() + 1.0);
        let node = frame
            .node
            .expect("the settled native frame must produce the composed window render node");
        let texture = renderer.render_texture(
            &node,
            Some(&gtk::graphene::Rect::new(
                0.0,
                0.0,
                width as f32,
                height as f32,
            )),
        );
        let stride = texture.width() as usize * 4;
        let mut pixels = vec![0; stride * texture.height() as usize];
        texture.download(&mut pixels, stride);
        let scale_x = texture.width() as f32 / width as f32;
        let scale_y = texture.height() as f32 / height as f32;
        for (dx, dy) in [(-4.0, -4.0), (0.0, 0.0), (4.0, 4.0)] {
            let x = ((bounds.x() + bounds.width() / 2.0 + dx) * scale_x).floor() as usize;
            let y = ((bounds.y() + bounds.height() / 2.0 + dy) * scale_y).floor() as usize;
            let offset = y * stride + x * 4;
            // gdk_texture_download uses native-endian CAIRO_FORMAT_ARGB32.
            let argb = u32::from_ne_bytes(pixels[offset..offset + 4].try_into().unwrap());
            let channels = [(argb >> 16) & 255, (argb >> 8) & 255, argb & 255];
            assert!(
                channels
                    .iter()
                    .zip([230_i32, 83, 57])
                    .all(|(actual, expected)| (*actual as i32 - expected).abs() <= 12),
                "closed information must not obscure the real composited coral cover: theme={scheme:?} size={width}x{height} frame={} bounds={bounds:?} viewport={viewport:?} rgb={channels:?}",
                frame.time
            );
        }
    }
}

fn response(stream: &mut TcpStream, status: u16, mime: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: max-age=86400, immutable\r\nConnection: close\r\n\r\n",
        body.len()
    );
    // Cancellation can legitimately close this loopback response early.
    let _ = stream.write_all(body);
}

fn synthetic_track() -> serde_json::Value {
    json!({"id":42,"title":"Synthetic coral track","available":true,"durationMs":123000,
        "coverUri":TRACK_COVER,"artists":[{"id":11,"name":"Synthetic artist"}],
        "albums":[{"id":7,"title":"Synthetic blue album","coverUri":ALBUM_COVER}]})
}

struct Fixture {
    origin: String,
    stop: Arc<AtomicBool>,
    image_count: Arc<AtomicUsize>,
    failed_images: Arc<AtomicUsize>,
    invalid_preset: Arc<AtomicUsize>,
    late_started: Arc<AtomicBool>,
    late_release: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl Fixture {
    fn new() -> Self {
        let mut images = BTreeMap::new();
        for size in [200, 400, 1000] {
            for (name, color, format) in [
                ("track-red", 0xe653_39ff, "png"),
                ("album-blue", 0x335a_d4ff, "jpeg"),
                ("playlist-green", 0x20a0_65ff, "png"),
                ("late", 0xc862_aeff, "png"),
            ] {
                let pixbuf = gtk::gdk_pixbuf::Pixbuf::new(
                    gtk::gdk_pixbuf::Colorspace::Rgb,
                    false,
                    8,
                    size,
                    size,
                )
                .unwrap();
                pixbuf.fill(color);
                images.insert(
                    (name.to_owned(), size),
                    (format, pixbuf.save_to_bufferv(format, &[]).unwrap()),
                );
            }
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        let media = player::tests::generated_mp3();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let image_count = Arc::new(AtomicUsize::new(0));
        let failed_images = Arc::new(AtomicUsize::new(0));
        let invalid_preset = Arc::new(AtomicUsize::new(0));
        let late_started = Arc::new(AtomicBool::new(false));
        let late_release = Arc::new(AtomicBool::new(false));
        let server = {
            let stop = stop.clone();
            let image_count = image_count.clone();
            let failed_images = failed_images.clone();
            let invalid_preset = invalid_preset.clone();
            let late_started = late_started.clone();
            let late_release = late_release.clone();
            let media_origin = origin.clone();
            thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(value) => value,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => panic!("synthetic artwork listener failed"),
                    };
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut headers = Vec::new();
                    let mut byte = [0];
                    while !headers.ends_with(b"\r\n\r\n") {
                        if stream.read(&mut byte).unwrap_or(0) == 0 {
                            break;
                        }
                        headers.push(byte[0]);
                        assert!(headers.len() < 16 * 1024);
                    }
                    if !headers.ends_with(b"\r\n\r\n") {
                        continue;
                    }
                    let request = String::from_utf8(headers).unwrap();
                    let lower = request.to_ascii_lowercase();
                    let length = lower
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length:")
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
                    if url.path().starts_with("/download-info/")
                        || url.path().starts_with("/get-mp3/")
                    {
                        assert!(
                            !lower.contains("authorization:")
                                && !lower.contains("cookie:")
                                && !lower.contains("referer:")
                        );
                        if url.path().starts_with("/download-info/") {
                            response(&mut stream, 200, "application/xml", b"<download-info><host>s268myt.storage.yandex.net</host><path>/karin-original-artwork.mp3</path><ts>1700000000</ts><s>original-synthetic-input</s></download-info>");
                        } else {
                            super::design_tests::serve_original_media(
                                &mut stream,
                                &request,
                                &media,
                            );
                        }
                        continue;
                    }
                    if url.path().starts_with("/get-music-") {
                        assert!(
                            !lower.contains("authorization:") && !lower.contains("cookie:"),
                            "public cover request must carry no session credentials"
                        );
                        image_count.fetch_add(1, Ordering::AcqRel);
                        let parts: Vec<_> = url.path().split('/').collect();
                        let size = match parts.last().copied() {
                            Some("200x200") => 200,
                            Some("400x400") => 400,
                            Some("1000x1000") => 1000,
                            _ => {
                                invalid_preset.fetch_add(1, Ordering::AcqRel);
                                response(&mut stream, 404, "application/octet-stream", b"");
                                continue;
                            }
                        };
                        let name = parts.get(parts.len().saturating_sub(2)).unwrap();
                        let failure = match *name {
                            "missing" => Some((404, "application/octet-stream", b"".as_slice())),
                            "unsupported" => {
                                Some((200, "text/html", b"synthetic non-image".as_slice()))
                            }
                            "bad-jpeg" => {
                                Some((200, "image/jpeg", b"synthetic corrupt JPEG".as_slice()))
                            }
                            _ => None,
                        };
                        if let Some((status, mime, body)) = failure {
                            failed_images.fetch_add(1, Ordering::AcqRel);
                            response(&mut stream, status, mime, body);
                            continue;
                        }
                        if *name == "late" {
                            late_started.store(true, Ordering::Release);
                            let deadline = Instant::now() + Duration::from_secs(3);
                            while !late_release.load(Ordering::Acquire)
                                && !stop.load(Ordering::Acquire)
                                && Instant::now() < deadline
                            {
                                thread::sleep(Duration::from_millis(2));
                            }
                        }
                        let (format, bytes) = images.get(&(name.to_string(), size)).unwrap();
                        response(
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
                    let track = synthetic_track();
                    let body = match url.path() {
                        path if path.starts_with("/tracks/") && path.ends_with("/download-info") => json!({"result":[{"codec":"mp3","bitrateInKbps":192,"preview":false,"direct":false,"downloadInfoUrl":format!("{media_origin}download-info/42")}]}),
                        "/account/status" => json!({"result":{"account":{"uid":1234,"displayName":"Synthetic artwork account"}}}),
                        "/users/1234/likes/tracks" => json!({"result":{"library":{"uid":1234,"revision":1,"tracks":[{"id":42,"track":track}]}}}),
                        "/users/1234/playlists/list" => json!({"result":[{"owner":{"uid":1234},"kind":3,"title":"Synthetic green playlist","trackCount":1,"cover":{"itemsUri":[PLAYLIST_COVER]}}]}),
                        "/users/1234/likes/playlists" => json!({"result":[]}),
                        "/albums/7/with-tracks" => json!({"result":{"id":7,"title":"Synthetic blue album","coverUri":ALBUM_COVER,"artists":[],"volumes":[[track]]}}),
                        "/albums/8/with-tracks" => {
                            let mut tracks = vec![track];
                            for (id, name) in [(43,"missing"),(44,"unsupported"),(45,"bad-jpeg")] {
                                let mut track = synthetic_track();
                                track["id"] = json!(id);
                                track["title"] = json!(format!("Synthetic {name} cover track"));
                                track["coverUri"] = json!(format!("avatars.yandex.net/get-music-content/fixture/{name}/%%"));
                                tracks.push(track);
                            }
                            json!({"result":{"id":8,"title":"Synthetic partial artwork album","coverUri":ALBUM_COVER,"artists":[],"volumes":[tracks]}})
                        },
                        "/tracks" => json!({"result":[track]}),
                        _ => panic!("unexpected synthetic artwork metadata route"),
                    }.to_string();
                    response(&mut stream, 200, "application/json", body.as_bytes());
                }
            })
        };
        Self {
            origin,
            stop,
            image_count,
            failed_images,
            invalid_preset,
            late_started,
            late_release,
            server: Some(server),
        }
    }

    fn count(&self) -> usize {
        self.image_count.load(Ordering::Acquire)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.late_release.store(true, Ordering::Release);
        if let Some(server) = self.server.take()
            && !thread::panicking()
        {
            server.join().unwrap();
        }
    }
}

struct TempRoot(PathBuf);
impl TempRoot {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
                "karin-native-cover-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )))
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn entries(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(children) = std::fs::read_dir(root) {
        for child in children {
            let path = child.unwrap().path();
            if path.is_dir() {
                result.extend(entries(&path));
            } else if path
                .extension()
                .is_some_and(|extension| extension == "entry")
            {
                result.push(path);
            }
        }
    }
    result
}

fn replace_player_art(this: &Controller, loader: &ArtworkLoader, cover: &CoverArt) -> gtk::Overlay {
    clear_box(&this.widgets.current_art);
    let thumbnail = super::super::artwork::thumbnail(loader, Some(cover), 56);
    this.widgets.current_art.append(&thumbnail);
    pump(|| !rendered_covers(&thumbnail).is_empty());
    assert_native_cover(&thumbnail);
    thumbnail
}

#[test]
#[ignore = "isolated native GTK/X11 fixture; run under Xvfb also with GDK_SCALE=2"]
fn native_cover_metadata_http_cache_and_render_lifecycle() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let _window_manager = WindowManager::start();
    let fixture = Fixture::new();
    let service = MusicService::for_test(&fixture.origin);
    let token = AccessToken::for_test(TOKEN, Duration::from_secs(300));
    service.account(&token).unwrap();
    let liked = service.liked_tracks(&token, 0).unwrap();
    let playlists = service.playlists(&token, 0).unwrap();
    let album = service.album(&token, "7").unwrap();
    let error_album = service.album(&token, "8").unwrap();
    let track = liked.items[0].clone();
    assert!(track.cover.is_some() && album.cover.is_some() && playlists.items[0].cover.is_some());
    let context = PlaybackContext::new(
        ContextKind::LikedTracks,
        None,
        liked.items.clone(),
        0,
        false,
    )
    .unwrap();
    let temp = TempRoot::new();
    let cache = Cache::new(temp.0.clone(), CachePolicy::default()).unwrap();
    cache.bind(Some(AccountScope::from_account_id("1234").unwrap()));
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.NativeCoverFixture")
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
    let loader = this.artwork.as_ref().unwrap();
    loader.set_test_origin(&fixture.origin).unwrap();
    loader.set_persistent_cache(Some(cache.clone()));
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(token);
        model.account_valid = true;
        model.liked = Some(Ok(liked.clone()));
        model.playlists = Some(Ok(playlists.clone()));
        model.queue.replace_context(context.clone());
    }
    this.render_controls();
    this.render_queue();
    this.render_liked(Ok(liked), 0);
    this.select_track_info(track.clone(), Some(context.clone()), true);
    this.widgets.stack.set_visible_child_name("library");
    ui.window.set_title(Some("Karin synthetic HTTP fixture"));
    ui.window.set_default_size(1440, 900);
    this.set_fixture_resolver_for_test(crate::stream::StreamResolver::for_test(
        &fixture.origin,
        &fixture.origin,
        &fixture.origin,
    ));
    ui.window.present();
    pump(|| !rendered_covers(&this.widgets.library_rows).is_empty());
    assert!(!this.widgets.player_revealer.reveals_child());
    assert!(!this.widgets.current_art.is_mapped());
    // Covers in a hidden idle player cannot be native visible pixels. Traverse
    // the real resolver and generated MP3 worker, then pause for stable evidence.
    this.play_music_context(context);
    pump(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == crate::playback::PlaybackState::Playing)
    });
    this.pause_music();
    pump(|| {
        this.music_snapshot()
            .is_some_and(|snapshot| snapshot.state == crate::playback::PlaybackState::Paused)
            && this.widgets.player_revealer.is_child_revealed()
            && !rendered_covers(&this.widgets.library_rows).is_empty()
            && !rendered_covers(&this.widgets.current_art).is_empty()
    });
    for root in [
        this.widgets.library_rows.clone().upcast::<gtk::Widget>(),
        this.widgets.current_art.clone().upcast(),
    ] {
        for overlay in rendered_covers(&root) {
            assert_native_cover(&overlay);
        }
    }
    assert!(
        this.info_host.borrow().is_some(),
        "ordinary shell attaches an information host"
    );
    this.set_info_open(true);
    pump(|| this.info_is_open() && !rendered_covers(&this.track_pane.widget).is_empty());
    for overlay in rendered_covers(&this.track_pane.widget) {
        assert_native_cover(&overlay);
    }
    capture_native("cover-http-library-player-info");

    // Real textures in the ordinary shell across the required responsive
    // sizes, themes (capture_native) and maximized state. GDK_SCALE=2 runs this
    // same metadata/HTTP/decode path in a separate native process.
    // Compact captures intentionally show the music surface rather than its
    // explicit information subview. Close through the controller's open intent,
    // never the presentation container's incidental mapped/revealed state.
    this.set_info_open(false);
    pump(|| !this.info_is_open());
    for (width, height, name) in [
        (420, 720, "compact"),
        (900, 650, "medium"),
        (1440, 900, "wide"),
    ] {
        resize_cover_window(&ui, width, height);
        pump(|| {
            !rendered_covers(&this.widgets.library_rows).is_empty()
                && !rendered_covers(&this.widgets.current_art).is_empty()
        });
        for root in [
            this.widgets.library_rows.clone().upcast::<gtk::Widget>(),
            this.widgets.current_art.clone().upcast(),
        ] {
            for overlay in rendered_covers(&root) {
                assert_native_cover(&overlay);
            }
        }
        let visible_library_cover = rendered_covers(&this.widgets.library_rows)
            .into_iter()
            .next()
            .expect("a real metadata/HTTP/decode library thumbnail is visible");
        assert_composited_library_cover(&ui, &visible_library_cover);
        if name == "wide" {
            this.set_info_open(true);
            pump(|| this.info_is_open() && !rendered_covers(&this.track_pane.widget).is_empty());
        }
        capture_native(&format!("cover-http-{name}"));
    }
    ui.window.maximize();
    pump(|| ui.window.is_maximized() && !rendered_covers(&this.widgets.current_art).is_empty());
    capture_native("cover-http-maximized");
    ui.window.unmaximize();
    pump(|| !ui.window.is_maximized());
    resize_cover_window(&ui, 1440, 900);

    // Exercise the section handler and its actual mapped combined page. It may
    // reuse the admitted owned snapshot while fetching only missing liked data.
    this.widgets.library_mode.set_selected(1);
    pump(|| {
        !this.model.borrow().busy
            && this.widgets.stack.visible_child_name().as_deref() == Some("playlists")
            && this.playlist_grid.widget.is_mapped()
            && !rendered_covers(&this.playlist_grid.widget).is_empty()
    });
    for overlay in rendered_covers(&this.playlist_grid.widget) {
        assert_native_cover(&overlay);
    }
    capture_native("cover-http-playlist-grid");

    this.render_album(Rc::new(album), 0);
    pump(|| rendered_covers(&this.widgets.detail).len() >= 2);
    for overlay in rendered_covers(&this.widgets.detail) {
        assert_native_cover(&overlay);
    }
    capture_native("cover-http-album-details");
    settle();
    assert!(cache.usage().unwrap().entries >= 3);
    assert_eq!(fixture.invalid_preset.load(Ordering::Acquire), 0);

    // Cover errors are presentation-local. This metadata is validated through
    // MusicService too; ordinary album rows issue actual HTTP and retain their
    // fallback without erasing a working sibling or the persistent player art.
    let before_errors = fixture.count();
    let notice = this.widgets.notice.text();
    let status = this.widgets.status.title();
    this.render_album(Rc::new(error_album), 0);
    pump(|| {
        fixture.failed_images.load(Ordering::Acquire) == 3
            && rendered_covers(&this.widgets.detail).len() >= 2
    });
    settle();
    for name in ["missing", "unsupported", "bad-jpeg"] {
        let row = descendants(this.widgets.detail.upcast_ref())
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::ListBoxRow>().ok())
            .find(|row| contains_text(row.upcast_ref(), &format!("Synthetic {name} cover track")))
            .unwrap();
        let thumbnail = descendants(row.upcast_ref())
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Overlay>().ok())
            .find(|overlay| {
                overlay
                    .child()
                    .is_some_and(|child| child.is::<gtk::Image>())
            })
            .unwrap();
        let fallback = thumbnail.child().unwrap().downcast::<gtk::Image>().unwrap();
        assert!(fallback.is_visible() && fallback.is_mapped());
        let picture = descendants(thumbnail.upcast_ref())
            .into_iter()
            .find_map(|widget| widget.downcast::<gtk::Picture>().ok())
            .unwrap();
        assert!(!picture.is_visible() && picture.paintable().is_none());
    }
    assert_eq!(fixture.count(), before_errors + 3);
    assert!(!rendered_covers(&this.widgets.current_art).is_empty());
    for overlay in rendered_covers(&this.widgets.current_art) {
        assert_native_cover(&overlay);
    }
    assert_eq!(this.widgets.notice.text(), notice);
    assert_eq!(this.widgets.status.title(), status);
    assert!(!this.widgets.status_revealer.reveals_child());
    capture_native("cover-http-partial-failure-fallbacks");

    // A completely new loader has empty RAM, but renders from the same scoped
    // raster cache without one extra HTTP request. No synthetic RAM injection.
    let before = fixture.count();
    let cold = ArtworkLoader::for_test_origin(&fixture.origin).unwrap();
    cold.set_persistent_cache(Some(cache.clone()));
    replace_player_art(this, &cold, track.cover.as_ref().unwrap());
    assert_eq!(
        fixture.count(),
        before,
        "cold restart must read the disk raster"
    );

    // Corrupt only app-private synthetic artwork entries. A fresh loader must
    // safely miss/recover through HTTP rather than pass corrupt pixels to GTK.
    let files = entries(&temp.0);
    assert!(!files.is_empty());
    for file in files {
        std::fs::write(file, b"synthetic corrupt cache record").unwrap();
    }
    let recovered = ArtworkLoader::for_test_origin(&fixture.origin).unwrap();
    recovered.set_persistent_cache(Some(cache.clone()));
    replace_player_art(this, &recovered, track.cover.as_ref().unwrap());
    assert_eq!(fixture.count(), before + 1);

    let disk_before = cache.usage().unwrap();
    cache
        .configure(CachePolicy {
            enabled: false,
            ..CachePolicy::default()
        })
        .unwrap();
    let disabled = ArtworkLoader::for_test_origin(&fixture.origin).unwrap();
    disabled.set_persistent_cache(Some(cache.clone()));
    replace_player_art(this, &disabled, track.cover.as_ref().unwrap());
    assert_eq!(
        fixture.count(),
        before + 2,
        "disabled disk cache requires HTTP"
    );
    assert_eq!(cache.usage().unwrap().entries, disk_before.entries);
    assert_eq!(cache.usage().unwrap().bytes, disk_before.bytes);

    // A late image cannot repaint a retired session or repopulate storage after
    // logout/clear. Explicit binding here mirrors the synthetic cache harness;
    // disabled persistence never touches a real wallet or XDG directory.
    cache.configure(CachePolicy::default()).unwrap();
    let late_cover =
        CoverArt::parse("avatars.yandex.net/get-music-content/fixture/late/%%").unwrap();
    clear_box(&this.widgets.current_art);
    let late = super::super::artwork::thumbnail(loader, Some(&late_cover), 56);
    this.widgets.current_art.append(&late);
    pump(|| fixture.late_started.load(Ordering::Acquire));
    this.logout();
    cache.bind(None);
    cache.clear().unwrap();
    fixture.late_release.store(true, Ordering::Release);
    settle();
    assert!(!this.model.borrow().account_valid);
    assert!(rendered_covers(&late).is_empty());
    assert_eq!(cache.usage().unwrap().entries, 0);
    assert_eq!(fixture.invalid_preset.load(Ordering::Acquire), 0);
    ui.music.close();
    ui.window.close();
    settle();
}

struct SaturatedImages {
    origin: String,
    stop: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
    started: Arc<AtomicUsize>,
    visible_started: Arc<AtomicUsize>,
    server: Option<JoinHandle<()>>,
}

impl SaturatedImages {
    fn new() -> Self {
        let mut images = BTreeMap::new();
        for size in [200, 400, 1000] {
            let pixbuf = gtk::gdk_pixbuf::Pixbuf::new(
                gtk::gdk_pixbuf::Colorspace::Rgb,
                false,
                8,
                size,
                size,
            )
            .unwrap();
            pixbuf.fill(0xb849_caff);
            images.insert(size, pixbuf.save_to_bufferv("png", &[]).unwrap());
        }
        let images = Arc::new(images);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let started = Arc::new(AtomicUsize::new(0));
        let visible_started = Arc::new(AtomicUsize::new(0));
        let server = {
            let stop = stop.clone();
            let release = release.clone();
            let started = started.clone();
            let visible_started = visible_started.clone();
            thread::spawn(move || {
                let mut clients = Vec::new();
                while !stop.load(Ordering::Acquire) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(value) => value,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => panic!("synthetic saturation listener failed"),
                    };
                    let stop = stop.clone();
                    let release = release.clone();
                    let started = started.clone();
                    let visible_started = visible_started.clone();
                    let images = images.clone();
                    clients.push(thread::spawn(move || {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut headers = Vec::new();
                        let mut byte = [0];
                        while !headers.ends_with(b"\r\n\r\n") {
                            if stream.read(&mut byte).unwrap_or(0) == 0 {
                                break;
                            }
                            headers.push(byte[0]);
                            assert!(headers.len() < 16 * 1024);
                        }
                        if !headers.ends_with(b"\r\n\r\n") {
                            return;
                        }
                        let request = String::from_utf8(headers).unwrap();
                        let lower = request.to_ascii_lowercase();
                        assert!(!lower.contains("authorization:") && !lower.contains("cookie:"));
                        let target = request
                            .lines()
                            .next()
                            .unwrap()
                            .split_whitespace()
                            .nth(1)
                            .unwrap();
                        let url =
                            reqwest::Url::parse(&format!("http://127.0.0.1{target}")).unwrap();
                        assert!(url.path().starts_with("/get-music-content/saturation/"));
                        let size = match url.path().rsplit('/').next().unwrap() {
                            "200x200" => 200,
                            "400x400" => 400,
                            "1000x1000" => 1000,
                            _ => panic!("unsupported synthetic CDN preset"),
                        };
                        if url.path().contains("/visible/") {
                            visible_started.fetch_add(1, Ordering::AcqRel);
                        }
                        started.fetch_add(1, Ordering::AcqRel);
                        let deadline = Instant::now() + Duration::from_secs(3);
                        while !release.load(Ordering::Acquire)
                            && !stop.load(Ordering::Acquire)
                            && Instant::now() < deadline
                        {
                            thread::sleep(Duration::from_millis(2));
                        }
                        response(&mut stream, 200, "image/png", images.get(&size).unwrap());
                    }));
                }
                for client in clients {
                    client.join().unwrap();
                }
            })
        };
        Self {
            origin,
            stop,
            release,
            started,
            visible_started,
            server: Some(server),
        }
    }
}

impl Drop for SaturatedImages {
    fn drop(&mut self) {
        self.release.store(true, Ordering::Release);
        self.stop.store(true, Ordering::Release);
        if let Some(server) = self.server.take()
            && !thread::panicking()
        {
            server.join().unwrap();
        }
    }
}

#[test]
#[ignore = "isolated GTK/X11 fixture; proves admission retry without a remap"]
fn native_visible_cover_retries_after_worker_queue_saturation_without_remap() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let fixture = SaturatedImages::new();
    let loader = ArtworkLoader::for_test_origin(&fixture.origin).unwrap();
    let window = gtk::Window::builder()
        .title("Karin synthetic HTTP fixture")
        .default_width(320)
        .default_height(240)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_halign(gtk::Align::Center);
    content.set_valign(gtk::Align::Center);
    window.set_child(Some(&content));
    window.present();
    pump(|| window.is_mapped());

    // Hold all four real network workers. Retain another 64 pending requests,
    // filling the bounded channel through its public API instead of fabricating
    // an internal waiting count or inserting predecoded image pixels.
    let mut active = Vec::new();
    for index in 0..4 {
        let cover = CoverArt::parse(&format!(
            "avatars.yandex.net/get-music-content/saturation/active-{index}/%%"
        ))
        .unwrap();
        active.push(loader.request(&cover, 96, loader.epoch()).unwrap());
    }
    pump(|| fixture.started.load(Ordering::Acquire) == 4);
    let mut pending = Vec::new();
    for index in 0..64 {
        let cover = CoverArt::parse(&format!(
            "avatars.yandex.net/get-music-content/saturation/pending-{index}/%%"
        ))
        .unwrap();
        pending.push(loader.request(&cover, 96, loader.epoch()).unwrap());
    }
    let cover =
        CoverArt::parse("avatars.yandex.net/get-music-content/saturation/visible/%%").unwrap();
    assert!(matches!(
        loader.request(&cover, 96, loader.epoch()),
        Err(crate::artwork::ArtworkError::Busy)
    ));
    let thumbnail = super::super::artwork::thumbnail(&loader, Some(&cover), 96);
    let maps = Rc::new(Cell::new(0_usize));
    thumbnail.connect_map({
        let maps = maps.clone();
        move |_| maps.set(maps.get() + 1)
    });
    content.append(&thumbnail);
    pump(|| thumbnail.is_mapped());
    let saturated_until = Instant::now() + Duration::from_millis(650);
    pump(|| Instant::now() >= saturated_until);
    assert_eq!(maps.get(), 1);
    assert!(rendered_covers(&thumbnail).is_empty());
    assert_eq!(fixture.started.load(Ordering::Acquire), 4);
    assert_eq!(fixture.visible_started.load(Ordering::Acquire), 0);

    // The old three 100 ms retries had already abandoned this visible widget.
    // Free the real queue without remapping it; the new bounded backoff must
    // admit its one request and deliver native pixels automatically.
    drop(pending);
    fixture.release.store(true, Ordering::Release);
    pump(|| !rendered_covers(&thumbnail).is_empty());
    assert_native_cover(&thumbnail);
    assert_eq!(
        maps.get(),
        1,
        "recovery must not depend on a second map signal"
    );
    assert_eq!(fixture.visible_started.load(Ordering::Acquire), 1);
    assert_eq!(fixture.started.load(Ordering::Acquire), 5);
    drop(active);
    loader.clear();
    window.close();
    settle();
}
