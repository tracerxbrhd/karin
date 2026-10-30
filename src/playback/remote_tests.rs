// SPDX-License-Identifier: GPL-3.0-or-later
//! Original synthetic media and loopback transports only. These tests never
//! contact a service/CDN or establish real Yandex Music audibility.

use super::*;
use std::{
    io::{BufRead, BufReader, Read},
    net::{SocketAddr, TcpListener, TcpStream},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    process::{Child, Stdio},
    sync::atomic::AtomicUsize,
};

struct PrivateDirectory(PathBuf);

impl PrivateDirectory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        for _ in 0..20 {
            let path = std::env::temp_dir().join(format!(
                "karin-generated-transport-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => panic!("private synthetic transport directory unavailable"),
            }
        }
        panic!("private synthetic transport directory unavailable")
    }
}

impl Drop for PrivateDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct SyntheticMp3 {
    directory: PrivateDirectory,
    bytes: Arc<Vec<u8>>,
}

impl SyntheticMp3 {
    fn new(duration: Duration) -> Self {
        gst::init().expect("GStreamer initializes for an original synthetic signal");
        let directory = PrivateDirectory::new();
        let path = directory.0.join("original-synthetic-signal.mp3");
        fs::write(&path, []).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let pipeline = gst::Pipeline::new();
        let source = gst::ElementFactory::make("audiotestsrc")
            .property("num-buffers", (duration.as_millis() / 10) as i32)
            .property("samplesperbuffer", 441_i32)
            .property("volume", 0.03_f64)
            .build()
            .expect("synthetic signal source exists");
        let convert = gst::ElementFactory::make("audioconvert").build().unwrap();
        let filter = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("audio/x-raw")
                    .field("rate", 44_100_i32)
                    .field("channels", 1_i32)
                    .build(),
            )
            .build()
            .unwrap();
        let encoder = gst::ElementFactory::make("lamemp3enc")
            .property("bitrate", 128_i32)
            .property("cbr", true)
            .build()
            .expect("the normal GStreamer MP3 encoder exists");
        encoder.set_property_from_str("target", "bitrate");
        // Ordinary streamed MP3 uses its frame-count/index header for duration
        // and seeks before playback. Preserve that real contract in the fixture
        // rather than assuming a raw encoder output has known duration at the
        // first PAUSED/preroll frame.
        let index = gst::ElementFactory::make("xingmux")
            .build()
            .expect("the normal MP3 Xing header component exists");
        let sink = gst::ElementFactory::make("filesink")
            .property("location", path.to_str().unwrap())
            .property("sync", false)
            .build()
            .unwrap();
        pipeline
            .add_many([&source, &convert, &filter, &encoder, &index, &sink])
            .unwrap();
        gst::Element::link_many([&source, &convert, &filter, &encoder, &index, &sink]).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let completed = pipeline
            .bus()
            .unwrap()
            .timed_pop_filtered(
                gst::ClockTime::from_seconds(5),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            )
            .is_some_and(|message| matches!(message.view(), gst::MessageView::Eos(_)));
        let _ = pipeline.set_state(gst::State::Null);
        assert!(completed, "original synthetic MP3 encoding completed");
        let bytes = fs::read(path).unwrap();
        assert!(bytes.len() > 1_000 && bytes.len() < 1024 * 1024);
        Self {
            directory,
            bytes: Arc::new(bytes),
        }
    }
}

