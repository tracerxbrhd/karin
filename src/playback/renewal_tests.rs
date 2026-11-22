// SPDX-License-Identifier: GPL-3.0-or-later
//! Original synthetic media: renewal keeps the existing worker and logical track.

use super::*;

fn playing(controller: &PlaybackController) -> PlaybackSnapshot {
    wait_for(controller, |snapshot| {
        snapshot.state == PlaybackState::Playing && !snapshot.playing_time.is_zero()
    })
    .expect("the original resource reaches actual native Playing")
}

fn paused(controller: &PlaybackController) -> PlaybackSnapshot {
    controller.pause().unwrap();
    wait_for(controller, |snapshot| {
        snapshot.state == PlaybackState::Paused
    })
    .expect("the existing native worker acknowledges Pause")
}

#[test]
fn renewing_remote_source_keeps_position_generation_volume_and_listening_ledger() {
    let media = SyntheticMp3::new(Duration::from_secs(5));
    let fixture = HttpFixture::new(Reply::Media(media.bytes.clone(), "audio/mpeg", false));
    let controller = controller();
    prepare(&controller, fixture.source());
    controller.set_volume(0.37).unwrap();
    controller.play().unwrap();
    playing(&controller);
    controller.seek(Duration::from_millis(900)).unwrap();
    wait_for(&controller, |snapshot| {
        snapshot
            .position
            .is_some_and(|position| position >= Duration::from_millis(850))
    })
    .unwrap();
    let before = paused(&controller);
    let position = before
        .position
        .expect("Pause retains the confirmed native position");
    let media_requests = fixture.requested.load(Ordering::Acquire);
    controller
        .advance_source_age_for_test(Duration::from_secs(60 * 60))
        .unwrap();
    wait_for(&controller, |snapshot| snapshot.source_expired).unwrap();
    let expired = controller.snapshot();
    assert_eq!(expired.state, PlaybackState::Paused);
    assert_eq!(expired.position, Some(position));
    assert_eq!(expired.playing_time, before.playing_time);
    assert_eq!(expired.last_error, None);
    assert_eq!(fixture.requested.load(Ordering::Acquire), media_requests);

    controller
        .renew_source(
            PlaybackSource::remote(fixture.source()),
            Some(position),
            true,
        )
        .unwrap();
    let after = wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Playing
            && !snapshot.source_expired
            && snapshot.position.is_some_and(|value| value >= position)
            && snapshot.playing_time > before.playing_time
    })
    .expect("one fresh source restores the native seek before audible continuation");
    assert_eq!(after.generation, before.generation);
    assert_eq!(after.source_revision, before.source_revision + 1);
    assert_eq!(
        after.seek_serial, before.seek_serial,
        "internal restore is not a user Seeked"
    );
    assert!((after.volume - 0.37).abs() < f64::EPSILON);
    assert_eq!(after.source_kind, PlaybackSourceKind::RemoteHttps);
    assert!(fixture.requested.load(Ordering::Acquire) > media_requests);
    assert!(!format!("{after:?}").contains("http"));
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Ended
    })
    .unwrap();
    shutdown(&controller);
}

#[test]
fn expiration_racing_resume_preserves_pause_until_one_explicit_renewal() {
    let media = SyntheticMp3::new(Duration::from_secs(4));
    let fixture = HttpFixture::new(Reply::Media(media.bytes.clone(), "audio/mpeg", false));
    let controller = controller();
    prepare(&controller, fixture.source());
    controller.play().unwrap();
    playing(&controller);
    let before = paused(&controller);
    controller.expire_on_next_resume_for_test().unwrap();
    assert!(
        !controller.snapshot().source_expired,
        "the preflight view is still fresh"
    );
    let requests = fixture.requested.load(Ordering::Acquire);
    controller.resume().unwrap();
    let refused = wait_for(&controller, |snapshot| {
        snapshot.last_error == Some(PlaybackError::SourceExpired)
    })
    .expect("the worker reports the finite expiration race");
    assert_eq!(refused.state, PlaybackState::Paused);
    assert_eq!(refused.generation, before.generation);
    assert_eq!(refused.position, before.position);
    assert_eq!(refused.playing_time, before.playing_time);
    assert_eq!(fixture.requested.load(Ordering::Acquire), requests);
    controller
        .renew_source(
            PlaybackSource::remote(fixture.source()),
            before.position,
            true,
        )
        .unwrap();
    let after = wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Playing
            && snapshot.last_error.is_none()
            && !snapshot.source_expired
    })
    .unwrap();
    assert_eq!(after.generation, before.generation);
    assert_eq!(after.source_revision, before.source_revision + 1);
    assert_eq!(after.seek_serial, before.seek_serial);
    shutdown(&controller);
}

