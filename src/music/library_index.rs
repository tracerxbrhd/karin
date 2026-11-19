// SPDX-License-Identifier: GPL-3.0-or-later
//! A current, bounded collection manifest followed by independent canonical
//! batches. Display fields in the compact manifest never become Track models.

use super::*;

pub const MAX_INDEX_POSITIONS: usize = 20_000;
const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub enum CollectionSource {
    Liked,
    Playlist(PlaylistId),
}
impl fmt::Debug for CollectionSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CollectionSource([redacted])")
    }
}

/// Process-only membership and session receipt, never serialized as a grant.
#[derive(Clone)]
pub struct CollectionSnapshot {
    source: CollectionSource,
    selectors: Arc<Vec<String>>,
    revision: Option<u64>,
    added: Arc<Vec<Option<i64>>>,
    binding: SearchBinding,
}
impl fmt::Debug for CollectionSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CollectionSnapshot")
            .field("positions", &self.len())
            .finish_non_exhaustive()
    }
}
impl CollectionSnapshot {
    pub fn len(&self) -> usize {
        self.selectors.len()
    }
    pub fn is_empty(&self) -> bool {
        self.selectors.is_empty()
    }
    pub fn matches_prefix(&self, tracks: &[Track]) -> bool {
        tracks.len() <= self.len()
            && tracks
                .iter()
                .zip(self.selectors.iter())
                .all(|(track, id)| track.id == *id)
    }
    /// Both full sequences must agree; equal counts/revision alone are not proof.
    pub fn same_membership(&self, other: &Self) -> bool {
        self.source == other.source
            && self.selectors == other.selectors
            && self.revision == other.revision
            && self.added == other.added
            && self.binding.epoch == other.binding.epoch
            && self.binding.uid == other.binding.uid
            && self.binding.token_key == other.binding.token_key
    }
    pub fn has_added_dates(&self) -> bool {
        !self.added.is_empty() && self.added.iter().all(Option::is_some)
    }
    pub fn added_at(&self, position: usize) -> Option<i64> {
        self.added.get(position).copied().flatten()
    }
}

#[derive(Clone)]
pub struct IndexedBatch {
    /// Absolute source positions survive omissions and repeated selectors.
    pub tracks: Vec<(usize, Track)>,
    pub next_position: usize,
    pub omitted: usize,
}
impl fmt::Debug for IndexedBatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IndexedBatch")
            .field("count", &self.tracks.len())
            .field("next_position", &self.next_position)
            .field("omitted", &self.omitted)
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reference {
    id: Id,
    album_id: Option<Id>,
    #[serde(default, deserialize_with = "display_text")]
    timestamp: Option<String>,
}
#[derive(Deserialize)]
struct Membership {
    library: Option<Manifest>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    uid: Option<Id>,
    owner: Option<OwnerWire>,
    kind: Option<Id>,
    revision: Option<Id>,
    track_count: Option<usize>,
    tracks: Option<Vec<Reference>>,
}

impl MusicService {
    /// The compact membership is fetched once per explicit indexing intent,
    /// rather than once for every fifty-position normalization request.
    pub fn collection_snapshot(
        &self,
        token: &AccessToken,
        source: &CollectionSource,
    ) -> Result<CollectionSnapshot, MusicError> {
        let binding = self.search_binding(token)?;
        let (segments, query) = match source {
            CollectionSource::Liked => (
                vec!["users", binding.uid.as_str(), "likes", "tracks"],
                vec![("if-modified-since-revision", "0")],
            ),
            CollectionSource::Playlist(id) => {
                validate_id(&id.owner)?;
                validate_id(&id.kind)?;
                (
                    vec!["users", id.owner.as_str(), "playlists", id.kind.as_str()],
                    vec![("richTracks", "false")],
                )
            }
        };
        let response = self.request_response(token, &segments, &query, None)?;
        let body = manifest_body(response)?;
        self.check_search_binding(token, &binding)?;
        let manifest = match source {
            CollectionSource::Liked => {
                let wire: Envelope<Membership> =
                    serde_json::from_slice(&body).map_err(|_| MusicError::InvalidResponse)?;
                let manifest = wire.result.library.ok_or(MusicError::InvalidResponse)?;
                if manifest
                    .uid
                    .as_ref()
                    .is_some_and(|uid| uid.0 != binding.uid)
                {
                    return Err(MusicError::InvalidResponse);
                }
                manifest
            }
            CollectionSource::Playlist(id) => {
                let wire: Envelope<Manifest> =
                    serde_json::from_slice(&body).map_err(|_| MusicError::InvalidResponse)?;
                let manifest = wire.result;
                if manifest.kind.as_ref().is_some_and(|kind| kind.0 != id.kind)
                    || manifest.uid.as_ref().is_some_and(|uid| uid.0 != id.owner)
                    || manifest
                        .owner
                        .as_ref()
                        .and_then(|owner| owner.uid.as_ref())
                        .is_some_and(|uid| uid.0 != id.owner)
                {
                    return Err(MusicError::InvalidResponse);
                }
                manifest
            }
        };
        let revision = manifest
            .revision
            .map(|value| {
                value
                    .0
                    .parse::<u64>()
                    .map_err(|_| MusicError::InvalidResponse)
            })
            .transpose()?;
        let references = match manifest.tracks {
            Some(values) => values,
            None if manifest.track_count == Some(0) => Vec::new(),
            None => return Err(MusicError::InvalidResponse),
        };
        if references.len() > MAX_INDEX_POSITIONS
            || manifest
                .track_count
                .is_some_and(|count| count > MAX_INDEX_POSITIONS)
        {
            return Err(MusicError::ResponseTooLarge);
        }
        if manifest
            .track_count
            .is_some_and(|count| count != references.len())
        {
            return Err(MusicError::InvalidResponse);
        }
        let added = references
            .iter()
            .map(|reference| reference.timestamp.as_deref().and_then(added_timestamp))
            .collect::<Vec<_>>();
        let selectors = references
            .into_iter()
            .map(|reference| {
                let id = match reference.album_id {
                    Some(album) => format!("{}:{}", reference.id.0, album.0),
                    None => reference.id.0,
                };
                validate_id(&id)?;
                Ok(id)
            })
            .collect::<Result<Vec<_>, MusicError>>()?;
        self.check_search_binding(token, &binding)?;
        Ok(CollectionSnapshot {
            source: source.clone(),
            selectors: Arc::new(selectors),
            revision,
            added: Arc::new(added),
            binding,
        })
    }

