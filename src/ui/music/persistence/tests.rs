// SPDX-License-Identifier: GPL-3.0-or-later
//! Isolated synthetic credentials/API/media. No host wallet or provider account.
use super::super::tests::{capture_native, pump_until};
use super::*;
use crate::{
    cache::{CacheKey, EntryKind},
    credentials::{Cancellation, CredentialBackend},
    playback::PlaybackState,
    stream::StreamResolver,
};
use gst::prelude::*;
use gstreamer as gst;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const TOKEN: &str = "synthetic-storage-ui-token";

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "karin-session-ui-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Backend {
    gtk_thread: thread::ThreadId,
    value: Mutex<Option<String>>,
    loads: AtomicUsize,
    fail_delete: AtomicBool,
    unavailable: AtomicBool,
    saves: AtomicUsize,
    hold_save: AtomicBool,
    save_entered: AtomicBool,
    release: (Mutex<bool>, Condvar),
}
impl Backend {
    fn new(value: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            gtk_thread: thread::current().id(),
            value: Mutex::new(value.map(str::to_owned)),
            loads: AtomicUsize::new(0),
            fail_delete: AtomicBool::new(false),
            unavailable: AtomicBool::new(false),
            saves: AtomicUsize::new(0),
            hold_save: AtomicBool::new(false),
            save_entered: AtomicBool::new(false),
            release: (Mutex::new(false), Condvar::new()),
        })
    }
    fn worker(&self) {
        assert_ne!(
            thread::current().id(),
            self.gtk_thread,
            "credential IO reached GTK"
        );
    }
    fn block_save(&self) {
        self.save_entered.store(false, Ordering::Release);
        *self.release.0.lock().unwrap() = false;
        self.hold_save.store(true, Ordering::Release);
    }
    fn release_save(&self) {
        *self.release.0.lock().unwrap() = true;
        self.release.1.notify_all();
    }
}
impl CredentialBackend for Arc<Backend> {
    fn probe(&self, cancel: &Cancellation) -> Result<BackendKind, StorageError> {
        self.worker();
        cancel.check()?;
        if self.unavailable.load(Ordering::Acquire) {
            return Err(StorageError::Unavailable);
        }
        Ok(BackendKind::SecretService)
    }
    fn load(&self, cancel: &Cancellation) -> Result<Option<AccessToken>, StorageError> {
        self.worker();
        cancel.check()?;
        self.loads.fetch_add(1, Ordering::AcqRel);
        Ok(self
            .value
            .lock()
            .unwrap()
            .as_ref()
            .map(|value| AccessToken::new(Zeroizing::new(value.clone()), None)))
    }
    fn save_validated(&self, token: &AccessToken, _: &Cancellation) -> Result<(), StorageError> {
        self.worker();
        self.saves.fetch_add(1, Ordering::AcqRel);
        if self.hold_save.swap(false, Ordering::AcqRel) {
            self.save_entered.store(true, Ordering::Release);
            let guard = self.release.0.lock().unwrap();
            let (guard, _) = self
                .release
                .1
                .wait_timeout_while(guard, Duration::from_secs(8), |released| !*released)
                .unwrap();
            assert!(*guard, "synthetic save gate was not released");
        }
        // Deliberately commit despite caller cancellation, as an already-sent
        // DBus mutation may do. The domain must compensate a superseded save.
        *self.value.lock().unwrap() = Some(token.expose().to_owned());
        Ok(())
    }
    fn delete(&self, cancel: &Cancellation) -> Result<(), StorageError> {
        self.worker();
        cancel.check()?;
        if self.fail_delete.load(Ordering::Acquire) {
            Err(StorageError::Unconfirmed)
        } else {
            *self.value.lock().unwrap() = None;
            Ok(())
        }
    }
}

