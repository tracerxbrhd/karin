// SPDX-License-Identifier: GPL-3.0-or-later
//! Session desktop controls are an adapter to the existing player and queue.
use super::*;
use crate::{
    artwork::{ArtworkError, ArtworkRequest, CoverPixels},
    mpris::{Command, LocalArtwork, Metadata, Mpris, Snapshot, Status},
    playback::PlaybackState,
};
use gtk::gdk_pixbuf::{Colorspace, Pixbuf};
use ring::rand::{SecureRandom, SystemRandom};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct SystemMedia {
    bus: Option<Mpris>,
    binding: Option<(u64, usize, String, ContextKind, Option<String>)>,
    serial: u64,
    repeat_restart: Option<u64>,
    snapshot: Snapshot,
    cover_request: Option<ArtworkRequest>,
    cover_admission: Option<(CoverArt, u8, Instant)>,
    cover_encoding: Option<Receiver<Option<PrivateCover>>>,
    cover: Option<PrivateCover>,
    seek: Option<(u64, u64, i64)>,
}

/// Only a decoded, public cover is written, never media or a provider address.
/// The app cache path is shared with the host in Flatpak without filesystem
/// grants; both file and directory are removed on replacement/logout/close.
struct PrivateCover {
    file: PathBuf,
    directory: PathBuf,
    artwork: Option<LocalArtwork>,
}
impl PrivateCover {
    fn write(pixels: &CoverPixels) -> Option<Self> {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let mut random = [0; 16];
        SystemRandom::new().fill(&mut random).ok()?;
        let suffix = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let base = glib::user_cache_dir();
        std::fs::create_dir_all(&base).ok()?;
        let directory = base.join(format!("karin-mpris-{suffix}"));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .ok()?;
        let mut result = Self {
            file: directory.join("cover.png"),
            directory,
            artwork: None,
        };
        let image = Pixbuf::from_mut_slice(
            pixels.bytes.clone(),
            Colorspace::Rgb,
            pixels.alpha,
            8,
            pixels.width,
            pixels.height,
            i32::try_from(pixels.rowstride).ok()?,
        );
        image.savev(&result.file, "png", &[]).ok()?;
        std::fs::set_permissions(&result.file, std::fs::Permissions::from_mode(0o600)).ok()?;
        result.artwork = Some(LocalArtwork::from_file(&result.file)?);
        Some(result)
    }
}
impl Drop for PrivateCover {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
        let _ = std::fs::remove_dir(&self.directory);
    }
}

impl Controller {
    pub(super) fn connect_system_media(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        let bus = Mpris::new(Rc::new(move |command| {
            if let Some(this) = weak.upgrade() {
                this.system_command(command);
            }
        }));
        self.system_media.borrow_mut().bus = Some(bus);
        self.publish_mpris();
    }

    #[cfg(test)]
    pub(super) fn system_media_available(&self) -> bool {
        self.system_media
            .borrow()
            .bus
            .as_ref()
            .is_some_and(Mpris::is_available)
    }

    #[cfg(test)]
    pub(super) fn note_music_seek(&self, position: Duration) {
        let before = self
            .music_snapshot()
            .map_or(0, |snapshot| snapshot.seek_serial);
        self.note_music_seek_from(position, before);
    }

    pub(super) fn note_music_seek_from(&self, position: Duration, before: u64) {
        let mut media = self.system_media.borrow_mut();
        if self.music_snapshot().is_some_and(|snapshot| {
            snapshot.seekable
                && matches!(
                    snapshot.state,
                    PlaybackState::Ready | PlaybackState::Playing | PlaybackState::Paused
                )
        }) {
            media.seek = Some((media.serial, before, micros(position)));
        }
    }

