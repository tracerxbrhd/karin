// SPDX-License-Identifier: GPL-3.0-or-later
//! Owned playlist changes with exact revision/occurrence checks and readback.
//!
//! Contract references: MarshalX/yandex-music-api 9ca132e5c18a72aab779c1fc68c169380a9357d0
//! `_client/playlists.py`, `utils/difference.py`, and Cassette v0.2.4
//! 9d34251d01bcd414ffbec2bef7931090d33c9124 `client/api/yam-client.vala`.
//! The published `/change` accepts one revision-bound JSON operation batch;
//! Cassette's `change-relative` is an unimplemented stub and is not used.
//! Metadata/delete routes have no documented compare-and-swap revision form.
//! They use exact fresh preflight plus independent readback, never invented CAS.

use super::interactions::InteractionError;
pub use super::playlist_creation::PlaylistVisibility;
use super::{
    AccountCache, Id, MusicError, MusicService, Playlist, PlaylistId, PlaylistWire, SearchBinding,
    Track, TrackWire, playlist_from_wire, read_body, track_from_wire, validate_id,
};
use crate::session::AccessToken;
use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};
use std::{
    fmt,
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

/// A revision actually supplied by Music, including explicit zero if supplied.
/// Missing revision is never synthesized as zero or the clients' default one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlaylistRevision(u64);
impl PlaylistRevision {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PlaylistOccurrence {
    pub position: usize,
    /// Bare provider track identity, distinct from its occurrence position.
    pub id: String,
    pub album_id: Option<String>,
    pub track: Option<Track>,
}
impl PlaylistOccurrence {
    pub fn selector(&self) -> String {
        self.album_id
            .as_ref()
            .map_or_else(|| self.id.clone(), |album| format!("{}:{album}", self.id))
    }
}
impl fmt::Debug for PlaylistOccurrence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlaylistOccurrence([redacted])")
    }
}

/// Complete, current, owned server state. Never built from a loaded UI prefix.
/// The account receipt is process-only and is never serialized.
#[derive(Clone)]
pub struct EditablePlaylist {
    pub playlist: Playlist,
    pub revision: PlaylistRevision,
    pub visibility: Option<PlaylistVisibility>,
    pub entries: Vec<PlaylistOccurrence>,
    binding: SearchBinding,
}
impl fmt::Debug for EditablePlaylist {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EditablePlaylist([redacted])")
    }
}

#[derive(Clone)]
pub enum PlaylistChange {
    Insert {
        track: Track,
        at: usize,
    },
    Remove {
        position: usize,
    },
    /// `to` is the final position after removing the selected occurrence.
    Move {
        from: usize,
        to: usize,
    },
    Rename(String),
    Visibility(PlaylistVisibility),
    Delete,
}
impl fmt::Debug for PlaylistChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlaylistChange([redacted])")
    }
}
#[derive(Clone, Debug)]
pub enum PlaylistMutationOutcome {
    Confirmed(Box<EditablePlaylist>),
    Deleted,
}

const MAX_EDIT_POSITIONS: usize = 2_000;

pub(super) struct WriteGuard(Arc<Mutex<AccountCache>>);
impl Drop for WriteGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock() {
            state.playlist_write = false;
        }
    }
}

impl MusicService {
    /// Memory-only eligibility; it grants no collaborative editing right.
    pub fn owns_playlist(&self, token: &AccessToken, id: &PlaylistId) -> Result<bool, MusicError> {
        validate_id(&id.owner)?;
        validate_id(&id.kind)?;
        Ok(self.account_id(token)? == id.owner)
    }

    /// One coherent own-list read for the native add picker. This does not
    /// concatenate independently fetched pages or grant any editing right.
    pub fn playlist_edit_choices(&self, token: &AccessToken) -> Result<Vec<Playlist>, MusicError> {
        let binding = self.search_binding(token)?;
        self.own_choices_bound(token, &binding)
    }