struct Api {
    origin: String,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Api {
    fn new(directory: &Path) -> Self {
        let media = Arc::new(original_mp3(directory));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = stop.clone();
            let origin = origin.clone();
            thread::spawn(move || {
                let mut children = Vec::new();
                while !stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            assert!(children.len() < 96);
                            let origin = origin.clone();
                            let media = media.clone();
                            children.push(thread::spawn(move || serve(stream, &origin, &media)));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(_) => panic!("synthetic API listener failed"),
                    }
                }
                for child in children {
                    child.join().unwrap();
                }
            })
        };
        Self {
            origin,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Api {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn serve(mut stream: TcpStream, origin: &str, media: &[u8]) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut byte = [0];
    while !request.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            return;
        }
        request.push(byte[0]);
        assert!(request.len() < 16 * 1024);
    }
    let request = String::from_utf8(request).unwrap();
    let first = request
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    assert!(matches!(first[0], "GET" | "HEAD"));
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{}", first[1])).unwrap();
    let header = |name: &str| {
        request.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then_some(value.trim())
        })
    };
    let transport =
        url.path().starts_with("/download-info/") || url.path().starts_with("/get-mp3/");
    if transport {
        for field in ["authorization", "cookie", "referer", "origin"] {
            assert!(header(field).is_none());
        }
    } else {
        assert_eq!(
            header("authorization"),
            Some("OAuth synthetic-storage-ui-token")
        );
    }
    let response = match url.path() {
        "/account/status" => json!({"result":{"account":{"uid":1234,"displayName":"Synthetic remembered account"}}}),
        "/users/1234/likes/tracks" => json!({"result":{"library":{"uid":1234,"revision":1,"tracks":[]}}}),
        "/users/1234/playlists/list" | "/users/1234/likes/playlists" => json!({"result":[]}),
        "/tracks/42/download-info" => json!({"result":[{"codec":"mp3","bitrateInKbps":192,"preview":false,"direct":false,"downloadInfoUrl":format!("{origin}download-info/42")}]}),
        "/download-info/42" => {
            let body = "<download-info><host>s268myt.storage.yandex.net</host><path>/fixture-42.mp3</path><ts>1700000000</ts><s>synthetic-server-input</s></download-info>";
            let _ = write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
            return;
        }
        path if path.starts_with("/get-mp3/") && path.ends_with("/fixture-42.mp3") => {
            let range = header("range").map(|value| {
                let (start,end) = value.strip_prefix("bytes=").unwrap().split_once('-').unwrap();
                let start = start.parse::<usize>().unwrap();
                let end = if end.is_empty() {media.len()-1} else {end.parse::<usize>().unwrap().min(media.len()-1)};
                assert!(start<=end && end<media.len());
                (start,end)
            });
            let (start,end) = range.unwrap_or((0,media.len()-1));
            let status = if range.is_some() {"206 Partial Content"} else {"200 OK"};
            let _ = write!(stream,"HTTP/1.1 {status}\r\nContent-Type: audio/mpeg\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\nConnection: close\r\n",end-start+1);
            if range.is_some() {let _ = write!(stream,"Content-Range: bytes {start}-{end}/{}\r\n",media.len());}
            let _ = stream.write_all(b"\r\n");
            if first[0]!="HEAD" {let _ = stream.write_all(&media[start..=end]);}
            return;
        }
        _ => panic!("unexpected synthetic persistence API route"),
    }.to_string();
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
        response.len()
    );
}

fn original_mp3(directory: &Path) -> Vec<u8> {
    // An original thirty-second low-volume synthetic tone. This proves that
    // existing GStreamer playback survives settings/cache work, not audibility
    // of a real service track or acceptance of any host wallet implementation.
    gst::init().unwrap();
    let file = directory.join("original-synthetic.mp3");
    let pipeline = gst::Pipeline::new();
    let source = gst::ElementFactory::make("audiotestsrc")
        .property("num-buffers", 3000_i32)
        .property("samplesperbuffer", 441_i32)
        .property("volume", 0.01_f64)
        .build()
        .unwrap();
    let convert = gst::ElementFactory::make("audioconvert").build().unwrap();
    let caps = gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("audio/x-raw")
                .field("rate", 44100_i32)
                .field("channels", 1_i32)
                .build(),
        )
        .build()
        .unwrap();
    let encoder = gst::ElementFactory::make("lamemp3enc")
        .property("bitrate", 128_i32)
        .property("cbr", true)
        .build()
        .unwrap();
    let sink = gst::ElementFactory::make("filesink")
        .property("location", file.to_str().unwrap())
        .build()
        .unwrap();
    pipeline
        .add_many([&source, &convert, &caps, &encoder, &sink])
        .unwrap();
    gst::Element::link_many([&source, &convert, &caps, &encoder, &sink]).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let done = pipeline
        .bus()
        .unwrap()
        .timed_pop_filtered(
            gst::ClockTime::from_seconds(5),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        )
        .is_some_and(|message| matches!(message.view(), gst::MessageView::Eos(_)));
    pipeline.set_state(gst::State::Null).unwrap();
    assert!(done, "synthetic MP3 encoding did not complete");
    let bytes = std::fs::read(&file).unwrap();
    std::fs::remove_file(file).unwrap();
    assert!(bytes.len() > 1000 && bytes.len() < 1024 * 1024);
    bytes
}