#[derive(Clone)]
enum Reply {
    Media(Arc<Vec<u8>>, &'static str, bool),
    Redirect(String),
    NotFound,
    Stall,
}

struct HttpFixture {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    requested: Arc<AtomicUsize>,
    ranged: Arc<AtomicUsize>,
    clean_headers: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<()>>,
}

impl HttpFixture {
    fn new(reply: Reply) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let requested = Arc::new(AtomicUsize::new(0));
        let ranged = Arc::new(AtomicUsize::new(0));
        let clean_headers = Arc::new(AtomicBool::new(true));
        let server = {
            let stop = stop.clone();
            let requested = requested.clone();
            let ranged = ranged.clone();
            let clean_headers = clean_headers.clone();
            thread::spawn(move || {
                let mut children = Vec::new();
                while !stop.load(Ordering::Acquire) {
                    let (stream, _) = match listener.accept() {
                        Ok(stream) => stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => panic!("synthetic HTTP listener failed"),
                    };
                    let reply = reply.clone();
                    let stop = stop.clone();
                    let requested = requested.clone();
                    let ranged = ranged.clone();
                    let clean_headers = clean_headers.clone();
                    children.push(thread::spawn(move || {
                        serve(stream, reply, &stop, &requested, &ranged, &clean_headers);
                    }));
                }
                for child in children {
                    child.join().unwrap();
                }
            })
        };
        Self {
            address,
            stop,
            requested,
            ranged,
            clean_headers,
            server: Some(server),
        }
    }

    fn uri(&self, marker: &str) -> String {
        format!("http://{}/get-mp3/{marker}", self.address)
    }

    fn source(&self) -> RemotePlaybackSource {
        RemotePlaybackSource::from_test_resolver(
            &self.uri("original-synthetic-resource"),
            Instant::now() + Duration::from_secs(60),
        )
        .unwrap()
    }
}

impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.server.take().unwrap().join().unwrap();
        if !thread::panicking() {
            assert!(
                self.clean_headers.load(Ordering::Acquire),
                "no session credentials, origin or referer reached the media fixture"
            );
        }
    }
}

fn serve(
    mut stream: TcpStream,
    reply: Reply,
    stop: &AtomicBool,
    requested: &AtomicUsize,
    ranged_requests: &AtomicUsize,
    clean_headers: &AtomicBool,
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut byte = [0];
    while !request.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => request.push(byte[0]),
            _ => return,
        }
        assert!(request.len() < 16 * 1024);
    }
    requested.fetch_add(1, Ordering::AcqRel);
    let request = String::from_utf8(request).unwrap();
    let lowered = request.to_ascii_lowercase();
    if lowered.lines().any(|line| {
        ["authorization:", "cookie:", "origin:", "referer:"]
            .iter()
            .any(|name| line.starts_with(name))
    }) {
        clean_headers.store(false, Ordering::Release);
    }
    let head = request.starts_with("HEAD ");
    assert!(head || request.starts_with("GET "));
    match reply {
        Reply::Redirect(target) => {
            let _ = write!(
                stream,
                "HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
        }
        Reply::NotFound => {
            let _ = write!(
                stream,
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
        }
        Reply::Stall => {
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: 100000\r\nConnection: close\r\n\r\n"
            );
            while !stop.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(10));
            }
        }
        Reply::Media(bytes, mime, set_cookie) => {
            let start = lowered
                .lines()
                .find_map(|line| line.strip_prefix("range: bytes="))
                .map(|range| range.split('-').next().unwrap().parse::<usize>().unwrap())
                .unwrap_or(0);
            if start > 0 {
                ranged_requests.fetch_add(1, Ordering::AcqRel);
            }
            if start >= bytes.len() {
                let _ = write!(
                    stream,
                    "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    bytes.len()
                );
                return;
            }
            let ranged = lowered.lines().any(|line| line.starts_with("range:"));
            let status = if ranged {
                "206 Partial Content"
            } else {
                "200 OK"
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\n",
                bytes.len() - start
            );
            if set_cookie {
                let _ = write!(
                    stream,
                    "Set-Cookie: karin-synthetic-cdn-cookie=present; Path=/; HttpOnly\r\n"
                );
            }
            if ranged {
                let _ = write!(
                    stream,
                    "Content-Range: bytes {start}-{}/{}\r\n",
                    bytes.len() - 1,
                    bytes.len()
                );
            }
            let _ = write!(stream, "Connection: close\r\n\r\n");
            if !head {
                if set_cookie && start == 0 {
                    // Keep the first response open after sufficient MP3/Xing
                    // data to preroll. Seeking beyond it must issue a genuine
                    // Range request in the same native session, not reuse a
                    // fully cached response or a fresh pipeline/session.
                    let end = bytes.len().min(8 * 1024);
                    let _ = stream.write_all(&bytes[..end]);
                    while !stop.load(Ordering::Acquire)
                        && ranged_requests.load(Ordering::Acquire) == 0
                    {
                        thread::sleep(Duration::from_millis(2));
                    }
                } else {
                    let _ = stream.write_all(&bytes[start..]);
                }
            }
        }
    }
}

