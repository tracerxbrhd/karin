// SPDX-License-Identifier: GPL-3.0-or-later
//! Native cache-source lifecycle tests with original permitted local media.
//! Synthetic storage authority here tests the player boundary, not Music rights.

use super::*;
use crate::{
    audio_cache::{AudioCache, CachePolicy, ResourceKey, test_support::SyntheticAuthority},
    cache::AccountScope,
};
use std::{os::unix::fs::DirBuilderExt, sync::OnceLock};

fn original_mp3() -> Vec<u8> {
    static MEDIA: OnceLock<Vec<u8>> = OnceLock::new();
    MEDIA
        .get_or_init(|| {
            gst::init().unwrap();
            let directory = std::env::temp_dir()
                .join(format!("karin-local-first-player-media-{}", std::process::id()));
            fs::DirBuilder::new().mode(0o700).create(&directory).unwrap();
            let path = directory.join("original.mp3");
            let pipeline = gst::parse::launch("audiotestsrc num-buffers=300 samplesperbuffer=441 volume=0.01 ! audioconvert ! audio/x-raw,rate=44100,channels=1 ! lamemp3enc target=bitrate bitrate=128 cbr=true ! xingmux ! filesink name=output")
                .unwrap().downcast::<gst::Pipeline>().unwrap();
            pipeline.by_name("output").unwrap().set_property("location", path.to_str().unwrap());
            pipeline.set_state(gst::State::Playing).unwrap();
            let completed = pipeline.bus().unwrap().timed_pop_filtered(
                gst::ClockTime::from_seconds(8),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            );
            pipeline.set_state(gst::State::Null).unwrap();
            assert!(completed.is_some_and(|message| matches!(message.view(), gst::MessageView::Eos(_))));
            let body = fs::read(path).unwrap();
            fs::remove_dir_all(directory).unwrap();
            body
        })
        .clone()
}

struct Stored {
    root: PathBuf,
    cache: AudioCache,
    scope: AccountScope,
    key: ResourceKey,
    authority: Arc<SyntheticAuthority>,
    source: PlaybackSource,
}

impl Stored {
    fn new(body: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "karin-local-first-player-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let scope = AccountScope::from_account_id("1234").unwrap();
        let key = ResourceKey::track("123").unwrap();
        let authority = Arc::new(SyntheticAuthority::default());
        authority.allow(&scope, &key);
        let cache = AudioCache::open(
            root.join("audio"),
            authority.clone(),
            CachePolicy::default(),
        )
        .unwrap();
        let lease = cache.bind(Some(scope.clone())).unwrap().unwrap();
        cache
            .put_reader(cache.begin(&lease, key).unwrap(), body)
            .unwrap();
        let source =
            PlaybackSource::cached_file_local(cache.get(&lease, key).unwrap().unwrap()).unwrap();
        assert!(source.remote.is_none() && source.cache_fallback.is_none());
        Self {
            root,
            cache,
            scope,
            key,
            authority,
            source,
        }
    }
}

impl Drop for Stored {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn wait_error(controller: &PlaybackController) -> PlaybackSnapshot {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let snapshot = controller.snapshot();
        if snapshot.state == PlaybackState::Error {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "cached source must fail in time");
        thread::sleep(POLL_INTERVAL);
    }
}

fn shutdown(controller: &PlaybackController) {
    controller.shutdown();
    wait_for_shutdown(controller).unwrap();
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

#[test]
fn local_first_native_source_needs_no_remote_and_survives_active_clear() {
    let stored = Stored::new(&original_mp3());
    assert!(!stored.source.cache_needs_online_resolution());
    let controller = PlaybackController::new(PlaybackOutput::Fakesink).unwrap();
    controller.prepare(stored.source.clone()).unwrap();
    let ready = wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Ready
    })
    .unwrap();
    assert_eq!(ready.source_kind, PlaybackSourceKind::CachedFile);
    controller.play().unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Playing
            && snapshot.playing_time >= Duration::from_millis(40)
    })
    .unwrap();
    stored.cache.clear().unwrap();
    assert_eq!(stored.cache.stats().unwrap().bytes, 0);
    controller.pause().unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Paused
    })
    .unwrap();
    controller.seek(Duration::from_millis(1500)).unwrap();
    let sought = wait_for(&controller, |snapshot| snapshot.seek_serial > 0).unwrap();
    assert_eq!(sought.source_kind, PlaybackSourceKind::CachedFile);
    controller.resume().unwrap();
    wait_for(&controller, |snapshot| {
        snapshot.state == PlaybackState::Ended
    })
    .unwrap();
    shutdown(&controller);
}

#[test]
fn local_first_final_admission_rejects_clear_and_revocation_before_native_open() {
    let body = original_mp3();
    for revoke in [false, true] {
        let stored = Stored::new(&body);
        if revoke {
            stored.authority.deny(&stored.scope, &stored.key);
        } else {
            stored.cache.clear().unwrap();
        }
        assert!(stored.source.cache_needs_online_resolution());
        let controller = PlaybackController::new(PlaybackOutput::Fakesink).unwrap();
        controller.prepare(stored.source.clone()).unwrap();
        let failed = wait_error(&controller);
        assert_eq!(
            failed.last_error,
            Some(PlaybackError::CachedSourceUnavailable)
        );
        assert_eq!(failed.source_kind, PlaybackSourceKind::None);
        assert_eq!(failed.playing_time, Duration::ZERO);
        assert!(
            controller
                .handle
                .shared
                .pipelines
                .lock()
                .unwrap()
                .is_empty()
        );
        shutdown(&controller);
    }
}

#[test]
fn local_first_decoder_failure_requests_lazy_online_resolution_without_a_native_retry() {
    let stored = Stored::new(b"#EXTM3U\nhttp://127.0.0.1:1/unrequested\n");
    let controller = PlaybackController::new(PlaybackOutput::Fakesink).unwrap();
    controller.prepare(stored.source.clone()).unwrap();
    let failed = wait_error(&controller);
    assert_eq!(
        failed.last_error,
        Some(PlaybackError::CachedSourceUnavailable)
    );
    assert_eq!(failed.playing_time, Duration::ZERO);
    assert_eq!(failed.source_kind, PlaybackSourceKind::None);
    assert_eq!(controller.handle.shared.pipelines.lock().unwrap().len(), 1);
    assert!(!format!("{failed:?}").contains("unrequested"));
    shutdown(&controller);
}

#[test]
fn local_first_failure_after_actual_playing_preserves_the_listening_ledger() {
    let stored = Stored::new(b"original fixture bytes");
    let shared = Arc::new(Shared {
        snapshot: Mutex::new(PlaybackSnapshot::default()),
        shutdown: AtomicBool::new(false),
        pipelines: Mutex::new(Vec::new()),
    });
    let mut worker = Worker {
        output: PlaybackOutput::Fakesink,
        shared: shared.clone(),
        source: Some(stored.source.clone()),
        pipeline: None,
        pending: None,
        playing: PlayingClock {
            total: Duration::from_secs(2),
            since: None,
        },
        restore: None,
        source_played: true,
        test_age: Duration::ZERO,
        expire_next_resume: false,
    };
    worker.fail(PlaybackError::Playback);
    let snapshot = shared.snapshot();
    assert_eq!(snapshot.last_error, Some(PlaybackError::Playback));
    assert_eq!(snapshot.playing_time, Duration::from_secs(2));
    assert!(worker.source.is_none());
}