fn window(
    api: &Api,
    settings: &SettingsStore,
    backend: &Arc<Backend>,
    cache: &Cache,
) -> (adw::Application, crate::ui::MainWindow) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let app = adw::Application::builder()
        .application_id(format!(
            "io.github.tracerxbrhd.Karin.PersistenceFixture.Window{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let state = State::for_testing(
        settings.clone(),
        CredentialStore::with_backend(backend.clone()),
        Some(cache.clone()),
    );
    let ui = crate::ui::build_with_music_pane(
        &app,
        crate::application::ConnectionService::new(crate::api::YandexApi::new().unwrap()),
        |app, window| {
            MusicPane::with_provider_state(
                app,
                window,
                Some(MusicService::for_test(&api.origin)),
                Arc::new(OfflineProvider),
                launch_system_browser,
                state,
            )
        },
    );
    ui.window.set_title(Some("Karin synthetic HTTP fixture"));
    ui.window.set_default_size(900, 650);
    ui.window.present();
    (app, ui)
}
fn settled(this: &Controller) -> bool {
    let p = this.persistence.borrow();
    p.ready
        && p.phase == Phase::Idle
        && !p.critical
        && !p.settings_busy
        && !p.cache_busy
        && !this.model.borrow().busy
}
fn token() -> AccessToken {
    AccessToken::for_test(TOKEN, Duration::from_secs(300))
}
fn track() -> Track {
    Track {
        id: "42".into(),
        title: "Synthetic retained playback".into(),
        artists: vec!["Synthetic artist".into()],
        artist_links: Vec::new(),
        album_id: None,
        album_title: None,
        duration_ms: Some(30000),
        cover: None,
        explicit: false,
        available: true,
    }
}