fn controller() -> PlaybackController {
    PlaybackController::new(PlaybackOutput::Fakesink).unwrap()
}

fn prepare(controller: &PlaybackController, source: RemotePlaybackSource) -> PlaybackSnapshot {
    let generation = controller.snapshot().generation;
    controller.prepare(PlaybackSource::remote(source)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(6);
    while controller.snapshot().generation == generation {
        assert!(Instant::now() < deadline, "remote preparation was admitted");
        thread::sleep(POLL_INTERVAL);
    }
    wait_for(controller, |snapshot| {
        snapshot.state == PlaybackState::Ready && snapshot.seekable && snapshot.duration.is_some()
    })
    .expect("the original synthetic remote MP3 prerolls")
}

fn shutdown(controller: &PlaybackController) {
    controller.shutdown();
    wait_for_shutdown(controller).expect("remote worker releases network and native resources");
    assert!(
        controller
            .handle
            .shared
            .pipelines
            .lock()
            .unwrap()
            .iter()
            .all(|pipeline| pipeline.upgrade().is_none())
    );
}

fn wait_error(controller: &PlaybackController) -> PlaybackSnapshot {
    let deadline = Instant::now() + Duration::from_secs(7);
    loop {
        let snapshot = controller.snapshot();
        if snapshot.state == PlaybackState::Error {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "remote failure became a finite public error"
        );
        thread::sleep(POLL_INTERVAL);
    }
}

fn prepare_error(
    controller: &PlaybackController,
    source: RemotePlaybackSource,
) -> PlaybackSnapshot {
    let generation = controller.snapshot().generation;
    controller.prepare(PlaybackSource::remote(source)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(6);
    while controller.snapshot().generation == generation {
        assert!(
            Instant::now() < deadline,
            "new remote failure source was admitted"
        );
        thread::sleep(POLL_INTERVAL);
    }
    wait_error(controller)
}

#[test]
fn remote_source_is_sealed_bounded_expiring_and_redacted() {
    assert!(!permitted_media_host("s12éé.storage.yandex.net"));
    let uri = "https://s268myt.storage.yandex.net/get-mp3/synthetic-private-source-marker";
    let source =
        RemotePlaybackSource::from_resolver(uri, Instant::now() + Duration::from_secs(60)).unwrap();
    assert!(!source.is_expired());
    for debug in [
        format!("{source:?}"),
        format!("{:?}", PlaybackSource::remote(source)),
    ] {
        assert!(!debug.contains("synthetic-private-source-marker"));
        assert!(!debug.contains("storage.yandex.net"));
    }
    for bad in [
        "http://s268myt.storage.yandex.net/get-mp3/x",
        "https://s268myt.storage.yandex.net:444/get-mp3/x",
        "https://s268myt.storage.yandex.net.evil.invalid/get-mp3/x",
        "https://s268myt.storage.yandex.net./get-mp3/x",
        "https://s268myt.extra.storage.yandex.net/get-mp3/x",
        "https://storage.yandex.net/get-mp3/x",
        "https://sabc.storage.yandex.net/get-mp3/x",
        "https://s123456abc.storage.yandex.net/get-mp3/x",
        "https://127.0.0.1/get-mp3/x",
        "https://name@s268myt.storage.yandex.net/get-mp3/x",
        "https://@s268myt.storage.yandex.net/get-mp3/x",
        "https://s268myt.storage.yandex.net:/get-mp3/x",
        "https:///s268myt.storage.yandex.net/get-mp3/x",
        "https://s268myt.storage.yandex.net/get-mp3/",
        "https://s268myt.storage.yandex.net/get-mp3/x?credential=synthetic",
        "https://s268myt.storage.yandex.net/get-mp3/x#synthetic",
        "https://s268myt.storage.yandex.net/other/x",
        "https://s268myt.storage.yandex.net/get-mp3/../other",
        " https://s268myt.storage.yandex.net/get-mp3/x",
    ] {
        assert!(matches!(
            RemotePlaybackSource::from_resolver(bad, Instant::now() + Duration::from_secs(60)),
            Err(PlaybackError::SourcePolicy)
        ));
    }
    assert!(matches!(
        RemotePlaybackSource::from_resolver(uri, Instant::now() - Duration::from_secs(1)),
        Err(PlaybackError::SourceExpired)
    ));
    assert!(matches!(
        RemotePlaybackSource::from_resolver(uri, Instant::now() + Duration::from_secs(121)),
        Err(PlaybackError::SourceExpired)
    ));
}

#[test]
fn actual_remote_mp3_controls_seek_stop_replacement_eos_and_cleanup() {
    let media = SyntheticMp3::new(Duration::from_secs(3));
    let fixture = HttpFixture::new(Reply::Media(media.bytes.clone(), "audio/mpeg", false));
    let controller = controller();
    let ready = prepare(&controller, fixture.source());
    assert!(ready.seekable && ready.duration.is_some());
    controller.set_volume(0.2).unwrap();
    controller.play().unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Playing
            && snapshot
                .position
                .is_some_and(|position| position > Duration::from_millis(100))
    })
    .unwrap();
    controller.pause().unwrap();
    let paused = wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Paused
    })
    .unwrap();
    thread::sleep(Duration::from_millis(80));
    assert!(
        controller
            .snapshot()
            .position
            .unwrap()
            .abs_diff(paused.position.unwrap())
            < Duration::from_millis(30)
    );
    controller.seek(Duration::from_millis(1_000)).unwrap();
    controller.resume().unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Playing
            && snapshot
                .position
                .is_some_and(|position| position >= Duration::from_millis(950))
    })
    .unwrap();
    assert_eq!(controller.snapshot().volume, 0.2);
    let generation = controller.snapshot().generation;
    prepare(&controller, fixture.source());
    assert_eq!(controller.snapshot().generation, generation + 1);
    controller.stop().unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Stopped
    })
    .unwrap();
    assert!(
        controller
            .handle
            .shared
            .pipelines
            .lock()
            .unwrap()
            .iter()
            .all(|pipeline| pipeline.upgrade().is_none())
    );
    prepare(&controller, fixture.source());
    controller.play().unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Playing
    })
    .unwrap();
    let duration = controller.snapshot().duration.unwrap();
    controller
        .seek(duration.saturating_sub(Duration::from_millis(100)))
        .unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Ended
    })
    .unwrap();
    assert!(fixture.requested.load(Ordering::Acquire) > 0);
    shutdown(&controller);
}