    fn system_command(self: &Rc<Self>, command: Command) {
        if self.model.borrow().closed {
            return;
        }
        match command {
            Command::Raise => {
                if let Some(window) = self.window.upgrade() {
                    window.present();
                }
            }
            Command::Quit => {
                if let Some(window) = self.window.upgrade() {
                    window.close();
                }
            }
            Command::Play => self.play_music(),
            Command::Pause => self.pause_music(),
            Command::PlayPause => self.toggle_music(),
            Command::Stop => self.stop_music(),
            Command::Next => self.skip_music(true, false),
            Command::Previous => self.skip_music(false, false),
            Command::SetVolume(value) => self.set_music_volume(value.clamp(0.0, 1.0)),
            Command::SetShuffle(enabled) => self.set_music_shuffle(enabled),
            Command::SetRepeat(mode) => self.set_music_repeat(mode),
            Command::SetPosition {
                track_id,
                position_us,
            } => {
                let media = self.system_media.borrow();
                let valid = media.serial == track_id
                    && media.snapshot.can_seek
                    && position_us >= 0
                    && media
                        .snapshot
                        .metadata
                        .as_ref()
                        .and_then(|m| m.length_us)
                        .is_none_or(|length| position_us <= length);
                drop(media);
                if valid {
                    self.seek_music(Duration::from_micros(position_us as u64));
                }
            }
            Command::Seek(offset) => {
                let media = self.system_media.borrow();
                if !media.snapshot.can_seek {
                    return;
                }
                let target = media.snapshot.position_us.saturating_add(offset).max(0);
                let beyond = media
                    .snapshot
                    .metadata
                    .as_ref()
                    .and_then(|m| m.length_us)
                    .is_some_and(|length| target > length);
                drop(media);
                if beyond {
                    self.skip_music(true, false);
                } else {
                    self.seek_music(Duration::from_micros(target as u64));
                }
            }
        }
        self.publish_mpris();
    }

    pub(super) fn note_repeat_restart(&self, serial: u64) {
        self.system_media.borrow_mut().repeat_restart = Some(serial);
    }

    pub(super) fn clear_system_media(&self) {
        let mut media = self.system_media.borrow_mut();
        media.binding = None;
        media.repeat_restart = None;
        media.cover_request = None;
        media.cover_admission = None;
        media.cover_encoding = None;
        media.cover = None;
        media.seek = None;
        media.snapshot = Snapshot::default();
        if let Some(bus) = &media.bus {
            bus.publish(media.snapshot.clone());
        }
    }
    pub(super) fn shutdown_system_media(&self) {
        self.clear_system_media();
        if let Some(bus) = self.system_media.borrow_mut().bus.take() {
            bus.shutdown();
        }
    }