#[test]
fn pause_and_stop_during_worker_renewal_never_autoplay_a_late_source() {
    let media = SyntheticMp3::new(Duration::from_secs(5));
    let fixture = HttpFixture::new(Reply::Media(media.bytes.clone(), "audio/mpeg", false));
    let controller = controller();
    prepare(&controller, fixture.source());
    controller.play().unwrap();
    playing(&controller);
    let before = paused(&controller);
    controller
        .renew_source(
            PlaybackSource::remote(fixture.source()),
            Some(Duration::from_secs(1)),
            true,
        )
        .unwrap();
    controller.pause().unwrap();
    let after = wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Paused
            && snapshot
                .position
                .is_some_and(|position| position >= Duration::from_millis(950))
    })
    .expect("Pause during preroll cancels autoplay while restoring the requested position");
    assert_eq!(after.generation, before.generation);
    assert_eq!(after.playing_time, before.playing_time);
    assert_eq!(after.seek_serial, before.seek_serial);
    {
        // Hold the real worker's snapshot barrier while enqueuing both
        // commands. Renew must acquire this barrier before changing source,
        // so Seek(0) is queued before any preroll/Ready acknowledgement.
        // Generation and seek range are already valid in this test; submit
        // the same Renew command through the controller's ordinary channel.
        let snapshot = controller.handle.shared.snapshot();
        assert_eq!(snapshot.state, PlaybackState::Paused);
        controller
            .send(Command::Renew(
                PlaybackSource::remote(fixture.source()),
                Some(Duration::from_secs(1)),
                false,
            ))
            .unwrap();
        controller.seek(Duration::ZERO).unwrap();
    }
    let zero = wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Paused
            && snapshot.source_revision == after.source_revision + 1
            && snapshot.seek_serial == before.seek_serial + 1
            && snapshot.seek_position == Some(Duration::ZERO)
    })
    .expect("a real user Seek(0) during renewal receives exactly one native acknowledgement");
    assert_eq!(zero.generation, before.generation);
    assert_eq!(zero.position, Some(Duration::ZERO));
    assert_eq!(zero.playing_time, before.playing_time);
    assert_eq!(zero.last_error, None);
    controller
        .renew_source(
            PlaybackSource::remote(fixture.source()),
            zero.position,
            true,
        )
        .unwrap();
    controller.stop().unwrap();
    let stopped = wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Stopped
    })
    .expect("Stop retires a pending source before playback");
    assert_eq!(stopped.playing_time, before.playing_time);
    assert_eq!(stopped.source_kind, PlaybackSourceKind::None);
    shutdown(&controller);
}

struct UnknownLengthMedia {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl UnknownLengthMedia {
    fn new(media: Arc<Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let stop = stop.clone();
            thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(value) => value,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => panic!("original chunked-media listener failed"),
                    };
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = Vec::new();
                    while !request.ends_with(b"\r\n\r\n") {
                        let mut byte = [0];
                        if stream.read(&mut byte).unwrap_or(0) == 0 {
                            break;
                        }
                        request.push(byte[0]);
                        assert!(request.len() <= 16 * 1024);
                    }
                    let request = String::from_utf8(request).unwrap();
                    for name in ["authorization:", "cookie:", "origin:", "referer:"] {
                        assert!(!request.to_ascii_lowercase().contains(name));
                    }
                    // A real unknown-size HTTP stream cannot provide a byte
                    // seeking contract. The native seeking query remains the
                    // authority; no fake capability bit is injected.
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nAccept-Ranges: none\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n",
                        media.len()
                    );
                    let _ = stream.write_all(&media);
                    let _ = stream.write_all(b"\r\n0\r\n\r\n");
                }
            })
        };
        Self {
            address,
            stop,
            worker: Some(worker),
        }
    }

    fn source(&self) -> RemotePlaybackSource {
        RemotePlaybackSource::from_test_resolver(
            &format!("http://{}/get-mp3/original-chunked", self.address),
            Instant::now() + Duration::from_secs(60),
        )
        .unwrap()
    }
}

impl Drop for UnknownLengthMedia {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}

#[test]
fn renewed_unseekable_resource_stays_paused_with_a_truthful_restore_error() {
    let media = SyntheticMp3::new(Duration::from_secs(5));
    let fixture = HttpFixture::new(Reply::Media(media.bytes.clone(), "audio/mpeg", false));
    let unknown = UnknownLengthMedia::new(media.bytes.clone());
    let controller = controller();
    prepare(&controller, fixture.source());
    controller.play().unwrap();
    playing(&controller);
    let before = paused(&controller);
    controller
        .renew_source(
            PlaybackSource::remote(unknown.source()),
            Some(Duration::from_secs(1)),
            true,
        )
        .unwrap();
    let after = wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Paused
            && snapshot.last_error == Some(PlaybackError::NotSeekable)
    })
    .expect("a real nonseekable stream cannot silently restart the song at zero");
    assert_eq!(after.generation, before.generation);
    assert_eq!(after.playing_time, before.playing_time);
    assert_eq!(after.seek_serial, before.seek_serial);
    assert!(!after.seekable);
    shutdown(&controller);
}