    fn own_choices_bound(
        &self,
        token: &AccessToken,
        binding: &SearchBinding,
    ) -> Result<Vec<Playlist>, MusicError> {
        self.check_search_binding(token, binding)?;
        let response = self.request_response(
            token,
            &["users", &binding.uid, "playlists", "list"],
            &[],
            None,
        )?;
        let body = read_body(response)?;
        self.check_search_binding(token, binding)?;
        let envelope: ListEnvelope =
            serde_json::from_slice(&body).map_err(|_| MusicError::InvalidResponse)?;
        if envelope.error.is_some() {
            return Err(MusicError::InvalidResponse);
        }
        if envelope.result.len() > MAX_EDIT_POSITIONS {
            return Err(MusicError::ResponseTooLarge);
        }
        let mut choices = Vec::new();
        for wire in envelope.result {
            let owner = wire_owner(&wire)?;
            validate_id(owner).map_err(|_| MusicError::InvalidResponse)?;
            let owned = owner == binding.uid;
            let playlist = playlist_from_wire(wire)?;
            if owned {
                if choices
                    .iter()
                    .any(|other: &Playlist| other.id == playlist.id)
                {
                    return Err(MusicError::InvalidResponse);
                }
                choices.push(playlist);
            }
        }
        self.check_search_binding(token, binding)?;
        Ok(choices)
    }

    /// Read complete server positions, revision and optional visibility. A
    /// partial/missing sequence, missing revision or foreign owner is an error.
    /// Run on the existing bounded application worker.
    pub fn playlist_edit_state(
        &self,
        token: &AccessToken,
        id: &PlaylistId,
    ) -> Result<EditablePlaylist, InteractionError> {
        let binding = self.search_binding(token)?;
        self.edit_state_bound(token, id, &binding)
    }

    /// GET-only recovery, including separately established deletion. A 404
    /// alone is insufficient: the successful current own-list must omit it too.
    pub fn reconcile_playlist(
        &self,
        token: &AccessToken,
        id: &PlaylistId,
    ) -> Result<Option<EditablePlaylist>, InteractionError> {
        let binding = self.search_binding(token)?;
        self.reconcile_bound(token, id, &binding)
    }

    /// One user intent, at most one write. The source snapshot is re-read
    /// immediately before sending; no positions come from a partial GTK view.
    /// Ambiguous acknowledgment causes read-only reconciliation and stays
    /// Unknown, never an automatic POST retry or optimistic local success.
    pub fn edit_playlist(
        &self,
        token: &AccessToken,
        snapshot: &EditablePlaylist,
        change: PlaylistChange,
    ) -> Result<PlaylistMutationOutcome, InteractionError> {
        self.check_search_binding(token, &snapshot.binding)?;
        let _write = self.claim_playlist_write(token, &snapshot.binding)?;
        let current = self.edit_state_bound(token, &snapshot.playlist.id, &snapshot.binding)?;
        if !same_state(&current, snapshot) {
            return Err(InteractionError::Conflict);
        }
        let intended = Intended::new(&current, change)?;
        self.check_search_binding(token, &snapshot.binding)?;
        let path = [
            "users",
            snapshot.playlist.id.owner.as_str(),
            "playlists",
            snapshot.playlist.id.kind.as_str(),
            intended.route,
        ];
        let form = encode_form(&self.origin, &intended.form);
        let acknowledgement = self
            .request_response(token, &path, &[], Some(form))
            .and_then(read_body)
            .and_then(|body| intended.acknowledge(&body, &current));
        self.check_search_binding(token, &snapshot.binding)?;
        // Even a lost response may follow an accepted write. Reconcile with
        // reads only, while preserving the uncertainty of the write receipt.
        let observed = self.reconcile_bound(token, &snapshot.playlist.id, &snapshot.binding);
        self.check_search_binding(token, &snapshot.binding)?;
        let acknowledged_revision = match acknowledgement {
            Ok(revision) => revision,
            Err(
                error @ (MusicError::AccessDenied
                | MusicError::Unauthenticated
                | MusicError::Forbidden
                | MusicError::MissingScopes
                | MusicError::Restricted
                | MusicError::RateLimited),
            ) => return Err(error.into()),
            Err(MusicError::Http(409 | 412)) => return Err(InteractionError::Conflict),
            Err(_) => return Err(InteractionError::WriteOutcomeUnknown),
        };
        let observed = observed.map_err(confirmation_error)?;
        match (intended.deleted, observed) {
            (true, None) => Ok(PlaylistMutationOutcome::Deleted),
            (true, Some(_)) | (false, None) => Err(InteractionError::ConfirmationMismatch),
            (false, Some(observed)) => {
                if !intended.matches(&observed)
                    || acknowledged_revision.is_some_and(|revision| observed.revision < revision)
                {
                    return Err(
                        if acknowledged_revision
                            .is_some_and(|revision| observed.revision > revision)
                        {
                            InteractionError::Conflict
                        } else {
                            InteractionError::ConfirmationMismatch
                        },
                    );
                }
                Ok(PlaylistMutationOutcome::Confirmed(Box::new(observed)))
            }
        }
    }

