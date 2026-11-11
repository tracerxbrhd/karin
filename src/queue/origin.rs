// SPDX-License-Identifier: GPL-3.0-or-later
//! Durable source identity, separate from ephemeral view/occurrence bindings.

use super::ContextKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(crate) enum PlaybackOrigin {
    LikedTracks,
    Search,
    Playlist {
        owner: String,
        kind: String,
    },
    Album {
        id: String,
    },
    Artist {
        id: String,
    },
    Queue,
    SingleTrack {
        id: Option<String>,
    },
    Wave,
    /// Legacy/injected contexts without an independently known server origin.
    Unknown,
}

// Serde's internally tagged *unit* variants discard the remaining object even
// with deny_unknown_fields. Empty struct variants keep the same wire shape but
// actually reject unknown fields, including on origins without any payload.
impl<'de> Deserialize<'de> for PlaybackOrigin {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
        enum Wire {
            LikedTracks {},
            Search {},
            Playlist { owner: String, kind: String },
            Album { id: String },
            Artist { id: String },
            Queue {},
            SingleTrack { id: Option<String> },
            Wave {},
            Unknown {},
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::LikedTracks {} => Self::LikedTracks,
            Wire::Search {} => Self::Search,
            Wire::Playlist { owner, kind } => Self::Playlist { owner, kind },
            Wire::Album { id } => Self::Album { id },
            Wire::Artist { id } => Self::Artist { id },
            Wire::Queue {} => Self::Queue,
            Wire::SingleTrack { id } => Self::SingleTrack { id },
            Wire::Wave {} => Self::Wave,
            Wire::Unknown {} => Self::Unknown,
        })
    }
}

impl PlaybackOrigin {
    pub(crate) fn from_source(kind: ContextKind, source: Option<&str>) -> Self {
        let id = source.filter(|value| crate::music::validate_id(value).is_ok());
        match kind {
            ContextKind::LikedTracks => Self::LikedTracks,
            ContextKind::Search => Self::Search,
            ContextKind::Playlist => id
                .and_then(|value| value.split_once(':'))
                .filter(|(owner, kind)| valid(owner) && valid(kind))
                .map_or(Self::Unknown, |(owner, kind)| Self::Playlist {
                    owner: owner.into(),
                    kind: kind.into(),
                }),
            ContextKind::Album => id
                .filter(|id| valid(id))
                .map_or(Self::Unknown, |id| Self::Album { id: id.into() }),
            ContextKind::Artist => id
                .filter(|id| valid(id))
                .map_or(Self::Unknown, |id| Self::Artist { id: id.into() }),
            ContextKind::Queue => Self::Queue,
            ContextKind::SingleTrack => Self::SingleTrack {
                id: id.map(str::to_owned),
            },
            ContextKind::Wave => Self::Wave,
        }
    }

    pub(crate) fn valid_for(&self, kind: ContextKind) -> bool {
        match (self, kind) {
            (Self::LikedTracks, ContextKind::LikedTracks)
            | (Self::Search, ContextKind::Search)
            | (Self::Queue, ContextKind::Queue)
            | (Self::Unknown, _) => true,
            (Self::Playlist { owner, kind }, ContextKind::Playlist) => valid(owner) && valid(kind),
            (Self::Album { id }, ContextKind::Album)
            | (Self::Artist { id }, ContextKind::Artist) => valid(id),
            (Self::SingleTrack { id }, ContextKind::SingleTrack) => id
                .as_deref()
                .is_none_or(|id| crate::music::validate_id(id).is_ok()),
            // Live radio provenance is never a durable playback context.
            _ => false,
        }
    }
}

fn valid(value: &str) -> bool {
    !value.contains(':') && crate::music::validate_id(value).is_ok()
}

impl std::fmt::Debug for PlaybackOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PlaybackOrigin([private source identity])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_origin_rejects_unknown_fields_including_payload_free_variants() {
        for origin in [
            PlaybackOrigin::LikedTracks,
            PlaybackOrigin::Search,
            PlaybackOrigin::Playlist {
                owner: "owner".into(),
                kind: "17".into(),
            },
            PlaybackOrigin::Album { id: "album".into() },
            PlaybackOrigin::Artist {
                id: "artist".into(),
            },
            PlaybackOrigin::Queue,
            PlaybackOrigin::SingleTrack {
                id: Some("track:album".into()),
            },
            PlaybackOrigin::SingleTrack { id: None },
            PlaybackOrigin::Wave,
            PlaybackOrigin::Unknown,
        ] {
            let mut value = serde_json::to_value(&origin).unwrap();
            assert_eq!(
                serde_json::from_value::<PlaybackOrigin>(value.clone()).unwrap(),
                origin
            );
            value["cookie"] = "synthetic-never-admitted".into();
            assert!(serde_json::from_value::<PlaybackOrigin>(value).is_err());
        }
    }
}
