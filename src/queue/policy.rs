// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded occurrence-order policy. No Track cloning, network or audio state.
use super::{MAX_QUEUE, RepeatMode};
use crate::music::Track;
use ring::rand::{SecureRandom, SystemRandom};

#[derive(Clone)]
pub(super) struct OrderPolicy {
    pub shuffle: bool,
    pub repeat: RepeatMode,
    random: u64,
    remaining: Vec<usize>,
    history: Vec<usize>,
    cursor: usize,
}
impl Default for OrderPolicy {
    fn default() -> Self {
        // Playback randomization is not a security primitive. If OS entropy is
        // unavailable, a fixed seed still provides a valid bounded permutation.
        let mut seed = [0; 8];
        let _ = SystemRandom::new().fill(&mut seed);
        Self::with_seed(u64::from_ne_bytes(seed))
    }
}
impl OrderPolicy {
    pub(super) fn with_seed(seed: u64) -> Self {
        Self {
            shuffle: false,
            repeat: RepeatMode::Off,
            random: seed,
            remaining: Vec::new(),
            history: Vec::new(),
            cursor: 0,
        }
    }
    // SplitMix64 and rejection sampling: Fisher-Yates never uses modulo-biased
    // indices. Tests inject a seed; production starts with OS entropy.
    fn random_below(&mut self, bound: usize) -> usize {
        let bound = bound as u64;
        let threshold = bound.wrapping_neg() % bound;
        loop {
            self.random = self.random.wrapping_add(0x9e3779b97f4a7c15);
            let mut value = self.random;
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
            value ^= value >> 31;
            if value >= threshold {
                return (value % bound) as usize;
            }
        }
    }
    fn mix(&mut self, mut items: Vec<usize>) -> Vec<usize> {
        for end in (1..items.len()).rev() {
            let other = self.random_below(end + 1);
            items.swap(end, other);
        }
        items
    }
    pub(super) fn clear(&mut self) {
        self.remaining.clear();
        self.history.clear();
        self.cursor = 0;
    }
    pub(super) fn reset(&mut self, items: &[Track], current: usize) {
        self.clear();
        self.history.push(current);
        self.rebuild_remaining(items, current);
    }
    pub(super) fn rebuild_remaining(&mut self, items: &[Track], current: usize) {
        let available = items
            .iter()
            .enumerate()
            .filter(|(index, track)| track.available && *index != current)
            .map(|(index, _)| index)
            .collect();
        self.remaining = self.mix(available);
    }
    pub(super) fn append(&mut self, index: usize, available: bool) {
        if available && self.shuffle {
            let position = self.random_below(self.remaining.len() + 1);
            self.remaining.insert(position, index);
        }
    }
    pub(super) fn record(&mut self, index: usize) {
        self.remaining.retain(|candidate| *candidate != index);
        self.history.truncate(self.cursor.saturating_add(1));
        if self.history.last() != Some(&index) {
            self.history.push(index);
        }
        if self.history.len() > MAX_QUEUE {
            self.history.remove(0);
        }
        self.cursor = self.history.len().saturating_sub(1);
    }
    fn forward_history(&self, items: &[Track]) -> Option<(usize, usize)> {
        self.history
            .iter()
            .enumerate()
            .skip(self.cursor.saturating_add(1))
            .find(|(_, index)| items.get(**index).is_some_and(|track| track.available))
            .map(|(cursor, index)| (cursor, *index))
    }
    pub(super) fn has_next(&self, items: &[Track], current: usize) -> bool {
        if self.shuffle {
            self.forward_history(items).is_some()
                || self
                    .remaining
                    .iter()
                    .any(|index| items.get(*index).is_some_and(|track| track.available))
                || (self.repeat == RepeatMode::Queue && items.iter().any(|track| track.available))
        } else {
            items
                .iter()
                .skip(current.saturating_add(1))
                .any(|track| track.available)
                || (self.repeat == RepeatMode::Queue && items.iter().any(|track| track.available))
        }
    }
    pub(super) fn next(&mut self, items: &[Track], current: usize) -> Option<usize> {
        if !self.shuffle {
            let next = items
                .iter()
                .enumerate()
                .skip(current.saturating_add(1))
                .find(|(_, track)| track.available)
                .map(|(index, _)| index)
                .or_else(|| {
                    (self.repeat == RepeatMode::Queue)
                        .then(|| items.iter().position(|track| track.available))
                        .flatten()
                })?;
            self.record(next);
            return Some(next);
        }
        if let Some((cursor, next)) = self.forward_history(items) {
            self.cursor = cursor;
            self.remaining.retain(|candidate| *candidate != next);
            return Some(next);
        }
        self.remaining
            .retain(|index| items.get(*index).is_some_and(|track| track.available));
        if self.remaining.is_empty() && self.repeat == RepeatMode::Queue {
            let all = items
                .iter()
                .enumerate()
                .filter(|(_, track)| track.available)
                .map(|(index, _)| index)
                .collect();
            self.remaining = self.mix(all);
            if self.remaining.len() > 1 && self.remaining.first() == Some(&current) {
                self.remaining.swap(0, 1);
            }
        }
        let next = self.remaining.first().copied()?;
        self.remaining.remove(0);
        self.record(next);
        Some(next)
    }
    pub(super) fn previous_index(&self, items: &[Track], current: usize) -> Option<usize> {
        if self.shuffle {
            return self.history[..self.cursor.min(self.history.len())]
                .iter()
                .rev()
                .find(|index| items.get(**index).is_some_and(|track| track.available))
                .copied();
        }
        items[..current]
            .iter()
            .rposition(|track| track.available)
            .or_else(|| {
                (self.repeat == RepeatMode::Queue)
                    .then(|| items.iter().rposition(|track| track.available))
                    .flatten()
            })
    }
    pub(super) fn previous(&mut self, items: &[Track], current: usize) -> Option<usize> {
        let index = self.previous_index(items, current)?;
        if self.shuffle {
            self.cursor = (0..self.cursor)
                .rev()
                .find(|cursor| self.history[*cursor] == index)?;
            self.remaining.retain(|candidate| *candidate != index);
        } else {
            self.record(index);
        }
        Some(index)
    }
    pub(super) fn upcoming(&self, items: &[Track], current: usize) -> Vec<usize> {
        if !self.shuffle {
            let mut next: Vec<_> = items
                .iter()
                .enumerate()
                .skip(current.saturating_add(1))
                .filter(|(_, track)| track.available)
                .map(|(index, _)| index)
                .collect();
            if self.repeat == RepeatMode::Queue {
                next.extend(
                    items
                        .iter()
                        .enumerate()
                        .take(current.saturating_add(1))
                        .filter(|(_, track)| track.available)
                        .map(|(index, _)| index),
                );
            }
            return next;
        }
        // This is a preview of the exact RNG state and exact indices; it does
        // not clone Track or mutate what the next command will select.
        let mut preview = self.clone();
        let mut next = Vec::with_capacity(items.len());
        for _ in 0..items.len() {
            let Some(index) = preview.next(items, next.last().copied().unwrap_or(current)) else {
                break;
            };
            if next.contains(&index) {
                break;
            }
            next.push(index);
        }
        next
    }
    pub(super) fn history(&self) -> Vec<usize> {
        self.history.iter().take(self.cursor).copied().collect()
    }
    pub(super) fn remove(&mut self, removed: usize) {
        self.remaining.retain(|index| *index != removed);
        for index in &mut self.remaining {
            if *index > removed {
                *index -= 1;
            }
        }
        let before = self
            .history
            .iter()
            .take(self.cursor)
            .filter(|index| **index != removed)
            .count();
        self.history.retain(|index| *index != removed);
        for index in &mut self.history {
            if *index > removed {
                *index -= 1;
            }
        }
        self.cursor = before.min(self.history.len().saturating_sub(1));
    }
}
