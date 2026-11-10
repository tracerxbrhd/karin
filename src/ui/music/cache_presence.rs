// SPDX-License-Identifier: GPL-3.0-or-later
//! A weak, account-scoped presentation of AudioCache's verified disk presence.
//!
//! GTK never opens a cache file or sends HTTP for an indicator. One coalesced
//! worker batches the visible track keys; all mounted occurrences share its
//! result. A saved mark describes physical storage, never offline entitlement.

use super::*;
use crate::audio_cache::{AudioCache, Lease, Presence, ResourceKey};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

const REFRESH: Duration = Duration::from_millis(500);
const MAX_KEYS: usize = 2000;
const SAVED: &str = "Аудио сохранено в кеше. При воспроизведении проверяется доступ.";
const SAVING: &str = "Сохраняем аудио в кеш…";

struct Binding {
    track: String,
    slot: glib::WeakRef<gtk::Box>,
}

#[derive(Default)]
pub(super) struct State {
    bindings: Vec<Binding>,
    values: BTreeMap<String, Presence>,
    generation: Option<u64>,
    lease: Option<(AudioCache, Lease)>,
    running: bool,
    revision: u64,
    next: Option<Instant>,
    footer: Option<String>,
    info: Option<String>,
}

/// Passive native status, deliberately outside the keyboard action chain.
/// Hidden status occupies no space in the existing title line.
pub(super) fn indicator(name: &str) -> gtk::Box {
    let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    slot.set_widget_name(name);
    slot.set_valign(gtk::Align::Center);
    slot.set_halign(gtk::Align::End);
    slot.set_hexpand(false);
    slot.set_focusable(false);
    slot.set_visible(false);
    let saving = gtk::Spinner::new();
    saving.set_widget_name("music-audio-cache-saving");
    saving.set_size_request(16, 16);
    saving.set_visible(false);
    saving.update_property(&[gtk::accessible::Property::Label(SAVING)]);
    let saved = gtk::Image::from_icon_name("folder-download-symbolic");
    saved.set_widget_name("music-audio-cache-stored");
    saved.set_pixel_size(16);
    saved.add_css_class("dim-label");
    saved.set_visible(false);
    saved.update_property(&[gtk::accessible::Property::Label(SAVED)]);
    saved.set_tooltip_text(Some(SAVED));
    slot.append(&saving);
    slot.append(&saved);
    slot
}

fn present(slot: &gtk::Box, state: Presence) {
    let Some(saving) = slot.first_child().and_downcast::<gtk::Spinner>() else {
        return;
    };
    let Some(saved) = saving.next_sibling().and_downcast::<gtk::Image>() else {
        return;
    };
    let downloading = state == Presence::Downloading;
    let stored = state == Presence::Stored;
    if saving.is_visible() == downloading
        && saved.is_visible() == stored
        && slot.is_visible() == (downloading || stored)
    {
        return;
    }
    if saving.is_spinning() != downloading {
        saving.set_spinning(downloading);
    }
    saving.set_visible(downloading);
    saved.set_visible(stored);
    // Reset the saved image's own accessible name as well as its parent. In
    // particular, a previous local-play cue must not survive Clear and resave.
    saved.set_tooltip_text(Some(SAVED));
    saved.update_property(&[gtk::accessible::Property::Label(SAVED)]);
    slot.set_tooltip_text(match state {
        Presence::Stored => Some(SAVED),
        Presence::Downloading => Some(SAVING),
        Presence::Missing => None,
    });
    slot.set_visible(downloading || stored);
}

/// The indicator itself is intentionally hidden for Missing. Observe its row
/// (not that hidden child), and exclude rows clipped outside a scroll viewport.
fn visible_owner(slot: &gtk::Box, window: &adw::ApplicationWindow) -> bool {
    let Some(owner) = slot
        .ancestor(gtk::ListBoxRow::static_type())
        .or_else(|| slot.parent())
    else {
        return false;
    };
    if !owner.is_mapped() {
        return false;
    }
    let Some(bounds) = owner.compute_bounds(window) else {
        return false;
    };
    let clip = gtk::graphene::Rect::new(0.0, 0.0, window.width() as f32, window.height() as f32);
    if !overlaps(&bounds, &clip) {
        return false;
    }
    let mut parent = owner.parent();
    while let Some(widget) = parent {
        parent = widget.parent();
        if widget.is::<gtk::ScrolledWindow>() {
            let Some(viewport) = widget.compute_bounds(window) else {
                return false;
            };
            if !overlaps(&bounds, &viewport) {
                return false;
            }
        }
    }
    true
}
fn overlaps(a: &gtk::graphene::Rect, b: &gtk::graphene::Rect) -> bool {
    a.width() > 0.0
        && a.height() > 0.0
        && b.width() > 0.0
        && b.height() > 0.0
        && a.x() < b.x() + b.width()
        && a.x() + a.width() > b.x()
        && a.y() < b.y() + b.height()
        && a.y() + a.height() > b.y()
}

