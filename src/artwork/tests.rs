// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, mpsc},
    thread,
};

use super::*;

fn cover() -> CoverArt {
    CoverArt::parse("avatars.yandex.net/get-music-content/synthetic/image/%%")
        .expect("synthetic public cover")
}

fn png() -> Vec<u8> {
    png_size(8)
}

fn png_size(size: i32) -> Vec<u8> {
    let pixbuf =
        gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, size, size)
            .expect("synthetic pixels");
    pixbuf.fill(0x4466_88ff);
    pixbuf
        .save_to_bufferv("png", &[])
        .expect("synthetic PNG encoding")
}

fn loader() -> ArtworkLoader {
    ArtworkLoader::with_client(
        Client::builder()
            .no_proxy()
            // Even a GTK cache regression can only hit loopback.
            .resolve("avatars.yandex.net", "127.0.0.1:443".parse().unwrap())
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(2))
            .build()
            .expect("local client"),
    )
    .expect("local cover loader")
}

pub(super) fn cached_fixture() -> (ArtworkLoader, CoverArt) {
    let (url, server) = fixture(3, 200, "image/png", png_size(512));
    let loader = loader();
    let cover = cover();
    for pixels in [128, 256, 512] {
        loader
            .load_from(&cover, pixels, loader.epoch(), url.clone())
            .unwrap();
    }
    server.join().unwrap();
    (loader, cover)
}

