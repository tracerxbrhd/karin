// SPDX-License-Identifier: GPL-3.0-or-later
//! Unicode matching and ordered projections over admitted collection metadata.
//! The displayed projection never changes the active queue.
use super::*;
use std::{cmp::Reverse, collections::BTreeSet};

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
    by_title: BTreeSet<SortKey>,
    by_artist: BTreeSet<SortKey>,
    newest: BTreeSet<(Reverse<Option<i64>>, usize)>,
    oldest: BTreeSet<(Option<i64>, usize)>,
    sort: CollectionSort,
}
type SortKey = (Rc<str>, Rc<str>, usize);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum CollectionSort {
    #[default]
    Provider,
    ReverseProvider,
    Title,
    Artist,
    AddedNewest,
    AddedOldest,
}
impl CollectionSort {
    const CHOICES: [(Self, &'static str, &'static str); 6] = [
        (Self::Provider, "provider", "Порядок коллекции"),
        (Self::ReverseProvider, "reverse", "Обратный порядок"),
        (Self::Title, "title", "По названию"),
        (Self::Artist, "artist", "По исполнителю"),
        (Self::AddedNewest, "newest", "Сначала недавно добавленные"),
        (Self::AddedOldest, "oldest", "Сначала давно добавленные"),
    ];

    fn key(self) -> &'static str {
        Self::CHOICES
            .iter()
            .find_map(|(choice, key, _)| (*choice == self).then_some(*key))
            .expect("every collection order has a menu target")
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::CHOICES
            .iter()
            .find_map(|(choice, target, _)| (*target == key).then_some(*choice))
    }

    pub(super) fn needs_dates(self) -> bool {
        matches!(self, Self::AddedNewest | Self::AddedOldest)
    }
}

