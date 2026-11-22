// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit, self-contained executable acceptance composition. No live credentials.
//!
//! This process owns its numeric loopback listener and original test recording.
//! The ordinary UI, request parser, resolver and player handle every response.
use gstreamer::{self as gst, prelude::*};
use reqwest::Url;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

const TOKEN: &str = "karin-original-local-acceptance";

/// Can only be created with the listener owned by this explicit composition.
#[derive(Clone)]
pub(crate) struct Origin(Url);
impl Origin {
    pub(crate) fn url(&self) -> &Url {
        &self.0
    }
    pub(crate) fn accepts(&self, url: &Url) -> bool {
        url.scheme() == "http"
            && url.origin() == self.0.origin()
            && url.host_str() == Some("127.0.0.1")
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
    }
}

pub struct ApplicationFixture {
    pub(crate) origin: Origin,
    stop: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<()>>,
}
impl ApplicationFixture {
    pub fn new() -> Result<Self, &'static str> {
        // This diagnostic never reads a desktop profile or protected login.
        for key in [
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_STATE_HOME",
        ] {
            let path = std::env::var_os(key).ok_or("Fixture requires an isolated XDG profile")?;
            let path = std::path::Path::new(&path);
            if !path.is_absolute()
                || !path.is_dir()
                || path
                    .metadata()
                    .map_err(|_| "Fixture profile unavailable")?
                    .permissions()
                    .mode()
                    & 0o077
                    != 0
            {
                return Err("Fixture requires existing private XDG directories");
            }
        }
        let config = std::env::var_os("XDG_CONFIG_HOME").unwrap();
        let marker = std::path::Path::new(&config).join(".karin-original-fixture");
        if marker.exists() {
            if std::fs::read_to_string(&marker).ok().as_deref()
                != Some("original-local-fixture-v1\n")
            {
                return Err("Fixture profile marker is invalid");
            }
        } else {
            for key in [
                "XDG_CONFIG_HOME",
                "XDG_DATA_HOME",
                "XDG_CACHE_HOME",
                "XDG_STATE_HOME",
            ] {
                if std::fs::read_dir(std::env::var_os(key).unwrap())
                    .map_err(|_| "Fixture profile unavailable")?
                    .next()
                    .is_some()
                {
                    return Err("Fixture refuses an unrelated existing profile");
                }
            }
            std::fs::write(&marker, "original-local-fixture-v1\n")
                .map_err(|_| "Fixture profile marker failed")?;
            std::fs::set_permissions(&marker, std::fs::Permissions::from_mode(0o600))
                .map_err(|_| "Fixture profile marker failed")?;
        }
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| "Fixture listener failed")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "Fixture listener failed")?;
        let origin = Origin(
            Url::parse(&format!(
                "http://{}/",
                listener
                    .local_addr()
                    .map_err(|_| "Fixture listener failed")?
            ))
            .unwrap(),
        );
        let media = Arc::new(original_mp3()?);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_origin = origin.clone();
        let server = thread::spawn(move || {
            let mut workers = Vec::new();
            while !worker_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if workers.len() >= 240 {
                            break;
                        }
                        let media = media.clone();
                        let origin = worker_origin.clone();
                        workers.push(thread::spawn(move || {
                            let _ = serve(stream, &origin, &media);
                        }));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
            for worker in workers {
                let _ = worker.join();
            }
        });
        Ok(Self {
            origin,
            stop,
            server: Some(server),
        })
    }
    pub(crate) fn token(&self) -> crate::session::AccessToken {
        crate::session::AccessToken::new(zeroize::Zeroizing::new(TOKEN.into()), None)
    }
}
impl Drop for ApplicationFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.server.take() {
            let _ = worker.join();
        }
    }
}

