// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded playback context of actual service metadata and manual additions.
//! Selection records an occurrence, not the first matching track identifier.

use crate::music::Track;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

mod policy;
use policy::OrderPolicy;
mod persistence;
pub(crate) use persistence::QueueSnapshot;
mod origin;
pub(crate) use origin::PlaybackOrigin;

/// Local playback behavior. It never changes the source collection order.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RepeatMode {
    #[default]
    Off,
    Queue,
    One,
}

impl RepeatMode {
    pub fn next_mode(self) -> Self {
        match self {
            Self::Off => Self::Queue,
            Self::Queue => Self::One,
            Self::One => Self::Off,
        }
    }
}

const MAX_QUEUE: usize = 500;

/// Internal identity of one admitted queue entry, independent of its position
/// and provider Track ID. Keeping a clone alive prevents allocation reuse while
/// an observer still refers to the entry. It is never serialized or exported.
#[derive(Clone)]
pub(crate) struct OccurrenceId(Arc<()>);

impl OccurrenceId {
    fn new() -> Self {
        Self(Arc::new(()))
    }
}

impl PartialEq for OccurrenceId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for OccurrenceId {}

impl std::fmt::Debug for OccurrenceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OccurrenceId([opaque])")
    }
}

#[derive(Default)]
pub struct MusicQueue {
    context: Option<PlaybackContext>,
    occurrences: Vec<OccurrenceId>,
    order: OrderPolicy,
}

/// Provenance of the loaded ordered items, never a network request or a claim
/// that a complete remote collection has been fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContextKind {
    Search,
    LikedTracks,
    Playlist,
    Album,
    Artist,
    Queue,
    SingleTrack,
    Wave,
}

/// An admitted loaded context. Manual additions remain after its loaded prefix.
/// Track and source identifiers stay private in Debug output.
#[derive(Clone, PartialEq, Eq)]
pub struct PlaybackContext {
    source_kind: ContextKind,
    source_id: Option<String>,
    origin: PlaybackOrigin,
    items: Arc<Vec<Track>>,
    // Original loaded occurrence positions survive queue edits. Manual extras
    // have no source position; track IDs cannot identify duplicate occurrences.
    source_positions: Arc<Vec<Option<usize>>>,
    current_index: usize,
    loaded_items: usize,
    partial: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ContextError {
    #[error("The playback context exceeds the queue limit.")]
    TooLarge,
    #[error("The playback context has no tracks.")]
    Empty,
    #[error("The selected playback occurrence is outside the context.")]
    InvalidSelection,
    #[error("The playback context source key is invalid.")]
    InvalidSource,
}

impl PlaybackContext {
    /// Admit only the already loaded, service-normalized items. `partial` must
    /// be true when the items represent a loaded page/subset rather than the
    /// complete source. This layer never fetches or invents additional tracks.
    ///
    /// `source_id` is a bounded opaque local provenance key, not an endpoint or
    /// a substitute for MusicService's validated network identities.
    pub fn new(
        source_kind: ContextKind,
        source_id: Option<String>,
        items: Vec<Track>,
        current_index: usize,
        partial: bool,
    ) -> Result<Self, ContextError> {
        if items.len() > MAX_QUEUE {
            return Err(ContextError::TooLarge);
        }
        if items.is_empty() {
            return Err(ContextError::Empty);
        }
        if current_index >= items.len() {
            return Err(ContextError::InvalidSelection);
        }
        if source_id.as_ref().is_some_and(|value| {
            value.len() > 512 || value.trim().is_empty() || value.chars().any(char::is_control)
        }) {
            return Err(ContextError::InvalidSource);
        }
        let loaded_items = items.len();
        Ok(Self {
            source_kind,
            origin: PlaybackOrigin::from_source(source_kind, source_id.as_deref()),
            source_id,
            source_positions: Arc::new((0..loaded_items).map(Some).collect()),
            items: Arc::new(items),
            current_index,
            loaded_items,
            partial,
        })
    }

    /// Admit a bounded UI projection without renumbering its original source
    /// occurrences. Filtering keeps provider order and may omit positions;
    /// equal track IDs at different positions remain independent occurrences.
    pub(crate) fn with_source_positions(
        source_kind: ContextKind,
        source_id: Option<String>,
        items: Vec<Track>,
        current_index: usize,
        partial: bool,
        positions: Vec<usize>,
    ) -> Result<Self, ContextError> {
        if positions.len() != items.len()
            || positions.iter().any(|position| *position >= 2_000)
            || positions.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(ContextError::InvalidSource);
        }
        let mut context = Self::new(source_kind, source_id, items, current_index, partial)?;
        context.source_positions = Arc::new(positions.into_iter().map(Some).collect());
        Ok(context)
    }

    pub fn source_kind(&self) -> ContextKind {
        self.source_kind
    }
    pub(crate) fn origin(&self) -> &PlaybackOrigin {
        &self.origin
    }
    /// The server origin is distinct from a view's ephemeral highlight binding.
    pub(crate) fn with_origin(mut self, origin: PlaybackOrigin) -> Self {
        self.origin = origin;
        self
    }
    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }
    pub fn items(&self) -> &[Track] {
        self.items.as_slice()
    }
    pub fn current_index(&self) -> usize {
        self.current_index
    }
    /// Relative occurrence in the originally admitted loaded source, independent
    /// of the queue's current physical index. Manually appended items return
    /// None, so a source list cannot highlight a coincidentally equal ID.
    pub fn source_position(&self, index: usize) -> Option<usize> {
        self.source_positions.get(index).copied().flatten()
    }
    pub fn is_partial(&self) -> bool {
        self.partial
    }
    pub fn loaded_items_len(&self) -> usize {
        self.loaded_items
    }
    pub fn manual_items_len(&self) -> usize {
        self.items.len() - self.loaded_items
    }

    /// Select another exact occurrence without copying the loaded track list.
    /// Catalog rows can share one immutable snapshot; queue edits copy on write.
    pub fn with_current_index(&self, index: usize) -> Option<Self> {
        if index >= self.items.len() {
            return None;
        }
        let mut selected = self.clone();
        selected.current_index = index;
        Some(selected)
    }
}

impl std::fmt::Debug for PlaybackContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaybackContext")
            .field("source_kind", &self.source_kind)
            .field("source_id", &"[redacted]")
            .field("item_count", &self.items.len())
            .field("current_index", &self.current_index)
            .field("loaded_items", &self.loaded_items)
            .field("partial", &self.partial)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("The queue is full (500 tracks).")]
pub struct QueueError;

