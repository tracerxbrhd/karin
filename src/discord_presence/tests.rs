// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        fs::DirBuilderExt,
        net::{UnixListener, UnixStream},
    },
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::{Value, json};

fn track() -> Track {
    Track {
        id: "82".into(),
        title: "Название".into(),
        artists: vec!["Исполнитель".into()],
        artist_links: Vec::new(),
        album_id: Some("19".into()),
        album_title: Some("Альбом".into()),
        duration_ms: Some(182_000),
        cover: crate::artwork::CoverArt::parse("avatars.yandex.net/get-music-content/123/def/%%"),
        explicit: false,
        available: true,
    }
}

fn activity(position: u64) -> Activity {
    Activity::new(
        &track(),
        Progress::Playing {
            position: Some(Duration::from_secs(position)),
            duration: Some(Duration::from_secs(182)),
        },
        UNIX_EPOCH + Duration::from_secs(1_000),
    )
    .unwrap()
}

#[test]
fn listening_activity_contains_only_public_catalog_fields() {
    let value = serde_json::to_value(activity(42)).unwrap();
    assert_eq!(
        value,
        json!({
            "type": 2,
            "details": "Название",
            "details_url": "https://music.yandex.ru/album/19/track/82",
            "state": "Исполнитель · Альбом",
            "timestamps": {"start": 958, "end": 1140},
            "assets": {
                "large_image": "https://avatars.yandex.net/get-music-content/123/def/1000x1000",
            },
        })
    );
    let mut incomplete = track();
    incomplete.cover =
        crate::artwork::CoverArt::parse("avatars.yandex.net/get-music-user-playlist/123/abc/%%?7");
    let paused =
        serde_json::to_value(Activity::new(&incomplete, Progress::Paused, UNIX_EPOCH).unwrap())
            .unwrap();
    assert!(paused.get("timestamps").is_none());
    assert!(paused.get("assets").is_none());
    assert_eq!(paused["state"], "Пауза · Исполнитель · Альбом");
    assert_eq!(paused["details_url"], value["details_url"]);
    incomplete.album_title = Some(incomplete.title.clone());
    let single = Activity::new(&incomplete, Progress::Paused, UNIX_EPOCH).unwrap();
    assert_eq!(single.state, "Пауза · Исполнитель");
    incomplete.artists.clear();
    let single = Activity::new(&incomplete, Progress::Paused, UNIX_EPOCH).unwrap();
    assert_eq!(single.state, "Пауза");
    let playing = Activity::new(
        &incomplete,
        Progress::Playing {
            position: None,
            duration: None,
        },
        UNIX_EPOCH,
    )
    .unwrap();
    assert!(
        serde_json::to_value(playing)
            .unwrap()
            .get("state")
            .is_none()
    );
    incomplete.album_id = None;
    assert!(Activity::new(&incomplete, Progress::Paused, UNIX_EPOCH).is_none());
}

#[test]
fn compound_track_selectors_require_matching_catalog_album_metadata() {
    let mut track = track();
    track.id = "82:19".into();
    let activity = Activity::new(&track, Progress::Paused, UNIX_EPOCH).unwrap();
    assert_eq!(
        activity.details_url,
        "https://music.yandex.ru/album/19/track/82"
    );

    for album in [None, Some("20"), Some("19?private=true")] {
        track.album_id = album.map(str::to_owned);
        assert!(Activity::new(&track, Progress::Paused, UNIX_EPOCH).is_none());
    }
    track.album_id = Some("19".into());
    for selector in ["0:19", "upload:19", "82:19:20", "82:19?private=true"] {
        track.id = selector.into();
        assert!(Activity::new(&track, Progress::Paused, UNIX_EPOCH).is_none());
    }
}

#[test]
fn sandbox_socket_discovery_is_confined_to_the_granted_paths() {
    let runtime = Path::new("/run/user/1000");
    let tmp = Path::new("/tmp");
    let expected = [
        runtime.join("discord-ipc-0"),
        runtime.join("app/com.discordapp.Discord/discord-ipc-0"),
    ];
    assert_eq!(socket_paths(Some(runtime), tmp, true), expected);
    assert!(socket_paths(None, tmp, true).is_empty());
    let native = socket_paths(Some(runtime), tmp, false);
    assert_eq!(native.len(), 11);
    assert_eq!(native[0], expected[0]);
    assert_eq!(native[9], runtime.join("discord-ipc-9"));
    assert_eq!(native[10], expected[1]);
    assert_eq!(socket_paths(None, tmp, false)[0], tmp.join("discord-ipc-0"));
}

