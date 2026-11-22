// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded text/LRC values. Timings are queried from the existing player;
//! this module owns neither a clock nor playback state.

use std::fmt;

pub const MAX_LYRICS_BYTES: usize = 128 * 1024;
pub const MAX_LYRIC_LINES: usize = 2_000;
const MAX_TIME_MS: u64 = 24 * 60 * 60 * 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LyricsSource {
    YandexSupplement,
    Lrclib,
}
impl LyricsSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::YandexSupplement => "Яндекс Музыка",
            Self::Lrclib => "LRCLIB · внешний источник",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct LyricLine {
    pub time_ms: u64,
    pub text: String,
}
impl fmt::Debug for LyricLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LyricLine([redacted])")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Lyrics {
    pub source: LyricsSource,
    pub plain: String,
    pub synchronized: Vec<LyricLine>,
}
impl fmt::Debug for Lyrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lyrics")
            .field("source", &self.source)
            .field("synchronized_lines", &self.synchronized.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LyricsError {
    TooLarge,
    InvalidText,
    InvalidTiming,
}

fn validate_text(value: &str) -> Result<(), LyricsError> {
    if value.len() > MAX_LYRICS_BYTES || value.lines().count() > MAX_LYRIC_LINES {
        return Err(LyricsError::TooLarge);
    }
    if value
        .chars()
        .any(|c| c == '\0' || (c.is_control() && !matches!(c, '\n' | '\r' | '\t')))
    {
        return Err(LyricsError::InvalidText);
    }
    Ok(())
}

impl Lyrics {
    pub fn from_text(source: LyricsSource, plain: &str) -> Result<Option<Self>, LyricsError> {
        validate_text(plain)?;
        let plain = plain.trim().replace("\r\n", "\n");
        Ok((!plain.is_empty()).then_some(Self {
            source,
            plain,
            synchronized: Vec::new(),
        }))
    }

    pub fn from_lrc(source: LyricsSource, lrc: &str) -> Result<Option<Self>, LyricsError> {
        validate_text(lrc)?;
        let mut offset = 0i64;
        for line in lrc.lines() {
            if let Some(value) = line.trim().strip_prefix("[offset:") {
                let value = value.strip_suffix(']').ok_or(LyricsError::InvalidTiming)?;
                offset = value.parse().map_err(|_| LyricsError::InvalidTiming)?;
                if offset.unsigned_abs() > MAX_TIME_MS {
                    return Err(LyricsError::InvalidTiming);
                }
            }
        }
        let mut synchronized = Vec::new();
        let mut expanded_bytes = 0usize;
        for line in lrc.lines() {
            let mut tail = line.trim();
            let mut times = Vec::new();
            while let Some(tagged) = tail.strip_prefix('[') {
                let Some((tag, remaining)) = tagged.split_once(']') else {
                    return Err(LyricsError::InvalidTiming);
                };
                if tag.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                    let time = timestamp(tag)?;
                    let adjusted = (i64::try_from(time).map_err(|_| LyricsError::InvalidTiming)?
                        + offset)
                        .max(0) as u64;
                    if adjusted > MAX_TIME_MS {
                        return Err(LyricsError::InvalidTiming);
                    }
                    times.push(adjusted);
                } else {
                    // Metadata before a timestamp is not a lyric line; a
                    // literal bracket in the actual lyric remains plain text.
                    break;
                }
                tail = remaining;
            }
            if synchronized.len().saturating_add(times.len()) > MAX_LYRIC_LINES {
                return Err(LyricsError::TooLarge);
            }
            let text = tail.trim_start();
            expanded_bytes = text
                .len()
                .saturating_add(1)
                .saturating_mul(times.len())
                .saturating_add(expanded_bytes);
            if expanded_bytes > MAX_LYRICS_BYTES {
                return Err(LyricsError::TooLarge);
            }
            for time_ms in times {
                synchronized.push(LyricLine {
                    time_ms,
                    text: text.to_owned(),
                });
            }
        }
        // Stable ordering preserves simultaneous/multi-tag source order.
        synchronized.sort_by_key(|line| line.time_ms);
        if synchronized.is_empty() {
            return Ok(None);
        }
        let plain = synchronized
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Some(Self {
            source,
            plain,
            synchronized,
        }))
    }

    /// Seek/backward/pause are just another observed player position. No local
    /// elapsed-time accumulation can drift from PlaybackController.
    pub fn active_line(&self, position_ms: u64) -> Option<usize> {
        self.synchronized
            .partition_point(|line| line.time_ms <= position_ms)
            .checked_sub(1)
    }
}