impl CollectionFilter {
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }
    pub(super) fn sort(&self) -> CollectionSort {
        self.sort
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
        self.append_dated(tracks, &[]);
    }
    pub(super) fn append_dated(&mut self, tracks: &[Track], dates: &[Option<i64>]) {
        for (index, track) in tracks.iter().enumerate() {
            let position = self.index.len();
            let artists = track.artists.join(" ");
            self.index.push(folded(&format!(
                "{} {} {}",
                track.title,
                artists,
                track.album_title.as_deref().unwrap_or("")
            )));
            let title: Rc<str> = folded(&track.title).into();
            let artist: Rc<str> = folded(&artists).into();
            self.by_title
                .insert((title.clone(), artist.clone(), position));
            self.by_artist.insert((artist, title, position));
            self.insert_date(position, dates.get(index).copied().flatten());
        }
        self.rebuild();
    }
    pub(super) fn replace_dates(&mut self, dates: impl IntoIterator<Item = Option<i64>>) {
        self.oldest.clear();
        self.newest.clear();
        for (position, date) in dates.into_iter().enumerate() {
            self.insert_date(position, date);
        }
        self.rebuild();
    }
    fn insert_date(&mut self, position: usize, date: Option<i64>) {
        self.oldest.insert((date, position));
        self.newest.insert((Reverse(date), position));
    }
    fn rebuild(&mut self) {
        // Maintain both text orders on admission. A sort/query change is a
        // scan of existing keys, not another full comparison sort over up to
        // 20,000 tracks on the GTK event loop.
        self.visible.clear();
        self.ranks.clear();
        self.ranks.resize(self.index.len(), None);
        let order: Box<dyn Iterator<Item = usize>> = match self.sort {
            CollectionSort::Provider => Box::new(0..self.index.len()),
            CollectionSort::ReverseProvider => Box::new((0..self.index.len()).rev()),
            CollectionSort::Title => {
                Box::new(self.by_title.iter().map(|(_, _, position)| *position))
            }
            CollectionSort::Artist => {
                Box::new(self.by_artist.iter().map(|(_, _, position)| *position))
            }
            CollectionSort::AddedNewest => {
                Box::new(self.newest.iter().map(|(_, position)| *position))
            }
            CollectionSort::AddedOldest => {
                Box::new(self.oldest.iter().map(|(_, position)| *position))
            }
        };
        let tokens = &self.tokens;
        for position in order {
            if tokens
                .iter()
                .all(|token| self.index[position].contains(token))
            {
                self.ranks[position] = Some(self.visible.len());
                self.visible.push(position);
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct FilterBar {
    pub widget: gtk::Box,
    pub search: gtk::SearchEntry,
    pub status: gtk::Label,
    status_row: gtk::Box,
    spinner: gtk::Spinner,
    pub index_action: gtk::Button,
    pub previous: gtk::Button,
    pub next: gtk::Button,
    pub sort: gtk::MenuButton,
    sort_action: gio::SimpleAction,
    has_date_options: Rc<Cell<bool>>,
}
impl FilterBar {
    pub(super) fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 4);
        widget.set_widget_name("music-collection-filter");
        let (toolbar, row, search) = super::search_toolbar::build(
            "Поиск по трекам",
            "music-collection-search",
            "Поиск по названию, исполнителю и альбому в коллекции",
        );
        let sort = gtk::MenuButton::builder()
            .icon_name("view-sort-ascending-symbolic")
            .has_frame(false)
            .tooltip_text("Сортировка")
            .build();
        sort.set_widget_name("music-collection-sort");
        sort.update_property(&[gtk::accessible::Property::Label("Сортировка")]);
        let sort_action = gio::SimpleAction::new_stateful(
            "order",
            Some(glib::VariantTy::STRING),
            &CollectionSort::Provider.key().to_variant(),
        );
        let actions = gio::SimpleActionGroup::new();
        actions.add_action(&sort_action);
        sort.insert_action_group("collection-sort", Some(&actions));
        Self::set_sort_menu(&sort, false);
        row.append(&sort);

        let status_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        status_row.set_widget_name("music-collection-filter-status-row");
        status_row.set_halign(gtk::Align::Start);
        status_row.set_visible(false);
        let status = text("", "dim-label");
        status.set_widget_name("music-collection-filter-status");
        status.add_css_class("caption");
        let spinner = gtk::Spinner::new();
        spinner.set_valign(gtk::Align::Center);
        spinner.set_visible(false);
        let index_action = gtk::Button::new();
        index_action.set_widget_name("music-collection-index-action");
        index_action.add_css_class("flat");
        index_action.add_css_class("caption");
        index_action.set_valign(gtk::Align::Center);
        index_action.set_visible(false);
        status_row.append(&spinner);
        status_row.append(&status);
        status_row.append(&index_action);

        let window = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let previous = gtk::Button::with_label("Предыдущие результаты");
        previous.set_widget_name("music-collection-results-previous");
        previous.set_visible(false);
        let next = gtk::Button::with_label("Следующие результаты");
        next.set_widget_name("music-collection-results-next");
        next.set_visible(false);
        window.append(&previous);
        window.append(&next);
        widget.append(&toolbar);
        widget.append(&status_row);
        widget.append(&window);
        Self {
            widget,
            search,
            status,
            status_row,
            spinner,
            index_action,
            previous,
            next,
            sort,
            sort_action,
            has_date_options: Rc::new(Cell::new(false)),
        }
    }

    fn set_sort_menu(button: &gtk::MenuButton, dates: bool) {
        let menu = gio::Menu::new();
        for (sort, key, label) in CollectionSort::CHOICES {
            if dates || !sort.needs_dates() {
                let item = gio::MenuItem::new(Some(label), None);
                item.set_action_and_target_value(
                    Some("collection-sort.order"),
                    Some(&key.to_variant()),
                );
                menu.append_item(&item);
            }
        }
        button.set_menu_model(Some(&menu));
        if let Some(popover) = button.popover() {
            // Anchor the menu to the trailing edge of the icon. Centering it
            // here can place labels outside a compact window, particularly at 2×.
            popover.set_halign(gtk::Align::End);
        }
    }

    pub(super) fn connect_sort(&self, select: impl Fn(CollectionSort) + 'static) {
        self.sort_action.connect_activate(move |_, target| {
            if let Some(sort) = target
                .and_then(glib::Variant::str)
                .and_then(CollectionSort::from_key)
            {
                select(sort);
            }
        });
    }

    pub(super) fn update_sort(&self, selected: CollectionSort, dates: bool, enabled: bool) {
        if self.has_date_options.replace(dates) != dates {
            Self::set_sort_menu(&self.sort, dates);
        }
        if self
            .sort_action
            .state()
            .as_ref()
            .and_then(glib::Variant::str)
            != Some(selected.key())
        {
            self.sort_action.set_state(&selected.key().to_variant());
        }
        let active = selected != CollectionSort::Provider;
        self.sort.set_has_frame(active);
        if active {
            self.sort.add_css_class("music-collection-sort-active");
        } else {
            self.sort.remove_css_class("music-collection-sort-active");
        }
        self.sort.set_sensitive(enabled);
    }

    #[cfg(test)]
    pub(super) fn selected_sort(&self) -> Option<CollectionSort> {
        self.sort_action
            .state()
            .as_ref()
            .and_then(glib::Variant::str)
            .and_then(CollectionSort::from_key)
    }

    pub(super) fn set_status(&self, message: &str, running: bool, action: Option<&str>) {
        self.status.set_text(message);
        self.spinner.set_spinning(running);
        self.spinner.set_visible(running);
        if let Some(label) = action {
            self.index_action.set_label(label);
        }
        self.index_action.set_visible(action.is_some());
        self.status_row.set_visible(!message.is_empty());
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
        filter.set_sort(CollectionSort::Artist);
        filter.append_dated(&[track("0", "AAA", "Collection")], &[Some(2_403)]);
        assert_eq!(filter.visible().first(), Some(&2_403));
        assert_eq!(filter.rank(42), Some(2_402));
        filter.set_sort(CollectionSort::AddedOldest);
        assert_eq!(filter.visible().first(), Some(&0));
        assert_eq!(filter.visible().last(), Some(&2_403));
        filter.set_sort(CollectionSort::ReverseProvider);
        assert_eq!(filter.visible().first(), Some(&2_403));
        assert_eq!(filter.visible().len(), 2_404);
    }
}

#[cfg(test)]
mod native_tests;
