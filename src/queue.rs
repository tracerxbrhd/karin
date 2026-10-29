// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded queue of actual service metadata. Selection never implies playback.

use crate::music::Track;

const MAX_QUEUE: usize = 500;

#[derive(Default)]
pub struct MusicQueue {
    items: Vec<Track>,
    current: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("The queue is full (500 tracks).")]
pub struct QueueError;

impl MusicQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append(&mut self, track: Track) -> Result<(), QueueError> {
        if self.items.len() == MAX_QUEUE {
            return Err(QueueError);
        }
        self.items.push(track);
        if self.current.is_none() {
            self.current = Some(0);
        }
        Ok(())
    }

    pub fn items(&self) -> &[Track] {
        &self.items
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    pub fn current(&self) -> Option<&Track> {
        self.current.and_then(|index| self.items.get(index))
    }

    pub fn select(&mut self, index: usize) -> Option<&Track> {
        if index >= self.items.len() {
            return None;
        }
        self.current = Some(index);
        self.current()
    }

    pub fn next_track(&mut self) -> Option<&Track> {
        self.select(self.current?.checked_add(1)?)
    }

    pub fn previous_track(&mut self) -> Option<&Track> {
        self.select(self.current?.checked_sub(1)?)
    }

    pub fn remove(&mut self, index: usize) -> Option<Track> {
        if index >= self.items.len() {
            return None;
        }
        let removed = self.items.remove(index);
        self.current = match self.current {
            _ if self.items.is_empty() => None,
            Some(current) if current > index => Some(current - 1),
            Some(current) => Some(current.min(self.items.len() - 1)),
            None => None,
        };
        Some(removed)
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.current = None;
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
}