#[test]
fn cdn_set_cookie_is_not_stored_or_replayed_on_a_later_range_request() {
    let media = SyntheticMp3::new(Duration::from_secs(3));
    let fixture = HttpFixture::new(Reply::Media(media.bytes.clone(), "audio/mpeg", true));
    let controller = controller();
    prepare(&controller, fixture.source());
    controller.seek(Duration::from_secs(2)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(6);
    while fixture.ranged.load(Ordering::Acquire) == 0 {
        assert!(
            Instant::now() < deadline,
            "seek issued a genuine second Range request"
        );
        assert_ne!(controller.snapshot().state, PlaybackState::Error);
        thread::sleep(POLL_INTERVAL);
    }
    assert!(fixture.requested.load(Ordering::Acquire) >= 2);
    assert!(
        fixture.clean_headers.load(Ordering::Acquire),
        "a CDN-issued cookie must not be replayed"
    );
    controller.play().unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Ended
    })
    .unwrap();
    shutdown(&controller);
}

#[test]
fn expired_admission_never_opens_network_and_active_playback_outlives_admission() {
    let media = SyntheticMp3::new(Duration::from_secs(5));
    let fixture = HttpFixture::new(Reply::Media(media.bytes.clone(), "audio/mpeg", false));
    let expired = RemotePlaybackSource::from_test_resolver(
        &fixture.uri("synthetic-expired"),
        Instant::now() + Duration::from_millis(5),
    )
    .unwrap();
    thread::sleep(Duration::from_millis(15));
    let controller = controller();
    assert_eq!(
        prepare_error(&controller, expired).last_error,
        Some(PlaybackError::SourceExpired)
    );
    assert_eq!(fixture.requested.load(Ordering::Acquire), 0);
    let source = RemotePlaybackSource::from_test_resolver(
        &fixture.uri("synthetic-active"),
        Instant::now() + Duration::from_secs(3),
    )
    .unwrap();
    let admission_deadline = source.expires_at;
    prepare(&controller, source);
    controller.play().unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Playing
    })
    .unwrap();
    while Instant::now() < admission_deadline + Duration::from_millis(80) {
        thread::sleep(POLL_INTERVAL);
    }
    assert_eq!(controller.snapshot().state, PlaybackState::Playing);
    controller.pause().unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Paused
    })
    .unwrap();
    controller.resume().unwrap();
    assert_eq!(
        wait_error(&controller).last_error,
        Some(PlaybackError::SourceExpired)
    );
    shutdown(&controller);
}