    pub(super) fn publish_mpris(&self) {
        let model = self.model.borrow();
        let ready = model.account_valid
            && !model.closed
            && model.token.as_ref().is_some_and(|t| !t.is_expired());
        let selected = if ready { model.queue.current() } else { None };
        let current = model.queue.current_index().unwrap_or(0);
        let key = selected.map(|track| {
            (
                model.generation,
                current,
                track.id.clone(),
                model
                    .queue
                    .context()
                    .map_or(ContextKind::Queue, PlaybackContext::source_kind),
                model
                    .queue
                    .context()
                    .and_then(|c| c.source_id().map(str::to_owned)),
            )
        });
        let snapshot = self.music_snapshot();
        let state = snapshot.as_ref().map_or(PlaybackState::Idle, |s| s.state);
        let requested = self.music_requested();
        let mut media = self.system_media.borrow_mut();
        if media.binding != key {
            media.binding = key;
            media.repeat_restart = None;
            media.serial = media.serial.wrapping_add(1).max(1);
            media.cover_request = None;
            media.cover_encoding = None;
            media.cover = None;
            media.seek = None;
            media.cover_admission = selected
                .and_then(|track| track.cover.clone())
                .map(|cover| (cover, 0, Instant::now()));
        }
        // Busy is only local admission failure; retry it at most three times.
        // A network rejection/failure is never retried automatically.
        if let Some((cover, attempt, due)) = media.cover_admission.clone()
            && Instant::now() >= due
        {
            media.cover_admission = None;
            if let Some(loader) = &self.artwork {
                match loader.request(&cover, 256, loader.epoch()) {
                    Ok(request) => media.cover_request = Some(request),
                    Err(ArtworkError::Busy) if attempt < 2 => {
                        media.cover_admission = Some((
                            cover,
                            attempt + 1,
                            Instant::now() + Duration::from_millis(100),
                        ))
                    }
                    Err(_) => {}
                }
            }
        }
        if let Some(result) = media
            .cover_request
            .as_ref()
            .and_then(ArtworkRequest::try_result)
        {
            media.cover_request = None;
            if let Ok(pixels) = result {
                let (tx, rx) = mpsc::sync_channel(1);
                media.cover_encoding = Some(rx);
                let app = self.app.upgrade();
                glib::spawn_future_local(async move {
                    let _hold = app.as_ref().map(|app| app.hold());
                    let result = gio::spawn_blocking(move || PrivateCover::write(&pixels))
                        .await
                        .ok()
                        .flatten();
                    let _ = tx.send(result);
                });
            }
        }
        if let Some(result) = media
            .cover_encoding
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            media.cover_encoding = None;
            media.cover = result;
        }
        let duration = snapshot
            .as_ref()
            .and_then(|s| s.duration)
            .map(micros)
            .or_else(|| {
                selected
                    .and_then(|track| track.duration_ms)
                    .map(|ms| i64::try_from(ms.saturating_mul(1000)).unwrap_or(i64::MAX))
            });
        let has_source = snapshot.as_ref().is_some_and(|s| s.worker_running);
        let status = match state {
            PlaybackState::Playing => Status::Playing,
            PlaybackState::Paused | PlaybackState::Ready => Status::Paused,
            _ => Status::Stopped,
        };
        let can_seek = ready
            && has_source
            && snapshot.as_ref().is_some_and(|s| s.seekable)
            && matches!(
                state,
                PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ready
            );
        let next = model.queue.can_go_next();
        let previous = model.queue.can_go_previous();
        let published = Snapshot {
            status,
            metadata: selected.map(|track| Metadata {
                track_id: media.serial,
                title: track.title.clone(),
                artists: track.artists.clone(),
                album: track.album_title.clone(),
                length_us: duration,
                artwork: media.cover.as_ref().and_then(|cover| cover.artwork.clone()),
            }),
            position_us: snapshot.as_ref().and_then(|s| s.position).map_or(0, micros),
            volume: snapshot
                .as_ref()
                .map_or_else(|| self.music_volume(), |snapshot| snapshot.volume),
            can_play: ready
                && selected.is_some_and(|track| track.available)
                && !requested
                && state != PlaybackState::Loading,
            // PlayPause is gated by CanPause in MPRIS, including a selected
            // stopped track that the same media key can explicitly restart.
            can_pause: ready
                && selected.is_some_and(|track| track.available)
                && !requested
                && state != PlaybackState::Loading,
            can_seek,
            can_go_next: ready && selected.is_some() && next,
            can_go_previous: ready && selected.is_some() && previous,
            can_control: !model.closed,
            repeat_mode: model.queue.repeat_mode(),
            shuffle: model.queue.shuffle(),
        };
        // A stopped/replaced/loading pipeline cannot acknowledge a seek from
        // the previous worker, even when the same queue item remains selected.
        if !can_seek {
            media.seek = None;
        }
        // A same-occurrence replay keeps its opaque track path. Acknowledge
        // the new worker's real position once; an unrelated Stop/replacement
        // invalidates the serial and cannot resurrect a discarded seek.
        let mut acknowledged_seek = None;
        if let Some(expected) = media.repeat_restart {
            if expected != self.music_serial() || state == PlaybackState::Error {
                media.repeat_restart = None;
            } else if can_seek {
                acknowledged_seek = snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.position)
                    .map(micros);
                if acknowledged_seek.is_some() {
                    media.repeat_restart = None;
                }
            }
        }
        if let Some((serial, before, target)) = media.seek {
            if serial != media.serial {
                media.seek = None;
            } else if can_seek
                && snapshot.as_ref().is_some_and(|snapshot| {
                    snapshot.seek_serial != before
                        && snapshot.seek_position.map(micros) == Some(target)
                })
            {
                acknowledged_seek = Some(target);
                media.seek = None;
            }
        }
        media.snapshot = published.clone();
        if let Some(bus) = &media.bus {
            bus.publish(published);
            if let Some(position) = acknowledged_seek {
                bus.seeked(position);
            }
        }
    }
}
fn micros(value: Duration) -> i64 {
    i64::try_from(value.as_micros()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_cover_is_a_private_png_and_is_removed_on_discard() {
        use std::os::unix::fs::PermissionsExt;
        let pixels = CoverPixels {
            width: 2,
            height: 2,
            rowstride: 8,
            alpha: true,
            bytes: vec![
                20, 40, 80, 255, 20, 40, 80, 255, 20, 40, 80, 255, 20, 40, 80, 255,
            ],
        };
        let cover = PrivateCover::write(&pixels).expect("bounded synthetic PNG export");
        let file = cover.file.clone();
        let directory = cover.directory.clone();
        assert!(cover.artwork.is_some());
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert!(LocalArtwork::from_file(&file).is_some());
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        assert!(sender.send(cover).is_err());
        assert!(!file.exists());
        assert!(!directory.exists());
    }
}
