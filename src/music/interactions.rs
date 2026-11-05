// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit account-bound library interactions, independent of authentication UI.
//!
//! Endpoint/form knowledge was compared with yamu 0.1.2 at
//! 9803f319b9ea6f81befbc3117e4592132ed07d0e and Cassette v0.2.4 at
//! 9d34251d01bcd414ffbec2bef7931090d33c9124. No upstream OAuth or signing code
//! is used. Writes are never retried automatically, including uncertain outcomes.
//!
//! Distinct track Dislike/readback and mutually exclusive reaction semantics were
//! compared with MarshalX/yandex-music-api at
//! 9ca132e5c18a72aab779c1fc68c169380a9357d0, `_client/likes.py`. Its account
//! libraries are separate from recommendation-only radio feedback. No radio
//! event or playback change follows from a library reaction here.

use super::{
    Id, MAX_COLLECTION, MusicError, MusicService, Page, Playlist, PlaylistWire, TrackReference,
    local_page, playlist_from_wire, validate_id,
};
use crate::session::AccessToken;
use serde::Deserialize;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InteractionError {
    #[error(transparent)]
    Music(#[from] MusicError),
    #[error("The library changed; refresh before trying another action")]
    Conflict,
    #[error(
        "The change may have reached Music, but no reliable write result was received; refresh the library"
    )]
    WriteOutcomeUnknown,
    #[error("The change was sent, but its outcome could not be confirmed; refresh the library")]
    ConfirmationUnavailable,
    #[error("The library did not confirm the requested change; refresh before trying again")]
    ConfirmationMismatch,
}

/// Confirmed current state, not an optimistic local toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LikeState {
    pub liked: bool,
}

/// Separate server-confirmed account marks, never an optimistic toggle. The
/// supported account contract does not admit both marks on the same track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackReactions {
    pub liked: bool,
    pub disliked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackReaction {
    Like,
    Dislike,
}

impl TrackReaction {
    fn library(self) -> &'static str {
        match self {
            Self::Like => "likes",
            Self::Dislike => "dislikes",
        }
    }

    fn marked(self, state: TrackReactions) -> bool {
        match self {
            Self::Like => state.liked,
            Self::Dislike => state.disliked,
        }
    }
}

impl MusicService {
    /// Read both independent account libraries using one validated account epoch.
    /// A missing, unavailable or contradictory library is not a false mark.
    pub fn track_reactions(
        &self,
        token: &AccessToken,
        id: &str,
    ) -> Result<TrackReactions, InteractionError> {
        validate_id(id)?;
        let binding = self.search_binding(token)?;
        let likes = self.reaction_membership(token, &binding, TrackReaction::Like)?;
        let dislikes = self.reaction_membership(token, &binding, TrackReaction::Dislike)?;
        self.check_search_binding(token, &binding)?;
        let state = TrackReactions {
            liked: likes.contains(id),
            disliked: dislikes.contains(id),
        };
        if state.liked && state.disliked {
            return Err(MusicError::InvalidResponse.into());
        }
        Ok(state)
    }

