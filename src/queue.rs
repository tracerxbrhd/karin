// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded playback context of actual service metadata and manual additions.
//! Selection records an occurrence, not the first matching track identifier.

use crate::music::Track;
use std::sync::Arc;

const MAX_QUEUE: usize = 500;

#[derive(Default)]
pub struct MusicQueue {
    context: Option<PlaybackContext>,
}

/// Provenance of the loaded ordered items, never a network request or a claim
/// that a complete remote collection has been fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextKind {
    Search,
    LikedTracks,
    Playlist,
    Album,
    Artist,
    Queue,
    SingleTrack,
}

/// An admitted loaded context. Manual additions remain after its loaded prefix.
/// Track and source identifiers stay private in Debug output.
#[derive(Clone, PartialEq, Eq)]
pub struct PlaybackContext {
    source_kind: ContextKind,
    source_id: Option<String>,
    items: Arc<Vec<Track>>,
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
            source_id,
            items: Arc::new(items),
            current_index,
            loaded_items,
            partial,
        })
    }

    pub fn source_kind(&self) -> ContextKind {
        self.source_kind
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

    pub fn append(&mut self, track: Track) -> Result<(), QueueError> {
        if let Some(context) = self.context.as_mut() {
            if context.items.len() >= MAX_QUEUE {
                return Err(QueueError);
            }
            Arc::make_mut(&mut context.items).push(track);
        } else {
            self.context = Some(PlaybackContext {
                source_kind: ContextKind::Queue,
                source_id: None,
                items: Arc::new(vec![track]),
                current_index: 0,
                loaded_items: 0,
                partial: false,
            });
        }
        Ok(())
    }

    /// Replace the prior context and its manual additions in one operation.
    /// The constructor has already checked bounds and the exact occurrence.
    pub fn replace_context(&mut self, context: PlaybackContext) {
        self.context = Some(context);
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

    pub fn select(&mut self, index: usize) -> Option<&Track> {
        let context = self.context.as_mut()?;
        if index >= context.items.len() {
            return None;
        }
        context.current_index = index;
        context.items.get(index)
    }

    pub fn next_track(&mut self) -> Option<&Track> {
        self.select(self.current_index()?.checked_add(1)?)
    }

    pub fn previous_track(&mut self) -> Option<&Track> {
        self.select(self.current_index()?.checked_sub(1)?)
    }

    pub fn remove(&mut self, index: usize) -> Option<Track> {
        let context = self.context.as_mut()?;
        if index >= context.items.len() {
            return None;
        }
        let removed = Arc::make_mut(&mut context.items).remove(index);
        if index < context.loaded_items {
            context.loaded_items -= 1;
        }
        if context.items.is_empty() {
            self.context = None;
        } else {
            context.current_index = if context.current_index > index {
                context.current_index - 1
            } else {
                context.current_index.min(context.items.len() - 1)
            };
        }
        Some(removed)
    }

    pub fn clear(&mut self) {
        self.context = None;
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
}