impl MusicQueue {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn with_seed(seed: u64) -> Self {
        Self {
            context: None,
            occurrences: Vec::new(),
            order: OrderPolicy::with_seed(seed),
        }
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.order.repeat
    }
    pub fn set_repeat_mode(&mut self, mode: RepeatMode) {
        self.order.repeat = mode;
    }
    pub fn shuffle(&self) -> bool {
        self.order.shuffle
    }
    pub fn set_shuffle(&mut self, enabled: bool) {
        if self.order.shuffle == enabled {
            return;
        }
        self.order.shuffle = enabled;
        if let Some(context) = &self.context {
            self.order
                .rebuild_remaining(context.items(), context.current_index);
        }
    }

    pub fn append(&mut self, track: Track) -> Result<(), QueueError> {
        if let Some(context) = self.context.as_mut() {
            if context.items.len() >= MAX_QUEUE {
                return Err(QueueError);
            }
            let index = context.items.len();
            let available = track.available;
            Arc::make_mut(&mut context.items).push(track);
            Arc::make_mut(&mut context.source_positions).push(None);
            self.order.append(index, available);
        } else {
            self.context = Some(PlaybackContext {
                source_kind: ContextKind::Queue,
                source_id: None,
                origin: PlaybackOrigin::Queue,
                items: Arc::new(vec![track]),
                source_positions: Arc::new(vec![None]),
                current_index: 0,
                loaded_items: 0,
                partial: false,
            });
            self.reset_order();
        }
        self.occurrences.push(OccurrenceId::new());
        Ok(())
    }

    /// Append one server-normalized radio batch without replacing identities
    /// of already admitted occurrences. Only an explicit Wave context accepts
    /// this operation; manual additions retain their ordinary separate meaning.
    pub(crate) fn append_wave_batch(&mut self, tracks: Vec<Track>) -> Result<(), QueueError> {
        let Some(context) = self
            .context
            .as_mut()
            .filter(|context| context.source_kind == ContextKind::Wave)
        else {
            return Err(QueueError);
        };
        if tracks.is_empty() || context.items.len().saturating_add(tracks.len()) > MAX_QUEUE {
            return Err(QueueError);
        }
        let mut position = context
            .source_positions
            .iter()
            .flatten()
            .copied()
            .max()
            .and_then(|value| value.checked_add(1))
            .ok_or(QueueError)?;
        position.checked_add(tracks.len()).ok_or(QueueError)?;
        for track in tracks {
            let index = context.items.len();
            let available = track.available;
            Arc::make_mut(&mut context.items).push(track);
            Arc::make_mut(&mut context.source_positions).push(Some(position));
            position = position.checked_add(1).ok_or(QueueError)?;
            context.loaded_items += 1;
            self.occurrences.push(OccurrenceId::new());
            self.order.append(index, available);
        }
        Ok(())
    }
    /// Keep a bounded heard prefix while preserving the current occurrence and
    /// upcoming order. Removed entries lose their provenance/Previous history.
    pub(crate) fn trim_wave_history(&mut self, keep: usize) {
        if self
            .context
            .as_ref()
            .is_none_or(|context| context.source_kind != ContextKind::Wave)
        {
            return;
        }
        let remove = self
            .current_index()
            .unwrap_or_default()
            .saturating_sub(keep);
        for _ in 0..remove {
            self.remove(0);
        }
    }

    /// A replacement keeps the user's local modes but starts a fresh bounded
    /// plan/history for this exact loaded context. No hidden remote pagination.
    pub fn replace_context(&mut self, context: PlaybackContext) {
        // Admission starts a new scope even when the caller reuses an immutable
        // source snapshot. Source positions remain provenance, not entry IDs.
        self.occurrences = (0..context.items.len())
            .map(|_| OccurrenceId::new())
            .collect();
        self.context = Some(context);
        self.reset_order();
    }

    fn reset_order(&mut self) {
        if let Some(context) = &self.context {
            self.order.reset(context.items(), context.current_index);
        } else {
            self.order.clear();
        }
    }

    pub fn context(&self) -> Option<&PlaybackContext> {
        self.context.as_ref()
    }
    pub fn items(&self) -> &[Track] {
        self.context.as_ref().map_or(&[], |context| context.items())
    }
    pub fn current_index(&self) -> Option<usize> {
        self.context.as_ref().map(PlaybackContext::current_index)
    }
    pub fn current(&self) -> Option<&Track> {
        self.current_index()
            .and_then(|index| self.items().get(index))
    }

    pub(crate) fn occurrence_id(&self, index: usize) -> Option<&OccurrenceId> {
        self.occurrences.get(index)
    }

    pub(crate) fn current_occurrence_id(&self) -> Option<&OccurrenceId> {
        self.current_index()
            .and_then(|index| self.occurrence_id(index))
    }

    pub fn select(&mut self, index: usize) -> Option<&Track> {
        let context = self.context.as_mut()?;
        if index >= context.items.len() {
            return None;
        }
        if context.current_index != index {
            context.current_index = index;
            self.order.record(index);
        }
        context.items.get(index)
    }

    /// The next manual transition ignores Repeat One, skips unavailable
    /// occurrences and follows history/remaining order when shuffled.
    pub fn next_track(&mut self) -> Option<&Track> {
        let context = self.context.as_mut()?;
        let index = self.order.next(context.items(), context.current_index)?;
        context.current_index = index;
        context.items.get(index)
    }

    pub fn previous_track(&mut self) -> Option<&Track> {
        let context = self.context.as_mut()?;
        let index = self
            .order
            .previous(context.items(), context.current_index)?;
        context.current_index = index;
        context.items.get(index)
    }

    /// Natural EOS has its own policy: Repeat One re-resolves only the same
    /// occurrence once; failures are surfaced by the existing player, not retried
    /// here. Manual Next/Previous remain independent from this rule.
    pub fn advance_eos(&mut self) -> Option<&Track> {
        if self.repeat_mode() == RepeatMode::One {
            return self.current().filter(|track| track.available);
        }
        self.next_track()
    }

    /// Positions refer to the original PlaybackContext, not unique track IDs.
    /// At most one future cycle is exposed. Previewing copies bounded index
    /// policy only, never the loaded Track models, and does not consume RNG.
    pub fn upcoming_indices(&self) -> Vec<usize> {
        self.context.as_ref().map_or_else(Vec::new, |context| {
            self.order.upcoming(context.items(), context.current_index)
        })
    }
    pub fn history_indices(&self) -> Vec<usize> {
        self.order.history()
    }
    pub fn can_go_next(&self) -> bool {
        self.context
            .as_ref()
            .is_some_and(|context| self.order.has_next(context.items(), context.current_index))
    }
    /// Explicit account/application retirement resets session-local preferences.
    /// Ordinary queue clear/context replacement deliberately do not call this.
    pub fn reset_modes(&mut self) {
        self.order.shuffle = false;
        self.order.repeat = RepeatMode::Off;
        self.reset_order();
    }
    pub fn can_go_previous(&self) -> bool {
        self.context.as_ref().is_some_and(|context| {
            self.order
                .previous_index(context.items(), context.current_index)
                .is_some()
        })
    }

