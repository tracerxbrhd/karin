// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded, pointer-free queue state. Fresh service metadata supplies restoration.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueueSnapshot {
    source_kind: ContextKind,
    origin: PlaybackOrigin,
    source_key: Option<[u8; 32]>,
    ids: Vec<String>,
    positions: Vec<Option<usize>>,
    loaded_items: usize,
    partial: bool,
    current: usize,
    order: SavedOrder,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedOrder {
    shuffle: bool,
    repeat: RepeatMode,
    random: u64,
    remaining: Vec<usize>,
    history: Vec<usize>,
    cursor: usize,
}

pub(crate) struct QueueRestore {
    pub queue: MusicQueue,
    pub omitted: usize,
    pub current_changed: bool,
}

impl std::fmt::Debug for QueueSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("QueueSnapshot([private bounded context])")
    }
}

impl QueueSnapshot {
    pub(crate) fn capture(queue: &MusicQueue) -> Result<Option<Self>, ContextError> {
        let Some(context) = queue.context() else {
            return Ok(None);
        };
        if context.source_kind == ContextKind::Wave {
            return Ok(None);
        }
        let saved = Self {
            source_kind: context.source_kind,
            origin: context.origin().clone(),
            // A view binding can contain a source/query supplied by callers;
            // persist only its opaque hash, never a URI/query/session marker.
            source_key: context.source_id.as_ref().map(|key| {
                let hash = ring::digest::digest(&ring::digest::SHA256, key.as_bytes());
                hash.as_ref().try_into().expect("SHA256 length")
            }),
            ids: context.items.iter().map(|track| track.id.clone()).collect(),
            positions: context.source_positions.as_ref().clone(),
            loaded_items: context.loaded_items,
            partial: context.partial,
            current: context.current_index,
            order: SavedOrder {
                shuffle: queue.order.shuffle,
                repeat: queue.order.repeat,
                random: queue.order.random,
                remaining: queue.order.remaining.clone(),
                history: queue.order.history.clone(),
                cursor: queue.order.cursor,
            },
        };
        saved.validate()?;
        Ok(Some(saved))
    }

    pub(crate) fn origin(&self) -> &PlaybackOrigin {
        &self.origin
    }

    /// One bounded normalized lookup may be used for equal service IDs. It does
    /// not collapse their independent occurrence positions in the queue.
    pub(crate) fn ids(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        self.ids
            .iter()
            .filter(|id| seen.insert(id.as_str()))
            .cloned()
            .collect()
    }

    pub(crate) fn validate(&self) -> Result<(), ContextError> {
        let size = self.ids.len();
        if size == 0 || size > MAX_QUEUE {
            return Err(ContextError::TooLarge);
        }
        if self.current >= size || self.source_kind == ContextKind::Wave {
            return Err(ContextError::InvalidSelection);
        }
        if !self.origin.valid_for(self.source_kind)
            || self
                .ids
                .iter()
                .any(|id| crate::music::validate_id(id).is_err())
            || self.positions.len() != size
            || self.loaded_items > size
            || self
                .positions
                .iter()
                .flatten()
                .any(|position| *position >= MAX_SOURCE_POSITIONS)
            || self.positions.iter().flatten().count() != self.loaded_items
            || self
                .positions
                .iter()
                .flatten()
                .collect::<BTreeSet<_>>()
                .len()
                != self.loaded_items
        {
            return Err(ContextError::InvalidSource);
        }
        if self.order.remaining.len() > size
            || self
                .order
                .remaining
                .iter()
                .any(|index| *index >= size || *index == self.current)
            || self.order.remaining.iter().collect::<BTreeSet<_>>().len()
                != self.order.remaining.len()
            || self.order.history.is_empty()
            || self.order.history.len() > MAX_QUEUE
            || self.order.history.iter().any(|index| *index >= size)
            || self.order.cursor >= self.order.history.len()
            || self.order.history[self.order.cursor] != self.current
        {
            return Err(ContextError::InvalidSelection);
        }
        Ok(())
    }

