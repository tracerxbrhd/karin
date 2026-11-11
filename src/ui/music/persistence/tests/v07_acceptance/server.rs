// SPDX-License-Identifier: GPL-3.0-or-later
//! Counted local Music/legacy delivery contracts; no real account or content.
use super::*;

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct Counts {
    pub account: u64,
    pub track_batches: u64,
    pub playlist_reads: u64,
    pub offers: u64,
    pub descriptors: u64,
    pub heads: u64,
    pub media_gets: u64,
    pub background_gets: u64,
    pub media_bytes: u64,
    pub changes: u64,
    pub renames: u64,
    pub visibility: u64,
    pub deletes: u64,
}
#[derive(Clone)]
pub(super) struct Mode {
    pub account: u64,
    pub unavailable: bool,
    pub deleted: bool,
    pub title: String,
    pub visibility: String,
    pub revision: u64,
    pub entries: Vec<(String, String)>,
    pub hold_restore: Option<PathBuf>,
}
impl Default for Mode {
    fn default() -> Self {
        Self {
            hold_restore: None,
            account: 1234,
            unavailable: false,
            deleted: false,
            title: "Disposable original playlist".into(),
            visibility: "private".into(),
            revision: 10,
            entries: vec![
                ("42".into(), "7".into()),
                ("43".into(), "7".into()),
                ("42".into(), "8".into()),
                ("44".into(), "7".into()),
            ],
        }
    }
}
pub(super) struct Server {
    pub origin: String,
    pub mode: Arc<Mutex<Mode>>,
    pub counts: Arc<Mutex<Counts>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}
