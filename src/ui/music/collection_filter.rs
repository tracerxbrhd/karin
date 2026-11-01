// SPDX-License-Identifier: GPL-3.0-or-later
//! Unicode matching over admitted metadata; never a remote collection query.
use super::*;

fn folded(value: &str) -> String {
    // Compatibility decomposition can introduce an ordinary capital letter
    // (for example mathematical 𝐀 → A). Normalize before casefolding so it
    // matches both its styled lowercase and the user's plain-text query.
    let normalized = glib::normalize(value, glib::NormalizeMode::AllCompose);
    glib::normalize(glib::casefold(normalized), glib::NormalizeMode::AllCompose).to_string()
}

#[derive(Default)]
pub(super) struct CollectionFilter {
    tokens: Vec<String>,
    revision: u64,
    index: Vec<String>,
    visible: Vec<usize>,
    ranks: Vec<Option<usize>>,
}
impl CollectionFilter {
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }
    pub(super) fn active(&self) -> bool {
        !self.tokens.is_empty()
    }
    pub(super) fn visible(&self) -> &[usize] {
        &self.visible
    }
    pub(super) fn rank(&self, original: usize) -> Option<usize> {
        self.ranks.get(original).copied().flatten()
    }
    pub(super) fn set_query(&mut self, query: &str) -> bool {
        let tokens = folded(query)
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if tokens == self.tokens {
            return false;
        }
        self.tokens = tokens;
        self.revision = self.revision.wrapping_add(1);
        self.rebuild();
        true
    }
    pub(super) fn append(&mut self, tracks: &[Track]) {
        for track in tracks {
            self.index.push(folded(&format!(
                "{} {} {}",
                track.title,
                track.artists.join(" "),
                track.album_title.as_deref().unwrap_or("")
            )));
        }
        self.rebuild();
    }
    fn rebuild(&mut self) {
        self.visible.clear();
        self.ranks.clear();
        for (position, text) in self.index.iter().enumerate() {
            let rank = self
                .tokens
                .iter()
                .all(|token| text.contains(token))
                .then_some(self.visible.len());
            if rank.is_some() {
                self.visible.push(position);
            }
            self.ranks.push(rank);
        }
    }
}

#[derive(Clone)]
pub(super) struct FilterBar {
    pub widget: gtk::Box,
    pub search: gtk::SearchEntry,
    pub status: gtk::Label,
    pub more: gtk::Button,
}
impl FilterBar {
    pub(super) fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 4);
        widget.set_widget_name("music-collection-filter");
        let search = gtk::SearchEntry::new();
        search.set_widget_name("music-collection-search");
        search.set_placeholder_text(Some("Поиск по трекам"));
        search.set_hexpand(true);
        search.update_property(&[gtk::accessible::Property::Label(
            "Поиск по загруженным трекам коллекции",
        )]);
        let status = text("", "dim-label");
        status.set_widget_name("music-collection-filter-status");
        status.add_css_class("caption");
        let more = gtk::Button::with_label("Искать в следующей части");
        more.set_widget_name("music-collection-filter-more");
        more.set_halign(gtk::Align::Start);
        more.set_visible(false);
        widget.append(&search);
        widget.append(&status);
        widget.append(&more);
        Self {
            widget,
            search,
            status,
            more,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track(title: &str, artist: &str, album: &str) -> Track {
        Track {
            id: "42".into(),
            title: title.into(),
            artists: vec![artist.into()],
            artist_links: vec![],
            album_id: None,
            album_title: Some(album.into()),
            duration_ms: None,
            available: true,
            cover: None,
            explicit: false,
        }
    }
    #[test]
    fn compatibility_capitals_match_lowercase_and_plain_queries_in_source_order() {
        let mut filter = CollectionFilter::default();
        filter.append(&[
            track("𝐀", "Group", "Collection"),
            track("𝐚", "Group", "Collection"),
            track("ᴬ", "Group", "Collection"),
        ]);
        for query in ["a", "A", "𝐀", "𝐚", "ᴬ"] {
            filter.set_query(query);
            assert_eq!(
                filter.visible(),
                [0, 1, 2],
                "compatibility characters retain distinct source occurrences"
            );
        }
        filter.set_query("𝐁");
        assert!(filter.visible().is_empty());
        filter.set_query("");
        assert_eq!(filter.visible(), [0, 1, 2]);
    }

    #[test]
    fn unicode_collection_filter_matches_metadata_without_deduplicating_occurrences() {
        let mut filter = CollectionFilter::default();
        filter.append(&[
            track("Ёлка", "Группа", "Альбом"),
            track("Straße", "Σίσυφος", "Cafe\u{301}"),
            track("Ёлка", "Группа", "Альбом"),
        ]);
        assert_eq!(filter.visible(), [0, 1, 2]);
        assert!(filter.set_query("гРУППА ёЛКА"));
        assert_eq!(filter.visible(), [0, 2]);
        assert!(!filter.set_query("ГРУППА ЁЛКА"));
        filter.set_query("STRASSE");
        assert_eq!(filter.visible(), [1]);
        filter.set_query("CAFÉ");
        assert_eq!(filter.visible(), [1]);
        filter.set_query("σίΣΥΦΟΣ");
        assert_eq!(filter.visible(), [1]);
        filter.set_query("нет совпадений");
        assert!(filter.visible().is_empty());
        filter.append(&[track("Нет совпадений", "Другой", "Поздний")]);
        assert_eq!(filter.visible(), [3]);
        filter.set_query("");
        assert_eq!(filter.visible(), [0, 1, 2, 3]);
    }
}

#[cfg(test)]
mod native_tests;