#[test]
fn redirects_manifests_and_network_failures_cannot_escape_or_expose_resource() {
    let target = HttpFixture::new(Reply::NotFound);
    let redirection = HttpFixture::new(Reply::Redirect(target.uri("must-not-be-requested")));
    let controller = controller();
    let failure = prepare_error(&controller, redirection.source());
    assert!(failure.last_error.is_some());
    assert_eq!(target.requested.load(Ordering::Acquire), 0);
    assert!(!format!("{failure:?}").contains("http"));
    let manifest = HttpFixture::new(Reply::Media(
        Arc::new(
            format!(
                "#EXTM3U\n#EXT-X-TARGETDURATION:3\n#EXTINF:3,\n{}\n",
                target.uri("manifest-must-not-follow")
            )
            .into_bytes(),
        ),
        "application/vnd.apple.mpegurl",
        false,
    ));
    assert!(
        prepare_error(&controller, manifest.source())
            .last_error
            .is_some()
    );
    assert!(manifest.requested.load(Ordering::Acquire) > 0);
    assert_eq!(target.requested.load(Ordering::Acquire), 0);
    let error = HttpFixture::new(Reply::NotFound);
    assert!(
        prepare_error(&controller, error.source())
            .last_error
            .is_some()
    );
    assert_eq!(target.requested.load(Ordering::Acquire), 0);
    shutdown(&controller);
}

#[test]
fn shutdown_cancels_a_stalled_remote_without_blocking_the_caller() {
    let fixture = HttpFixture::new(Reply::Stall);
    let controller = controller();
    controller
        .prepare(PlaybackSource::remote(fixture.source()))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture.requested.load(Ordering::Acquire) == 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(POLL_INTERVAL);
    }
    let started = Instant::now();
    controller.shutdown();
    assert!(started.elapsed() < Duration::from_millis(50));
    shutdown(&controller);
}

struct HttpsFixture {
    child: Child,
    address: String,
    media: SyntheticMp3,
}

impl HttpsFixture {
    fn new() -> Self {
        let media = SyntheticMp3::new(Duration::from_millis(1_200));
        let key = media.directory.0.join("generated-test-key.pem");
        let certificate = media.directory.0.join("generated-test-ca.pem");
        let status = std::process::Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-sha256",
                "-nodes",
                "-days",
                "1",
                "-subj",
                "/CN=localhost",
                "-addext",
                "subjectAltName=DNS:localhost,IP:127.0.0.1",
                "-addext",
                "basicConstraints=critical,CA:TRUE",
                "-keyout",
            ])
            .arg(&key)
            .arg("-out")
            .arg(&certificate)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "ephemeral test CA generation succeeded");
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
        let script = r#"