impl Server {
    pub fn new(media: Vec<u8>) -> Self {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let origin = format!("http://{}/", socket.local_addr().unwrap());
        let counts = Arc::new(Mutex::new(Counts::default()));
        let mode = Arc::new(Mutex::new(Mode::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let handle = {
            let counts = counts.clone();
            let mode = mode.clone();
            let stop = stop.clone();
            let origin = origin.clone();
            let media = Arc::new(media);
            thread::spawn(move || {
                let mut handlers = Vec::new();
                while !stop.load(Ordering::Acquire) {
                    match socket.accept() {
                        Ok((stream, _)) => {
                            assert!(handlers.len() < 512, "bounded native fixture requests");
                            let counts = counts.clone();
                            let mode = mode.clone();
                            let origin = origin.clone();
                            let media = media.clone();
                            handlers.push(thread::spawn(move || {
                                serve(stream, &origin, &media, &counts, &mode)
                            }));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(_) => panic!("local application listener failed"),
                    }
                }
                for handler in handlers {
                    handler.join().unwrap();
                }
            })
        };
        Self {
            origin,
            counts,
            mode,
            stop,
            handle: Some(handle),
        }
    }
    pub fn counts(&self) -> Counts {
        self.counts.lock().unwrap().clone()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.handle.take().unwrap().join().unwrap();
    }
}
fn track(id: &str, available: bool) -> serde_json::Value {
    let bare = id.split(':').next().unwrap();
    let album = id.split(':').nth(1).unwrap_or("7");
    json!({"id":bare,"title":format!("Original audio {bare}"),"artists":[{"id":7,"name":"Karin original fixture"}],"albums":[{"id":album,"title":"Original CC0 collection"}],"durationMs":30000,"available":available})
}
fn playlist(mode: &Mode, partial: bool, rich: bool) -> serde_json::Value {
    let entries = if partial {
        (0..220)
            .map(|index| ((42 + index % 3).to_string(), "7".into()))
            .collect::<Vec<_>>()
    } else {
        mode.entries.clone()
    };
    let tracks = entries
        .iter()
        .map(|(id, album)| {
            let mut value = json!({"id":id,"albumId":album});
            if rich {
                value["track"] = track(&format!("{id}:{album}"), !(mode.unavailable && id == "42"));
            }
            value
        })
        .collect::<Vec<_>>();
    json!({"owner":{"uid":1234},"kind":if partial {78}else{77},"title":mode.title,"revision":mode.revision,"visibility":mode.visibility,"trackCount":tracks.len(),"tracks":tracks})
}
fn serve(
    mut stream: TcpStream,
    origin: &str,
    media: &[u8],
    counts: &Mutex<Counts>,
    mode: &Mutex<Mode>,
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            return;
        }
        bytes.push(byte[0]);
        assert!(bytes.len() < 16384);
    }
    let head = String::from_utf8(bytes).unwrap();
    let line = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    let method = line[0];
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{}", line[1])).unwrap();
    let header = |name: &str| {
        head.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then_some(value.trim())
        })
    };
    if url.path() == "/__fixture/counts" {
        assert_eq!(method, "GET");
        write_response(
            &mut stream,
            "200 OK",
            "application/json",
            &serde_json::to_vec(&*counts.lock().unwrap()).unwrap(),
        );
        return;
    }
    let transport =
        url.path().starts_with("/download-info/") || url.path().starts_with("/get-mp3/");
    if transport {
        for name in ["authorization", "cookie", "origin", "referer"] {
            assert!(header(name).is_none());
        }
    } else {
        assert_eq!(
            header("authorization"),
            Some("OAuth synthetic-storage-ui-token")
        );
    }
    let length = header("content-length").map_or(0, |value| value.parse::<usize>().unwrap());
    assert!(length < 32768);
    let mut body = vec![0; length];
    if length > 0 {
        stream.read_exact(&mut body).unwrap();
    }
    let form_url = reqwest::Url::parse(&format!(
        "http://127.0.0.1/?{}",
        String::from_utf8(body).unwrap()
    ))
    .unwrap();
    let values = form_url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    let field = |name: &str| {
        values
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    let mut current = mode.lock().unwrap();
    let path = url.path();
    let response = match path {
        "/account/status" => {
            counts.lock().unwrap().account += 1;
            json!({"result":{"account":{"uid":current.account,"displayName":"Synthetic restart account"}}})
        }
        path if path.ends_with("/likes/tracks") => {
            json!({"result":{"library":{"uid":current.account,"revision":1,"tracks":[{"id":42}]}}})
        }
        path if path.ends_with("/dislikes/tracks") => {
            json!({"result":{"library":{"uid":current.account,"revision":1,"tracks":[]}}})
        }
        path if path.ends_with("/likes/playlists") => json!({"result":[]}),
        path if path.ends_with("/playlists/list") => {
            let all = if current.account != 1234 {
                Vec::new()
            } else if current.deleted {
                vec![playlist(&current, true, false)]
            } else {
                vec![
                    playlist(&current, false, false),
                    playlist(&current, true, false),
                ]
            };
            json!({"result":all})
        }
        "/tracks" => {
            if let Some(directory) = current.hold_restore.take() {
                fs::write(directory.join("restore-blocked"), b"waiting").unwrap();
                drop(current);
                let deadline = Instant::now() + Duration::from_secs(8);
                while !directory.join("restore-release").exists() {
                    assert!(
                        Instant::now() < deadline,
                        "local restore response gate exceeded eight seconds"
                    );
                    thread::sleep(Duration::from_millis(2));
                }
                current = mode.lock().unwrap();
            }
            assert_eq!(method, "POST");
            counts.lock().unwrap().track_batches += 1;
            let mut ids = values
                .iter()
                .filter(|(key, _)| key == "track-ids")
                .map(|(_, id)| id.clone())
                .collect::<Vec<_>>();
            ids.sort();
            ids.dedup();
            assert!(!ids.is_empty() && ids.len() <= 50);
            json!({"result":ids.iter().map(|id|track(id,!(current.unavailable && id.split(':').next()==Some("42")))).collect::<Vec<_>>()})
        }
        "/users/1234/playlists/77" | "/users/1234/playlists/78" => {
            assert_eq!(method, "GET");
            counts.lock().unwrap().playlist_reads += 1;
            if path.ends_with("/77") && current.deleted {
                write_response(&mut stream, "404 Not Found", "application/json", b"{}");
                return;
            }
            json!({"result":playlist(&current,path.ends_with("/78"),true)})
        }
        "/users/1234/playlists/77/change" => {
            assert_eq!(method, "POST");
            assert!(!current.deleted);
            assert_eq!(field("kind"), Some("77"));
            assert_eq!(
                field("revision").unwrap().parse::<u64>().unwrap(),
                current.revision
            );
            let diff: serde_json::Value = serde_json::from_str(field("diff").unwrap()).unwrap();
            let operations = diff.as_array().unwrap();
            assert!(!operations.is_empty() && operations.len() <= 2);
            for op in operations {
                match op["op"].as_str().unwrap() {
                    "delete" => {
                        let from = op["from"].as_u64().unwrap() as usize;
                        let to = op["to"].as_u64().unwrap() as usize;
                        assert_eq!(to, from + 1);
                        current.entries.drain(from..to);
                    }
                    "insert" => {
                        let at = op["at"].as_u64().unwrap() as usize;
                        let entries = op["tracks"].as_array().unwrap();
                        assert_eq!(entries.len(), 1);
                        let entry = &entries[0];
                        current.entries.insert(
                            at,
                            (
                                entry["id"].as_str().unwrap().into(),
                                entry["albumId"].as_str().unwrap().into(),
                            ),
                        );
                    }
                    _ => panic!("unknown playlist write"),
                }
            }
            counts.lock().unwrap().changes += 1;
            current.revision += 1;
            json!({"result":playlist(&current,false,true)})
        }
        "/users/1234/playlists/77/name" => {
            assert_eq!(method, "POST");
            current.title = field("value").unwrap().into();
            current.revision += 1;
            counts.lock().unwrap().renames += 1;
            json!({"result":playlist(&current,false,true)})
        }
        "/users/1234/playlists/77/visibility" => {
            assert_eq!(method, "POST");
            let value = field("value").unwrap();
            assert!(matches!(value, "private" | "public"));
            current.visibility = value.into();
            current.revision += 1;
            counts.lock().unwrap().visibility += 1;
            json!({"result":playlist(&current,false,true)})
        }
        "/users/1234/playlists/77/delete" => {
            assert_eq!(method, "POST");
            current.deleted = true;
            counts.lock().unwrap().deletes += 1;
            json!({"result":"ok"})
        }
        path if path.starts_with("/tracks/") && path.ends_with("/download-info") => {
            counts.lock().unwrap().offers += 1;
            let id = path
                .trim_start_matches("/tracks/")
                .trim_end_matches("/download-info")
                .replace(':', "-");
            json!({"result":[{"codec":"mp3","bitrateInKbps":128,"preview":false,"direct":false,"downloadInfoUrl":format!("{origin}download-info/v07/{id}/128?sign={}&ts=65600000","a".repeat(64))}]})
        }
        path if path.starts_with("/download-info/v07/") => {
            counts.lock().unwrap().descriptors += 1;
            let id = path
                .trim_start_matches("/download-info/v07/")
                .trim_end_matches("/128");
            let xml = format!(
                "<download-info><host>s268myt.storage.yandex.net</host><path>/original-v07-{id}.mp3</path><ts>1700000000</ts><s>original-fixture-input</s></download-info>"
            );
            write_response(&mut stream, "200 OK", "application/xml", xml.as_bytes());
            return;
        }
        path if path.starts_with("/get-mp3/") => {
            drop(current);
            let range = header("range").map(|value| {
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
            {
                let mut count = counts.lock().unwrap();
                if method == "HEAD" {
                    count.heads += 1;
                } else {
                    count.media_gets += 1;
                    count.media_bytes += (end - start + 1) as u64;
                    if header("cache-control") == Some("no-cache") {
                        count.background_gets += 1;
                    }
                }
            }
            let _ = write!(
                stream,
                "HTTP/1.1 {}\r\nContent-Type: audio/mpeg\r\nCache-Control: private, max-age=3600\r\nDate: {}\r\nETag: \"original-v07-fixture-v1\"\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\nConnection: close\r\n",
                if range.is_some() {
                    "206 Partial Content"
                } else {
                    "200 OK"
                },
                http_date(),
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
            if method != "HEAD" {
                let _ = stream.write_all(&media[start..=end]);
            }
            return;
        }
        _ => panic!("unexpected v0.7 fixture route"),
    };
    write_response(
        &mut stream,
        "200 OK",
        "application/json",
        response.to_string().as_bytes(),
    );
}
fn http_date() -> String {
    let date = glib::DateTime::now_utc().unwrap();
    let day = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        [usize::try_from(date.day_of_week() - 1).unwrap()];
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][usize::try_from(date.month() - 1).unwrap()];
    format!(
        "{day}, {:02} {month} {} {:02}:{:02}:{:02} GMT",
        date.day_of_month(),
        date.year(),
        date.hour(),
        date.minute(),
        date.second()
    )
}
fn write_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}
