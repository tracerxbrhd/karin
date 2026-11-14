// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only related tracks and explicitly requested text sources.
//!
//! Contracts: MarshalX/yandex-music-api 9ca132e5 (`tracks_similar`,
//! `track_supplement`), Cassette v0.2.4 9d34251d (related-track envelope and
//! player-position lyric highlighting), https://lrclib.net/docs.md (2026-11-14).
//! Cassette/MarshalX's modern lyrics signer is deliberately not implemented.
//! LRCLIB is an independent, opt-in public-metadata lookup, never a Music grant.

use super::*;
use crate::lyrics::{Lyrics, LyricsError, LyricsSource};
use std::{
    sync::TryLockError,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const MAX_SIMILAR: usize = 50;
const MAX_SIMILAR_RESPONSE: usize = 200;
const EXTERNAL_URL: &str = "https://lrclib.net/api/get";
const EXTERNAL_USER_AGENT: &str = concat!(
    "Karin/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/tracerxbrhd/karin)"
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LyricsRequest {
    Yandex,
    /// The UI discloses LRCLIB and public metadata before this explicit action.
    External,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SimilarTracks {
    pub tracks: Vec<Track>,
    /// Invalid/unavailable/duplicate/truncated positions, never fake tracks.
    pub omitted: usize,
}
impl fmt::Debug for SimilarTracks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SimilarTracks")
            .field("count", &self.tracks.len())
            .field("omitted", &self.omitted)
            .finish()
    }
}

/// Shared by service clones; never read on the GTK thread. The guard covers
/// the external request, so there is at most one outstanding LRCLIB request.
#[derive(Default)]
pub(super) struct ExternalHttpState {
    next_allowed: Option<Instant>,
    blocked: bool,
}

#[derive(Deserialize)]
struct ExtraEnvelope<T> {
    result: Option<T>,
    error: Option<Box<RawValue>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SimilarWire {
    track: SearchTrackHit,
    similar_tracks: Vec<Box<RawValue>>,
}

#[derive(Deserialize)]
struct Supplement {
    lyrics: Option<SupplementLyrics>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SupplementLyrics {
    full_lyrics: Option<String>,
    /// Explicit provider refusal is not overridden by a text-bearing object.
    has_rights: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExternalLyrics {
    track_name: String,
    artist_name: String,
    duration: Option<f64>,
    #[serde(default)]
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

impl MusicService {
    /// A selected track's published related list. Rich display fields from this
    /// endpoint are ignored; one existing bounded batch supplies normalized rows.
    pub fn similar_tracks(
        &self,
        token: &AccessToken,
        track_id: &str,
    ) -> Result<SimilarTracks, MusicError> {
        validate_id(track_id)?;
        let binding = self.search_binding(token)?;
        let bare = track_id.split(':').next().ok_or(MusicError::InvalidInput)?;
        let response = self.request_response(token, &["tracks", bare, "similar"], &[], None);
        self.check_search_binding(token, &binding)?;
        let response = response?;
        let body = read_body(response)?;
        self.check_search_binding(token, &binding)?;
        let wire: ExtraEnvelope<SimilarWire> =
            serde_json::from_slice(&body).map_err(|_| MusicError::InvalidResponse)?;
        if wire.error.is_some() {
            return Err(MusicError::InvalidResponse);
        }
        let wire = wire.result.ok_or(MusicError::InvalidResponse)?;
        if wire.track.id.0.split(':').next() != Some(bare) {
            return Err(MusicError::InvalidResponse);
        }
        if wire.similar_tracks.len() > MAX_SIMILAR_RESPONSE {
            return Err(MusicError::ResponseTooLarge);
        }
        let mut ids = Vec::new();
        let mut omitted = 0;
        for raw in wire.similar_tracks {
            let Ok(hit) = serde_json::from_str::<SearchTrackHit>(raw.get()) else {
                omitted += 1;
                continue;
            };
            if hit.id.0.split(':').next() == Some(bare)
                || ids.contains(&hit.id.0)
                || ids.len() == MAX_SIMILAR
            {
                omitted += 1;
            } else {
                ids.push(hit.id.0);
            }
        }
        let mut tracks = Vec::new();
        if !ids.is_empty() {
            self.check_search_binding(token, &binding)?;
            let response = self.track_batch_response(token, &ids);
            self.check_search_binding(token, &binding)?;
            let response = response?;
            let body = read_body(response)?;
            self.check_search_binding(token, &binding)?;
            let normalized = decode_track_batch(&body, ids.len())?;
            let hits = ids
                .into_iter()
                .map(|id| SearchTrackHit { id: Id(id) })
                .collect();
            let (ordered, missing) = order_search_tracks(hits, normalized)?;
            tracks = ordered;
            omitted += missing;
        }
        self.check_search_binding(token, &binding)?;
        Ok(SimilarTracks { tracks, omitted })
    }

    pub fn lyrics(
        &self,
        token: &AccessToken,
        track: &Track,
        request: LyricsRequest,
    ) -> Result<Option<Lyrics>, MusicError> {
        validate_id(&track.id)?;
        let binding = self.search_binding(token)?;
        match request {
            LyricsRequest::Yandex => {
                let bare = track.id.split(':').next().ok_or(MusicError::InvalidInput)?;
                let response =
                    self.request_response(token, &["tracks", bare, "supplement"], &[], None);
                self.check_search_binding(token, &binding)?;
                let response = match response {
                    Err(MusicError::Http(404)) => return Ok(None),
                    other => other?,
                };
                let body = read_body(response)?;
                self.check_search_binding(token, &binding)?;
                let envelope: ExtraEnvelope<Supplement> =
                    serde_json::from_slice(&body).map_err(|_| MusicError::InvalidResponse)?;
                if envelope.error.is_some() {
                    return Err(MusicError::InvalidResponse);
                }
                let Some(lyrics) = envelope.result.and_then(|supplement| supplement.lyrics) else {
                    return Ok(None);
                };
                if lyrics.has_rights != Some(true) {
                    return Ok(None);
                }
                // The legacy `lyrics` field is only an opening excerpt. Do not
                // present that excerpt as the complete song text.
                let text = lyrics.full_lyrics.unwrap_or_default();
                Lyrics::from_text(LyricsSource::YandexSupplement, &text).map_err(lyrics_error)
            }
            LyricsRequest::External => self.external_lyrics(token, track, &binding),
        }
    }

    fn external_lyrics(
        &self,
        token: &AccessToken,
        track: &Track,
        binding: &SearchBinding,
    ) -> Result<Option<Lyrics>, MusicError> {
        let artist = track.artists.first().ok_or(MusicError::InvalidInput)?;
        for value in [track.title.as_str(), artist.as_str()] {
            if value.trim().is_empty() || text(value.to_owned()).is_err() {
                return Err(MusicError::InvalidInput);
            }
        }
        let mut state = match self.external_lyrics.try_lock() {
            Ok(state) => state,
            Err(TryLockError::WouldBlock) => return Err(MusicError::RateLimited),
            Err(TryLockError::Poisoned(_)) => return Err(MusicError::Configuration),
        };
        if state.blocked
            || state
                .next_allowed
                .is_some_and(|deadline| deadline > Instant::now())
        {
            return Err(MusicError::RateLimited);
        }
        let mut url = self.lyrics_external_url()?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("track_name", track.title.trim());
            query.append_pair("artist_name", artist.trim());
            if let Some(album) = &track.album_title {
                if text(album.clone()).is_err() {
                    return Err(MusicError::InvalidInput);
                }
                query.append_pair("album_name", album);
            }
            if let Some(ms) = track
                .duration_ms
                .filter(|ms| (1_000..=3_600_000).contains(ms))
            {
                query.append_pair("duration", &format!("{:.3}", ms as f64 / 1_000.0));
            }
        }
        self.check_search_binding(token, binding)?;
        // The shared HTTP client has no default auth/cookies and follows no
        // redirects. Only metadata and a truthful application User-Agent leave
        // for this fixed public origin. Music track/account IDs never do.
        let response = self
            .client
            .get(url)
            .header(ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, EXTERNAL_USER_AGENT)
            .send();
        state.next_allowed = Instant::now().checked_add(Duration::from_millis(300));
        self.check_search_binding(token, binding)?;
        let response = response.map_err(network_error)?;
        let status = response.status().as_u16();
        if matches!(status, 429 | 503) {
            let seconds = match response.headers().get(reqwest::header::RETRY_AFTER) {
                None => 60,
                Some(value) => match value
                    .to_str()
                    .ok()
                    .and_then(|value| retry_after(value, SystemTime::now()))
                {
                    Some(seconds) => seconds,
                    None => {
                        // An unparseable deadline is not permission to retry
                        // before it. No further request in this service session.
                        state.blocked = true;
                        return Err(MusicError::RateLimited);
                    }
                },
            };
            let mut jitter = [0u8; 1];
            use ring::rand::SecureRandom;
            ring::rand::SystemRandom::new()
                .fill(&mut jitter)
                .map_err(|_| MusicError::Configuration)?;
            // Never retry sooner than Retry-After, including zero. No automatic
            // retry occurs; subsequent explicit requests observe this deadline.
            let delay = Duration::from_secs(seconds)
                .checked_add(Duration::from_millis(300 + u64::from(jitter[0])));
            state.next_allowed = delay.and_then(|delay| Instant::now().checked_add(delay));
            state.blocked = state.next_allowed.is_none();
            return Err(MusicError::RateLimited);
        }
        if status == 404 {
            return Ok(None);
        }
        if status != 200 {
            return Err(MusicError::Http(status));
        }
        let is_json = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))
            });
        if !is_json {
            return Err(MusicError::InvalidResponse);
        }
        let body = read_body(response)?;
        self.check_search_binding(token, binding)?;
        let wire: ExternalLyrics =
            serde_json::from_slice(&body).map_err(|_| MusicError::InvalidResponse)?;
        if match_text(&wire.track_name) != match_text(&track.title)
            || match_text(&wire.artist_name) != match_text(artist)
            || wire.duration.is_some_and(|duration| {
                !duration.is_finite()
                    || duration < 0.0
                    || track
                        .duration_ms
                        .is_some_and(|ms| (duration * 1_000.0 - ms as f64).abs() > 2_001.0)
            })
        {
            return Err(MusicError::InvalidResponse);
        }
        if wire.instrumental {
            return Ok(None);
        }
        if let Some(lrc) = wire.synced_lyrics.filter(|text| !text.trim().is_empty()) {
            match Lyrics::from_lrc(LyricsSource::Lrclib, &lrc) {
                Ok(Some(lyrics)) => return Ok(Some(lyrics)),
                Err(LyricsError::TooLarge) => return Err(MusicError::ResponseTooLarge),
                _ if wire.plain_lyrics.is_none() => return Err(MusicError::InvalidResponse),
                _ => {}
            }
        }
        Lyrics::from_text(
            LyricsSource::Lrclib,
            wire.plain_lyrics.as_deref().unwrap_or_default(),
        )
        .map_err(lyrics_error)
    }

    fn lyrics_external_url(&self) -> Result<Url, MusicError> {
        #[cfg(test)]
        if self.origin.scheme() == "http" {
            return self
                .origin
                .join("external-lyrics")
                .map_err(|_| MusicError::Configuration);
        }
        Url::parse(EXTERNAL_URL).map_err(|_| MusicError::Configuration)
    }
}