#[test]
#[ignore = "requires isolated GTK/X11/GStreamer and original MP3 plugins; synthetic accounts only"]
fn native_persistent_session_restore_settings_forget_and_close() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let directory = Directory::new();
    let api = Api::new(&directory.0);
    let settings = SettingsStore::at_directory(directory.0.join("settings")).unwrap();
    let prefs = Preferences {
        appearance: Appearance::Dark,
        ..Preferences::default()
    };
    settings.save(&prefs).unwrap();
    let backend = Backend::new(Some(TOKEN));
    let cache = Cache::new(directory.0.join("cache"), CachePolicy::default()).unwrap();
    let (_app, ui) = window(&api, &settings, &backend, &cache);
    let this = &ui.music.controller;
    pump_until(|| this.model.borrow().account_valid && settled(this));
    assert_eq!(backend.loads.load(Ordering::Acquire), 1);
    assert!(this.login_dialog.borrow().is_none());
    assert_eq!(
        this.persistence.borrow().values.appearance,
        Appearance::Dark
    );
    assert_eq!(
        this.persistence.borrow().manager.as_ref().unwrap().state(),
        crate::session_lifecycle::PersistenceState::Remembered
    );
    this.set_fixture_resolver_for_test(StreamResolver::for_test(
        &api.origin,
        &api.origin,
        &api.origin,
    ));
    this.model.borrow_mut().queue.append(track()).unwrap();
    this.render_queue();
    this.play_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == PlaybackState::Playing)
    });
    let original = this.music_snapshot().unwrap();
    let mut changed = this.persistence.borrow().values.clone();
    changed.remember_login = false;
    this.preferences_changed(changed);
    pump_until(|| settled(this) && backend.value.lock().unwrap().is_none());
    assert!(this.model.borrow().account_valid);
    assert_eq!(this.music_snapshot().unwrap().state, PlaybackState::Playing);
    assert_eq!(
        this.music_snapshot().unwrap().generation,
        original.generation
    );
    let key = CacheKey::new(EntryKind::Track, "99", 0).unwrap();
    let lease = cache.lease().unwrap();
    let mut cached_track = track();
    cached_track.id = "99".into();
    let encoded = crate::music::cache_codec::encode(
        EntryKind::Track,
        &crate::music::cache_codec::MetadataRecord::Track(cached_track),
    )
    .unwrap();
    cache.put(&lease, &key, &encoded).unwrap();
    this.clear_persistent_cache();
    pump_until(|| !this.persistence.borrow().cache_busy);
    assert_eq!(cache.usage().unwrap().entries, 0);
    assert!(this.model.borrow().account_valid);
    assert_eq!(this.model.borrow().queue.current().unwrap().id, "42");
    let after = this.music_snapshot().unwrap();
    assert_eq!(after.state, PlaybackState::Playing);
    assert_eq!(after.generation, original.generation);
    pump_until(|| {
        this.music_snapshot()
            .and_then(|s| s.position)
            .zip(original.position)
            .is_some_and(|(new, old)| new > old)
    });
    this.present_preferences();
    pump_until(|| {
        this.persistence
            .borrow()
            .view
            .as_ref()
            .is_some_and(|view| view.dialog.is_mapped())
    });
    capture_native("v03-secure-session-preferences-dark");
    this.close();
    ui.window.close();

    // Remember-off persisted independently of current-account/audio state.
    let (_app2, ui2) = window(&api, &settings, &backend, &cache);
    let second = &ui2.music.controller;
    pump_until(|| settled(second));
    assert!(!second.model.borrow().account_valid);
    assert!(!second.persistence.borrow().values.remember_login);
    assert_eq!(
        second.persistence.borrow().values.appearance,
        Appearance::Dark
    );
    let mut changed = second.persistence.borrow().values.clone();
    changed.remember_login = true;
    second.preferences_changed(changed);
    pump_until(|| settled(second));
    backend.block_save();
    second.authenticate(token());
    pump_until(|| backend.save_entered.load(Ordering::Acquire));
    assert!(settings.load().unwrap().preferences.forget_pending);
    second.set_track_info_preference(false);
    assert!(
        !second.track_info_preference(),
        "a pane close during save must take effect immediately"
    );
    second.close();
    ui2.window.close();
    // Reopen BEFORE the old held save completes, with independently-created
    // stores for the same paths. No restore/delete may race the retired write.
    let settings = SettingsStore::at_directory(directory.0.join("settings")).unwrap();
    let cache = Cache::new(directory.0.join("cache"), CachePolicy::default()).unwrap();
    let loads_before_reopen = backend.loads.load(Ordering::Acquire);
    let (_app3, ui3) = window(&api, &settings, &backend, &cache);
    let third = &ui3.music.controller;
    let observed = Instant::now() + Duration::from_millis(150);
    pump_until(|| Instant::now() >= observed);
    assert_eq!(backend.loads.load(Ordering::Acquire), loads_before_reopen);
    assert!(third.persistence_blocks_login());
    assert!(!third.persistence.borrow().ready);
    assert!(settings.load().unwrap().preferences.forget_pending);
    backend.release_save();
    pump_until(|| third.model.borrow().account_valid && settled(third));
    assert!(!settings.load().unwrap().preferences.forget_pending);
    assert!(
        !third.track_info_preference(),
        "the queued nonsecret pane preference must survive critical close and reopen"
    );
    assert!(!settings.load().unwrap().preferences.track_info_open);
    backend.fail_delete.store(true, Ordering::Release);
    third.logout();
    pump_until(|| settled(third) && third.persistence.borrow().retry == Retry::Forget);
    assert!(!third.model.borrow().account_valid);
    assert!(settings.load().unwrap().preferences.forget_pending);
    assert!(backend.value.lock().unwrap().is_some());
    third.close();
    ui3.window.close();
    let loads = backend.loads.load(Ordering::Acquire);
    let (_app4, ui4) = window(&api, &settings, &backend, &cache);
    let fourth = &ui4.music.controller;
    pump_until(|| settled(fourth) && fourth.persistence.borrow().retry == Retry::Forget);
    assert_eq!(
        backend.loads.load(Ordering::Acquire),
        loads,
        "a pending forget barrier must not restore"
    );
    assert!(!fourth.model.borrow().account_valid);
    assert!(
        !fourth.persistence_blocks_login(),
        "settled wallet failure must allow memory-only sign-in"
    );
    backend.fail_delete.store(false, Ordering::Release);
    fourth.retry_persistence();
    pump_until(|| settled(fourth) && !fourth.persistence.borrow().values.forget_pending);
    assert!(backend.value.lock().unwrap().is_none());

    // A failed CACHE purge is a separate unfinished cleanup intent. A new
    // memory-only login must neither save over it nor clear its durable barrier.
    fourth.authenticate(token());
    pump_until(|| {
        fourth.model.borrow().account_valid
            && settled(fourth)
            && backend.value.lock().unwrap().is_some()
    });
    std::fs::set_permissions(
        directory.0.join("cache"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fourth.logout();
    pump_until(|| settled(fourth) && fourth.persistence.borrow().retry == Retry::Forget);
    assert!(backend.value.lock().unwrap().is_none());
    assert!(fourth.persistence.borrow().purge_after_forget);
    let saves = backend.saves.load(Ordering::Acquire);
    fourth.authenticate(token());
    pump_until(|| fourth.model.borrow().account_valid && settled(fourth));
    assert_eq!(backend.saves.load(Ordering::Acquire), saves);
    assert_eq!(fourth.persistence.borrow().retry, Retry::Forget);
    assert!(settings.load().unwrap().preferences.forget_pending);
    assert!(cache.lease().is_none());
    std::fs::set_permissions(
        directory.0.join("cache"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    fourth.retry_persistence();
    pump_until(|| settled(fourth) && !fourth.persistence.borrow().values.forget_pending);
    assert!(!fourth.persistence.borrow().purge_after_forget);
    assert!(fourth.model.borrow().account_valid);
    fourth.logout();
    pump_until(|| settled(fourth) && !fourth.persistence.borrow().values.forget_pending);
    // Logout supersedes a save already dispatched to a backend. Its late
    // commit must be removed before logout can report confirmed completion.
    backend.block_save();
    fourth.authenticate(token());
    pump_until(|| backend.save_entered.load(Ordering::Acquire));
    fourth.logout();
    backend.release_save();
    pump_until(|| settled(fourth) && !fourth.persistence.borrow().values.forget_pending);
    assert!(backend.value.lock().unwrap().is_none());
    assert!(!settings.load().unwrap().preferences.forget_pending);
    assert!(!fourth.model.borrow().account_valid);
    fourth.close();
    ui4.window.close();

    // Unavailable protected storage must not disable independent metadata
    // caching or create a forget barrier merely because default remember is ON.
    backend.unavailable.store(true, Ordering::Release);
    let saves = backend.saves.load(Ordering::Acquire);
    let (_app5, ui5) = window(&api, &settings, &backend, &cache);
    let fifth = &ui5.music.controller;
    pump_until(|| settled(fifth));
    assert!(!fifth.persistence.borrow().supported);
    fifth.authenticate(token());
    pump_until(|| fifth.model.borrow().account_valid && settled(fifth));
    assert_eq!(backend.saves.load(Ordering::Acquire), saves);
    assert!(!fifth.persistence.borrow().values.forget_pending);
    assert!(!settings.load().unwrap().preferences.forget_pending);
    assert!(cache.lease().is_some());
    assert!(cache.policy().unwrap().enabled);
    fifth.close();
    ui5.window.close();
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);
}