    pub(super) fn claim_playlist_write(
        &self,
        token: &AccessToken,
        binding: &SearchBinding,
    ) -> Result<WriteGuard, InteractionError> {
        self.check_search_binding(token, binding)?;
        let mut state = self.account.lock().map_err(|_| MusicError::Configuration)?;
        if state.epoch != binding.epoch
            || token.is_expired()
            || !state
                .validated
                .as_ref()
                .is_some_and(|(key, uid)| *key == binding.token_key && *uid == binding.uid)
        {
            return Err(MusicError::Unauthenticated.into());
        }
        if state.playlist_write {
            return Err(InteractionError::Conflict);
        }
        state.playlist_write = true;
        Ok(WriteGuard(self.account.clone()))
    }

    fn edit_state_bound(
        &self,
        token: &AccessToken,
        id: &PlaylistId,
        binding: &SearchBinding,
    ) -> Result<EditablePlaylist, InteractionError> {
        validate_owned(id, binding)?;
        self.check_search_binding(token, binding)?;
        let response = self.request_response(
            token,
            &["users", &id.owner, "playlists", &id.kind],
            &[("richTracks", "true")],
            None,
        )?;
        let body = read_body(response)?;
        self.check_search_binding(token, binding)?;
        decode_state(&body, id, binding.clone()).map_err(Into::into)
    }