    /// Rebuild only from freshly normalized available tracks. Source provenance
    /// stays immutable while a new highlight namespace prevents a saved queue
    /// from pretending to be the server's present playlist/loaded page.
    pub(crate) fn restore(&self, normalized: &[Track]) -> Result<QueueRestore, ContextError> {
        self.validate()?;
        if normalized.len() > MAX_QUEUE {
            return Err(ContextError::TooLarge);
        }
        let mut tracks = BTreeMap::new();
        for track in normalized {
            if crate::music::validate_id(&track.id).is_err()
                || tracks.insert(track.id.as_str(), track).is_some()
            {
                return Err(ContextError::InvalidSource);
            }
        }
        let mut items = Vec::new();
        let mut positions = Vec::new();
        let mut mapping = vec![None; self.ids.len()];
        let mut loaded_items = 0;
        for (index, id) in self.ids.iter().enumerate() {
            let Some(track) = tracks.get(id.as_str()).filter(|track| track.available) else {
                continue;
            };
            mapping[index] = Some(items.len());
            items.push((**track).clone());
            positions.push(self.positions[index]);
            loaded_items += usize::from(self.positions[index].is_some());
        }
        let omitted = self.ids.len() - items.len();
        let current_changed = mapping[self.current].is_none();
        let mut queue = MusicQueue::new();
        queue.order.shuffle = self.order.shuffle;
        queue.order.repeat = self.order.repeat;
        queue.order.random = self.order.random;
        if items.is_empty() {
            return Ok(QueueRestore {
                queue,
                omitted,
                current_changed: true,
            });
        }
        let current = mapping[self.current]
            .or_else(|| {
                if self.order.shuffle {
                    self.order
                        .history
                        .iter()
                        .skip(self.order.cursor + 1)
                        .chain(&self.order.remaining)
                        .find_map(|index| mapping[*index])
                } else {
                    None
                }
            })
            .or_else(|| mapping.iter().skip(self.current).flatten().copied().next())
            .or_else(|| mapping.iter().flatten().copied().next())
            .ok_or(ContextError::InvalidSelection)?;
        let mut history = Vec::new();
        let mut cursor = 0;
        for (index, old) in self.order.history.iter().enumerate() {
            if let Some(mapped) = mapping[*old] {
                history.push(mapped);
                if index <= self.order.cursor {
                    cursor = history.len() - 1;
                }
            }
        }
        if history.is_empty() {
            history.push(current);
            cursor = 0;
        } else if current_changed {
            history.truncate(cursor + 1);
            if history.last() != Some(&current) {
                history.push(current);
            }
            if history.len() > MAX_QUEUE {
                history.remove(0);
            }
            cursor = history.len() - 1;
        }
        let remaining = self
            .order
            .remaining
            .iter()
            .filter_map(|old| mapping[*old])
            .filter(|index| *index != current)
            .collect();
        let source_id = self.source_key.map(|key| {
            format!(
                "restored:{}",
                key.iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            )
        });
        let partial = self.partial
            || omitted > 0
            || !matches!(
                self.source_kind,
                ContextKind::Queue | ContextKind::SingleTrack
            );
        queue.context = Some(PlaybackContext {
            source_kind: self.source_kind,
            origin: self.origin.clone(),
            source_id,
            items: Arc::new(items),
            source_positions: Arc::new(positions),
            current_index: current,
            loaded_items,
            partial,
        });
        queue.occurrences = (0..queue.items().len())
            .map(|_| OccurrenceId::new())
            .collect();
        queue.order.remaining = remaining;
        queue.order.history = history;
        queue.order.cursor = cursor;
        Ok(QueueRestore {
            queue,
            omitted,
            current_changed,
        })
    }
}

#[cfg(test)]
mod tests;