struct SocketFixture(PathBuf);

impl SocketFixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = env::temp_dir().join(format!(
            "karin-ipc-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> PathBuf {
        self.0.join("rpc")
    }

    fn listen(&self) -> UnixListener {
        let listener = UnixListener::bind(self.path()).unwrap();
        listener.set_nonblocking(true).unwrap();
        listener
    }
}

impl Drop for SocketFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn until<T>(mut check: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(7);
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(Instant::now() < deadline, "Discord fixture timed out");
        thread::sleep(Duration::from_millis(10));
    }
}

fn accept(listener: &UnixListener) -> UnixStream {
    until(|| match listener.accept() {
        Ok((stream, _)) => {
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            Some(stream)
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => None,
        Err(error) => panic!("{error}"),
    })
}

fn send(stream: &mut UnixStream, opcode: u32, payload: &Value) {
    let payload = serde_json::to_vec(payload).unwrap();
    stream.write_all(&opcode.to_le_bytes()).unwrap();
    stream
        .write_all(&(payload.len() as u32).to_le_bytes())
        .unwrap();
    stream.write_all(&payload).unwrap();
}

fn receive(stream: &mut UnixStream) -> (u32, Value) {
    let mut header = [0; 8];
    stream.read_exact(&mut header).unwrap();
    let opcode = u32::from_le_bytes(header[..4].try_into().unwrap());
    let length = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
    assert!(length <= 4096);
    let mut payload = vec![0; length];
    stream.read_exact(&mut payload).unwrap();
    (opcode, serde_json::from_slice(&payload).unwrap())
}

fn ready(stream: &mut UnixStream) {
    assert_eq!(
        receive(stream),
        (0, json!({"v":1,"client_id":APPLICATION_ID}))
    );
    let pulse = json!({"synthetic_ping":1});
    send(stream, 3, &pulse);
    assert_eq!(receive(stream), (4, pulse));
    send(
        stream,
        1,
        &json!({"cmd":"DISPATCH","evt":"READY","data":{"user":{"id":"127"}}}),
    );
}

fn take_activity(stream: &mut UnixStream) -> Value {
    let (opcode, request) = receive(stream);
    assert_eq!(opcode, 1);
    assert_eq!(request["cmd"], "SET_ACTIVITY");
    assert_eq!(request["args"]["pid"], std::process::id());
    send(
        stream,
        1,
        &json!({"cmd":"SET_ACTIVITY","nonce":request["nonce"],"data":[]}),
    );
    request["args"]["activity"].clone()
}

#[test]
fn local_rpc_resumes_after_late_start_and_restart_then_clears() {
    let fixture = SocketFixture::new();
    let mut presence = Presence::default();
    assert_eq!(presence.status(), Status::Disabled);
    assert!(presence.worker.is_none());
    let shared = presence.shared.clone();
    let path = fixture.path();
    presence.worker = Some(thread::spawn(move || run(&shared, &[path], false)));

    assert!(presence.enable(true));
    until(|| (presence.status() == Status::Unavailable).then_some(()));
    let listener = fixture.listen();
    presence.update(Some(activity(42)));
    let mut stream = accept(&listener);
    ready(&mut stream);
    let playing = take_activity(&mut stream);
    assert_eq!(playing["timestamps"]["start"], 958);
    assert!(playing.get("buttons").is_none());
    assert_eq!(
        playing["details_url"],
        "https://music.yandex.ru/album/19/track/82"
    );
    assert_eq!(presence.status(), Status::Connected);

    // Identical playback observations never become another RPC frame.
    for _ in 0..20 {
        presence.update(Some(activity(42)));
    }
    stream
        .set_read_timeout(Some(Duration::from_millis(250)))
        .unwrap();
    assert_eq!(
        stream.read(&mut [0; 1]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    presence.update(Activity::new(&track(), Progress::Paused, UNIX_EPOCH));
    let paused = take_activity(&mut stream);
    assert!(paused.get("timestamps").is_none());
    assert!(paused["state"].as_str().unwrap().starts_with("Пауза"));

    drop(stream);
    drop(listener);
    fs::remove_file(fixture.path()).unwrap();
    until(|| (presence.status() == Status::Unavailable).then_some(()));
    let listener = fixture.listen();
    presence.update(Some(activity(19)));
    let mut stream = accept(&listener);
    ready(&mut stream);
    assert_eq!(take_activity(&mut stream)["timestamps"]["start"], 981);

    assert!(presence.enable(false));
    assert_eq!(presence.status(), Status::Disabled);
    assert_eq!(take_activity(&mut stream), Value::Null);
    assert_eq!(stream.read(&mut [0; 1]).unwrap(), 0);
    assert!(listener.accept().is_err());

    assert!(presence.enable(true));
    let mut stream = accept(&listener);
    ready(&mut stream);
    presence.update(Some(activity(3)));
    assert_eq!(take_activity(&mut stream)["timestamps"]["start"], 997);
    presence.update(None);
    assert_eq!(take_activity(&mut stream), Value::Null);
    presence.update(Some(activity(90)));
    stream
        .set_read_timeout(Some(ACTIVITY_INTERVAL + Duration::from_secs(2)))
        .unwrap();
    assert_eq!(take_activity(&mut stream)["timestamps"]["start"], 910);
    presence.retire_activity();
    presence.update(Some(activity(99)));
    assert_eq!(take_activity(&mut stream), Value::Null);
    assert_eq!(stream.read(&mut [0; 1]).unwrap(), 0);
    let mut stream = accept(&listener);
    ready(&mut stream);
    assert_eq!(take_activity(&mut stream)["timestamps"]["start"], 901);
    let worker = presence.shutdown().unwrap();
    assert_eq!(take_activity(&mut stream), Value::Null);
    worker.join().unwrap();
}

#[test]
fn fragmented_idle_ping_survives_poll_deadlines() {
    let fixture = SocketFixture::new();
    let listener = fixture.listen();
    let peer = thread::spawn(move || {
        let mut stream = accept(&listener);
        ready(&mut stream);
        stream
    });
    let mut connection = Connection::open(&fixture.path()).unwrap();
    let mut peer = peer.join().unwrap();

    let pulse = json!({"fragmented":true});
    let payload = serde_json::to_vec(&pulse).unwrap();
    let mut frame = Vec::new();
    frame.extend_from_slice(&3_u32.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&payload);

    // Each incomplete fragment is deliberately held until the caller's idle
    // deadline passes, independently of thread scheduling.
    peer.write_all(&frame[..3]).unwrap();
    connection.poll().unwrap();
    peer.write_all(&frame[3..9]).unwrap();
    connection.poll().unwrap();
    peer.write_all(&frame[9..]).unwrap();
    connection.poll().unwrap();
    assert_eq!(receive(&mut peer), (4, pulse));
}

#[test]
fn invalid_peer_frame_is_bounded_and_never_connects() {
    let fixture = SocketFixture::new();
    let listener = fixture.listen();
    let worker = thread::spawn(move || {
        let mut stream = accept(&listener);
        let _ = receive(&mut stream);
        stream.write_all(&1_u32.to_le_bytes()).unwrap();
        stream.write_all(&u32::MAX.to_le_bytes()).unwrap();
    });
    let error = Connection::open(&fixture.path()).err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    worker.join().unwrap();
}

#[test]
fn rejected_handshake_exposes_only_a_fixed_error_category() {
    let fixture = SocketFixture::new();
    let listener = fixture.listen();
    let worker = thread::spawn(move || {
        let mut stream = accept(&listener);
        let _ = receive(&mut stream);
        send(
            &mut stream,
            1,
            &json!({"evt":"ERROR","data":{"code":4000,"message":"synthetic account detail"}}),
        );
    });
    let error = Connection::open(&fixture.path()).err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(error.get_ref().is_none());
    worker.join().unwrap();
}