    /// Send exactly one deliberate library reaction, then independently read
    /// both libraries. Adding Like clears Dislike and adding Dislike clears Like
    /// under the published account contract; removing either does not restore
    /// the other. Only observed server state is returned.
    ///
    /// A malformed/lost response or failed confirmation never permits a replay.
    /// The selected library's revision must not precede the write acknowledgment.
    pub fn set_track_reaction(
        &self,
        token: &AccessToken,
        id: &str,
        reaction: TrackReaction,
        enabled: bool,
    ) -> Result<TrackReactions, InteractionError> {
        validate_id(id)?;
        let binding = self.search_binding(token)?;
        let action = if enabled { "add-multiple" } else { "remove" };
        let mut form_url = self.origin.clone();
        form_url.query_pairs_mut().append_pair("track-ids", id);
        let form = Zeroizing::new(form_url.query().unwrap_or_default().as_bytes().to_vec());
        self.check_search_binding(token, &binding)?;
        let write = self.request(
            token,
            &["users", &binding.uid, reaction.library(), "tracks", action],
            &[],
            Some(form),
        );
        let write = confirmed_write_ack(write)?;
        self.check_search_binding(token, &binding)?;
        let likes = self
            .reaction_membership(token, &binding, TrackReaction::Like)
            .map_err(confirmation_error)?;
        let dislikes = self
            .reaction_membership(token, &binding, TrackReaction::Dislike)
            .map_err(confirmation_error)?;
        self.check_search_binding(token, &binding)?;
        let state = TrackReactions {
            liked: likes.contains(id),
            disliked: dislikes.contains(id),
        };
        let revision = match reaction {
            TrackReaction::Like => likes.revision,
            TrackReaction::Dislike => dislikes.revision,
        };
        if revision < write.revision
            || reaction.marked(state) != enabled
            || (state.liked && state.disliked)
        {
            return Err(InteractionError::ConfirmationMismatch);
        }
        Ok(state)
    }

    /// Set/remove the account's distinct "Do not recommend" mark. This is not
    /// Unlike, a radio feedback report, a local row hide or a Next command.
    pub fn set_track_disliked(
        &self,
        token: &AccessToken,
        id: &str,
        disliked: bool,
    ) -> Result<TrackReactions, InteractionError> {
        self.set_track_reaction(token, id, TrackReaction::Dislike, disliked)
    }

    /// Read compact membership only. No media or track expansion is requested.
    pub fn is_track_liked(&self, token: &AccessToken, id: &str) -> Result<bool, InteractionError> {
        validate_id(id)?;
        let uid = self.account_id(token)?;
        let library = self.liked_membership(token, &uid)?;
        Ok(library.contains(id))
    }

    /// Change a single track and independently confirm membership afterward.
    ///
    /// This method may perform the write before a timeout, logout or confirmation
    /// error. Callers must refresh, not silently replay the write or optimistically
    /// claim success. A token cannot choose another account's write path.
    pub fn set_track_liked(
        &self,
        token: &AccessToken,
        id: &str,
        liked: bool,
    ) -> Result<LikeState, InteractionError> {
        validate_id(id)?;
        let uid = self.account_id(token)?;
        let action = if liked { "add-multiple" } else { "remove" };
        let mut form_url = self.origin.clone();
        form_url.query_pairs_mut().append_pair("track-ids", id);
        let form = Zeroizing::new(form_url.query().unwrap_or_default().as_bytes().to_vec());
        let result: Result<RevisionWire, MusicError> = self.request(
            token,
            &["users", &uid, "likes", "tracks", action],
            &[],
            Some(form),
        );
        let write = match result {
            Err(MusicError::Http(409)) => return Err(InteractionError::Conflict),
            Err(
                MusicError::Network
                | MusicError::Timeout
                | MusicError::InvalidResponse
                | MusicError::ResponseTooLarge,
            )
            | Err(MusicError::Http(200..=399 | 500..=599)) => {
                return Err(InteractionError::WriteOutcomeUnknown);
            }
            result => result?,
        };
        let library = self
            .liked_membership(token, &uid)
            .map_err(confirmation_error)?;
        if library.revision < write.revision || library.contains(id) != liked {
            return Err(InteractionError::ConfirmationMismatch);
        }
        Ok(LikeState { liked })
    }

    /// Liked playlists retain their actual owner/kind identity. Liking a
    /// playlist does not turn it into an owned playlist or permit mutation.
    pub fn liked_playlists(
        &self,
        token: &AccessToken,
        page: u32,
    ) -> Result<Page<Playlist>, MusicError> {
        let uid = self.account_id(token)?;
        let wire: Vec<LikedPlaylistWire> =
            self.get(token, &["users", &uid, "likes", "playlists"], &[])?;
        let (wire, has_more) = local_page(wire, page)?;
        let items = wire
            .into_iter()
            .map(|entry| playlist_from_wire(entry.playlist))
            .collect::<Result<_, _>>()?;
        Ok(Page { items, has_more })
    }