fn fixture(
    count: usize,
    status: u16,
    mime: &'static str,
    bytes: Vec<u8>,
) -> (Url, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("local fixture listener");
    let url = Url::parse(&format!(
        "http://{}/synthetic",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let worker = thread::spawn(move || {
        for _ in 0..count {
            let (mut stream, _) = listener.accept().expect("local request");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut received = Vec::new();
            let mut buffer = [0; 512];
            while !received.windows(4).any(|value| value == b"\r\n\r\n") {
                let n = stream.read(&mut buffer).expect("bounded request");
                assert!(n > 0 && received.len() < 8192);
                received.extend_from_slice(&buffer[..n]);
            }
            let headers = String::from_utf8(received).unwrap().to_ascii_lowercase();
            assert!(!headers.contains("authorization:"));
            assert!(!headers.contains("cookie:"));
            assert!(!headers.contains("referer:"));
            write!(
                stream,
                "HTTP/1.1 {status} Fixture\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .unwrap();
            // Rejected headers/oversized bodies are deliberately not consumed.
            let _ = stream.write_all(&bytes);
        }
    });
    (url, worker)
}

#[test]
fn cover_accepts_only_bounded_exact_public_templates() {
    let parsed = cover();
    assert_eq!(
        parsed.url(256).unwrap().as_str(),
        "https://avatars.yandex.net/get-music-content/synthetic/image/256x256"
    );
    assert_eq!(
        Some(parsed.clone()),
        CoverArt::parse("https://avatars.yandex.net/get-music-content/synthetic/image/%%")
    );
    for raw in [
        "http://avatars.yandex.net/get-music/synthetic/%%",
        "https://avatars.yandex.net.evil.invalid/get-music/synthetic/%%",
        "https://evil.invalid/get-music/synthetic/%%",
        "https://avatars.yandex.net:443/get-music/synthetic/%%",
        "https://token@avatars.yandex.net/get-music/synthetic/%%",
        "https://avatars.yandex.net/get-music/synthetic/%%?token=private",
        "https://avatars.yandex.net/get-music/synthetic/%%#private",
        "https://avatars.yandex.net/get-music/../%%",
        "https://avatars.yandex.net/get-music/%2e%2e/%%",
        "https://avatars.yandex.net/get-music//%%",
        "https://avatars.yandex.net/get-music/\\private/%%",
        "https://avatars.yandex.net/get-music/личное/%%",
        "https://avatars.yandex.net/get-music/private/1000x1000",
        "https://avatars.yandex.net/%%",
        "data:image/png;base64,private",
        "file:///private/%%",
    ] {
        assert!(
            CoverArt::parse(raw).is_none(),
            "untrusted template accepted"
        );
    }
    assert!(
        CoverArt::parse(&format!(
            "avatars.yandex.net/get-music/{}/%%",
            "x".repeat(257)
        ))
        .is_none()
    );
    assert_eq!(pixel_bucket(48), 128);
    assert_eq!(pixel_bucket(144), 256);
    assert_eq!(pixel_bucket(384), 512);
    assert_eq!(pixel_bucket(i32::MAX), 512);
    assert!(!format!("{parsed:?}").contains("synthetic"));
}

#[test]
fn cover_decodes_real_synthetic_png_and_uses_memory_cache() {
    let (url, worker) = fixture(1, 200, "image/png", png());
    let loader = loader();
    let epoch = loader.epoch();
    let image = loader.load_from(&cover(), 128, epoch, url.clone()).unwrap();
    assert_eq!((image.width, image.height), (8, 8));
    assert!(image.alpha);
    assert!(image.bytes.len() <= 8 * 8 * 4);
    let cached = loader.load_from(&cover(), 128, epoch, url).unwrap();
    assert!(Arc::ptr_eq(&image, &cached));
    assert_eq!(loader.0.state.lock().unwrap().cache.len(), 1);
    assert!(!format!("{image:?}").contains("4466"));
    worker.join().unwrap();
    loader.clear();
    assert!(loader.0.state.lock().unwrap().cache.is_empty());
    assert_eq!(loader.0.state.lock().unwrap().bytes, 0);
    assert!(matches!(
        loader.load(&cover(), 128, epoch),
        Err(ArtworkError::Cancelled)
    ));
}

#[test]
fn dedicated_cover_worker_returns_decoded_pixels_without_the_gtk_pool() {
    let (url, server) = fixture(1, 200, "image/png", png());
    let loader = loader();
    let request = loader
        .request_from(&cover(), 128, loader.epoch(), url)
        .unwrap();
    let mut completed = None;
    for _ in 0..200 {
        completed = request.try_result();
        if completed.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let image = completed.expect("bounded worker completion").unwrap();
    assert_eq!((image.width, image.height), (8, 8));
    server.join().unwrap();
    loader.clear();
}

#[test]
fn cover_rejects_redirects_wrong_types_bad_images_and_oversized_sources() {
    for (status, mime, bytes, expected) in [
        (302, "image/png", png(), ArtworkError::Rejected),
        (
            403,
            "application/json",
            b"private provider body".to_vec(),
            ArtworkError::Rejected,
        ),
        (
            200,
            "image/svg+xml",
            b"<svg/>".to_vec(),
            ArtworkError::InvalidImage,
        ),
        (
            200,
            "image/png",
            b"<svg/>".to_vec(),
            ArtworkError::InvalidImage,
        ),
        (200, "image/jpeg", png(), ArtworkError::InvalidImage),
        (
            200,
            "image/png",
            vec![0; MAX_ENCODED_BYTES + 1],
            ArtworkError::TooLarge,
        ),
    ] {
        let (url, worker) = fixture(1, status, mime, bytes);
        let loader = loader();
        assert!(
            matches!(loader.load_from(&cover(), 128, loader.epoch(), url), Err(error) if error == expected)
        );
        worker.join().unwrap();
    }
    let mut oversized = png();
    oversized[16..20].copy_from_slice(&(MAX_SOURCE_EDGE + 1).to_be_bytes());
    assert!(matches!(
        decode(&oversized, "png", 128),
        Err(ArtworkError::TooLarge)
    ));
    assert!(matches!(
        decode(b"", "png", 128),
        Err(ArtworkError::InvalidImage)
    ));
    assert!(jpeg_dimensions(&[0xff, 0xd8, 0xff, 0xc0, 0, 255]).is_none());
    // Minimal dimensions header; the real decoder still rejects absent pixels.
    let jpeg = [0xff, 0xd8, 0xff, 0xc0, 0, 8, 8, 0, 16, 0, 32, 3];
    assert_eq!(jpeg_dimensions(&jpeg), Some((32, 16)));
}

#[test]
fn logout_discards_a_successful_inflight_image() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!(
        "http://{}/synthetic",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 2048];
        let _ = stream.read(&mut request).unwrap();
        started_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let bytes = png();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).unwrap();
        stream.write_all(&bytes).unwrap();
    });
    let loader = loader();
    let epoch = loader.epoch();
    let job_loader = loader.clone();
    let job = thread::spawn(move || job_loader.load_from(&cover(), 128, epoch, url));
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    loader.clear();
    release_tx.send(()).unwrap();
    assert!(matches!(job.join().unwrap(), Err(ArtworkError::Cancelled)));
    server.join().unwrap();
    let state = loader.0.state.lock().unwrap();
    assert_eq!(
        (state.active, state.waiting, state.bytes, state.cache.len()),
        (0, 0, 0, 0)
    );
}