fn original_mp3() -> Result<Vec<u8>, &'static str> {
    gst::init().map_err(|_| "Fixture GStreamer unavailable")?;
    let pipeline = gst::parse::launch("audiotestsrc num-buffers=3000 samplesperbuffer=441 volume=0.03 ! audioconvert ! audio/x-raw,rate=44100,channels=1 ! lamemp3enc target=bitrate bitrate=128 cbr=true ! appsink name=recording sync=false")
        .map_err(|_| "Fixture MP3 encoder unavailable")?.downcast::<gst::Pipeline>().map_err(|_| "Fixture recording failed")?;
    let sink = pipeline
        .by_name("recording")
        .ok_or("Fixture recording failed")?;
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|_| "Fixture recording failed")?;
    let mut media = Vec::new();
    while let Some(sample) = sink.emit_by_name::<Option<gst::Sample>>("pull-sample", &[]) {
        let Some(buffer) = sample.buffer() else {
            break;
        };
        let Ok(map) = buffer.map_readable() else {
            break;
        };
        media.extend_from_slice(map.as_slice());
        if media.len() > 1024 * 1024 {
            break;
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
    if !(10_000..1024 * 1024).contains(&media.len()) {
        return Err("Fixture recording failed");
    }
    Ok(media)
}
fn track(id: &str) -> Value {
    let title = if id == "46" {
        "Original river song"
    } else {
        "Original lantern song"
    };
    json!({
        "id": id,
        "title": title,
        "artists": [{"id": 11, "name": "Karin original recording"}],
        "albums": [{"id": 7, "title": "Original acceptance collection"}],
        "available": true,
        "durationMs": 30000
    })
}

fn reply(stream: &mut TcpStream, status: u16, content: &str, body: &[u8]) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Type: {content}\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)
}
fn serve(mut stream: TcpStream, origin: &Origin, media: &[u8]) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut head = Vec::new();
    let mut b = [0];
    while !head.ends_with(b"\r\n\r\n") && head.len() < 16384 {
        if stream.read(&mut b)? == 0 {
            return Ok(());
        }
        head.push(b[0]);
    }
    let head = String::from_utf8_lossy(&head);
    let mut first = head.lines().next().unwrap_or_default().split_whitespace();
    let method = first.next().unwrap_or_default();
    let path = first.next().unwrap_or_default();
    let Some(url) = origin
        .url()
        .join(path)
        .ok()
        .filter(|url| origin.accepts(url))
    else {
        return Ok(());
    };
    let header = |name: &str| {
        head.lines().skip(1).find_map(|line| {
            line.split_once(':')
                .filter(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.trim())
        })
    };
    let len = header("Content-Length")
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(0);
    if len > 16384 {
        return Ok(());
    }
    let mut body = vec![0; len];
    stream.read_exact(&mut body)?;
    let external = url.path() == "/external-lyrics";
    let unauthenticated = external
        || url.path().starts_with("/download-info/")
        || url.path().starts_with("/get-mp3/");
    if (!unauthenticated
        && header("Authorization") != Some(concat!("OAuth ", "karin-original-local-acceptance")))
        || (unauthenticated
            && ["Authorization", "Cookie"]
                .iter()
                .any(|n| header(n).is_some()))
    {
        return reply(&mut stream, 403, "application/json", b"{}");
    }
    let parts = url.path().split('/').collect::<Vec<_>>();
    let result = match url.path() {
        "/account/status" => {
            json!({
                "result": {"account": {
                    "uid": 1234,
                    "displayName": "Original local fixture"
                }}
            })
        }
        "/users/1234/likes/tracks" => {
            json!({
                "result": {"library": {
                    "uid": 1234, "revision": 1,
                    "tracks": [{"id": 46}, {"id": 47}]
                }}
            })
        }
        "/users/1234/dislikes/tracks" => {
            json!({
                "result": {"library": {"uid": 1234, "revision": 1, "tracks": []}}
            })
        }
        "/users/1234/playlists/list" => {
            json!({"result": [{
                "uid": 1234, "kind": 77,
                "title": "Original fixture playlist", "trackCount": 2
            }]})
        }
        "/users/1234/likes/playlists" => json!({"result":[]}),
        "/users/1234/playlists/77" => {
            json!({"result": {
                "uid": 1234, "kind": 77,
                "title": "Original fixture playlist", "trackCount": 2,
                "tracks": [{"id": 46}, {"id": 47}]
            }})
        }
        "/albums/7/with-tracks" => {
            json!({"result": {
                "id": 7, "title": "Original acceptance collection",
                "artists": [{"id": 11, "name": "Karin original recording"}],
                "volumes": [[track("46"), track("47")]]
            }})
        }
        "/tracks" => {
            let form = Url::parse(&format!(
                "http://fixture/?{}",
                String::from_utf8_lossy(&body)
            ))
            .unwrap();
            json!({"result":form.query_pairs().filter(|(k,_)|k=="track-ids").map(|(_,id)|track(&id)).collect::<Vec<_>>()})
        }
        "/search" => {
            json!({"result": {
                "tracks": {
                    "total": 2, "perPage": 20, "page": 0,
                    "results": [{"id": 46}, {"id": 47}]
                },
                "albums": {
                    "total": 1, "perPage": 20, "page": 0,
                    "results": [{
                        "id": 7, "title": "Original acceptance collection",
                        "artists": [{"id": 11, "name": "Karin original recording"}]
                    }]
                },
                "artists": {"total": 0, "perPage": 20, "page": 0, "results": []},
                "playlists": {"total": 0, "perPage": 20, "page": 0, "results": []}
            }})
        }
        "/external-lyrics" => {
            eprintln!("Karin executable fixture: request=lrclib credential-free=true");
            let query = url
                .query_pairs()
                .collect::<std::collections::BTreeMap<_, _>>();
            let lrc = concat!(
                "[00:00.000]Журавли\n",
                "[00:01.400]Фонари\n",
                "[00:03.300]Берёзы отражаются в воде\n",
                "[00:05.800]Свет над рекой\n",
                "[00:09.000]Над водой тишина\n",
                "[00:12.000]Дальний берег спит\n",
                "[00:16.000]Снова утро\n",
                "[00:20.000]Тает синий лёд\n",
                "[00:25.000]Ветви шепчут"
            );
            json!({
                "trackName": query.get("track_name"),
                "artistName": "Karin original recording",
                "duration": 30, "instrumental": false,
                "plainLyrics": "Original river text", "syncedLyrics": lrc
            })
        }
        _ if parts.len() == 4 && parts[1] == "tracks" && parts[3] == "supplement" => {
            eprintln!("Karin executable fixture: request=yandex-text synchronized=false");
            json!({"result": {"lyrics": {
                "hasRights": true,
                "fullLyrics": "Original river text\nOriginal untimed second line"
            }}})
        }
        _ if parts.len() == 4 && parts[1] == "tracks" && parts[3] == "similar" => {
            json!({"result":{"track":{"id":parts[2]},"similarTracks":[]}})
        }
        _ if parts.len() == 4 && parts[1] == "tracks" && parts[3] == "download-info" => {
            let descriptor = origin
                .url()
                .join(&format!("download-info/{}", parts[2]))
                .unwrap();
            json!({"result": [{
                "codec": "mp3", "bitrateInKbps": 128,
                "preview": false, "direct": false,
                "downloadInfoUrl": descriptor.as_str()
            }]})
        }
        _ if url.path().starts_with("/download-info/") => {
            let xml=b"<download-info><host>s268myt.storage.yandex.net</host><path>/original-recording.mp3</path><ts>1700000000</ts><s>original-fixture</s></download-info>";
            return reply(&mut stream, 200, "application/xml", xml);
        }
        _ if url.path().starts_with("/get-mp3/") => {
            eprintln!("Karin executable fixture: request=original-audio credential-free=true");
            if let Some(range) = header("Range")
                .and_then(|s| s.strip_prefix("bytes="))
                .and_then(|s| s.split_once('-'))
            {
                let start = range.0.parse::<usize>().unwrap_or(0).min(media.len());
                let end = range
                    .1
                    .parse::<usize>()
                    .unwrap_or(media.len() - 1)
                    .min(media.len() - 1);
                if start > end {
                    return reply(&mut stream, 416, "audio/mpeg", &[]);
                }
                write!(
                    stream,
                    "HTTP/1.1 206 Fixture\r\nContent-Type: audio/mpeg\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {start}-{end}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    media.len(),
                    end - start + 1
                )?;
                if method != "HEAD" {
                    stream.write_all(&media[start..=end])?;
                }
                return Ok(());
            }
            return reply(&mut stream, 200, "audio/mpeg", media);
        }
        _ => json!({"result":[]}),
    };
    reply(
        &mut stream,
        200,
        "application/json",
        result.to_string().as_bytes(),
    )
}

/// This composition has no access to the desktop credential backend.
pub(crate) struct NoCredentials;
impl crate::credentials::CredentialBackend for NoCredentials {
    fn probe(
        &self,
        _: &crate::credentials::Cancellation,
    ) -> Result<crate::credentials::BackendKind, crate::credentials::StorageError> {
        Err(crate::credentials::StorageError::Unavailable)
    }
    fn load(
        &self,
        _: &crate::credentials::Cancellation,
    ) -> Result<Option<crate::session::AccessToken>, crate::credentials::StorageError> {
        Err(crate::credentials::StorageError::Unavailable)
    }
    fn save_validated(
        &self,
        _: &crate::session::AccessToken,
        _: &crate::credentials::Cancellation,
    ) -> Result<(), crate::credentials::StorageError> {
        Err(crate::credentials::StorageError::Unavailable)
    }
    fn delete(
        &self,
        _: &crate::credentials::Cancellation,
    ) -> Result<(), crate::credentials::StorageError> {
        Ok(())
    }
}