    pub fn remove(&mut self, index: usize) -> Option<Track> {
        let context = self.context.as_ref()?;
        if index >= context.items.len() {
            return None;
        }
        let current_removed = context.current_index == index;
        let successor = if current_removed {
            self.order
                .upcoming(context.items(), context.current_index)
                .into_iter()
                .find(|candidate| *candidate != index)
                .or_else(|| {
                    self.order
                        .previous_index(context.items(), context.current_index)
                        .filter(|candidate| *candidate != index)
                })
                .or_else(|| {
                    context
                        .items
                        .iter()
                        .enumerate()
                        .find(|(position, track)| *position != index && track.available)
                        .map(|(position, _)| position)
                })
        } else {
            Some(context.current_index)
        };
        let context = self.context.as_mut()?;
        let removed = Arc::make_mut(&mut context.items).remove(index);
        Arc::make_mut(&mut context.source_positions).remove(index);
        self.occurrences.remove(index);
        if index < context.loaded_items {
            context.loaded_items -= 1;
        }
        self.order.remove(index);
        if context.items.is_empty() {
            self.context = None;
            self.order.clear();
        } else {
            context.current_index = successor
                .map(|position| {
                    if position > index {
                        position - 1
                    } else {
                        position
                    }
                })
                .unwrap_or_else(|| index.min(context.items.len() - 1));
            if current_removed {
                self.order.record(context.current_index);
            }
        }
        Some(removed)
    }

