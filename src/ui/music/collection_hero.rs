// SPDX-License-Identifier: GPL-3.0-or-later
//! A liked collection's native identity and truthful loaded-source summary.
//!
//! The controller supplies its existing Play/Refresh buttons and the admitted
//! source snapshot. This component neither requests music nor owns playback.

use std::cell::Cell;

use adw::prelude::*;
use gtk::glib;

use crate::music::Track;

#[derive(Clone)]
pub(super) struct LikedCollectionHero {
    widget: gtk::Box,
    facts: gtk::Label,
}

impl LikedCollectionHero {
    /// The caller retains action sensitivity, request epochs and lifecycle.
    /// Supplied buttons must be unparented before insertion into this hero.
    pub(super) fn new(play: &gtk::Button, refresh: &gtk::Button) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        widget.set_widget_name("music-liked-hero");
        widget.add_css_class("music-liked-hero");
        widget.set_hexpand(true);
        widget.set_margin_top(8);
        widget.set_margin_bottom(8);

        // A stock symbolic heart and theme color are Karin's own composition,
        // not an image/asset copied from another music client or account.
        let cover = gtk::Image::from_icon_name(super::interactions::like_icon_name(Some(true)));
        cover.set_widget_name("music-liked-cover");
        cover.add_css_class("music-liked-cover");
        cover.set_pixel_size(40);
        cover.set_size_request(88, 88);
        cover.set_halign(gtk::Align::Start);
        cover.set_valign(gtk::Align::Center);
        cover.update_property(&[gtk::accessible::Property::Label(
            "Коллекция понравившихся треков",
        )]);
        widget.append(&cover);

        let metadata = gtk::Box::new(gtk::Orientation::Vertical, 10);
        metadata.set_widget_name("music-liked-metadata");
        metadata.set_hexpand(true);
        metadata.set_valign(gtk::Align::Center);
        let title = gtk::Label::new(Some("Мне нравится"));
        title.set_widget_name("music-liked-title");
        title.add_css_class("music-collection-title");
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        title.set_width_chars(1);
        metadata.append(&title);
        let facts = gtk::Label::new(Some("Загрузка коллекции…"));
        facts.set_widget_name("music-liked-facts");
        facts.add_css_class("music-collection-facts");
        facts.add_css_class("dim-label");
        facts.set_xalign(0.0);
        facts.set_wrap(true);
        facts.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        facts.set_width_chars(1);
        metadata.append(&facts);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_widget_name("music-liked-actions");
        actions.set_halign(gtk::Align::Start);
        play.add_css_class("suggested-action");
        play.add_css_class("pill");
        play.set_hexpand(false);
        play.set_valign(gtk::Align::Center);
        refresh.set_hexpand(false);
        refresh.set_valign(gtk::Align::Center);
        actions.append(play);
        actions.append(refresh);
        metadata.append(&actions);
        widget.append(&metadata);

        // Observe the real content width, including an open pinned panel. No
        // controller is borrowed from map/layout, no widgets are reparented on
        // resize, and unchanged tiers cause no property writes or relayout.
        // Weak children avoid retaining private views after logout/removal.
        let weak_cover = cover.downgrade();
        let profile = Cell::new(0);
        widget.add_tick_callback(move |host, _| {
            let width = host.width();
            let Some(cover) = weak_cover.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if width > 0 {
                let next = width_profile(width);
                if profile.replace(next) != next {
                    let (art_size, icon_size) = profile_sizes(next);
                    cover.set_size_request(art_size, art_size);
                    cover.set_pixel_size(icon_size);
                    host.set_spacing(if next == 0 { 16 } else { 24 });
                }
            }
            glib::ControlFlow::Continue
        });

        Self { widget, facts }
    }

    pub(super) fn widget(&self) -> &gtk::Box {
        &self.widget
    }

    /// Describe all admitted source occurrences, never just filtered matches.
    /// `complete` is false for remote continuation, omitted records, a local
    /// bound, retired data or an incomplete/error snapshot. No remote total is
    /// inferred from a page length or metadata's unverified count hint.
    pub(super) fn update(&self, items: &[Track], complete: bool) {
        let value = collection_facts(items, complete);
        self.facts.set_text(&value);
        self.facts
            .update_property(&[gtk::accessible::Property::Label(&value)]);
    }
}

fn width_profile(width: i32) -> u8 {
    if width >= 1_000 {
        2
    } else if width >= 640 {
        1
    } else {
        0
    }
}

fn profile_sizes(profile: u8) -> (i32, i32) {
    match profile {
        2 => (192, 88),
        1 => (144, 64),
        _ => (88, 40),
    }
}

fn collection_facts(items: &[Track], complete: bool) -> String {
    let count = super::track_count_text(items.len() as u64);
    if !complete {
        return format!("Загружено: {count}");
    }
    if items.is_empty() {
        return count;
    }
    // Sum musical occurrences, including repeated IDs. One missing duration
    // or overflow makes a total unknowable; it never becomes a partial sum.
    let duration = items
        .iter()
        .try_fold(0_u64, |total, track| total.checked_add(track.duration_ms?));
    match duration {
        Some(duration) => format!("{count} · {}", duration_summary(duration)),
        None => count,
    }
}

fn duration_summary(milliseconds: u64) -> String {
    let minutes = milliseconds / 60_000;
    let hours = minutes / 60;
    let remainder = minutes % 60;
    match (hours, remainder) {
        (0, 0) => "менее минуты".to_owned(),
        (0, minutes) => format!("{minutes} мин"),
        (hours, 0) => format!("{hours} ч"),
        (hours, minutes) => format!("{hours} ч {minutes} мин"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(duration: Option<u64>) -> Track {
        Track {
            id: "1".to_owned(),
            title: "Синтетический трек".to_owned(),
            artists: Vec::new(),
            artist_links: Vec::new(),
            album_id: None,
            album_title: None,
            duration_ms: duration,
            cover: None,
            explicit: false,
            available: true,
        }
    }

    #[test]
    fn partial_collection_never_claims_a_total_or_duration() {
        let items = [track(Some(180_000)), track(Some(240_000))];
        assert_eq!(collection_facts(&items, false), "Загружено: 2 трека");
        assert_eq!(collection_facts(&[], false), "Загружено: 0 треков");
    }

    #[test]
    fn complete_duration_counts_occurrences_not_distinct_ids() {
        let repeated = track(Some(180_000));
        assert_eq!(
            collection_facts(&[repeated.clone(), repeated], true),
            "2 трека · 6 мин"
        );
        assert_eq!(duration_summary(3_660_000), "1 ч 1 мин");
        assert_eq!(duration_summary(3_600_000), "1 ч");
    }

    #[test]
    fn unknown_overflow_or_empty_duration_is_not_a_partial_total() {
        assert_eq!(
            collection_facts(&[track(Some(180_000)), track(None)], true),
            "2 трека"
        );
        assert_eq!(
            collection_facts(&[track(Some(u64::MAX)), track(Some(1))], true),
            "2 трека"
        );
        assert_eq!(collection_facts(&[], true), "0 треков");
    }

    #[test]
    fn cover_sizes_use_actual_collection_width_including_pinned_layout() {
        assert_eq!(profile_sizes(width_profile(362)), (88, 40));
        assert_eq!(profile_sizes(width_profile(584)), (88, 40));
        assert_eq!(profile_sizes(width_profile(850)), (144, 64));
        assert_eq!(profile_sizes(width_profile(1_040)), (192, 88));
    }
}
