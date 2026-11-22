// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in stage facts. Only fixed categories, booleans and counts reach stderr.

use std::{
    cell::Cell,
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use adw::prelude::*;

use crate::{
    lyrics::{Lyrics, LyricsSource},
    music::{MusicError, track_extras::LyricsRequest},
    playback::{PlaybackSnapshot, PlaybackState},
};

const LIMIT: usize = 256;

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("KARIN_LYRICS_DIAGNOSTICS").is_some_and(|v| v == "1"))
}

fn emit(message: impl FnOnce() -> String) {
    if !enabled() {
        return;
    }
    static EMITTED: AtomicUsize = AtomicUsize::new(0);
    match EMITTED.fetch_add(1, Ordering::Relaxed) {
        n if n < LIMIT => eprintln!("Karin lyrics: {}", message()),
        LIMIT => eprintln!("Karin lyrics: stage=diagnostics category=limit-reached"),
        _ => {}
    }
}

fn provider(request: LyricsRequest) -> &'static str {
    match request {
        LyricsRequest::Yandex => "yandex",
        LyricsRequest::External => "lrclib",
    }
}

pub(super) fn request(request: LyricsRequest) {
    emit(|| format!("stage=request provider={}", provider(request)));
}

pub(super) fn consent(remembered: bool) {
    emit(|| format!("stage=consent remembered={remembered}"));
}

fn result_message(request: LyricsRequest, result: &Result<Option<Lyrics>, MusicError>) -> String {
    let provider = provider(request);
    let Ok(Some(lyrics)) = result else {
        let kind = match result {
            Ok(None) => "absent",
            Err(MusicError::LyricsNotPermitted) => "not-permitted",
            Err(_) => "error",
            _ => unreachable!(),
        };
        return format!("stage=data provider={provider} type={kind} synchronized-lines=0");
    };
    let source = match lyrics.source {
        LyricsSource::YandexSupplement => "yandex",
        LyricsSource::Lrclib => "lrclib",
    };
    let synchronized = &lyrics.synchronized;
    let kind = if synchronized.is_empty() {
        "TEXT"
    } else {
        "LRC"
    };
    let ordered = !synchronized.is_empty()
        && synchronized
            .windows(2)
            .all(|pair| pair[0].time_ms <= pair[1].time_ms);
    format!(
        "stage=data provider={provider} source={source} type={kind} synchronized-lines={} timestamps={ordered}",
        synchronized.len()
    )
}

pub(super) fn result(request: LyricsRequest, result: &Result<Option<Lyrics>, MusicError>) {
    emit(|| result_message(request, result));
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Clock {
    state: &'static str,
    position: bool,
    synchronized: bool,
    mapped: bool,
    active: bool,
    continuity: bool,
    animations: bool,
    high_contrast: bool,
}

#[derive(Default)]
pub(super) struct Trace {
    clock: Cell<Option<Clock>>,
}

impl Trace {
    pub(super) fn update(
        &self,
        snapshot: Option<&PlaybackSnapshot>,
        synchronized: bool,
        list: &gtk::ListView,
        active: bool,
        changed: bool,
        continuity: bool,
    ) {
        if !enabled() {
            return;
        }
        let state = match snapshot.map(|value| value.state) {
            Some(PlaybackState::Playing) => "playing",
            Some(PlaybackState::Paused) => "paused",
            Some(_) => "other",
            None => "detached",
        };
        let mapped = list.is_mapped();
        // A closed Lyrics view has no presentation to diagnose. Retain one
        // explicit unmap after a visible view, without tracing every player poll.
        if !mapped && self.clock.get().is_none_or(|previous| !previous.mapped) {
            return;
        }
        let animations = list.settings().is_gtk_enable_animations();
        let high_contrast = adw::StyleManager::default().is_high_contrast();
        let value = Clock {
            state,
            position: snapshot.is_some_and(|value| value.position.is_some()),
            synchronized,
            mapped,
            active,
            continuity,
            animations,
            high_contrast,
        };
        if self.clock.replace(Some(value)) != Some(value) || changed {
            emit(|| {
                format!(
                    "stage=clock state={state} position={} synchronized={synchronized} mapped={mapped} active={active} changed={changed} continuity={continuity} animations={animations} high-contrast={high_contrast}",
                    value.position
                )
            });
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Tick {
    Started,
    Completed,
    Cancelled,
}

pub(super) fn tick(tick: Tick) {
    let category = match tick {
        Tick::Started => "started",
        Tick::Completed => "completed",
        Tick::Cancelled => "cancelled",
    };
    emit(|| format!("stage=presentation category={category}"));
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Paint {
    Resting,
    Intermediate,
    Active,
}

impl Paint {
    pub(super) fn for_strength(strength: f64) -> Self {
        if strength <= 0.0 {
            Self::Resting
        } else if strength >= 1.0 {
            Self::Active
        } else {
            Self::Intermediate
        }
    }
}

pub(super) fn painted(paint: Paint) {
    let category = match paint {
        Paint::Resting => "resting",
        Paint::Intermediate => "intermediate",
        Paint::Active => "active",
    };
    emit(|| format!("stage=paint category={category}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_data_facts_never_format_private_lyrics() {
        let lyrics =
            Lyrics::from_lrc(LyricsSource::Lrclib, "[00:01]private marker\n[00:02]secret").unwrap();
        assert_eq!(
            result_message(LyricsRequest::External, &Ok(lyrics)),
            "stage=data provider=lrclib source=lrclib type=LRC synchronized-lines=2 timestamps=true"
        );
        let text = Lyrics::from_text(LyricsSource::YandexSupplement, "personal text").unwrap();
        assert_eq!(
            result_message(LyricsRequest::Yandex, &Ok(text)),
            "stage=data provider=yandex source=yandex type=TEXT synchronized-lines=0 timestamps=false"
        );
        assert_eq!(
            result_message(LyricsRequest::External, &Err(MusicError::InvalidResponse)),
            "stage=data provider=lrclib type=error synchronized-lines=0"
        );
    }
}