import http.server, ssl, sys
data = open(sys.argv[3], 'rb').read()
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_GET(self): self.respond(False)
    def do_HEAD(self): self.respond(True)
    def respond(self, head):
        if any(self.headers.get(h) for h in ('Authorization','Cookie','Origin','Referer')):
            self.send_error(400); return
        start = 0
        value = self.headers.get('Range')
        if value:
            try: start = int(value.removeprefix('bytes=').split('-')[0])
            except ValueError: self.send_error(416); return
        if start >= len(data): self.send_error(416); return
        self.send_response(206 if value else 200)
        self.send_header('Content-Type','audio/mpeg')
        self.send_header('Content-Length',str(len(data)-start))
        self.send_header('Accept-Ranges','bytes')
        self.send_header('Connection','close')
        if value: self.send_header('Content-Range',f'bytes {start}-{len(data)-1}/{len(data)}')
        self.end_headers()
        if not head:
            try: self.wfile.write(data[start:])
            except (BrokenPipeError, ConnectionResetError, ssl.SSLError): pass
server=http.server.HTTPServer(('127.0.0.1',0),Handler)
context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.minimum_version=ssl.TLSVersion.TLSv1_2
context.load_cert_chain(sys.argv[1],sys.argv[2])
server.socket=context.wrap_socket(server.socket,server_side=True)
print(server.server_address[1],flush=True)
server.serve_forever()
"#;
        let mut child = std::process::Command::new("python3")
            .args(["-u", "-c", script])
            .arg(&certificate)
            .arg(&key)
            .arg(media.directory.0.join("original-synthetic-signal.mp3"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line);
            let _ = sender.send(result.ok().and_then(|count| (count > 0).then_some(line)));
        });
        let port = receiver
            .recv_timeout(Duration::from_secs(5))
            .ok()
            .flatten()
            .and_then(|line| line.trim().parse::<u16>().ok());
        let Some(port) = port else {
            let _ = child.kill();
            let _ = child.wait();
            panic!("private TLS fixture failed to start");
        };
        Self {
            child,
            address: format!("https://127.0.0.1:{port}/get-mp3/original-synthetic-tls"),
            media,
        }
    }

    fn source(&self, trust_test_ca: bool) -> RemotePlaybackSource {
        let mut source = RemotePlaybackSource::new_admission(
            &self.address,
            Instant::now() + Duration::from_secs(60),
        )
        .unwrap();
        if trust_test_ca {
            source.ca_file = Some(self.media.directory.0.join("generated-test-ca.pem"));
        }
        source
    }
}

impl Drop for HttpsFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn https_transport_validates_its_certificate_and_plays_with_test_ca_only() {
    let fixture = HttpsFixture::new();
    let controller = controller();
    assert!(
        prepare_error(&controller, fixture.source(false))
            .last_error
            .is_some()
    );
    let ready = prepare(&controller, fixture.source(true));
    assert!(ready.duration.is_some());
    controller.play().unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Playing
    })
    .unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Ended
    })
    .unwrap();
    shutdown(&controller);
}

// The parent test below always exercises this branch in an isolated process.
// An ordinary workspace run does not recursively spawn it.
#[test]
fn remote_debug_output_child() {
    if std::env::var_os("KARIN_TEST_REMOTE_DEBUG_CHILD").is_none() {
        return;
    }
    let fixture = HttpFixture::new(Reply::NotFound);
    let source = RemotePlaybackSource::from_test_resolver(
        &fixture.uri("synthetic-private-signed-uri-canary"),
        Instant::now() + Duration::from_secs(60),
    )
    .unwrap();
    let controller = controller();
    controller.prepare(PlaybackSource::remote(source)).unwrap();
    let error = wait_error(&controller);
    assert!(error.last_error.is_some());
    shutdown(&controller);
}

#[test]
fn enabled_native_gstreamer_debug_never_prints_the_remote_uri_or_signed_path() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "playback::remote_tests::remote_debug_output_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("KARIN_TEST_REMOTE_DEBUG_CHILD", "1")
        .env("GST_DEBUG", "6")
        .env("GST_DEBUG_NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated native-debug privacy test completed"
    );
    for bytes in [&output.stdout, &output.stderr] {
        assert!(
            !String::from_utf8_lossy(bytes).contains("synthetic-private-signed-uri-canary"),
            "native output disclosed a synthetic private resource marker"
        );
    }
}