#[test]
fn queue_is_bounded_and_logout_wakes_waiting_requests_without_http() {
    let loader = loader();
    let epoch = loader.epoch();
    loader.0.state.lock().unwrap().waiting = MAX_WAITING;
    assert!(matches!(
        loader.load(&cover(), 128, epoch),
        Err(ArtworkError::Busy)
    ));
    {
        let mut state = loader.0.state.lock().unwrap();
        state.waiting = 0;
        state.active = MAX_ACTIVE;
    }
    let waiting_loader = loader.clone();
    let job = thread::spawn(move || waiting_loader.load(&cover(), 128, epoch));
    for _ in 0..100 {
        if loader.0.state.lock().unwrap().waiting == 1 {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(loader.0.state.lock().unwrap().waiting, 1);
    loader.clear();
    assert!(matches!(job.join().unwrap(), Err(ArtworkError::Cancelled)));
    assert_eq!(loader.0.state.lock().unwrap().waiting, 0);
}

#[test]
fn cover_cache_evicts_oldest_entries() {
    let (url, worker) = fixture(MAX_CACHE_ENTRIES + 1, 200, "image/png", png());
    let loader = loader();
    let epoch = loader.epoch();
    for index in 0..=MAX_CACHE_ENTRIES {
        let cover = CoverArt::parse(&format!(
            "avatars.yandex.net/get-music/synthetic-{index}/%%"
        ))
        .unwrap();
        loader.load_from(&cover, 128, epoch, url.clone()).unwrap();
    }
    worker.join().unwrap();
    let state = loader.0.state.lock().unwrap();
    assert_eq!(state.cache.len(), MAX_CACHE_ENTRIES);
    assert!(state.bytes <= MAX_CACHE_BYTES);
    assert!(
        !state
            .cache
            .iter()
            .any(|entry| entry.key.0.0.ends_with("synthetic-0"))
    );
}

#[test]
fn cancelling_a_queued_cover_never_starts_its_http_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = Url::parse(&format!(
        "http://{}/synthetic",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let loader = loader();
    loader.0.state.lock().unwrap().active = MAX_ACTIVE;
    let request = loader
        .request_from(&cover(), 128, loader.epoch(), url)
        .unwrap();
    for _ in 0..200 {
        if loader.0.state.lock().unwrap().waiting != 0 {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(loader.0.state.lock().unwrap().waiting, 1);
    request.cancellation().cancel();
    let mut result = None;
    for _ in 0..200 {
        result = request.try_result();
        if result.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(result, Some(Err(ArtworkError::Cancelled))));
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    loader.0.state.lock().unwrap().active = 0;
    loader.clear();
}

#[test]
fn idle_cover_workers_do_not_retain_the_last_loader() {
    let loader = loader();
    let weak = Arc::downgrade(&loader.0);
    drop(loader);
    assert!(weak.upgrade().is_none());
}

#[test]
fn cover_cache_also_bounds_decoded_bytes_at_large_thumbnail_size() {
    let pixbuf =
        gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 1024, 1024)
            .unwrap();
    pixbuf.fill(0x5577_99ff);
    let png = pixbuf.save_to_bufferv("png", &[]).unwrap();
    let decoded_bytes = decode(&png, "png", 512).unwrap().bytes.len();
    let count = MAX_CACHE_BYTES / decoded_bytes + 1;
    let (url, server) = fixture(count, 200, "image/png", png);
    let loader = loader();
    for index in 0..count {
        let cover =
            CoverArt::parse(&format!("avatars.yandex.net/get-music/large-{index}/%%")).unwrap();
        loader
            .load_from(&cover, 512, loader.epoch(), url.clone())
            .unwrap();
    }
    server.join().unwrap();
    let state = loader.0.state.lock().unwrap();
    assert!(state.bytes <= MAX_CACHE_BYTES);
    assert!(state.cache.len() < count);
}