impl State {
    fn clear_values(&mut self) -> Vec<gtk::Box> {
        self.values.clear();
        self.lease = None;
        self.next = None;
        self.revision = self.revision.wrapping_add(1);
        let mut slots = Vec::new();
        self.bindings.retain(|binding| {
            if let Some(slot) = binding.slot.upgrade() {
                slots.push(slot);
                true
            } else {
                false
            }
        });
        slots
    }
}

impl Controller {
    /// Drop the derived scope immediately at explicit account/cache fences.
    /// This also releases its cache owner on close while an old window object
    /// may still be retained by the caller.
    pub(super) fn clear_cache_presence(&self) {
        let slots = {
            let mut state = self.cache_presence.borrow_mut();
            state.generation = None;
            state.clear_values()
        };
        if !self.model.borrow().closed {
            for slot in slots {
                present(&slot, Presence::Missing);
            }
        }
    }

    pub(super) fn register_cache_indicator(&self, track: &str, slot: &gtk::Box) {
        let model = self.model.borrow();
        let valid = model.account_valid && !model.closed;
        let generation = model.generation;
        drop(model);
        let mut state = self.cache_presence.borrow_mut();
        // Ordinary rows register once at creation. Pruning belongs to the
        // periodic batch, not every insertion of a large library window.
        let cleared = if !valid || state.generation != Some(generation) {
            let slots = state.clear_values();
            state.generation = valid.then_some(generation);
            slots
        } else {
            Vec::new()
        };
        let value = state
            .values
            .get(track)
            .copied()
            .unwrap_or(Presence::Missing);
        if valid && ResourceKey::track(track).is_ok() {
            state.bindings.push(Binding {
                track: track.to_owned(),
                slot: slot.downgrade(),
            });
            // A newly mounted row need not wait for the next periodic interval.
            state.next = None;
        }
        drop(state);
        for slot in cleared {
            present(&slot, Presence::Missing);
        }
        present(slot, value);
    }

    /// Rebind the two persistent slots as metadata selection changes. A previous
    /// song's saved mark can never migrate to its replacement's title.
    pub(super) fn sync_cache_presence_targets(&self) {
        let footer = self
            .model
            .borrow()
            .queue
            .current()
            .map(|track| track.id.clone());
        let info = self.track_pane.selected().map(|track| track.id);
        let (footer_changed, info_changed) = {
            let mut state = self.cache_presence.borrow_mut();
            let footer_changed = state.footer != footer;
            let info_changed = state.info != info;
            state.footer = footer.clone();
            state.info = info.clone();
            (footer_changed, info_changed)
        };
        if footer_changed {
            self.bind_cache_target(footer.as_deref(), &self.widgets.current_cache);
        }
        if info_changed {
            self.bind_cache_target(info.as_deref(), &self.track_pane.cache_indicator);
        }
    }

    fn bind_cache_target(&self, track: Option<&str>, slot: &gtk::Box) {
        // Only these two persistent slots can change their track binding.
        self.cache_presence.borrow_mut().bindings.retain(|binding| {
            binding
                .slot
                .upgrade()
                .is_some_and(|existing| existing != *slot)
        });
        if let Some(track) = track {
            self.register_cache_indicator(track, slot);
        } else {
            present(slot, Presence::Missing);
        }
    }