fn timestamp(value: &str) -> Result<u64, LyricsError> {
    let pieces: Vec<_> = value.split(':').collect();
    let (hours, minutes, seconds) = match pieces.as_slice() {
        [minutes, seconds] => (0, number(minutes)?, *seconds),
        [hours, minutes, seconds] => {
            let minutes = number(minutes)?;
            if minutes >= 60 {
                return Err(LyricsError::InvalidTiming);
            }
            (number(hours)?, minutes, *seconds)
        }
        _ => return Err(LyricsError::InvalidTiming),
    };
    let (seconds, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    let seconds = number(seconds)?;
    if seconds >= 60 || fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return Err(LyricsError::InvalidTiming);
    }
    let fractional_ms = if fraction.is_empty() {
        0
    } else {
        number(fraction)? * 10u64.pow(3 - fraction.len() as u32)
    };
    hours
        .checked_mul(60)
        .and_then(|v| v.checked_add(minutes))
        .and_then(|v| v.checked_mul(60))
        .and_then(|v| v.checked_add(seconds))
        .and_then(|v| v.checked_mul(1_000))
        .and_then(|v| v.checked_add(fractional_ms))
        .filter(|v| *v <= MAX_TIME_MS)
        .ok_or(LyricsError::InvalidTiming)
}

fn number(value: &str) -> Result<u64, LyricsError> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(LyricsError::InvalidTiming);
    }
    value.parse().map_err(|_| LyricsError::InvalidTiming)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_lrc_multitag_offsets_and_clock_lookup_seek_backwards() {
        let text = "[ar:Karin original fixture]\n[offset:-50]\n[00:02.005]Last original line\n[00:00.1][00:01.20]First original line\n[00:01.200]Translation original\n";
        let lyrics = Lyrics::from_lrc(LyricsSource::Lrclib, text)
            .unwrap()
            .unwrap();
        assert_eq!(
            lyrics
                .synchronized
                .iter()
                .map(|l| l.time_ms)
                .collect::<Vec<_>>(),
            [50, 1150, 1150, 1955]
        );
        assert_eq!(lyrics.active_line(0), None);
        assert_eq!(lyrics.active_line(60), Some(0));
        assert_eq!(lyrics.active_line(1200), Some(2));
        assert_eq!(lyrics.active_line(2000), Some(3));
        assert_eq!(lyrics.active_line(500), Some(0));
        assert_eq!(lyrics.active_line(500), Some(0)); // paused position is stable
        assert!(!format!("{lyrics:?}").contains("original line"));
    }

    #[test]
    fn hour_fraction_and_positive_offset_are_checked() {
        let lyrics = Lyrics::from_lrc(
            LyricsSource::Lrclib,
            "[offset:+12]\n[01:02:03.4]Original fixture",
        )
        .unwrap()
        .unwrap();
        assert_eq!(lyrics.synchronized[0].time_ms, 3_723_412);
        for bad in [
            "[00:60]x",
            "[01:61:00]x",
            "[00:01.1234]x",
            "[99999999999999:01]x",
            "[00:01]x\n[offset:999999999]",
            "[00:01",
        ] {
            assert_eq!(
                Lyrics::from_lrc(LyricsSource::Lrclib, bad),
                Err(LyricsError::InvalidTiming)
            );
        }
    }

    #[test]
    fn text_and_lrc_are_bounded_and_do_not_interpret_markup() {
        let lyric = Lyrics::from_text(
            LyricsSource::YandexSupplement,
            " <b>Original & literal</b>\r\nsecond ",
        )
        .unwrap()
        .unwrap();
        assert_eq!(lyric.plain, "<b>Original & literal</b>\nsecond");
        assert!(lyric.synchronized.is_empty());
        assert_eq!(Lyrics::from_text(LyricsSource::Lrclib, " "), Ok(None));
        assert_eq!(
            Lyrics::from_lrc(LyricsSource::Lrclib, "[ar:fixture]\nno timings"),
            Ok(None)
        );
        assert_eq!(
            Lyrics::from_text(LyricsSource::Lrclib, "bad\0private"),
            Err(LyricsError::InvalidText)
        );
        assert_eq!(
            Lyrics::from_text(LyricsSource::Lrclib, &"x".repeat(MAX_LYRICS_BYTES + 1)),
            Err(LyricsError::TooLarge)
        );
        assert_eq!(
            Lyrics::from_lrc(
                LyricsSource::Lrclib,
                &"[00:01]x\n".repeat(MAX_LYRIC_LINES + 1)
            ),
            Err(LyricsError::TooLarge)
        );
        let expanded = "[00:01]".repeat(100) + &"x".repeat(2_000);
        assert!(expanded.len() < MAX_LYRICS_BYTES);
        assert_eq!(
            Lyrics::from_lrc(LyricsSource::Lrclib, &expanded),
            Err(LyricsError::TooLarge)
        );
    }
}