    /// At most one existing `/tracks` batch, no N+1 or per-key membership reads.
    /// Missing canonical records keep holes in the original occurrence map.
    pub fn collection_index_batch(
        &self,
        token: &AccessToken,
        snapshot: &CollectionSnapshot,
        position: usize,
    ) -> Result<IndexedBatch, MusicError> {
        self.check_search_binding(token, &snapshot.binding)?;
        if position > snapshot.len() {
            return Err(MusicError::InvalidInput);
        }
        let end = position.saturating_add(PAGE_SIZE).min(snapshot.len());
        let ids = &snapshot.selectors[position..end];
        if ids.is_empty() {
            return Ok(IndexedBatch {
                tracks: Vec::new(),
                next_position: end,
                omitted: 0,
            });
        }
        let response = self.track_batch_response(token, ids)?;
        let body = read_body(response)?;
        self.check_search_binding(token, &snapshot.binding)?;
        let distinct = ids.iter().collect::<std::collections::BTreeSet<_>>().len();
        let tracks = decode_track_batch(&body, distinct)?;
        let hits = ids
            .iter()
            .map(|id| SearchTrackHit { id: Id(id.clone()) })
            .collect();
        let (normalized, omitted) = order_search_tracks(hits, tracks)?;
        let tracks = ids
            .iter()
            .enumerate()
            .filter_map(|(offset, id)| {
                normalized
                    .iter()
                    .find(|track| track.id == *id)
                    .cloned()
                    .map(|track| (position + offset, track))
            })
            .collect();
        self.check_search_binding(token, &snapshot.binding)?;
        Ok(IndexedBatch {
            tracks,
            next_position: end,
            omitted,
        })
    }
}

/// Strict RFC3339 civil timestamp, normalized to UTC milliseconds. Malformed
/// optional dates remove date-sort capability, never an otherwise valid track.
fn added_timestamp(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if !value.is_ascii()
        || bytes.len() < 20
        || bytes.len() > 35
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let number = |start, end| {
        let number = value.get(start..end)?;
        if !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        number.parse::<i64>().ok()
    };
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
    let leap = |year: i64| year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap(year) {
                29
            } else {
                28
            }
        }
        _ => return None,
    };
    if !(1970..=9999).contains(&year)
        || !(1..=days).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let mut at = 19;
    let mut millis = 0;
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        let start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == start || at - start > 9 {
            return None;
        }
        for offset in 0..3 {
            millis = millis * 10
                + bytes
                    .get(start + offset)
                    .filter(|_| start + offset < at)
                    .map_or(0, |byte| i64::from(byte - b'0'));
        }
    }
    let zone = match bytes.get(at)? {
        b'Z' | b'z' if at + 1 == bytes.len() => 0,
        sign @ (b'+' | b'-') if at + 6 == bytes.len() && bytes[at + 3] == b':' => {
            let hours = number(at + 1, at + 3)?;
            let minutes = number(at + 4, at + 6)?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            (hours * 60 + minutes) * 60 * if *sign == b'+' { 1 } else { -1 }
        }
        _ => return None,
    };
    let preceding_years = year - 1;
    let before_year = (preceding_years - 1969) * 365 + preceding_years / 4
        - 1969 / 4
        - (preceding_years / 100 - 1969 / 100)
        + preceding_years / 400
        - 1969 / 400;
    let month_days: [i64; 12] = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let before_month: i64 = month_days[..(month - 1) as usize].iter().sum();
    Some(
        ((before_year + before_month + day - 1) * 86_400 + hour * 3600 + minute * 60 + second
            - zone)
            * 1000
            + millis,
    )
}

fn manifest_body(mut response: Response) -> Result<Zeroizing<Vec<u8>>, MusicError> {
    if response
        .content_length()
        .is_some_and(|size| size > MAX_MANIFEST_BYTES as u64)
    {
        return Err(MusicError::ResponseTooLarge);
    }
    let mut output = Zeroizing::new(Vec::new());
    let mut bytes = Zeroizing::new([0u8; 8192]);
    loop {
        let count = response.read(bytes.as_mut()).map_err(|error| {
            if error.kind() == std::io::ErrorKind::TimedOut {
                MusicError::Timeout
            } else {
                MusicError::Network
            }
        })?;
        if count == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(count) > MAX_MANIFEST_BYTES {
            return Err(MusicError::ResponseTooLarge);
        }
        output.extend_from_slice(&bytes[..count]);
    }
}

#[cfg(test)]
pub(crate) mod tests;