    pub(super) fn tick_cache_presence(self: &Rc<Self>) {
        self.sync_cache_presence_targets();
        let (generation, active) = {
            let model = self.model.borrow();
            (model.generation, model.account_valid && !model.closed)
        };
        let binding = active.then(|| self.audio_cache_binding()).flatten();
        let Some((cache, lease)) = binding else {
            let mut state = self.cache_presence.borrow_mut();
            let cleared = if state.lease.is_some()
                || !state.values.is_empty()
                || state.generation != active.then_some(generation)
            {
                state.clear_values()
            } else {
                Vec::new()
            };
            state.generation = active.then_some(generation);
            drop(state);
            for slot in cleared {
                present(&slot, Presence::Missing);
            }
            return;
        };
        let cleared = {
            let mut state = self.cache_presence.borrow_mut();
            let slots = if state.generation != Some(generation)
                || state
                    .lease
                    .as_ref()
                    .is_some_and(|(cache, lease)| !cache.presence_current(lease))
            {
                state.clear_values()
            } else {
                Vec::new()
            };
            state.generation = Some(generation);
            state.lease = Some((cache.clone(), lease.clone()));
            state
                .bindings
                .retain(|binding| binding.slot.upgrade().is_some());
            slots
        };
        for slot in cleared {
            present(&slot, Presence::Missing);
        }
        let window = self.window.upgrade();
        let (tracks, keys, revision) = {
            let mut state = self.cache_presence.borrow_mut();
            if state.running || state.next.is_some_and(|next| Instant::now() < next) {
                return;
            }
            let mut tracks = Vec::new();
            let mut selected = BTreeSet::new();
            // Fixed slots are first; bounded hidden lists cannot starve the
            // current song or newly mapped search/queue rows.
            for id in [&state.footer, &state.info].into_iter().flatten() {
                if selected.insert(id.clone()) {
                    tracks.push(id.clone());
                }
            }
            for binding in &state.bindings {
                if tracks.len() >= MAX_KEYS {
                    break;
                }
                if binding.slot.upgrade().is_some_and(|slot| {
                    window
                        .as_ref()
                        .is_some_and(|window| visible_owner(&slot, window))
                }) && selected.insert(binding.track.clone())
                {
                    tracks.push(binding.track.clone());
                }
            }
            // Refresh already known positive facts too. This checks cheap
            // unchanged fingerprints off GTK and observes LRU for scrolled-away
            // rows, while cold missing/offscreen library entries stay unqueried.
            for track in state.values.keys() {
                if tracks.len() >= MAX_KEYS {
                    break;
                }
                if selected.insert(track.clone()) {
                    tracks.push(track.clone());
                }
            }
            let (tracks, keys): (Vec<_>, Vec<_>) = tracks
                .into_iter()
                .filter_map(|track| ResourceKey::track(&track).ok().map(|key| (track, key)))
                .unzip();
            if tracks.is_empty() {
                return;
            }
            state.running = true;
            (tracks, keys, state.revision)
        };
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let work = cache.clone();
            let current = lease.clone();
            let result = gio::spawn_blocking(move || work.presence(&current, &keys)).await;
            let Some(this) = weak.upgrade() else { return };
            let model = this.model.borrow();
            let valid = model.account_valid && !model.closed && model.generation == generation;
            drop(model);
            let mut state = this.cache_presence.borrow_mut();
            state.running = false;
            if state.revision != revision || !valid || !cache.presence_current(&lease) {
                return;
            }
            state.next = Some(Instant::now() + REFRESH);
            // Contention or an unavailable worker never declares an existing
            // completed record absent. The next bounded query retries it.
            let Ok(Ok(values)) = result else { return };
            if values.len() != tracks.len() {
                return;
            }
            let measured: BTreeMap<_, _> = tracks.into_iter().zip(values).collect();
            // This is a bounded derived view of live row bindings. Preserve a
            // known saved record when it scrolls out of the current batch; a
            // later footer/Info bind must not falsely mark it absent. Missing
            // facts need no retained entry, and only measured absence removes
            // a live positive fact (apart from an explicit global fence).
            for (track, value) in &measured {
                if *value == Presence::Missing {
                    state.values.remove(track);
                } else {
                    state.values.insert(track.clone(), *value);
                }
            }
            state
                .bindings
                .retain(|binding| binding.slot.upgrade().is_some());
            let registered: BTreeSet<_> = state
                .bindings
                .iter()
                .map(|binding| binding.track.clone())
                .collect();
            state.values.retain(|track, _| registered.contains(track));
            let updates = state
                .bindings
                .iter()
                .filter_map(|binding| {
                    Some((binding.slot.upgrade()?, *measured.get(&binding.track)?))
                })
                .collect::<Vec<_>>();
            drop(state);
            for (slot, value) in updates {
                present(&slot, value);
            }
            this.sync_cache_source_tooltip();
        });
    }

    /// The optional local-play cue is based on the one real engine, never on
    /// a successful save or the physical-presence mark alone.
    pub(super) fn sync_cache_source_tooltip(&self) {
        let stored = {
            let state = self.cache_presence.borrow();
            state
                .footer
                .as_ref()
                .is_some_and(|track| state.values.get(track) == Some(&Presence::Stored))
        };
        if !stored || !self.widgets.current_cache.is_visible() {
            return;
        }
        let local = self.music_snapshot().is_some_and(|snapshot| {
            snapshot.source_kind == crate::playback::PlaybackSourceKind::CachedFile
                && matches!(
                    snapshot.state,
                    crate::playback::PlaybackState::Playing
                        | crate::playback::PlaybackState::Paused
                )
        });
        let tooltip = if local {
            concat!(
                "Аудио сохранено в кеше. При воспроизведении проверяется доступ.",
                "\nВоспроизводится из локального кеша"
            )
        } else {
            SAVED
        };
        if self.widgets.current_cache.tooltip_text().as_deref() != Some(tooltip) {
            self.widgets.current_cache.set_tooltip_text(Some(tooltip));
        }
        if let Some(image) = self.widgets.current_cache.last_child()
            && image.tooltip_text().as_deref() != Some(tooltip)
        {
            image.set_tooltip_text(Some(tooltip));
            image.update_property(&[gtk::accessible::Property::Label(tooltip)]);
        }
    }

    #[cfg(test)]
    pub(super) fn cache_presence_for_test(&self, track: &str) -> Presence {
        self.cache_presence
            .borrow()
            .values
            .get(track)
            .copied()
            .unwrap_or(Presence::Missing)
    }

    #[cfg(test)]
    pub(super) fn cache_presence_query_running_for_test(&self) -> bool {
        self.cache_presence.borrow().running
    }

    #[cfg(test)]
    pub(super) fn cache_indicators_for_test(&self, track: &str) -> Vec<gtk::Box> {
        self.cache_presence
            .borrow()
            .bindings
            .iter()
            .filter(|binding| binding.track == track)
            .filter_map(|binding| binding.slot.upgrade())
            .collect()
    }
}
