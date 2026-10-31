// SPDX-License-Identifier: GPL-3.0-or-later
//! Deliberate playlist creation with independent server confirmation.
//!
//! The form/endpoint contract is corroborated by Cassette v0.2.4 at
//! 9d34251d01bcd414ffbec2bef7931090d33c9124, yamu 0.1.2 at
//! 9803f319b9ea6f81befbc3117e4592132ed07d0e and yamuse at
//! 368c74646520e8bcd30c44a9887e1bf99cb58172. This is an original adapter;
//! no upstream OAuth identity, implementation or signing material is imported.

use super::{MusicError, MusicService, Playlist, PlaylistWire, playlist_from_wire, read_body};
use crate::{music::interactions::InteractionError, session::AccessToken};
use serde::Deserialize;
use serde_json::value::RawValue;
use zeroize::Zeroizing;

/// An explicit server visibility value, not a caller-supplied form field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaylistVisibility {
    #[default]
    Private,
    Public,
}

impl PlaylistVisibility {
    fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Public => "public",
        }
    }
}

impl MusicService {
    /// Create exactly once for the currently validated account, then read the
    /// returned owner/kind from the server again before reporting success.
    ///
    /// The title is trimmed and limited to 200 characters/1024 UTF-8 bytes.
    /// A timeout, malformed success or lost confirmation may follow an accepted
    /// write. Callers must offer GET-only library refresh, never automatic replay
    /// or an optimistic local playlist. Local cancellation is not server rollback.
    pub fn create_playlist(
        &self,
        token: &AccessToken,
        title: &str,
        visibility: PlaylistVisibility,
    ) -> Result<Playlist, InteractionError> {
        let title = title.trim();
        if title.is_empty()
            || title.len() > 1024
            || title.chars().count() > 200
            || title.chars().any(char::is_control)
        {
            return Err(MusicError::InvalidInput.into());
        }
        let binding = self.search_binding(token)?;
        let mut form_url = self.origin.clone();
        form_url
            .query_pairs_mut()
            .append_pair("title", title)
            .append_pair("visibility", visibility.as_str());
        let form = Zeroizing::new(form_url.query().unwrap_or_default().as_bytes().to_vec());
        self.check_search_binding(token, &binding)?;
        let response = self
            .request_response(
                token,
                &["users", &binding.uid, "playlists", "create"],
                &[],
                Some(form),
            )
            .and_then(read_body)
            .and_then(|body| decode_created(&body));
        // An accepted write must not be allowed to repopulate a logged-out or
        // replaced session even if the network response arrives afterward.
        self.check_search_binding(token, &binding)?;
        let wire = match response {
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
        if wire
            .visibility
            .as_deref()
            .is_some_and(|value| value != visibility.as_str())
        {
            return Err(InteractionError::WriteOutcomeUnknown);
        }
        let created =
            playlist_from_wire(wire.playlist).map_err(|_| InteractionError::WriteOutcomeUnknown)?;
        if created.id.owner != binding.uid || created.title != title {
            return Err(InteractionError::WriteOutcomeUnknown);
        }
        let confirmed = self
            .request_response(
                token,
                &["users", &binding.uid, "playlists", &created.id.kind],
                &[],
                None,
            )
            .and_then(read_body)
            .and_then(|body| decode_created(&body))
            .map_err(confirmation_error)?;
        self.check_search_binding(token, &binding)?;
        match confirmed.visibility.as_deref() {
            None => return Err(InteractionError::ConfirmationUnavailable),
            Some(value) if value != visibility.as_str() => {
                return Err(InteractionError::ConfirmationMismatch);
            }
            Some(_) => {}
        }
        let confirmed = playlist_from_wire(confirmed.playlist)
            .map_err(|_| InteractionError::ConfirmationUnavailable)?;
        if confirmed.id != created.id || confirmed.title != title {
            return Err(InteractionError::ConfirmationMismatch);
        }
        Ok(confirmed)
    }
}

#[derive(Deserialize)]
struct CreatedWire {
    #[serde(flatten)]
    playlist: PlaylistWire,
    visibility: Option<String>,
}

#[derive(Deserialize)]
struct CreatedEnvelope {
    result: Option<CreatedWire>,
    error: Option<Box<RawValue>>,
}

fn decode_created(body: &[u8]) -> Result<CreatedWire, MusicError> {
    let envelope: CreatedEnvelope =
        serde_json::from_slice(body).map_err(|_| MusicError::InvalidResponse)?;
    if envelope.error.is_some() {
        return Err(MusicError::InvalidResponse);
    }
    envelope.result.ok_or(MusicError::InvalidResponse)
}

fn confirmation_error(error: MusicError) -> InteractionError {
    match error {
        MusicError::AccessDenied | MusicError::Unauthenticated => InteractionError::Music(error),
        error if matches!(error.http_status(), Some(403 | 451)) => InteractionError::Music(error),
        _ => InteractionError::ConfirmationUnavailable,
    }
}

#[cfg(test)]
mod tests;