    fn reconcile_bound(
        &self,
        token: &AccessToken,
        id: &PlaylistId,
        binding: &SearchBinding,
    ) -> Result<Option<EditablePlaylist>, InteractionError> {
        match self.edit_state_bound(token, id, binding) {
            Ok(state) => Ok(Some(state)),
            Err(InteractionError::Music(MusicError::Http(404 | 410))) => {
                if self
                    .own_choices_bound(token, binding)?
                    .iter()
                    .any(|playlist| playlist.id == *id)
                {
                    return Err(InteractionError::ConfirmationMismatch);
                }
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
}

fn validate_owned(id: &PlaylistId, binding: &SearchBinding) -> Result<(), MusicError> {
    validate_id(&id.owner)?;
    validate_id(&id.kind)?;
    if id.owner != binding.uid {
        return Err(MusicError::InvalidInput);
    }
    Ok(())
}
fn same_entries(a: &[PlaylistOccurrence], b: &[PlaylistOccurrence]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.position == b.position && a.id == b.id && a.album_id == b.album_id)
}
fn same_state(a: &EditablePlaylist, b: &EditablePlaylist) -> bool {
    a.playlist.id == b.playlist.id
        && a.revision == b.revision
        && a.playlist.title == b.playlist.title
        && a.visibility == b.visibility
        && same_entries(&a.entries, &b.entries)
}

#[derive(Deserialize)]
struct EditEnvelope {
    result: Option<EditWire>,
    error: Option<Box<RawValue>>,
}
#[derive(Deserialize)]
struct ListEnvelope {
    result: Vec<PlaylistWire>,
    error: Option<Box<RawValue>>,
}
#[derive(Deserialize)]
struct EditWire {
    #[serde(flatten)]
    playlist: PlaylistWire,
    // Positions/identities form the editing contract. Optional rich metadata
    // must not make a correct authoritative occurrence disappear or shift.
    tracks: Option<Vec<EditReferenceWire>>,
    revision: Option<u64>,
    visibility: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditReferenceWire {
    id: Id,
    album_id: Option<Id>,
    track: Option<Box<RawValue>>,
}
fn decode_wire(body: &[u8]) -> Result<EditWire, MusicError> {
    let envelope: EditEnvelope =
        serde_json::from_slice(body).map_err(|_| MusicError::InvalidResponse)?;
    if envelope.error.is_some() {
        return Err(MusicError::InvalidResponse);
    }
    envelope.result.ok_or(MusicError::InvalidResponse)
}
fn wire_owner(wire: &PlaylistWire) -> Result<&str, MusicError> {
    let owner = wire
        .owner
        .as_ref()
        .and_then(|owner| owner.uid.as_ref())
        .map(|id| id.0.as_str());
    let uid = wire.uid.as_ref().map(|id| id.0.as_str());
    if owner.zip(uid).is_some_and(|(a, b)| a != b) {
        return Err(MusicError::InvalidResponse);
    }
    owner.or(uid).ok_or(MusicError::InvalidResponse)
}
fn decode_state(
    body: &[u8],
    id: &PlaylistId,
    binding: SearchBinding,
) -> Result<EditablePlaylist, MusicError> {
    let mut wire = decode_wire(body)?;
    if wire_owner(&wire.playlist)? != id.owner
        || wire.playlist.kind.as_ref().map(|kind| kind.0.as_str()) != Some(id.kind.as_str())
    {
        return Err(MusicError::InvalidResponse);
    }
    let revision = PlaylistRevision(wire.revision.ok_or(MusicError::InvalidResponse)?);
    let visibility = match wire.visibility.as_deref() {
        Some("private") => Some(PlaylistVisibility::Private),
        Some("public") => Some(PlaylistVisibility::Public),
        None => None,
        Some(_) => return Err(MusicError::InvalidResponse),
    };
    let count = wire
        .playlist
        .track_count
        .ok_or(MusicError::InvalidResponse)?;
    let refs = match wire.tracks.take() {
        Some(refs) => refs,
        None if count == 0 => Vec::new(),
        None => return Err(MusicError::InvalidResponse),
    };
    if refs.len() > MAX_EDIT_POSITIONS || count > MAX_EDIT_POSITIONS as u64 {
        return Err(MusicError::ResponseTooLarge);
    }
    if count != refs.len() as u64 {
        return Err(MusicError::InvalidResponse);
    }
    let entries = refs
        .into_iter()
        .enumerate()
        .map(|(position, entry)| {
            let (id, album_id) = reference(&entry.id.0, entry.album_id.map(|id| id.0).as_deref())
                .map_err(|_| MusicError::InvalidResponse)?;
            let selector = album_id
                .as_ref()
                .map_or_else(|| id.clone(), |album| format!("{id}:{album}"));
            let track = optional_rich_track(entry.track.as_deref(), &id, &selector)?;
            Ok(PlaylistOccurrence {
                position,
                id,
                album_id,
                track,
            })
        })
        .collect::<Result<Vec<_>, MusicError>>()?;
    Ok(EditablePlaylist {
        playlist: playlist_from_wire(wire.playlist)?,
        revision,
        visibility,
        entries,
        binding,
    })
}
fn optional_rich_track(
    raw: Option<&RawValue>,
    id: &str,
    selector: &str,
) -> Result<Option<Track>, MusicError> {
    #[derive(Deserialize)]
    struct RichIdentity {
        id: Option<Box<RawValue>>,
    }
    let Some(raw) = raw else {
        return Ok(None);
    };
    // A non-object display field has no admissible rich identity. Preserve the
    // separately checked occurrence, rather than pretending this is a track.
    if !raw.get().trim_start().starts_with('{') {
        return Ok(None);
    }
    let rich: RichIdentity =
        serde_json::from_str(raw.get()).map_err(|_| MusicError::InvalidResponse)?;
    let Some(rich_id) = rich.id else {
        return Ok(None);
    };
    let rich_id: Id =
        serde_json::from_str(rich_id.get()).map_err(|_| MusicError::InvalidResponse)?;
    if rich_id.0 != id && rich_id.0 != selector {
        return Err(MusicError::InvalidResponse);
    }
    Ok(serde_json::from_str::<TrackWire>(raw.get())
        .ok()
        .and_then(|wire| track_from_wire(wire).ok())
        .map(|mut track| {
            track.id = selector.into();
            track
        }))
}
fn reference(id: &str, album: Option<&str>) -> Result<(String, Option<String>), MusicError> {
    validate_id(id)?;
    let parts = id.split(':').collect::<Vec<_>>();
    if parts.len() > 2 || parts.iter().any(|part| part.is_empty()) {
        return Err(MusicError::InvalidInput);
    }
    if let Some(album) = album {
        validate_id(album)?;
        if album.contains(':') {
            return Err(MusicError::InvalidInput);
        }
    }
    if parts.get(1).zip(album).is_some_and(|(a, b)| *a != b) {
        return Err(MusicError::InvalidInput);
    }
    Ok((
        parts[0].into(),
        parts
            .get(1)
            .map(|album| (*album).to_owned())
            .or_else(|| album.map(str::to_owned)),
    ))
}

struct Intended {
    route: &'static str,
    form: Vec<(String, String)>,
    title: String,
    visibility: Option<PlaylistVisibility>,
    entries: Vec<PlaylistOccurrence>,
    before_revision: PlaylistRevision,
    content_change: bool,
    deleted: bool,
}
impl Intended {
    fn new(before: &EditablePlaylist, change: PlaylistChange) -> Result<Self, InteractionError> {
        let mut target = Self {
            route: "change",
            form: Vec::new(),
            title: before.playlist.title.clone(),
            visibility: before.visibility,
            entries: before.entries.clone(),
            before_revision: before.revision,
            content_change: false,
            deleted: false,
        };
        let diff = match change {
            PlaylistChange::Insert { track, at } => {
                if at > target.entries.len() || target.entries.len() >= MAX_EDIT_POSITIONS {
                    return Err(MusicError::InvalidInput.into());
                }
                let (id, album_id) = reference(&track.id, track.album_id.as_deref())?;
                let album = album_id.clone().ok_or(MusicError::InvalidInput)?;
                target.entries.insert(
                    at,
                    PlaylistOccurrence {
                        position: at,
                        id: id.clone(),
                        album_id,
                        track: Some(track),
                    },
                );
                Some(json!([{"op":"insert","at":at,"tracks":[{"id":id,"albumId":album}]}]))
            }
            PlaylistChange::Remove { position } => {
                if position >= target.entries.len() {
                    return Err(MusicError::InvalidInput.into());
                }
                target.entries.remove(position);
                Some(json!([{"op":"delete","from":position,"to":position+1}]))
            }
            PlaylistChange::Move { from, to } => {
                if from >= target.entries.len() || to >= target.entries.len() || from == to {
                    return Err(MusicError::InvalidInput.into());
                }
                let moved = target.entries.remove(from);
                let album = moved.album_id.clone().ok_or(MusicError::InvalidInput)?;
                let id = moved.id.clone();
                target.entries.insert(to, moved);
                Some(
                    json!([{"op":"delete","from":from,"to":from+1},{"op":"insert","at":to,"tracks":[{"id":id,"albumId":album}]}]),
                )
            }
            PlaylistChange::Rename(title) => {
                let title = title.trim();
                if title.is_empty()
                    || title.len() > 1024
                    || title.chars().count() > 200
                    || title.chars().any(char::is_control)
                    || title == target.title
                {
                    return Err(MusicError::InvalidInput.into());
                }
                target.route = "name";
                target.title = title.into();
                target.form.push(("value".into(), title.into()));
                None
            }
            PlaylistChange::Visibility(visibility) => {
                if target.visibility.is_none() {
                    return Err(InteractionError::ConfirmationUnavailable);
                }
                if target.visibility == Some(visibility) {
                    return Err(MusicError::InvalidInput.into());
                }
                target.route = "visibility";
                target.visibility = Some(visibility);
                target
                    .form
                    .push(("value".into(), visibility.as_str().into()));
                None
            }
            PlaylistChange::Delete => {
                target.route = "delete";
                target.deleted = true;
                None
            }
        };
        if let Some(diff) = diff {
            before
                .revision
                .0
                .checked_add(1)
                .ok_or(InteractionError::Conflict)?;
            target.content_change = true;
            target.form = vec![
                ("kind".into(), before.playlist.id.kind.clone()),
                ("revision".into(), before.revision.0.to_string()),
                ("diff".into(), diff.to_string()),
            ];
            for (position, entry) in target.entries.iter_mut().enumerate() {
                entry.position = position;
            }
        }
        Ok(target)
    }
    fn acknowledge(
        &self,
        body: &[u8],
        before: &EditablePlaylist,
    ) -> Result<Option<PlaylistRevision>, MusicError> {
        if self.deleted {
            #[derive(Deserialize)]
            struct Ack {
                result: Value,
                error: Option<Box<RawValue>>,
            }
            let ack: Ack = serde_json::from_slice(body).map_err(|_| MusicError::InvalidResponse)?;
            return if ack.error.is_none() && (ack.result == "ok" || ack.result == true) {
                Ok(None)
            } else {
                Err(MusicError::InvalidResponse)
            };
        }
        let ack = decode_wire(body)?;
        if wire_owner(&ack.playlist)? != before.playlist.id.owner
            || ack.playlist.kind.as_ref().map(|kind| kind.0.as_str())
                != Some(before.playlist.id.kind.as_str())
        {
            return Err(MusicError::InvalidResponse);
        }
        let revision = PlaylistRevision(ack.revision.ok_or(MusicError::InvalidResponse)?);
        if revision < self.before_revision
            || self.content_change && revision <= self.before_revision
        {
            return Err(MusicError::InvalidResponse);
        }
        Ok(Some(revision))
    }
    fn matches(&self, observed: &EditablePlaylist) -> bool {
        observed.playlist.title == self.title
            && observed.visibility == self.visibility
            && same_entries(&observed.entries, &self.entries)
            && observed.revision >= self.before_revision
            && (!self.content_change || observed.revision > self.before_revision)
    }
}
fn encode_form(origin: &reqwest::Url, values: &[(String, String)]) -> Zeroizing<Vec<u8>> {
    let mut form = origin.clone();
    form.query_pairs_mut()
        .extend_pairs(values.iter().map(|(a, b)| (a.as_str(), b.as_str())));
    Zeroizing::new(form.query().unwrap_or_default().as_bytes().to_vec())
}
fn confirmation_error(error: InteractionError) -> InteractionError {
    match error {
        InteractionError::Music(
            error @ (MusicError::AccessDenied
            | MusicError::Unauthenticated
            | MusicError::Forbidden
            | MusicError::MissingScopes
            | MusicError::Restricted),
        ) => InteractionError::Music(error),
        InteractionError::ConfirmationMismatch => InteractionError::ConfirmationMismatch,
        _ => InteractionError::ConfirmationUnavailable,
    }
}

#[cfg(test)]
mod tests;
