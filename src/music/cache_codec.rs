// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit on-disk metadata schema. Never serialize Music HTTP responses,
//! credentials, delivery descriptors or playback-source objects.

use super::{
    Album, Artist, MusicError, MusicService, Page, Playlist, PlaylistId, Track, validate_id,
};
use crate::{
    artwork::CoverArt,
    cache::{AccountScope, CacheError, EntryKind},
    session::AccessToken,
};
use serde::{Deserialize, Serialize};

const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_RECORD_ITEMS: usize = 20_000;
const MAX_CREDITS: usize = 128;

#[derive(Clone, PartialEq, Eq)]
pub enum MetadataRecord {
    Tracks(Page<Track>),
    Playlists(Page<Playlist>),
    Track(Track),
    Album(Album),
}
impl std::fmt::Debug for MetadataRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MetadataRecord([redacted])")
    }
}

impl MusicService {
    /// Cache provenance is available only after this token's Music account was
    /// validated. No account identifier needs to cross into the GTK layer.
    pub fn cache_scope(&self, token: &AccessToken) -> Result<AccountScope, MusicError> {
        AccountScope::from_account_id(&self.account_id(token)?)
            .map_err(|_| MusicError::InvalidResponse)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "record",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
enum Record {
    Tracks(TrackPage),
    Playlists(PlaylistPage),
    Track(TrackData),
    Album(AlbumData),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackPage {
    items: Vec<TrackData>,
    has_more: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaylistPage {
    items: Vec<PlaylistData>,
    has_more: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackData {
    id: String,
    title: String,
    artists: Vec<String>,
    artist_links: Vec<ArtistData>,
    album_id: Option<String>,
    album_title: Option<String>,
    duration_ms: Option<u64>,
    cover: Option<String>,
    explicit: bool,
    available: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtistData {
    id: String,
    name: String,
    cover: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlbumData {
    id: String,
    title: String,
    artists: Vec<String>,
    tracks: Vec<TrackData>,
    cover: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaylistData {
    owner: String,
    kind: String,
    title: String,
    track_count: u64,
    cover: Option<String>,
}

/// Encode only explicit presentation metadata. This function admits no generic
/// JSON payload and cannot accept an account/token/delivery model.
pub fn encode(kind: EntryKind, value: &MetadataRecord) -> Result<Vec<u8>, CacheError> {
    let record = match value {
        MetadataRecord::Tracks(page) => Record::Tracks(TrackPage {
            items: page.items.iter().map(TrackData::from).collect(),
            has_more: page.has_more,
        }),
        MetadataRecord::Playlists(page) => Record::Playlists(PlaylistPage {
            items: page.items.iter().map(PlaylistData::from).collect(),
            has_more: page.has_more,
        }),
        MetadataRecord::Track(track) => Record::Track(TrackData::from(track)),
        MetadataRecord::Album(album) => Record::Album(AlbumData::from(album)),
    };
    if !record.matches(kind) || !record.valid() {
        return Err(CacheError::InvalidData);
    }
    let bytes = serde_json::to_vec(&record).map_err(|_| CacheError::InvalidData)?;
    if bytes.len() > MAX_BYTES {
        return Err(CacheError::TooLarge);
    }
    Ok(bytes)
}

/// Decode and revalidate IDs, bounded display fields and admitted public cover
/// templates. Corruption must become a cache miss, never a network capability.
pub fn decode(kind: EntryKind, bytes: &[u8]) -> Result<MetadataRecord, CacheError> {
    if bytes.len() > MAX_BYTES {
        return Err(CacheError::TooLarge);
    }
    let record: Record = serde_json::from_slice(bytes).map_err(|_| CacheError::InvalidData)?;
    if !record.matches(kind) || !record.valid() {
        return Err(CacheError::InvalidData);
    }
    Ok(match record {
        Record::Tracks(page) => MetadataRecord::Tracks(Page {
            items: page.items.into_iter().map(TrackData::into_track).collect(),
            has_more: page.has_more,
        }),
        Record::Playlists(page) => MetadataRecord::Playlists(Page {
            items: page
                .items
                .into_iter()
                .map(PlaylistData::into_playlist)
                .collect(),
            has_more: page.has_more,
        }),
        Record::Track(track) => MetadataRecord::Track(track.into_track()),
        Record::Album(album) => MetadataRecord::Album(album.into_album()),
    })
}

pub(crate) fn validate_for_key(kind: EntryKind, selector: &str, bytes: &[u8]) -> bool {
    match decode(kind, bytes) {
        Ok(MetadataRecord::Track(track)) => track.id == selector,
        Ok(MetadataRecord::Album(album)) => album.id == selector,
        Ok(_) => true,
        Err(_) => false,
    }
}

impl Record {
    fn matches(&self, kind: EntryKind) -> bool {
        matches!(
            (self, kind),
            (
                Self::Tracks(_),
                EntryKind::LikedTracks | EntryKind::PlaylistTracks | EntryKind::ArtistTracks
            ) | (
                Self::Playlists(_),
                EntryKind::Playlists | EntryKind::LikedPlaylists
            ) | (Self::Track(_), EntryKind::Track)
                | (Self::Album(_), EntryKind::Album)
        )
    }
    fn valid(&self) -> bool {
        match self {
            Self::Tracks(page) => {
                page.items.len() <= MAX_RECORD_ITEMS && page.items.iter().all(TrackData::valid)
            }
            Self::Playlists(page) => {
                page.items.len() <= MAX_RECORD_ITEMS && page.items.iter().all(PlaylistData::valid)
            }
            Self::Track(track) => track.valid(),
            Self::Album(album) => album.valid(),
        }
    }
}
fn valid_text(value: &str) -> bool {
    value.len() <= 4096
        && !value
            .chars()
            .any(|c| c == '\0' || (c.is_control() && !matches!(c, '\n' | '\t')))
}
fn valid_texts(values: &[String]) -> bool {
    values.len() <= MAX_CREDITS && values.iter().all(|value| valid_text(value))
}
fn valid_cover(value: &Option<String>) -> bool {
    value
        .as_ref()
        .is_none_or(|value| CoverArt::parse(value).is_some())
}
fn cover(value: &Option<CoverArt>) -> Option<String> {
    value.as_ref().map(CoverArt::cache_template)
}
impl From<&Track> for TrackData {
    fn from(track: &Track) -> Self {
        Self {
            id: track.id.clone(),
            title: track.title.clone(),
            artists: track.artists.clone(),
            artist_links: track.artist_links.iter().map(ArtistData::from).collect(),
            album_id: track.album_id.clone(),
            album_title: track.album_title.clone(),
            duration_ms: track.duration_ms,
            cover: cover(&track.cover),
            explicit: track.explicit,
            available: track.available,
        }
    }
}
impl TrackData {
    fn valid(&self) -> bool {
        validate_id(&self.id).is_ok()
            && valid_text(&self.title)
            && valid_texts(&self.artists)
            && self.artist_links.len() <= MAX_CREDITS
            && self.artist_links.iter().all(ArtistData::valid)
            && self
                .album_id
                .as_ref()
                .is_none_or(|id| validate_id(id).is_ok())
            && self
                .album_title
                .as_ref()
                .is_none_or(|title| valid_text(title))
            && valid_cover(&self.cover)
    }
    fn into_track(self) -> Track {
        Track {
            id: self.id,
            title: self.title,
            artists: self.artists,
            artist_links: self
                .artist_links
                .into_iter()
                .map(ArtistData::into_artist)
                .collect(),
            album_id: self.album_id,
            album_title: self.album_title,
            duration_ms: self.duration_ms,
            cover: self.cover.and_then(|value| CoverArt::parse(&value)),
            explicit: self.explicit,
            available: self.available,
        }
    }
}
impl From<&Artist> for ArtistData {
    fn from(artist: &Artist) -> Self {
        Self {
            id: artist.id.clone(),
            name: artist.name.clone(),
            cover: cover(&artist.cover),
        }
    }
}
impl ArtistData {
    fn valid(&self) -> bool {
        validate_id(&self.id).is_ok() && valid_text(&self.name) && valid_cover(&self.cover)
    }
    fn into_artist(self) -> Artist {
        Artist {
            id: self.id,
            name: self.name,
            cover: self.cover.and_then(|value| CoverArt::parse(&value)),
        }
    }
}
impl From<&Album> for AlbumData {
    fn from(album: &Album) -> Self {
        Self {
            id: album.id.clone(),
            title: album.title.clone(),
            artists: album.artists.clone(),
            tracks: album.tracks.iter().map(TrackData::from).collect(),
            cover: cover(&album.cover),
        }
    }
}
impl AlbumData {
    fn valid(&self) -> bool {
        validate_id(&self.id).is_ok()
            && valid_text(&self.title)
            && valid_texts(&self.artists)
            && self.tracks.len() <= MAX_RECORD_ITEMS
            && self.tracks.iter().all(TrackData::valid)
            && valid_cover(&self.cover)
    }
    fn into_album(self) -> Album {
        Album {
            id: self.id,
            title: self.title,
            artists: self.artists,
            tracks: self.tracks.into_iter().map(TrackData::into_track).collect(),
            cover: self.cover.and_then(|value| CoverArt::parse(&value)),
        }
    }
}
impl From<&Playlist> for PlaylistData {
    fn from(playlist: &Playlist) -> Self {
        Self {
            owner: playlist.id.owner.clone(),
            kind: playlist.id.kind.clone(),
            title: playlist.title.clone(),
            track_count: playlist.track_count,
            cover: cover(&playlist.cover),
        }
    }
}
impl PlaylistData {
    fn valid(&self) -> bool {
        validate_id(&self.owner).is_ok()
            && validate_id(&self.kind).is_ok()
            && valid_text(&self.title)
            && valid_cover(&self.cover)
    }
    fn into_playlist(self) -> Playlist {
        Playlist {
            id: PlaylistId {
                owner: self.owner,
                kind: self.kind,
            },
            title: self.title,
            track_count: self.track_count,
            cover: self.cover.and_then(|value| CoverArt::parse(&value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track() -> Track {
        Track {
            id: "42:7".into(),
            title: "Original synthetic track".into(),
            artists: vec!["Synthetic artist".into()],
            artist_links: vec![Artist {
                id: "6".into(),
                name: "Synthetic artist".into(),
                cover: None,
            }],
            album_id: Some("7".into()),
            album_title: Some("Synthetic album".into()),
            duration_ms: Some(123_000),
            cover: CoverArt::parse("avatars.yandex.net/get-music-content/original-test/%%"),
            explicit: false,
            available: true,
        }
    }
    #[test]
    fn every_cached_metadata_variant_round_trips_without_secrets_or_media_fields() {
        let track = track();
        let values = [
            (EntryKind::Track, MetadataRecord::Track(track.clone())),
            (
                EntryKind::Album,
                MetadataRecord::Album(Album {
                    id: "7".into(),
                    title: "Synthetic album".into(),
                    artists: track.artists.clone(),
                    tracks: vec![track.clone()],
                    cover: track.cover.clone(),
                }),
            ),
            (
                EntryKind::LikedTracks,
                MetadataRecord::Tracks(Page {
                    items: vec![track.clone(), track.clone()],
                    has_more: true,
                }),
            ),
            (
                EntryKind::PlaylistTracks,
                MetadataRecord::Tracks(Page {
                    items: vec![track.clone()],
                    has_more: false,
                }),
            ),
            (
                EntryKind::ArtistTracks,
                MetadataRecord::Tracks(Page {
                    items: vec![track],
                    has_more: false,
                }),
            ),
            (
                EntryKind::Playlists,
                MetadataRecord::Playlists(Page {
                    items: vec![Playlist {
                        id: PlaylistId {
                            owner: "123".into(),
                            kind: "5".into(),
                        },
                        title: "Synthetic playlist".into(),
                        track_count: 2,
                        cover: None,
                    }],
                    has_more: false,
                }),
            ),
        ];
        for (kind, value) in values {
            let bytes = encode(kind, &value).expect("typed synthetic record");
            assert_eq!(decode(kind, &bytes).unwrap(), value);
            let text = String::from_utf8(bytes).unwrap();
            for forbidden in [
                "access_token",
                "refresh_token",
                "sessionid",
                "cookie",
                "get-mp3",
                "storage.yandex.net",
            ] {
                assert!(!text.contains(forbidden));
            }
        }
    }
    #[test]
    fn cache_decoder_rejects_unknown_fields_invalid_ids_and_signed_cover_addresses() {
        let original = encode(EntryKind::Track, &MetadataRecord::Track(track())).unwrap();
        let mut data: serde_json::Value = serde_json::from_slice(&original).unwrap();
        data["value"]["access_token"] = "synthetic-must-not-persist".into();
        assert_eq!(
            decode(EntryKind::Track, &serde_json::to_vec(&data).unwrap()).unwrap_err(),
            CacheError::InvalidData
        );
        let mut data: serde_json::Value = serde_json::from_slice(&original).unwrap();
        data["value"]["id"] = "https://invalid.example/private".into();
        assert!(decode(EntryKind::Track, &serde_json::to_vec(&data).unwrap()).is_err());
        let mut data: serde_json::Value = serde_json::from_slice(&original).unwrap();
        data["value"]["cover"] =
            "https://storage.yandex.net/get-mp3/synthetic-signature?token=synthetic".into();
        assert!(decode(EntryKind::Track, &serde_json::to_vec(&data).unwrap()).is_err());
        assert!(decode(EntryKind::Album, &original).is_err());
        assert!(!validate_for_key(EntryKind::Track, "43", &original));
        assert!(validate_for_key(EntryKind::Track, "42:7", &original));
    }
    #[test]
    fn corrupt_or_oversized_display_and_artist_links_do_not_become_cache_capabilities() {
        let mut value = track();
        value.artist_links[0].id = "../invalid".into();
        assert!(encode(EntryKind::Track, &MetadataRecord::Track(value)).is_err());
        let mut value = track();
        value.title = "x".repeat(4097);
        assert!(encode(EntryKind::Track, &MetadataRecord::Track(value)).is_err());
        assert!(decode(EntryKind::Track, b"{\"record\":\"token\",\"value\":{}}").is_err());
        assert!(decode(EntryKind::Track, &vec![b'x'; MAX_BYTES + 1]).is_err());
    }
}