    fn liked_membership(
        &self,
        token: &AccessToken,
        expected_uid: &str,
    ) -> Result<MembershipWire, MusicError> {
        if self.account_id(token)? != expected_uid {
            return Err(MusicError::Unauthenticated);
        }
        let response: MembershipEnvelope = self.get(
            token,
            &["users", expected_uid, "likes", "tracks"],
            &[("if-modified-since-revision", "0")],
        )?;
        let library = response.library.ok_or(MusicError::InvalidResponse)?;
        if library.uid.0 != expected_uid {
            return Err(MusicError::InvalidResponse);
        }
        if library.tracks.len() > MAX_COLLECTION {
            return Err(MusicError::ResponseTooLarge);
        }
        // A logout or successful revalidation while the request was in flight
        // must invalidate the result instead of repopulating a stale view.
        if self.account_id(token)? != expected_uid {
            return Err(MusicError::Unauthenticated);
        }
        Ok(library)
    }

    fn reaction_membership(
        &self,
        token: &AccessToken,
        binding: &super::SearchBinding,
        reaction: TrackReaction,
    ) -> Result<MembershipWire, MusicError> {
        self.check_search_binding(token, binding)?;
        // The published Dislike endpoint uses underscores; the already-working
        // Like endpoint keeps its independently confirmed hyphenated spelling.
        let revision_query = match reaction {
            TrackReaction::Like => "if-modified-since-revision",
            TrackReaction::Dislike => "if_modified_since_revision",
        };
        let response: MembershipEnvelope = self.get(
            token,
            &["users", &binding.uid, reaction.library(), "tracks"],
            &[(revision_query, "0")],
        )?;
        self.check_search_binding(token, binding)?;
        let library = response.library.ok_or(MusicError::InvalidResponse)?;
        if library.uid.0 != binding.uid {
            return Err(MusicError::InvalidResponse);
        }
        if library.tracks.len() > MAX_COLLECTION {
            return Err(MusicError::ResponseTooLarge);
        }
        Ok(library)
    }
}

fn confirmed_write_ack(
    result: Result<RevisionWire, MusicError>,
) -> Result<RevisionWire, InteractionError> {
    match result {
        Err(MusicError::Http(409)) => Err(InteractionError::Conflict),
        Err(
            MusicError::Network
            | MusicError::Timeout
            | MusicError::InvalidResponse
            | MusicError::ResponseTooLarge,
        )
        | Err(MusicError::Http(200..=399 | 500..=599)) => {
            Err(InteractionError::WriteOutcomeUnknown)
        }
        result => result.map_err(InteractionError::Music),
    }
}

// A rejected or locally invalidated session must reach the ordinary session
// teardown path even if the mutation itself was accepted. This does not establish
// that the earlier write failed and never authorizes replaying it.
fn confirmation_error(error: MusicError) -> InteractionError {
    match error {
        MusicError::AccessDenied | MusicError::Unauthenticated => InteractionError::Music(error),
        _ => InteractionError::ConfirmationUnavailable,
    }
}

#[derive(Deserialize)]
struct RevisionWire {
    revision: u64,
}
#[derive(Deserialize)]
struct MembershipEnvelope {
    library: Option<MembershipWire>,
}
#[derive(Deserialize)]
struct MembershipWire {
    uid: Id,
    revision: u64,
    tracks: Vec<TrackReference>,
}
impl MembershipWire {
    fn contains(&self, id: &str) -> bool {
        let bare = id.split(':').next().unwrap_or(id);
        self.tracks.iter().any(|entry| {
            let candidate = entry.id.0.split(':').next().unwrap_or(&entry.id.0);
            candidate == bare
        })
    }
}
#[derive(Deserialize)]
struct LikedPlaylistWire {
    playlist: PlaylistWire,
}

#[cfg(test)]
mod tests;