    /// Clearing contents retires history/plans but preserves modes, like replacing
    /// a context. They are session-local and contain no account metadata.
    pub fn clear(&mut self) {
        self.context = None;
        self.occurrences.clear();
        self.order.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> Track {
        Track {
            id: id.into(),
            title: "Synthetic queue fixture".into(),
            artists: vec![],
            artist_links: Vec::new(),
            album_id: None,
            album_title: None,
            duration_ms: None,
            available: true,
            cover: None,
            explicit: false,
        }
    }

    #[test]
    fn selection_and_boundaries_preserve_current_track() {
        let mut queue = MusicQueue::new();
        assert!(queue.next_track().is_none());
        queue.append(track("first")).unwrap();
        queue.append(track("second")).unwrap();
        assert_eq!(queue.current().unwrap().id, "first");
        assert!(queue.previous_track().is_none());
        assert_eq!(queue.next_track().unwrap().id, "second");
        assert!(queue.next_track().is_none());
        assert_eq!(queue.current().unwrap().id, "second");
        assert!(queue.select(99).is_none());
        assert_eq!(queue.previous_track().unwrap().id, "first");
    }

    #[test]
    fn removing_before_current_and_current_preserves_order() {
        let mut queue = MusicQueue::new();
        for id in ["a", "b", "c"] {
            queue.append(track(id)).unwrap();
        }
        queue.select(1);
        queue.remove(0);
        assert_eq!(queue.current_index(), Some(0));
        assert_eq!(queue.current().unwrap().id, "b");
        queue.remove(0);
        assert_eq!(queue.current().unwrap().id, "c");
        queue.remove(0);
        assert!(queue.current().is_none());
        assert!(queue.remove(0).is_none());
    }

    #[test]
    fn full_queue_is_bounded_and_logout_clear_drops_metadata() {
        let mut queue = MusicQueue::new();
        for _ in 0..MAX_QUEUE {
            queue.append(track("repeat")).unwrap();
        }
        assert_eq!(queue.append(track("overflow")), Err(QueueError));
        assert_eq!(queue.items().len(), MAX_QUEUE);
        queue.clear();
        assert!(queue.items().is_empty());
        assert!(queue.current_index().is_none());
        queue.append(track("new-session")).unwrap();
        assert_eq!(queue.current_index(), Some(0));
    }

    #[test]
    fn loaded_context_selects_the_clicked_duplicate_occurrence_and_keeps_order() {
        let mut queue = MusicQueue::new();
        queue.replace_context(
            PlaybackContext::new(
                ContextKind::Playlist,
                Some("1234:7".into()),
                ["42:7", "43", "42:7", "44"]
                    .into_iter()
                    .map(track)
                    .collect(),
                2,
                true,
            )
            .unwrap(),
        );
        assert_eq!(
            queue
                .items()
                .iter()
                .map(|track| track.id.as_str())
                .collect::<Vec<_>>(),
            ["42:7", "43", "42:7", "44"]
        );
        assert_eq!(queue.current_index(), Some(2));
        assert_eq!(queue.current().unwrap().id, "42:7");
        assert_eq!(queue.next_track().unwrap().id, "44");
        assert_eq!(queue.previous_track().unwrap().id, "42:7");
        assert_eq!(
            queue.current_index(),
            Some(2),
            "previous selects this occurrence, not the first equal ID"
        );
        let context = queue.context().unwrap();
        assert_eq!(context.source_kind(), ContextKind::Playlist);
        assert_eq!(context.source_id(), Some("1234:7"));
        assert!(
            context.is_partial(),
            "a loaded page is not a whole remote playlist"
        );
        assert_eq!(context.loaded_items_len(), 4);
        assert_eq!(context.manual_items_len(), 0);
    }

    #[test]
    fn manual_additions_follow_loaded_items_and_replacement_drops_old_context() {
        let mut queue = MusicQueue::new();
        queue.append(track("old-manual")).unwrap();
        assert_eq!(queue.context().unwrap().source_kind(), ContextKind::Queue);
        assert_eq!(queue.context().unwrap().loaded_items_len(), 0);
        assert_eq!(queue.context().unwrap().manual_items_len(), 1);
        queue.replace_context(
            PlaybackContext::new(
                ContextKind::Search,
                None,
                ["first", "second", "third"]
                    .into_iter()
                    .map(track)
                    .collect(),
                1,
                true,
            )
            .unwrap(),
        );
        queue.append(track("second")).unwrap();
        assert_eq!(
            queue
                .items()
                .iter()
                .map(|track| track.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second", "third", "second"]
        );
        assert_eq!(queue.context().unwrap().loaded_items_len(), 3);
        assert_eq!(queue.context().unwrap().manual_items_len(), 1);
        assert_eq!(queue.next_track().unwrap().id, "third");
        assert_eq!(queue.next_track().unwrap().id, "second");
        assert_eq!(queue.current_index(), Some(3));
        assert!(queue.next_track().is_none());
        queue.replace_context(
            PlaybackContext::new(
                ContextKind::SingleTrack,
                Some("new".into()),
                vec![track("new")],
                0,
                false,
            )
            .unwrap(),
        );
        assert_eq!(queue.items().len(), 1);
        assert_eq!(queue.current_index(), Some(0));
        assert_eq!(queue.current().unwrap().id, "new");
        assert_eq!(queue.context().unwrap().manual_items_len(), 0);
        assert!(!queue.context().unwrap().is_partial());
    }

    #[test]
    fn removal_tracks_loaded_prefix_manual_suffix_and_exact_current_occurrence() {
        let mut queue = MusicQueue::new();
        queue.replace_context(
            PlaybackContext::new(
                ContextKind::Album,
                Some("7".into()),
                ["a", "b", "a"].into_iter().map(track).collect(),
                2,
                true,
            )
            .unwrap(),
        );
        queue.append(track("extra")).unwrap();
        queue.append(track("a")).unwrap();
        queue.remove(0);
        assert_eq!(queue.current_index(), Some(1));
        assert_eq!(queue.context().unwrap().loaded_items_len(), 2);
        assert_eq!(queue.context().unwrap().manual_items_len(), 2);
        queue.remove(1);
        assert_eq!(queue.current().unwrap().id, "extra");
        assert_eq!(queue.context().unwrap().loaded_items_len(), 1);
        queue.remove(2);
        assert_eq!(queue.context().unwrap().manual_items_len(), 1);
        assert_eq!(queue.current_index(), Some(1));
        queue.remove(0);
        assert_eq!(queue.current_index(), Some(0));
        assert_eq!(queue.context().unwrap().loaded_items_len(), 0);
        assert_eq!(queue.context().unwrap().manual_items_len(), 1);
        queue.remove(0);
        assert!(
            queue.context().is_none(),
            "removing the final occurrence drops provenance too"
        );
        assert!(queue.current_index().is_none());
        assert!(queue.items().is_empty());
    }

    #[test]
    fn context_admission_is_bounded_and_does_not_revalidate_trusted_track_models() {
        assert_eq!(
            PlaybackContext::new(ContextKind::Search, None, vec![], 0, true).unwrap_err(),
            ContextError::Empty
        );
        assert_eq!(
            PlaybackContext::new(ContextKind::Search, None, vec![track("a")], 1, true).unwrap_err(),
            ContextError::InvalidSelection
        );
        assert_eq!(
            PlaybackContext::new(
                ContextKind::Search,
                None,
                vec![track("a"); MAX_QUEUE + 1],
                0,
                true
            )
            .unwrap_err(),
            ContextError::TooLarge
        );
        for source in [
            "".to_owned(),
            "   ".to_owned(),
            "bad\nkey".to_owned(),
            "x".repeat(513),
        ] {
            assert_eq!(
                PlaybackContext::new(ContextKind::Album, Some(source), vec![track("a")], 0, true)
                    .unwrap_err(),
                ContextError::InvalidSource
            );
        }
        let mut queue = MusicQueue::new();
        queue.replace_context(
            PlaybackContext::new(
                ContextKind::Artist,
                Some("11".into()),
                vec![track("trusted-local-model"); MAX_QUEUE],
                MAX_QUEUE - 1,
                true,
            )
            .unwrap(),
        );
        assert_eq!(queue.items().len(), MAX_QUEUE);
        assert_eq!(queue.append(track("overflow")), Err(QueueError));
        assert_eq!(queue.current_index(), Some(MAX_QUEUE - 1));
        assert!(queue.select(MAX_QUEUE).is_none());
        assert_eq!(queue.current_index(), Some(MAX_QUEUE - 1));
        queue.clear();
        assert!(queue.context().is_none());
    }

    #[test]
    fn playback_context_debug_excludes_track_metadata_and_source_keys() {
        let mut private_track = track("synthetic-private-track-id");
        private_track.title = "synthetic-private-track-title".into();
        private_track.artists = vec!["synthetic-private-artist".into()];
        let context = PlaybackContext::new(
            ContextKind::Playlist,
            Some("synthetic-private-playlist-key".into()),
            vec![private_track],
            0,
            true,
        )
        .unwrap();
        let debug = format!("{context:?}");
        for private in [
            "synthetic-private-track-id",
            "synthetic-private-track-title",
            "synthetic-private-artist",
            "synthetic-private-playlist-key",
        ] {
            assert!(!debug.contains(private));
        }
        assert!(debug.contains("Playlist"));
        assert!(debug.contains("item_count: 1"));
    }

    #[test]
    fn row_occurrences_share_loaded_items_and_queue_edits_copy_on_write() {
        let base = PlaybackContext::new(
            ContextKind::Search,
            None,
            ["first", "second", "first"]
                .into_iter()
                .map(track)
                .collect(),
            0,
            true,
        )
        .unwrap();
        let clicked = base.with_current_index(2).unwrap();
        assert!(
            Arc::ptr_eq(&base.items, &clicked.items),
            "row selection must not clone every loaded track"
        );
        assert_eq!(base.current_index(), 0);
        assert_eq!(clicked.current_index(), 2);
        assert!(base.with_current_index(3).is_none());
        assert_eq!(base.current_index(), 0);

        let mut appended = MusicQueue::new();
        appended.replace_context(clicked.clone());
        appended.append(track("manual")).unwrap();
        assert!(!Arc::ptr_eq(
            &base.items,
            &appended.context().unwrap().items
        ));
        assert!(Arc::ptr_eq(&base.items, &clicked.items));
        assert_eq!(base.items().len(), 3);
        assert_eq!(clicked.items().len(), 3);
        assert_eq!(appended.items().len(), 4);
        assert_eq!(appended.current_index(), Some(2));
        assert_eq!(appended.context().unwrap().manual_items_len(), 1);

        let mut removed = MusicQueue::new();
        removed.replace_context(base.with_current_index(1).unwrap());
        assert!(Arc::ptr_eq(&base.items, &removed.context().unwrap().items));
        removed.remove(0);
        assert!(!Arc::ptr_eq(&base.items, &removed.context().unwrap().items));
        assert_eq!(
            removed
                .items()
                .iter()
                .map(|track| track.id.as_str())
                .collect::<Vec<_>>(),
            ["second", "first"]
        );
        assert_eq!(removed.current_index(), Some(0));
        assert_eq!(removed.current().unwrap().id, "second");
        assert_eq!(
            base.items()
                .iter()
                .map(|track| track.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second", "first"]
        );
        assert_eq!(base.loaded_items_len(), 3);
        assert_eq!(removed.context().unwrap().loaded_items_len(), 2);
    }
    fn context(items: usize, current: usize) -> PlaybackContext {
        PlaybackContext::new(
            ContextKind::Playlist,
            Some("synthetic-local-source".into()),
            (0..items)
                .map(|index| track(if index % 2 == 0 { "duplicate" } else { "other" }))
                .collect(),
            current,
            true,
        )
        .unwrap()
    }

    #[test]
    fn repeat_one_changes_only_eos_and_repeat_queue_wraps_available_positions() {
        let mut queue = MusicQueue::with_seed(7);
        let mut source = context(5, 2);
        Arc::make_mut(&mut source.items)[3].available = false;
        queue.replace_context(source);
        queue.set_repeat_mode(RepeatMode::One);
        assert!(queue.advance_eos().is_some());
        assert_eq!(queue.current_index(), Some(2));
        assert!(queue.next_track().is_some());
        assert_eq!(queue.current_index(), Some(4));
        assert!(!queue.can_go_next());
        assert!(queue.next_track().is_none());
        queue.previous_track();
        assert_eq!(queue.current_index(), Some(2));
        queue.set_repeat_mode(RepeatMode::Queue);
        queue.select(4);
        assert!(queue.can_go_next());
        queue.advance_eos();
        assert_eq!(queue.current_index(), Some(0));
        queue.previous_track();
        assert_eq!(queue.current_index(), Some(4));
        assert_eq!(RepeatMode::Off.next_mode(), RepeatMode::Queue);
        assert_eq!(RepeatMode::Queue.next_mode(), RepeatMode::One);
        assert_eq!(RepeatMode::One.next_mode(), RepeatMode::Off);
    }

    #[test]
    fn seeded_shuffle_is_occurrence_based_and_preview_matches_every_transition() {
        for seed in 0..64 {
            let mut queue = MusicQueue::with_seed(seed);
            let source = context(19, 7);
            queue.replace_context(source.clone());
            queue.set_shuffle(true);
            assert_eq!(queue.current_index(), Some(7));
            assert!(Arc::ptr_eq(&source.items, &queue.context().unwrap().items));
            let expected = queue.upcoming_indices();
            assert_eq!(expected.len(), 18);
            assert_eq!(
                expected
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                18
            );
            assert!(!expected.contains(&7));
            for index in expected {
                assert_eq!(queue.upcoming_indices().first(), Some(&index));
                queue.next_track().unwrap();
                assert_eq!(queue.current_index(), Some(index));
            }
            assert!(!queue.can_go_next());
            assert!(queue.next_track().is_none());
            assert!(Arc::ptr_eq(&source.items, &queue.context().unwrap().items));
        }
    }

    #[test]
    fn shuffle_previous_uses_actual_history_and_next_retraces_before_unplayed_items() {
        let mut queue = MusicQueue::with_seed(45);
        queue.replace_context(context(9, 4));
        queue.set_shuffle(true);
        let first = queue.upcoming_indices()[0];
        queue.next_track();
        let second = queue.upcoming_indices()[0];
        queue.next_track();
        assert_eq!(queue.history_indices(), [4, first]);
        queue.previous_track();
        assert_eq!(queue.current_index(), Some(first));
        queue.previous_track();
        assert_eq!(queue.current_index(), Some(4));
        assert!(queue.previous_track().is_none());
        assert_eq!(&queue.upcoming_indices()[..2], &[first, second]);
        queue.next_track();
        queue.next_track();
        assert_eq!(queue.current_index(), Some(second));
        queue.set_shuffle(false);
        assert_eq!(queue.current_index(), Some(second));
        let ordered_next = second + 1;
        if ordered_next < queue.items().len() {
            queue.next_track();
            assert_eq!(queue.current_index(), Some(ordered_next));
        }
    }

    #[test]
    fn shuffled_repeat_queue_cycles_are_bounded_unique_and_skip_unavailable() {
        let mut queue = MusicQueue::with_seed(38);
        let mut source = context(8, 3);
        Arc::make_mut(&mut source.items)[1].available = false;
        queue.replace_context(source);
        queue.set_shuffle(true);
        queue.set_repeat_mode(RepeatMode::Queue);
        for _ in 0..6 {
            queue.next_track().unwrap();
        }
        for _ in 0..5 {
            let prior = queue.current_index().unwrap();
            let preview = queue.upcoming_indices();
            assert_eq!(preview.len(), 7);
            assert_ne!(
                preview[0], prior,
                "avoid an immediate cycle-boundary repeat"
            );
            let mut cycle = Vec::new();
            for expected in preview {
                queue.advance_eos().unwrap();
                assert_eq!(queue.current_index(), Some(expected));
                cycle.push(expected);
            }
            assert_eq!(
                cycle
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                7
            );
            assert!(!cycle.contains(&1));
        }
    }

    #[test]
    fn shuffle_repeat_one_keeps_exact_duplicate_occurrence_and_remaining_plan() {
        let mut queue = MusicQueue::with_seed(20);
        queue.replace_context(context(5, 2));
        queue.set_shuffle(true);
        queue.set_repeat_mode(RepeatMode::One);
        let next = queue.upcoming_indices();
        for _ in 0..3 {
            queue.advance_eos().unwrap();
            assert_eq!(queue.current_index(), Some(2));
            assert_eq!(queue.upcoming_indices(), next);
        }
        queue.next_track().unwrap();
        assert_eq!(queue.current_index(), Some(next[0]));
    }

    #[test]
    fn removal_remaps_occurrences_without_changing_remaining_relative_order() {
        let mut queue = MusicQueue::with_seed(987);
        queue.replace_context(context(8, 4));
        queue.set_shuffle(true);
        let before = queue.upcoming_indices();
        queue.remove(1);
        assert_eq!(queue.current_index(), Some(3));
        let expected: Vec<_> = before
            .into_iter()
            .filter(|index| *index != 1)
            .map(|index| if index > 1 { index - 1 } else { index })
            .collect();
        assert_eq!(queue.upcoming_indices(), expected);
        let next = expected[0];
        queue.remove(3);
        assert_eq!(
            queue.current_index(),
            Some(if next > 3 { next - 1 } else { next })
        );
        let current = queue.current_index();
        let future = queue.upcoming_indices();
        queue.append(track("manual")).unwrap();
        assert_eq!(queue.current_index(), current);
        assert!(queue.upcoming_indices().contains(&6));
        let without_manual: Vec<_> = queue
            .upcoming_indices()
            .into_iter()
            .filter(|index| *index != 6)
            .collect();
        assert_eq!(without_manual, future);
    }

    #[test]
    fn mode_preferences_survive_context_clear_but_retirement_resets_them() {
        let mut queue = MusicQueue::with_seed(9);
        queue.replace_context(context(6, 3));
        queue.set_shuffle(true);
        queue.set_repeat_mode(RepeatMode::Queue);
        queue.next_track();
        queue.clear();
        assert!(queue.items().is_empty());
        assert!(queue.history_indices().is_empty());
        assert!(queue.upcoming_indices().is_empty());
        assert!(queue.shuffle());
        assert_eq!(queue.repeat_mode(), RepeatMode::Queue);
        queue.replace_context(context(3, 1));
        assert_eq!(queue.current_index(), Some(1));
        assert!(queue.shuffle());
        assert!(queue.history_indices().is_empty());
        assert_eq!(queue.upcoming_indices().len(), 3);
        queue.reset_modes();
        assert!(!queue.shuffle());
        assert_eq!(queue.repeat_mode(), RepeatMode::Off);
        assert_eq!(queue.current_index(), Some(1));
        assert_eq!(queue.upcoming_indices(), [2]);
    }

    #[test]
    fn unavailable_only_and_single_available_repeat_never_search_forever() {
        let mut queue = MusicQueue::with_seed(3);
        let mut source = context(4, 0);
        for track in Arc::make_mut(&mut source.items) {
            track.available = false;
        }
        queue.replace_context(source);
        for shuffle in [false, true] {
            queue.set_shuffle(shuffle);
            for repeat in [RepeatMode::Off, RepeatMode::Queue, RepeatMode::One] {
                queue.set_repeat_mode(repeat);
                assert!(!queue.can_go_next());
                assert!(!queue.can_go_previous());
                assert!(queue.next_track().is_none());
                assert!(queue.advance_eos().is_none());
            }
        }
        queue.append(track("only-available")).unwrap();
        queue.next_track().unwrap();
        assert_eq!(queue.current_index(), Some(4));
        queue.set_repeat_mode(RepeatMode::Queue);
        for _ in 0..100 {
            queue.advance_eos().unwrap();
            assert_eq!(queue.current_index(), Some(4));
        }
        assert!(queue.history_indices().len() <= MAX_QUEUE);
        // Removing the sole available final occurrence cannot leave the
        // RepeatQueue previous-wrap target pointing outside the shortened list.
        queue.set_shuffle(false);
        queue.remove(4);
        assert_eq!(queue.items().len(), 4);
        assert_eq!(queue.current_index(), Some(3));
        assert!(queue.current().is_some());
        assert!(!queue.can_go_next());
        assert!(!queue.can_go_previous());
    }
    fn assert_policy_bounds(queue: &MusicQueue) {
        assert_eq!(queue.occurrences.len(), queue.items().len());
        for (index, occurrence) in queue.occurrences.iter().enumerate() {
            assert!(
                queue.occurrences[..index]
                    .iter()
                    .all(|earlier| earlier != occurrence),
                "each live entry needs a distinct identity"
            );
        }
        if queue.items().is_empty() {
            assert!(queue.context().is_none());
            assert!(queue.current_index().is_none());
            assert!(queue.current().is_none());
            assert!(queue.current_occurrence_id().is_none());
        } else {
            assert!(queue.current_index().unwrap() < queue.items().len());
            assert!(queue.current().is_some());
            assert!(queue.current_occurrence_id().is_some());
            assert_eq!(
                queue.context().unwrap().source_positions.len(),
                queue.items().len()
            );
        }
        for index in queue.upcoming_indices() {
            assert!(index < queue.items().len());
            assert!(queue.items()[index].available);
        }
        for index in queue.history_indices() {
            assert!(index < queue.items().len());
        }
        let _ = (queue.can_go_next(), queue.can_go_previous());
    }

    #[test]
    fn finite_small_queue_mutations_keep_all_policy_positions_in_bounds() {
        for len in 1..=6 {
            for mask in 0..(1 << len) {
                for current in 0..len {
                    for removed in 0..len {
                        for repeat in [RepeatMode::Off, RepeatMode::Queue, RepeatMode::One] {
                            for shuffle in [false, true] {
                                for seed in [0, 91] {
                                    let mut queue = MusicQueue::with_seed(seed);
                                    let items = (0..len)
                                        .map(|index| {
                                            let mut item =
                                                track(&format!("synthetic-slot-{index}"));
                                            item.available = mask & (1 << index) != 0;
                                            item
                                        })
                                        .collect();
                                    queue.replace_context(
                                        PlaybackContext::new(
                                            ContextKind::Playlist,
                                            Some("synthetic-bounded-mutation".into()),
                                            items,
                                            current,
                                            true,
                                        )
                                        .unwrap(),
                                    );
                                    queue.set_repeat_mode(repeat);
                                    queue.set_shuffle(shuffle);
                                    // Build real history/forward state before editing, then
                                    // return to an exact possibly unavailable occurrence.
                                    queue.next_track();
                                    queue.next_track();
                                    queue.previous_track();
                                    queue.select(current);
                                    assert_policy_bounds(&queue);
                                    queue.remove(removed).unwrap();
                                    assert_policy_bounds(&queue);
                                    for step in 0..8 {
                                        match step % 3 {
                                            0 => {
                                                queue.next_track();
                                            }
                                            1 => {
                                                queue.previous_track();
                                            }
                                            _ => {
                                                queue.advance_eos();
                                            }
                                        }
                                        assert_policy_bounds(&queue);
                                    }
                                    let mut extra = track("synthetic-manual-extra");
                                    extra.available = mask & 1 != 0;
                                    queue.append(extra).unwrap();
                                    assert_policy_bounds(&queue);
                                    queue.set_shuffle(!shuffle);
                                    queue.select(queue.items().len() - 1);
                                    queue.advance_eos();
                                    assert_policy_bounds(&queue);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn original_source_positions_survive_duplicate_removal_and_exclude_manual_extras() {
        let source = PlaybackContext::new(
            ContextKind::LikedTracks,
            Some("synthetic-window".into()),
            vec![track("42"), track("42"), track("42")],
            2,
            true,
        )
        .unwrap();
        assert_eq!(source.source_position(2), Some(2));
        let mut queue = MusicQueue::new();
        queue.replace_context(source.clone());
        assert!(Arc::ptr_eq(
            &source.source_positions,
            &queue.context().unwrap().source_positions
        ));
        queue.remove(0);
        let edited = queue.context().unwrap();
        assert_eq!(edited.current_index(), 1);
        assert_eq!(
            edited.source_position(edited.current_index()),
            Some(2),
            "physical reindexing cannot turn the third equal-ID occurrence into the second"
        );
        assert_eq!(edited.source_position(0), Some(1));
        assert_eq!(
            source.source_position(0),
            Some(0),
            "the loaded row snapshot stays immutable"
        );
        assert!(!Arc::ptr_eq(
            &source.source_positions,
            &edited.source_positions
        ));
        queue.append(track("42")).unwrap();
        assert_eq!(queue.context().unwrap().source_position(2), None);
        queue.select(2);
        assert_eq!(queue.current().unwrap().id, "42");
        assert_eq!(
            queue.context().unwrap().source_position(2),
            None,
            "a manually appended duplicate must not highlight an original source row"
        );
        queue.remove(0);
        assert_eq!(queue.current_index(), Some(1));
        assert_eq!(queue.context().unwrap().source_position(1), None);
        queue.clear();
        queue.append(track("42")).unwrap();
        assert_eq!(queue.context().unwrap().source_position(0), None);
        queue.replace_context(source.with_current_index(1).unwrap());
        assert_eq!(queue.context().unwrap().source_position(1), Some(1));
    }
    #[test]
    fn projected_context_positions_validate_bounds_and_keep_occurrences_immutable() {
        let values = vec![track("42"), track("42")];
        for positions in [vec![0], vec![0, 0], vec![7, 6], vec![0, 2_000]] {
            assert_eq!(
                PlaybackContext::with_source_positions(
                    ContextKind::Playlist,
                    Some("projection".into()),
                    values.clone(),
                    0,
                    true,
                    positions
                ),
                Err(ContextError::InvalidSource)
            );
        }
        let source = PlaybackContext::with_source_positions(
            ContextKind::Playlist,
            Some("projection".into()),
            values,
            1,
            true,
            vec![499, 1_999],
        )
        .unwrap();
        let mut queue = MusicQueue::new();
        queue.replace_context(source.clone());
        queue.remove(0);
        assert_eq!(queue.context().unwrap().source_position(0), Some(1_999));
        assert_eq!(source.source_position(0), Some(499));
        queue.append(track("42")).unwrap();
        assert_eq!(queue.context().unwrap().source_position(1), None);
    }

    #[test]
    fn manual_occurrence_ids_survive_reindexing_and_distinguish_equal_track_ids() {
        let mut queue = MusicQueue::new();
        for _ in 0..5 {
            queue.append(track("41")).unwrap();
        }
        let original = queue.occurrences.clone();
        queue.select(2);
        assert_eq!(queue.current_occurrence_id(), Some(&original[2]));

        queue.remove(4); // A following entry does not change the current one.
        assert_eq!(queue.current_occurrence_id(), Some(&original[2]));
        queue.remove(0); // The old physical-index fallback changed here.
        assert_eq!(queue.current_index(), Some(1));
        assert_eq!(queue.current_occurrence_id(), Some(&original[2]));
        assert_eq!(queue.context().unwrap().source_position(1), None);
        assert_eq!(queue.occurrences.as_slice(), &original[1..4]);

        queue.remove(1); // Removing current selects the actual successor.
        assert_eq!(queue.current_occurrence_id(), Some(&original[3]));
        queue.append(track("41")).unwrap();
        let appended = queue.occurrence_id(2).unwrap().clone();
        assert!(original.iter().all(|previous| previous != &appended));
        queue.select(2);
        queue.remove(1);
        assert_eq!(queue.current_index(), Some(1));
        assert_eq!(queue.current_occurrence_id(), Some(&appended));
        queue.previous_track().unwrap();
        assert_eq!(queue.current_occurrence_id(), Some(&original[1]));
        queue.next_track().unwrap();
        assert_eq!(queue.current_occurrence_id(), Some(&appended));
        assert!(queue.advance_eos().is_none());
        assert_eq!(queue.current_occurrence_id(), Some(&appended));
        assert_policy_bounds(&queue);
    }

    #[test]
    fn entry_identity_is_separate_from_source_projection_and_replacement_scope() {
        let source = PlaybackContext::with_source_positions(
            ContextKind::LikedTracks,
            Some("synthetic-loaded-source".into()),
            vec![track("41"); 3],
            1,
            true,
            vec![0, 499, 1_999],
        )
        .unwrap();
        let mut queue = MusicQueue::new();
        queue.replace_context(source.clone());
        assert_eq!(queue.context(), Some(&source));
        let loaded = queue.current_occurrence_id().unwrap().clone();
        queue.remove(0);
        assert_eq!(queue.current_occurrence_id(), Some(&loaded));
        assert_eq!(queue.context().unwrap().source_position(0), Some(499));
        assert_eq!(source.source_position(1), Some(499));

        // Merely creating/selecting another immutable view context must not
        // rewrite the live queue, including its manually appended occurrences.
        let filtered = source.with_current_index(2).unwrap();
        queue.append(track("41")).unwrap();
        queue.select(2);
        let manual = queue.current_occurrence_id().unwrap().clone();
        assert_ne!(manual, loaded);
        assert_eq!(queue.context().unwrap().source_position(2), None);
        assert_eq!(filtered.source_position(2), Some(1_999));
        assert_eq!(queue.current_occurrence_id(), Some(&manual));

        queue.replace_context(filtered.clone());
        assert_eq!(queue.context(), Some(&filtered));
        let first_admission = queue.current_occurrence_id().unwrap().clone();
        assert_ne!(first_admission, loaded);
        assert_ne!(first_admission, manual);
        queue.replace_context(filtered);
        assert_ne!(queue.current_occurrence_id(), Some(&first_admission));
        assert_eq!(queue.context().unwrap().source_position(2), Some(1_999));
        assert_policy_bounds(&queue);
    }

    #[test]
    fn occurrence_ids_follow_shuffle_history_repeat_and_eos_without_being_reissued() {
        for shuffle in [false, true] {
            for repeat in [RepeatMode::Off, RepeatMode::Queue, RepeatMode::One] {
                let mut queue = MusicQueue::with_seed(31);
                for _ in 0..5 {
                    queue.append(track("41")).unwrap();
                }
                let entries = queue.occurrences.clone();
                queue.select(1);
                queue.select(2);
                queue.select(3);
                queue.set_shuffle(shuffle);
                queue.set_repeat_mode(repeat);
                assert_eq!(queue.occurrences, entries);
                let current = queue.current_occurrence_id().unwrap().clone();
                if repeat == RepeatMode::One {
                    queue.advance_eos().unwrap();
                    assert_eq!(queue.current_occurrence_id(), Some(&current));
                }
                queue.next_track().unwrap();
                let next = queue.current_occurrence_id().unwrap().clone();
                assert_ne!(current, next);
                queue.previous_track().unwrap();
                assert_eq!(queue.current_occurrence_id(), Some(&current));
                queue.advance_eos().unwrap();
                assert_eq!(
                    queue.current_occurrence_id(),
                    Some(if repeat == RepeatMode::One {
                        &current
                    } else {
                        &next
                    })
                );

                queue.select(3);
                queue.remove(0);
                assert_eq!(queue.current_occurrence_id(), Some(&entries[3]));
                assert_eq!(queue.occurrences.as_slice(), &entries[1..]);
                queue.set_repeat_mode(RepeatMode::Queue);
                for _ in 0..10 {
                    queue.advance_eos().unwrap();
                    assert!(entries[1..].contains(queue.current_occurrence_id().unwrap()));
                }
                assert_eq!(queue.occurrences.as_slice(), &entries[1..]);
                assert_policy_bounds(&queue);
            }
        }
    }

    #[test]
    fn entry_tokens_retire_on_clear_replacement_session_retirement_and_drop() {
        let mut queue = MusicQueue::new();
        queue.append(track("41")).unwrap();
        let observed = queue.current_occurrence_id().unwrap().clone();
        let retired = Arc::downgrade(&observed.0);
        queue.set_shuffle(true);
        queue.set_repeat_mode(RepeatMode::Queue);
        queue.clear();
        assert!(queue.current_occurrence_id().is_none());
        assert!(queue.occurrences.is_empty());
        assert_eq!(Arc::strong_count(&observed.0), 1);
        queue.append(track("41")).unwrap();
        assert_ne!(queue.current_occurrence_id(), Some(&observed));
        drop(observed);
        assert!(retired.upgrade().is_none());

        let source = context(2, 1);
        queue.replace_context(source.clone());
        let replaced = Arc::downgrade(&queue.current_occurrence_id().unwrap().0);
        let retained_source_snapshot = queue.context().unwrap().clone();
        queue.replace_context(source);
        assert!(replaced.upgrade().is_none());
        assert_eq!(retained_source_snapshot.current_index(), 1);
        let logout = Arc::downgrade(&queue.current_occurrence_id().unwrap().0);
        // The existing logout/account-close boundary clears contents and modes.
        queue.clear();
        queue.reset_modes();
        assert!(logout.upgrade().is_none());
        assert!(!queue.shuffle());
        assert_eq!(queue.repeat_mode(), RepeatMode::Off);
        queue.append(track("41")).unwrap();
        let closed = Arc::downgrade(&queue.current_occurrence_id().unwrap().0);
        drop(queue);
        assert!(closed.upgrade().is_none());
    }

    #[test]
    fn entry_identity_stays_bounded_and_opaque_when_mutations_are_rejected() {
        let mut queue = MusicQueue::new();
        for _ in 0..MAX_QUEUE {
            queue.append(track("synthetic-private-track-id")).unwrap();
        }
        let before = queue.occurrences.clone();
        assert_eq!(queue.append(track("extra")), Err(QueueError));
        assert!(queue.remove(MAX_QUEUE).is_none());
        assert!(queue.select(MAX_QUEUE).is_none());
        assert_eq!(queue.occurrences, before);
        assert!(queue.occurrence_id(MAX_QUEUE).is_none());
        assert_eq!(
            format!("{:?}", queue.current_occurrence_id().unwrap()),
            "OccurrenceId([opaque])"
        );
        assert_policy_bounds(&queue);
    }
}

#[cfg(test)]
mod wave_tests {
    use super::*;
    fn track() -> Track {
        Track {
            id: "42".into(),
            title: "Synthetic Wave occurrence".into(),
            artists: Vec::new(),
            artist_links: Vec::new(),
            album_id: None,
            album_title: None,
            duration_ms: None,
            cover: None,
            explicit: false,
            available: true,
        }
    }
    #[test]
    fn wave_append_and_history_trim_preserve_exact_current_and_duplicate_identities() {
        let mut queue = MusicQueue::default();
        queue.replace_context(
            PlaybackContext::new(
                ContextKind::Wave,
                Some("wave-1".into()),
                vec![track(); 40],
                39,
                true,
            )
            .unwrap(),
        );
        let current = queue.current_occurrence_id().unwrap().clone();
        let previous = queue.occurrence_id(38).unwrap().clone();
        queue.trim_wave_history(25);
        assert_eq!(queue.current_index(), Some(25));
        assert_eq!(queue.current_occurrence_id(), Some(&current));
        assert_eq!(queue.context().unwrap().source_position(25), Some(39));
        queue.append_wave_batch(vec![track(), track()]).unwrap();
        assert_eq!(queue.current_occurrence_id(), Some(&current));
        assert_ne!(queue.occurrence_id(26), queue.occurrence_id(27));
        assert_ne!(queue.occurrence_id(26), Some(&current));
        queue.previous_track().unwrap();
        assert_eq!(queue.current_occurrence_id(), Some(&previous));
        assert_eq!(queue.items().len(), 28);
    }
    #[test]
    fn wave_append_rejects_nonradio_capacity_and_position_overflow_atomically() {
        let mut queue = MusicQueue::default();
        queue.replace_context(
            PlaybackContext::new(ContextKind::Playlist, None, vec![track()], 0, false).unwrap(),
        );
        assert!(queue.append_wave_batch(vec![track()]).is_err());
        assert_eq!(queue.items().len(), 1);
        queue.replace_context(
            PlaybackContext::new(ContextKind::Wave, None, vec![track()], 0, true).unwrap(),
        );
        let before = queue.current_occurrence_id().unwrap().clone();
        queue.context.as_mut().unwrap().source_positions = Arc::new(vec![Some(usize::MAX)]);
        assert!(queue.append_wave_batch(vec![track()]).is_err());
        assert_eq!(queue.items().len(), 1);
        assert_eq!(queue.current_occurrence_id(), Some(&before));
        queue.replace_context(
            PlaybackContext::new(ContextKind::Wave, None, vec![track(); 500], 0, true).unwrap(),
        );
        assert!(queue.append_wave_batch(vec![track()]).is_err());
        assert_eq!(queue.items().len(), 500);
    }
}