fn lyrics_error(error: LyricsError) -> MusicError {
    match error {
        LyricsError::TooLarge => MusicError::ResponseTooLarge,
        LyricsError::InvalidText | LyricsError::InvalidTiming => MusicError::InvalidResponse,
    }
}

fn match_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Delta-seconds and IMF-fixdate from RFC 9110. No external clock/HTTP helper
/// dependency, and no provider header is retained in application diagnostics.
fn retry_after(value: &str, now: SystemTime) -> Option<u64> {
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        return value.parse().ok();
    }
    let parts: Vec<_> = value.split_ascii_whitespace().collect();
    let [weekday, day, month, year, time, "GMT"] = parts.as_slice() else {
        return None;
    };
    if !matches!(
        *weekday,
        "Mon," | "Tue," | "Wed," | "Thu," | "Fri," | "Sat," | "Sun,"
    ) {
        return None;
    }
    let year = year
        .parse::<u64>()
        .ok()
        .filter(|year| (1970..=9999).contains(year))?;
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|candidate| candidate == month)?;
    let leap = u64::from(year % 4 == 0 && (year % 100 != 0 || year % 400 == 0));
    let days_per_month = [31, 28 + leap, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let day = day
        .parse::<u64>()
        .ok()
        .filter(|day| (1..=days_per_month[month]).contains(day))?;
    let time: Vec<_> = time
        .split(':')
        .map(str::parse::<u64>)
        .collect::<Result<_, _>>()
        .ok()?;
    let [hour, minute, second] = time.as_slice() else {
        return None;
    };
    if *hour >= 24 || *minute >= 60 || *second >= 60 {
        return None;
    }
    let leap_years = |year: u64| year / 4 - year / 100 + year / 400;
    let days = 365 * (year - 1970) + leap_years(year - 1) - leap_years(1969)
        + days_per_month[..month].iter().sum::<u64>()
        + day
        - 1;
    let timestamp = days * 86_400 + hour * 3_600 + minute * 60 + second;
    let deadline = UNIX_EPOCH.checked_add(Duration::from_secs(timestamp))?;
    Some(deadline.duration_since(now).map_or(0, |duration| {
        duration.as_secs() + u64::from(duration.subsec_nanos() > 0)
    }))
}

#[cfg(test)]
mod tests;
