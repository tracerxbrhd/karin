// SPDX-License-Identifier: GPL-3.0-or-later
//! Unicode matching/sorting over admitted metadata. Explicit bounded indexing
//! is separate from typing; neither projection changes the active queue.
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
    keys: Vec<(String, String)>,
    sort: CollectionSort,
    dates: Vec<Option<i64>>,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum CollectionSort {
    #[default]
    Provider,
    ReverseProvider,
    Title,
    Artist,
    AddedNewest,
    AddedOldest,
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
    pub(super) fn set_sort(&mut self, sort: CollectionSort) -> bool {
        if self.sort == sort {
            return false;
        }
        self.sort = sort;
        self.revision = self.revision.wrapping_add(1);
        self.rebuild();
        true
    }
    pub(super) fn append(&mut self, tracks: &[Track]) {
        self.append_dated(tracks, &vec![None; tracks.len()]);
    }
    pub(super) fn append_dated(&mut self, tracks: &[Track], dates: &[Option<i64>]) {
        for (index, track) in tracks.iter().enumerate() {
            self.index.push(folded(&format!(
                "{} {} {}",
                track.title,
                track.artists.join(" "),
                track.album_title.as_deref().unwrap_or("")
            )));
            self.keys
                .push((folded(&track.title), folded(&track.artists.join(" "))));
            self.dates.push(dates.get(index).copied().flatten());
        }
        self.rebuild();
    }
    fn rebuild(&mut self) {
        self.visible.clear();
        self.ranks.clear();
        for (position, text) in self.index.iter().enumerate() {
            let matches = self.tokens.iter().all(|token| text.contains(token));
            if matches {
                self.visible.push(position);
            }
            self.ranks.push(None);
        }
        match self.sort {
            CollectionSort::Provider => {}
            CollectionSort::ReverseProvider => self.visible.reverse(),
            CollectionSort::Title => self.visible.sort_by(|left, right| {
                self.keys[*left]
                    .0
                    .cmp(&self.keys[*right].0)
                    .then_with(|| self.keys[*left].1.cmp(&self.keys[*right].1))
                    .then_with(|| left.cmp(right))
            }),
            CollectionSort::Artist => self.visible.sort_by(|left, right| {
                self.keys[*left]
                    .1
                    .cmp(&self.keys[*right].1)
                    .then_with(|| self.keys[*left].0.cmp(&self.keys[*right].0))
                    .then_with(|| left.cmp(right))
            }),
            CollectionSort::AddedNewest => self.visible.sort_by(|left, right| {
                self.dates[*right]
                    .cmp(&self.dates[*left])
                    .then_with(|| left.cmp(right))
            }),
            CollectionSort::AddedOldest => self.visible.sort_by(|left, right| {
                self.dates[*left]
                    .cmp(&self.dates[*right])
                    .then_with(|| left.cmp(right))
            }),
        }
        for (rank, original) in self.visible.iter().enumerate() {
            self.ranks[*original] = Some(rank);
        }
    }
}

#[derive(Clone)]
pub(super) struct FilterBar {
    pub widget: gtk::Box,
    pub search: gtk::SearchEntry,
    pub status: gtk::Label,
    pub more: gtk::Button,
    pub index_all: gtk::Button,
    pub cancel_index: gtk::Button,
    pub previous: gtk::Button,
    pub next: gtk::Button,
    pub sort: gtk::DropDown,
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
            "Поиск по названию, исполнителю и альбому в коллекции",
        )]);
        let sort = gtk::DropDown::from_strings(&[
            "Порядок коллекции",
            "Обратный порядок",
            "По названию",
            "По исполнителю",
        ]);
        sort.set_widget_name("music-collection-sort");
        sort.update_property(&[gtk::accessible::Property::Label(
            "Сортировка треков коллекции",
        )]);
        let status = text("", "dim-label");
        status.set_widget_name("music-collection-filter-status");
        status.add_css_class("caption");
        let more = gtk::Button::with_label("Искать в следующей части");
        more.set_widget_name("music-collection-filter-more");
        more.set_halign(gtk::Align::Start);
        more.set_visible(false);
        let index_all = gtk::Button::with_label("Искать во всей коллекции");
        index_all.set_widget_name("music-collection-index-all");
        index_all.set_halign(gtk::Align::Start);
        index_all.set_visible(false);
        let cancel_index = gtk::Button::with_label("Приостановить загрузку");
        cancel_index.set_widget_name("music-collection-index-cancel");
        cancel_index.set_halign(gtk::Align::Start);
        cancel_index.set_visible(false);
        let window = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let previous = gtk::Button::with_label("Предыдущие результаты");
        previous.set_widget_name("music-collection-results-previous");
        previous.set_visible(false);
        let next = gtk::Button::with_label("Следующие результаты");
        next.set_widget_name("music-collection-results-next");
        next.set_visible(false);
        window.append(&previous);
        window.append(&next);
        widget.append(&search);
        widget.append(&sort);
        widget.append(&status);
        widget.append(&more);
        widget.append(&index_all);
        widget.append(&cancel_index);
        widget.append(&window);
        Self {
            widget,
            search,
            status,
            more,
            index_all,
            cancel_index,
            previous,
            next,
            sort,
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

    #[test]
    fn whole_collection_projection_sorts_more_than_2000_positions_and_preserves_equal_occurrences()
    {
        let mut filter = CollectionFilter::default();
        let tracks = (0..2_403)
            .map(|index| {
                if index == 42 || index == 2_401 {
                    track("Ёлка", "Straße", "Cafe\u{301}")
                } else {
                    track(&format!("Original {index:05}"), "Other", "Collection")
                }
            })
            .collect::<Vec<_>>();
        let dates = (0..tracks.len())
            .map(|index| Some(index as i64))
            .collect::<Vec<_>>();
        filter.append_dated(&tracks, &dates);
        filter.set_query("STRASSE CAFÉ");
        assert_eq!(filter.visible(), [42, 2_401]);
        filter.set_sort(CollectionSort::AddedNewest);
        assert_eq!(filter.visible(), [2_401, 42]);
        assert_eq!(filter.rank(2_401), Some(0));
        filter.set_sort(CollectionSort::Title);
        assert_eq!(
            filter.visible(),
            [42, 2_401],
            "stable equal display fields retain two provider occurrences"
        );
        filter.set_query("Original 02402");
        assert_eq!(filter.visible(), [2_402]);
        filter.set_query("");
        filter.set_sort(CollectionSort::ReverseProvider);
        assert_eq!(filter.visible().first(), Some(&2_402));
        assert_eq!(filter.visible().len(), 2_403);
    }
}

#[cfg(test)]
mod native_tests;
