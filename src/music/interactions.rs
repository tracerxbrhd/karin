// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit account-bound library interactions, independent of authentication UI.
//!
//! Endpoint/form knowledge was compared with yamu 0.1.2 at
//! 9803f319b9ea6f81befbc3117e4592132ed07d0e and Cassette v0.2.4 at
//! 9d34251d01bcd414ffbec2bef7931090d33c9124. No upstream OAuth or signing code
//! is used. Writes are never retried automatically, including uncertain outcomes.

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

impl MusicService {
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
