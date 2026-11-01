// SPDX-License-Identifier: GPL-3.0-or-later
//! A bounded, occurrence-preserving loaded view, separate from the live queue.

use super::*;
use std::time::Duration;

pub(super) const VIEW_LIMIT: usize = 2_000;
const CONTEXT_LIMIT: usize = 500;
const PAGE_LIMIT: u32 = 100;

#[derive(Default)]
pub(super) struct PageOrigins(
    std::collections::BTreeMap<u32, (crate::catalog_cache::Provenance, bool)>,
);
impl PageOrigins {
    pub(super) fn record(
        &mut self,
        page: u32,
        origin: crate::catalog_cache::Provenance,
        error: bool,
    ) {
        if page < PAGE_LIMIT {
            self.0.insert(page, (origin, error));
        }
    }
    pub(super) fn aggregate(&self) -> Option<(crate::catalog_cache::Provenance, bool)> {
        aggregate_origins(self.0.values().copied())
    }
}
pub(super) fn aggregate_origins(
    origins: impl IntoIterator<Item = (crate::catalog_cache::Provenance, bool)>,
) -> Option<(crate::catalog_cache::Provenance, bool)> {
    use crate::catalog_cache::Provenance;
    origins
        .into_iter()
        .reduce(|(left, error), (right, next_error)| {
            let origin = if left == Provenance::StaleCache || right == Provenance::StaleCache {
                Provenance::StaleCache
            } else if left == Provenance::Network || right == Provenance::Network {
                Provenance::Network
            } else {
                Provenance::FreshCache
            };
            (origin, error || next_error)
        })
}

pub(super) struct LoadedTracks {
    pub items: Vec<Track>,
    pub next_page: u32,
    pub has_more: bool,
    pub loading: bool,
    pub error: Option<MusicError>,
    #[cfg(test)]
    pub epoch: u64,
    pub incomplete: bool,
    kind: ContextKind,
    source_key: String,
    pub filter: collection_filter::CollectionFilter,
    applied: std::collections::BTreeSet<u32>,
    contexts: RefCell<std::collections::BTreeMap<usize, PlaybackContext>>,
}

impl LoadedTracks {
    pub(super) fn new(kind: ContextKind, source: Option<String>, epoch: u64) -> Self {
        let digest = ring::digest::digest(
            &ring::digest::SHA256,
            source.as_deref().unwrap_or("").as_bytes(),
        );
        let opaque = digest
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Self {
            items: Vec::new(),
            next_page: 0,
            has_more: true,
            loading: false,
            error: None,
            #[cfg(test)]
            epoch,
            incomplete: false,
            kind,
            source_key: format!("continuous:{epoch}:{opaque}"),
            filter: collection_filter::CollectionFilter::default(),
            applied: Default::default(),
            contexts: RefCell::new(Default::default()),
        }
    }

    /// Page identity, never track ID, prevents repeated delivery. Two equal
    /// IDs at different provider positions remain two playback occurrences.
    pub(super) fn append(&mut self, page: u32, result: Page<Track>) -> std::ops::Range<usize> {
        let start = self.items.len();
        if page != self.next_page || !self.applied.insert(page) {
            return start..start;
        }
        self.contexts.borrow_mut().clear();
        self.loading = false;
        self.error = None;
        let room = VIEW_LIMIT.saturating_sub(start);
        let truncated = result.items.len() > room;
        self.items.extend(result.items.into_iter().take(room));
        self.filter.append(&self.items[start..]);
        self.next_page = page.saturating_add(1);
        self.has_more = result.has_more || truncated;
        start..self.items.len()
    }

    pub(super) fn can_request(&self) -> bool {
        !self.loading && self.error.is_none() && self.has_more && !self.at_limit()
    }

    pub(super) fn at_limit(&self) -> bool {
        self.items.len() >= VIEW_LIMIT || self.next_page >= PAGE_LIMIT
    }

    pub(super) fn set_filter(&mut self, query: &str) -> bool {
        if !self.filter.set_query(query) {
            return false;
        }
        self.contexts.borrow_mut().clear();
        true
    }
    pub(super) fn first_playable(&self) -> Option<usize> {
        self.filter
            .visible()
            .iter()
            .copied()
            .find(|index| self.items[*index].available)
    }
    pub(super) fn matches_current(&self, index: usize, current: &PlaybackContext) -> bool {
        self.filter.rank(index).is_some()
            && current.source_kind() == self.kind
            && current.source_id() == Some(self.source_key.as_str())
            && current.source_position(current.current_index()) == Some(index)
            && self.items.get(index).map(|track| track.id.as_str())
                == current
                    .items()
                    .get(current.current_index())
                    .map(|track| track.id.as_str())
    }
    pub(super) fn context(&self, index: usize) -> Option<PlaybackContext> {
        let rank = self.filter.rank(index)?;
        let visible = self.filter.visible();
        let start = rank / CONTEXT_LIMIT * CONTEXT_LIMIT;
        let end = (start + CONTEXT_LIMIT).min(visible.len());
        let partial = visible.len() < self.items.len()
            || start > 0
            || end < visible.len()
            || self.has_more
            || self.incomplete;
        if let Some(context) = self.contexts.borrow().get(&start)
            && context.is_partial() == partial
        {
            return context.with_current_index(rank - start);
        }
        let positions = visible[start..end].to_vec();
        let tracks = positions
            .iter()
            .map(|position| self.items[*position].clone())
            .collect();
        let context = PlaybackContext::with_source_positions(
            self.kind,
            Some(self.source_key.clone()),
            tracks,
            0,
            partial,
            positions,
        )
        .ok()?;
        self.contexts.borrow_mut().insert(start, context.clone());
        context.with_current_index(rank - start)
    }
}

#[derive(Clone)]
pub(super) enum FeedSource {
    Liked,
    Search(String),
    Playlist(Playlist),
    SearchPlaylist(crate::music::SearchPlaylist),
    Artist(Artist),
    Album(Arc<Album>),
}

pub(super) struct TrackFeed {
    pub data: Rc<RefCell<LoadedTracks>>,
    pub source: FeedSource,
    pub rows: gtk::ListBox,
    pub status: gtk::Box,
    pub generation: u64,
    pub origins: RefCell<PageOrigins>,
    prefill_remaining: Cell<u8>,
    restart_required: Cell<bool>,
    play_button: RefCell<glib::WeakRef<gtk::Button>>,
    pub filter_ui: Option<collection_filter::FilterBar>,
    pub hero: RefCell<Option<collection_hero::LikedCollectionHero>>,
    adjustment_guard: Cell<bool>,
}

pub(super) type Feed = Rc<TrackFeed>;

impl MusicPane {
    pub(in crate::ui) fn attach_content_scroll(&self, scroll: &gtk::ScrolledWindow) {
        self.controller.content_scroll.replace(scroll.downgrade());
        let weak = Rc::downgrade(&self.controller);
        let previous = Cell::new(scroll.vadjustment().value());
        let scroll_input =
            gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let weak_intent = Rc::downgrade(&self.controller);
        let weak_scroll = scroll.downgrade();
        scroll_input.connect_scroll(move |_, _, dy| {
            if dy > 0.0
                && let (Some(this), Some(scroll)) = (weak_intent.upgrade(), weak_scroll.upgrade())
            {
                let adjustment = scroll.vadjustment();
                if adjustment.value() + adjustment.page_size() + 320.0 >= adjustment.upper() {
                    this.load_visible_feed(true);
                }
            }
            glib::Propagation::Proceed
        });
        scroll.add_controller(scroll_input);
        let keys = gtk::EventControllerKey::new();
        let weak_keys = Rc::downgrade(&self.controller);
        let weak_scroll = scroll.downgrade();
        keys.connect_key_pressed(move |_, key, _, _| {
            if matches!(key, gtk::gdk::Key::Page_Down | gtk::gdk::Key::End)
                && let (Some(this), Some(scroll)) = (weak_keys.upgrade(), weak_scroll.upgrade())
            {
                let adjustment = scroll.vadjustment();
                if adjustment.value() + adjustment.page_size() + 320.0 >= adjustment.upper() {
                    this.load_visible_feed(true);
                }
            }
            glib::Propagation::Proceed
        });
        scroll.add_controller(keys);
        scroll
            .vadjustment()
            .connect_value_changed(move |adjustment| {
                let old = previous.replace(adjustment.value());
                // Only forward scrolling drives requests. Appending/remeasuring the
                // viewport does not recursively drain a remote collection.
                if adjustment.value() <= old + 0.5
                    || adjustment.value() + adjustment.page_size() + 320.0 < adjustment.upper()
                {
                    return;
                }
                if let Some(this) = weak.upgrade() {
                    this.load_visible_feed(false);
                }
            });
    }
}

impl Controller {
    pub(super) fn new_track_feed(
        self: &Rc<Self>,
        source: FeedSource,
        kind: ContextKind,
        source_id: Option<String>,
        rows: &gtk::ListBox,
        status: &gtk::Box,
    ) -> Feed {
        self.feed_epoch.set(self.feed_epoch.get().wrapping_add(1));
        let feed = Rc::new(TrackFeed {
            data: Rc::new(RefCell::new(LoadedTracks::new(
                kind,
                source_id,
                self.feed_epoch.get(),
            ))),
            source: source.clone(),
            rows: rows.clone(),
            status: status.clone(),
            generation: self.model.borrow().generation,
            origins: RefCell::new(PageOrigins::default()),
            prefill_remaining: Cell::new(2),
            restart_required: Cell::new(false),
            play_button: RefCell::new(glib::WeakRef::new()),
            filter_ui: (!matches!(source, FeedSource::Search(_)))
                .then(collection_filter::FilterBar::new),
            hero: RefCell::new(None),
            adjustment_guard: Cell::new(false),
        });
        if matches!(feed.source, FeedSource::Album(_))
            && let Some((origin, error)) = self.detail_cache.get()
        {
            feed.origins.borrow_mut().record(0, origin, error);
        }
        for old in self
            .track_feeds
            .borrow()
            .iter()
            .filter(|old| old.rows == *rows)
        {
            Self::remove_filter_bar(old);
        }
        self.track_feeds
            .borrow_mut()
            .retain(|old| old.rows.root().is_some() && old.rows != *rows);
        self.track_feeds.borrow_mut().push(feed.clone());
        clear_list(rows);
        self.attach_filter_bar(&feed);
        self.render_feed_status(&feed);
        self.refresh_catalog_notice();
        feed
    }

    fn set_liked_markers(feed: &Feed, visible: bool) {
        if !matches!(feed.source, FeedSource::Liked) {
            return;
        }
        let mut row = feed.rows.first_child();
        while let Some(widget) = row {
            row = widget.next_sibling();
            let Some(body) = widget
                .downcast_ref::<gtk::ListBoxRow>()
                .and_then(gtk::ListBoxRow::child)
            else {
                continue;
            };
            let mut child = body.first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                if widget.widget_name() == "music-loaded-liked-marker" {
                    widget.set_visible(visible);
                }
            }
        }
    }

    fn remove_filter_bar(feed: &Feed) {
        if let Some(bar) = &feed.filter_ui
            && let Some(parent) = bar.widget.parent().and_downcast::<gtk::Box>()
        {
            parent.remove(&bar.widget);
        }
    }
    pub(super) fn reset_collection_filters(&self) {
        let feeds = self.track_feeds.borrow().clone();
        for feed in feeds {
            if let Some(bar) = &feed.filter_ui {
                bar.search.set_text("");
            }
        }
    }

    pub(super) fn clear_track_feeds(&self) {
        let feeds = std::mem::take(&mut *self.track_feeds.borrow_mut());
        for feed in feeds {
            Self::set_liked_markers(&feed, false);
            Self::remove_filter_bar(&feed);
        }
    }
    fn attach_filter_bar(self: &Rc<Self>, feed: &Feed) {
        let Some(bar) = &feed.filter_ui else {
            return;
        };
        if let Some(parent) = feed.rows.parent().and_downcast::<gtk::Box>() {
            parent.insert_child_after(&bar.widget, feed.rows.prev_sibling().as_ref());
        }
        let weak = Rc::downgrade(self);
        let weak_feed = Rc::downgrade(feed);
        bar.search.connect_changed(move |entry| {
            let bounded = entry.text().chars().take(256).collect::<String>();
            if entry.text().as_str() != bounded {
                entry.set_text(&bounded);
                entry.set_position(-1);
                return;
            }
            if let (Some(this), Some(feed)) = (weak.upgrade(), weak_feed.upgrade())
                && this.feed_is_current(&feed)
            {
                let changed = feed.data.borrow_mut().set_filter(&bounded);
                if !changed {
                    return;
                }
                feed.adjustment_guard.set(true);
                let revision = feed.data.borrow().filter.revision();
                let weak_feed = Rc::downgrade(&feed);
                let frames = Cell::new(0);
                entry.add_tick_callback(move |_, _| {
                    let Some(feed) = weak_feed.upgrade() else {
                        return glib::ControlFlow::Break;
                    };
                    if feed.data.borrow().filter.revision() != revision {
                        return glib::ControlFlow::Break;
                    }
                    frames.set(frames.get() + 1);
                    if frames.get() >= 2 {
                        feed.adjustment_guard.set(false);
                        glib::ControlFlow::Break
                    } else {
                        glib::ControlFlow::Continue
                    }
                });
                let data = feed.data.borrow();
                for index in 0..data.items.len() {
                    if let Some(row) = feed.rows.row_at_index(index as i32) {
                        row.set_visible(data.filter.rank(index).is_some());
                    }
                }
                drop(data);
                this.render_feed_status(&feed);
                this.sync_catalog_rows();
            }
        });
        let weak = Rc::downgrade(self);
        let weak_feed = Rc::downgrade(feed);
        bar.more.connect_clicked(move |_| {
            if let (Some(this), Some(feed)) = (weak.upgrade(), weak_feed.upgrade()) {
                this.request_track_feed(&feed, false);
            }
        });
    }
    fn update_filter_bar(&self, feed: &Feed) {
        let Some(bar) = &feed.filter_ui else {
            return;
        };
        let data = feed.data.borrow();
        let active = data.filter.active();
        let partial =
            data.has_more || data.incomplete || data.error.is_some() || feed.restart_required.get();
        let count = data.filter.visible().len();
        bar.status.set_text(&if active {
            if partial {
                format!(
                    "В загруженной части ({}): найдено {count}",
                    track_count_text(data.items.len() as u64)
                )
            } else if count == 0 {
                "Совпадений нет".into()
            } else {
                format!("Найдено: {count}")
            }
        } else if partial {
            format!(
                "Поиск по загруженной части: {}",
                track_count_text(data.items.len() as u64)
            )
        } else {
            String::new()
        });
        bar.status.set_visible(active || partial);
        bar.more.set_visible(
            active
                && data.has_more
                && !data.at_limit()
                && data.error.is_none()
                && !feed.restart_required.get(),
        );
        bar.more
            .set_sensitive(!data.loading && self.feed_is_current(feed));
    }
    pub(super) fn install_liked_hero(self: &Rc<Self>, feed: &Feed) {
        clear_box(&self.widgets.library_hero);
        if let Some(parent) = self
            .widgets
            .library_refresh
            .parent()
            .and_downcast::<gtk::Box>()
        {
            parent.remove(&self.widgets.library_refresh);
        }
        self.widgets
            .library_hero
            .set_orientation(gtk::Orientation::Vertical);
        self.widgets.library_hero.set_spacing(0);
        self.widgets
            .library_hero
            .remove_css_class("music-library-summary");
        let play = self.feed_play_button(feed);
        let hero = collection_hero::LikedCollectionHero::new(&play, &self.widgets.library_refresh);
        hero.update(&feed.data.borrow().items, false);
        self.widgets.library_hero.append(hero.widget());
        feed.hero.replace(Some(hero));
    }
    fn feed_is_current(&self, feed: &Feed) -> bool {
        let model = self.model.borrow();
        model.account_valid
            && !model.closed
            && model.generation == feed.generation
            && self
                .track_feeds
                .borrow()
                .iter()
                .any(|current| Rc::ptr_eq(current, feed))
    }

    pub(super) fn visible_feed_cache_state(
        &self,
    ) -> Option<Option<(crate::catalog_cache::Provenance, bool)>> {
        self.track_feeds
            .borrow()
            .iter()
            .find(|feed| feed.rows.is_mapped() && feed.status.is_mapped())
            .map(|feed| feed.origins.borrow().aggregate())
    }

    pub(super) fn append_track_feed(self: &Rc<Self>, feed: &Feed, page: u32, result: Page<Track>) {
        if !self.feed_is_current(feed) {
            return;
        }
        if page == 0
            && feed.data.borrow().next_page == 0
            && matches!(feed.source, FeedSource::Liked)
        {
            let mut model = self.model.borrow_mut();
            model.liked = Some(Ok(result.clone()));
            model.library_displaying = Some((0, 0));
            model.library_page = 0;
        }
        let range = feed.data.borrow_mut().append(page, result);
        for index in range {
            let track = feed.data.borrow().items[index].clone();
            let data = feed.data.clone();
            let weak = Rc::downgrade(self);
            let generation = feed.generation;
            let resolve: Rc<dyn Fn() -> Option<PlaybackContext>> = Rc::new(move || {
                let this = weak.upgrade()?;
                if this.model.borrow().generation != generation
                    || !this
                        .track_feeds
                        .borrow()
                        .iter()
                        .any(|feed| Rc::ptr_eq(&feed.data, &data))
                {
                    return None;
                }
                data.borrow().context(index)
            });
            let marker_data = feed.data.clone();
            let marker_weak = Rc::downgrade(self);
            let marker: Rc<dyn Fn(&PlaybackContext) -> bool> = Rc::new(move |current| {
                marker_weak.upgrade().is_some_and(|this| {
                    this.model.borrow().generation == generation
                        && this
                            .track_feeds
                            .borrow()
                            .iter()
                            .any(|feed| Rc::ptr_eq(&feed.data, &marker_data))
                }) && marker_data.borrow().matches_current(index, current)
            });
            self.track_row_bound(&feed.rows, track, resolve, Some(marker));
            if matches!(feed.source, FeedSource::Liked)
                && let Some(body) = feed
                    .rows
                    .row_at_index(index as i32)
                    .and_then(|row| row.child())
                    .and_downcast::<gtk::Box>()
            {
                let heart = gtk::Image::from_icon_name(interactions::like_icon_name(Some(true)));
                heart.set_widget_name("music-loaded-liked-marker");
                heart.set_pixel_size(16);
                heart.add_css_class("dim-label");
                heart.set_tooltip_text(Some("В загруженной коллекции «Мне нравится»"));
                heart.update_property(&[gtk::accessible::Property::Label(
                    "В загруженной коллекции «Мне нравится»",
                )]);
                let previous = body.last_child().and_then(|child| child.prev_sibling());
                body.insert_child_after(&heart, previous.as_ref());
            }
            if let Some(row) = feed.rows.row_at_index(index as i32) {
                row.set_visible(feed.data.borrow().filter.rank(index).is_some());
            }
        }
        self.render_feed_status(feed);
        self.sync_catalog_rows();
        // A sparse normalized page may not create a scrollbar. Fill at most
        // three provider pages per new source, then only real wheel/key intent
        // admits further reads; no empty-page loop or manual pagination button.
        let weak = Rc::downgrade(self);
        let weak_feed = Rc::downgrade(feed);
        let query_revision = feed.data.borrow().filter.revision();
        glib::timeout_add_local_once(Duration::from_millis(100), move || {
            if let (Some(this), Some(feed)) = (weak.upgrade(), weak_feed.upgrade())
                && this.feed_is_current(&feed)
                && feed.rows.is_mapped()
                && !feed.data.borrow().filter.active()
                && feed.data.borrow().filter.revision() == query_revision
                && feed.prefill_remaining.get() > 0
                && feed.data.borrow().can_request()
                && this
                    .content_scroll
                    .borrow()
                    .upgrade()
                    .is_some_and(|scroll| {
                        let adjustment = scroll.vadjustment();
                        adjustment.upper() <= adjustment.page_size() + 1.0
                    })
            {
                feed.prefill_remaining.set(feed.prefill_remaining.get() - 1);
                this.request_track_feed(&feed, false);
            }
        });
    }

    pub(super) fn feed_play_button(self: &Rc<Self>, feed: &Feed) -> gtk::Button {
        let button = gtk::Button::with_label("Слушать");
        button.add_css_class("suggested-action");
        button.set_widget_name("music-collection-play");
        button.set_sensitive(feed.data.borrow().first_playable().is_some());
        feed.play_button.replace(button.downgrade());
        let weak = Rc::downgrade(self);
        let feed = Rc::downgrade(feed);
        button.connect_clicked(move |_| {
            if let (Some(this), Some(feed)) = (weak.upgrade(), feed.upgrade())
                && this.feed_is_current(&feed)
            {
                let data = feed.data.borrow();
                if let Some(index) = data.first_playable()
                    && let Some(context) = data.context(index)
                {
                    drop(data);
                    this.play_music_context(context);
                }
            }
        });
        button
    }

    fn render_feed_status(self: &Rc<Self>, feed: &Feed) {
        self.update_filter_bar(feed);
        Self::set_liked_markers(feed, !feed.restart_required.get());
        if let Some(hero) = feed.hero.borrow().as_ref() {
            let data = feed.data.borrow();
            hero.update(
                &data.items,
                !data.has_more
                    && !data.incomplete
                    && data.error.is_none()
                    && !feed.restart_required.get(),
            );
        }
        clear_box(&feed.status);
        feed.status.set_widget_name("music-feed-status");
        let data = feed.data.borrow();
        if let Some(play) = feed.play_button.borrow().upgrade() {
            play.set_sensitive(self.feed_is_current(feed) && data.first_playable().is_some());
        }
        if data.loading {
            let spinner = gtk::Spinner::new();
            spinner.set_spinning(true);
            spinner.set_halign(gtk::Align::Center);
            feed.status.append(&spinner);
        } else if data.error.is_some() {
            feed.status.append(&text(
                if feed.restart_required.get() {
                    "Библиотека изменилась. Обновите список."
                } else {
                    "Не удалось загрузить следующие треки."
                },
                "dim-label",
            ));
            let retry = gtk::Button::with_label(if feed.restart_required.get() {
                "Обновить"
            } else {
                "Повторить"
            });
            retry.set_widget_name("music-feed-retry");
            retry.set_halign(gtk::Align::Center);
            let weak = Rc::downgrade(self);
            let weak_feed = Rc::downgrade(feed);
            retry.connect_clicked(move |_| {
                if let (Some(this), Some(feed)) = (weak.upgrade(), weak_feed.upgrade()) {
                    if feed.restart_required.get() {
                        this.load_library_refresh(0);
                    } else {
                        feed.data.borrow_mut().error = None;
                        this.request_track_feed(&feed, false);
                    }
                }
            });
            feed.status.append(&retry);
        } else if data.has_more && data.at_limit() {
            feed.status.append(&text(
                "Показана ограниченная часть коллекции. Остальные треки пока не загружены.",
                "dim-label",
            ));
        } else if !data.has_more && !data.filter.active() {
            feed.status.append(&text(
                if data.items.is_empty() {
                    "Треков пока нет."
                } else if data.incomplete {
                    "Доступные результаты загружены. Часть треков недоступна."
                } else {
                    "Все доступные треки загружены"
                },
                "dim-label",
            ));
        } else if data.items.is_empty() {
            feed.status.append(&text(
                "В этой части коллекции нет доступных треков. Прокрутите вниз, чтобы продолжить.",
                "dim-label",
            ));
        }
    }

    pub(super) fn refresh_feed_status(self: &Rc<Self>, feed: &Feed) {
        self.render_feed_status(feed);
    }

    pub(super) fn show_playlist_sections(self: &Rc<Self>) {
        self.playlist_grid.widget.set_visible(true);
        self.liked_playlist_grid.widget.set_visible(true);
        let cached = {
            let model = self.model.borrow();
            (model.playlists.clone(), model.liked_playlists.clone())
        };
        if let (Some(owned), Some(liked)) = cached {
            self.apply_playlist_sections(owned, liked);
        } else {
            self.load_playlist_sections(false);
        }
    }

    fn apply_playlist_sections(
        self: &Rc<Self>,
        owned: Result<Page<Playlist>, MusicError>,
        liked: Result<Page<Playlist>, MusicError>,
    ) {
        self.model.borrow_mut().playlists = Some(owned);
        self.model.borrow_mut().liked_playlists = Some(liked);
        self.render_playlist_section(false);
        self.render_playlist_section(true);
    }

    fn render_playlist_section(self: &Rc<Self>, liked: bool) {
        let grid = if liked {
            &self.liked_playlist_grid
        } else {
            &self.playlist_grid
        };
        let mut child = grid.widget.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if widget.widget_name() == "music-playlist-more" {
                grid.widget.remove(&widget);
            }
        }
        if self.playlist_refreshing.get()[usize::from(liked)] {
            grid.set_loading();
            return;
        }
        let result = {
            let model = self.model.borrow();
            if liked {
                model.liked_playlists.clone()
            } else {
                model.playlists.clone()
            }
        };
        match result {
            Some(Ok(page)) => {
                grid.set_playlists(&page.items, !liked && self.can_create_playlist.get());
                let mut child = grid.widget.first_child();
                while let Some(widget) = child {
                    child = widget.next_sibling();
                    if widget.widget_name() == "music-playlist-more" {
                        grid.widget.remove(&widget);
                    }
                }
                if page.has_more {
                    if page.items.len() >= VIEW_LIMIT
                        || self.playlist_pages.get()[usize::from(liked)] >= PAGE_LIMIT - 1
                    {
                        let label = text("Показана ограниченная часть плейлистов.", "dim-label");
                        label.set_widget_name("music-playlist-more");
                        grid.widget.append(&label);
                    } else {
                        let more = gtk::Button::with_label("Ещё плейлисты");
                        more.set_widget_name("music-playlist-more");
                        more.set_halign(gtk::Align::Start);
                        let weak = Rc::downgrade(self);
                        let epoch = self.playlist_epochs.get()[usize::from(liked)];
                        more.connect_clicked(move |button| {
                            if let Some(this) = weak.upgrade()
                                && this.playlist_epochs.get()[usize::from(liked)] == epoch
                                && !this.playlist_refreshing.get()[usize::from(liked)]
                            {
                                button.set_sensitive(false);
                                this.load_more_playlists(liked);
                            }
                        });
                        grid.widget.append(&more);
                    }
                }
            }
            Some(Err(_)) => grid.set_error(),
            None => grid.set_loading(),
        }
    }

    fn load_more_playlists(self: &Rc<Self>, liked: bool) {
        let index = usize::from(liked);
        if self.playlist_refreshing.get()[index] {
            return;
        }
        let page = self.playlist_pages.get()[index].saturating_add(1);
        let epoch = self.playlist_epochs.get()[index];
        let previous = {
            let model = self.model.borrow();
            if liked {
                model.liked_playlists.clone()
            } else {
                model.playlists.clone()
            }
        };
        let Some(Ok(mut previous)) = previous else {
            return;
        };
        if !previous.has_more || previous.items.len() >= VIEW_LIMIT || page >= PAGE_LIMIT {
            return;
        }
        self.run_catalog(
            if liked {
                Operation::LikedPlaylistsMore
            } else {
                Operation::OwnedPlaylistsMore
            },
            Some((if liked { 2 } else { 1 }, page)),
            false,
            move |service, token, catalog, force| match (liked, catalog) {
                (true, Some(cache)) => cache.liked_playlists(&service, &token, page, force),
                (false, Some(cache)) => cache.playlists(&service, &token, page, force),
                (true, None) => service
                    .liked_playlists(&token, page)
                    .map(catalog_view::network),
                (false, None) => service.playlists(&token, page).map(catalog_view::network),
            },
            move |this, result| {
                let same_prefix = {
                    let model = this.model.borrow();
                    let current = if liked {
                        model.liked_playlists.as_ref()
                    } else {
                        model.playlists.as_ref()
                    };
                    current
                        .and_then(|page| page.as_ref().ok())
                        .is_some_and(|page| {
                            page.items.len() == previous.items.len()
                                && page
                                    .items
                                    .iter()
                                    .map(|item| &item.id)
                                    .eq(previous.items.iter().map(|item| &item.id))
                        })
                };
                if this.playlist_epochs.get()[index] != epoch
                    || this.playlist_pages.get()[index].saturating_add(1) != page
                    || !same_prefix
                {
                    return;
                }
                match result {
                    Ok(result) => {
                        previous.has_more = result.has_more;
                        previous.items.extend(
                            result
                                .items
                                .into_iter()
                                .take(VIEW_LIMIT.saturating_sub(previous.items.len())),
                        );
                        let mut pages = this.playlist_pages.get();
                        pages[index] = page;
                        this.playlist_pages.set(pages);
                        if liked {
                            this.model.borrow_mut().liked_playlists = Some(Ok(previous));
                        } else {
                            this.model.borrow_mut().playlists = Some(Ok(previous));
                        }
                        this.render_playlist_section(liked);
                    }
                    Err(error) => {
                        if matches!(
                            error,
                            MusicError::AccessDenied | MusicError::Unauthenticated
                        ) {
                            this.show_error(error);
                            return;
                        }
                        this.render_playlist_section(liked);
                        let grid = if liked {
                            &this.liked_playlist_grid
                        } else {
                            &this.playlist_grid
                        };
                        let message = text(
                            "Не удалось загрузить следующие плейлисты. Попробуйте ещё раз.",
                            "dim-label",
                        );
                        message.set_widget_name("music-playlist-more");
                        grid.widget.append(&message);
                    }
                }
            },
        );
    }

    pub(super) fn load_playlist_sections(self: &Rc<Self>, force: bool) {
        self.load_playlist_sections_mask(force, None);
    }

    fn load_playlist_sections_mask(self: &Rc<Self>, force: bool, mask: Option<(bool, bool)>) {
        if !self.model.borrow().account_valid || self.model.borrow().closed {
            return;
        }
        let (need_owned, need_liked) = {
            let model = self.model.borrow();
            mask.unwrap_or((
                force
                    || self.playlist_refreshing.get()[0]
                    || !model.playlists.as_ref().is_some_and(Result::is_ok),
                force
                    || self.playlist_refreshing.get()[1]
                    || !model.liked_playlists.as_ref().is_some_and(Result::is_ok),
            ))
        };
        self.model
            .borrow_mut()
            .library_origins
            .retain(|(section, _, _, _)| {
                !((*section == 1 && need_owned) || (*section == 2 && need_liked))
            });
        let mut refreshing = self.playlist_refreshing.get();
        for (needed, index) in [(need_owned, 0), (need_liked, 1)] {
            if needed {
                refreshing[index] = true;
            }
        }
        self.playlist_refreshing.set(refreshing);
        let mut epochs = self.playlist_epochs.get();
        for (needed, index, lane) in [
            (need_owned, 0, Operation::OwnedPlaylistsMore),
            (need_liked, 1, Operation::LikedPlaylistsMore),
        ] {
            if needed {
                epochs[index] = epochs[index].wrapping_add(1);
                self.requests.borrow_mut().invalidate(lane);
            }
        }
        self.playlist_epochs.set(epochs);
        let mut pages = self.playlist_pages.get();
        if need_owned {
            pages[0] = 0;
            self.render_playlist_section(false);
        } else {
            self.render_playlist_section(false);
        }
        if need_liked {
            pages[1] = 0;
            self.render_playlist_section(true);
        } else {
            self.render_playlist_section(true);
        }
        self.playlist_pages.set(pages);
        if !need_owned && !need_liked {
            return;
        }
        let catalog = self.cached_catalog();
        self.run(
            Operation::PlaylistSections,
            move |service, token| {
                let owned = need_owned.then(|| match &catalog {
                    Some(cache) => cache.playlists(&service, &token, 0, force),
                    None => service.playlists(&token, 0).map(catalog_view::network),
                });
                let liked = need_liked.then(|| match &catalog {
                    Some(cache) => cache.liked_playlists(&service, &token, 0, force),
                    None => service
                        .liked_playlists(&token, 0)
                        .map(catalog_view::network),
                });
                Ok((owned, liked))
            },
            move |this, result| match result {
                Ok((owned, liked)) => {
                    let retired = owned
                        .as_ref()
                        .and_then(|result| result.as_ref().ok())
                        .into_iter()
                        .chain(liked.as_ref().and_then(|result| result.as_ref().ok()))
                        .any(|reply| {
                            reply
                                .lease
                                .as_ref()
                                .is_some_and(|lease| !lease.is_current())
                        });
                    if retired && !force {
                        this.load_playlist_sections_mask(true, Some((need_owned, need_liked)));
                        return;
                    }
                    for (section, result) in [(1, owned.as_ref()), (2, liked.as_ref())] {
                        if let Some(Ok(reply)) = result {
                            this.remember_library_origin(
                                section,
                                0,
                                reply.provenance,
                                reply.cache_error.is_some(),
                            );
                        }
                    }
                    let accept = |reply: Result<
                        crate::catalog_cache::Cached<Page<Playlist>>,
                        MusicError,
                    >| {
                        reply.and_then(|reply| {
                            if reply
                                .lease
                                .as_ref()
                                .is_some_and(|lease| !lease.is_current())
                            {
                                Err(MusicError::Network)
                            } else {
                                Ok(reply.value)
                            }
                        })
                    };
                    let owned = owned.map(accept).unwrap_or_else(|| {
                        this.model
                            .borrow()
                            .playlists
                            .clone()
                            .unwrap_or(Err(MusicError::Network))
                    });
                    let liked = liked.map(accept).unwrap_or_else(|| {
                        this.model
                            .borrow()
                            .liked_playlists
                            .clone()
                            .unwrap_or(Err(MusicError::Network))
                    });
                    if owned
                        .as_ref()
                        .err()
                        .into_iter()
                        .chain(liked.as_ref().err())
                        .any(|error| {
                            matches!(
                                error,
                                MusicError::AccessDenied | MusicError::Unauthenticated
                            )
                        })
                    {
                        this.show_error(MusicError::AccessDenied);
                        return;
                    }
                    let mut refreshing = this.playlist_refreshing.get();
                    for (needed, index) in [(need_owned, 0), (need_liked, 1)] {
                        if needed {
                            refreshing[index] = false;
                        }
                    }
                    this.playlist_refreshing.set(refreshing);
                    this.apply_playlist_sections(owned, liked);
                    this.refresh_catalog_notice();
                }
                Err(error) => {
                    let mut refreshing = this.playlist_refreshing.get();
                    for (needed, index) in [(need_owned, 0), (need_liked, 1)] {
                        if needed {
                            refreshing[index] = false;
                        }
                    }
                    this.playlist_refreshing.set(refreshing);
                    let owned = if need_owned {
                        Err(error)
                    } else {
                        this.model.borrow().playlists.clone().unwrap_or(Err(error))
                    };
                    let liked = if need_liked {
                        Err(error)
                    } else {
                        this.model
                            .borrow()
                            .liked_playlists
                            .clone()
                            .unwrap_or(Err(error))
                    };
                    this.apply_playlist_sections(owned, liked);
                }
            },
        );
    }

    pub(super) fn invalidate_liked_feeds(self: &Rc<Self>) {
        let feeds = self
            .track_feeds
            .borrow()
            .iter()
            .filter(|feed| matches!(feed.source, FeedSource::Liked))
            .cloned()
            .collect::<Vec<_>>();
        for feed in feeds {
            feed.restart_required.set(true);
            feed.data.borrow_mut().loading = false;
            feed.data.borrow_mut().error = Some(MusicError::Network);
            self.render_feed_status(&feed);
        }
    }

    pub(super) fn interrupt_feed_reads(self: &Rc<Self>, operation: Operation) {
        let feeds = self.track_feeds.borrow().clone();
        for feed in feeds {
            let same_lane = match feed.source {
                FeedSource::Liked => operation == Operation::Library,
                FeedSource::Search(_) => operation == Operation::Search,
                FeedSource::Album(_) => false,
                _ => operation == Operation::Detail,
            };
            if same_lane && feed.data.borrow().loading {
                feed.data.borrow_mut().loading = false;
                feed.data.borrow_mut().error = Some(MusicError::Network);
                self.render_feed_status(&feed);
            }
        }
    }

    fn load_visible_feed(self: &Rc<Self>, explicit: bool) {
        let feeds = self.track_feeds.borrow().clone();
        for feed in feeds {
            if feed.rows.is_mapped()
                && feed.status.is_mapped()
                && feed.data.borrow().can_request()
                && (explicit
                    || (!feed.adjustment_guard.get() && !feed.data.borrow().filter.active()))
            {
                self.request_track_feed(&feed, false);
                break;
            }
        }
    }

    pub(super) fn request_track_feed(self: &Rc<Self>, feed: &Feed, force: bool) {
        if !self.feed_is_current(feed) || !feed.data.borrow().can_request() {
            return;
        }
        let page = feed.data.borrow().next_page;
        feed.data.borrow_mut().loading = true;
        self.render_feed_status(feed);
        if let FeedSource::Album(album) = &feed.source {
            let start = page as usize * 50;
            let end = (start + 50).min(album.tracks.len());
            self.append_track_feed(
                feed,
                page,
                Page {
                    items: album.tracks[start.min(end)..end].to_vec(),
                    has_more: end < album.tracks.len(),
                },
            );
            return;
        }
        let source = feed.source.clone();
        let operation = match source {
            FeedSource::Liked => Operation::Library,
            FeedSource::Search(_) => Operation::Search,
            _ => Operation::Detail,
        };
        let completed = feed.clone();
        let omitted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let omitted_result = omitted.clone();
        let origin = Arc::new(std::sync::Mutex::new(None));
        let admitted_origin = origin.clone();
        self.run_catalog(
            operation,
            matches!(source, FeedSource::Liked).then_some((0, page)),
            force,
            move |service, token, catalog, force| {
                let reply = match &source {
                    FeedSource::Liked => match catalog {
                        Some(catalog) => catalog.liked_tracks(&service, &token, page, force),
                        None => service
                            .liked_tracks(&token, page)
                            .map(catalog_view::network),
                    },
                    FeedSource::Playlist(playlist) => match catalog {
                        Some(catalog) => {
                            catalog.playlist_tracks(&service, &token, &playlist.id, page, force)
                        }
                        None => service
                            .playlist_tracks(&token, &playlist.id, page)
                            .map(catalog_view::network),
                    },
                    FeedSource::SearchPlaylist(playlist) => match catalog {
                        Some(catalog) => {
                            catalog.playlist_tracks(&service, &token, &playlist.id, page, force)
                        }
                        None => service
                            .playlist_tracks(&token, &playlist.id, page)
                            .map(catalog_view::network),
                    },
                    FeedSource::Artist(artist) => match catalog {
                        Some(catalog) => {
                            catalog.artist_tracks(&service, &token, &artist.id, page, force)
                        }
                        None => service
                            .artist_tracks(&token, &artist.id, page)
                            .map(catalog_view::network),
                    },
                    FeedSource::Search(query) => service
                        .search(&token, query, SearchKind::Tracks, page)
                        .map(|reply| {
                            omitted.store(
                                reply.omitted_tracks > 0,
                                std::sync::atomic::Ordering::Relaxed,
                            );
                            catalog_view::network(Page {
                                items: reply.tracks,
                                has_more: reply.has_more,
                            })
                        }),
                    FeedSource::Album(_) => {
                        unreachable!("local album slices do not enter a network worker")
                    }
                };
                if let Ok(reply) = &reply {
                    *origin.lock().unwrap_or_else(|error| error.into_inner()) =
                        Some((reply.provenance, reply.cache_error.is_some()));
                }
                reply
            },
            move |this, result| {
                if !this.feed_is_current(&completed) {
                    return;
                }
                match result {
                    Ok(page_result) => {
                        if completed.data.borrow().next_page != page
                            || completed.restart_required.get()
                        {
                            return;
                        }
                        if let Some((origin, error)) = *admitted_origin
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                        {
                            completed.origins.borrow_mut().record(page, origin, error);
                        }
                        completed.data.borrow_mut().incomplete |=
                            omitted_result.load(std::sync::atomic::Ordering::Relaxed);
                        this.append_track_feed(&completed, page, page_result);
                    }
                    Err(error) => {
                        completed.data.borrow_mut().loading = false;
                        completed.data.borrow_mut().error = Some(error);
                        if matches!(
                            error,
                            MusicError::AccessDenied | MusicError::Unauthenticated
                        ) {
                            this.show_error(error);
                            return;
                        }
                        this.render_feed_status(&completed);
                    }
                }
            },
        );
    }
}

pub(super) fn same_loaded_occurrence(row: &PlaybackContext, current: &PlaybackContext) -> bool {
    row.source_id()
        .is_some_and(|id| id.starts_with("continuous:"))
        && row.source_kind() == current.source_kind()
        && row.source_id() == current.source_id()
        && row.source_position(row.current_index()).is_some()
        && row.source_position(row.current_index())
            == current.source_position(current.current_index())
        && row.items().get(row.current_index()).map(|track| &track.id)
            == current
                .items()
                .get(current.current_index())
                .map(|track| &track.id)
}

#[cfg(test)]
mod more_tests;
#[cfg(test)]
mod retirement_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(super) fn exercise_parallel_playlist_more() {
    more_tests::exercise_parallel_more();
    retirement_tests::exercise_refresh_like_independence();
}

#[cfg(test)]
mod origin_tests {
    use super::*;
    use crate::catalog_cache::Provenance;
    #[test]
    fn mixed_admitted_page_origins_keep_stale_and_storage_errors_until_new_epoch() {
        let mut origins = PageOrigins::default();
        origins.record(0, Provenance::Network, false);
        origins.record(1, Provenance::StaleCache, true);
        origins.record(2, Provenance::Network, false);
        assert_eq!(origins.aggregate(), Some((Provenance::StaleCache, true)));
        for page in 3..PAGE_LIMIT {
            origins.record(page, Provenance::FreshCache, false);
        }
        origins.record(PAGE_LIMIT, Provenance::Network, false);
        assert_eq!(origins.0.len(), PAGE_LIMIT as usize);
        assert_eq!(origins.aggregate(), Some((Provenance::StaleCache, true)));
        let mut new_epoch = PageOrigins::default();
        assert_eq!(new_epoch.aggregate(), None);
        new_epoch.record(0, Provenance::FreshCache, false);
        assert_eq!(new_epoch.aggregate(), Some((Provenance::FreshCache, false)));
    }
    #[test]
    fn maximum_valid_search_identity_is_opaque_and_fits_context_bound() {
        let identity = "я".repeat(256);
        assert_eq!(identity.len(), 512);
        let track = Track {
            id: "42".into(),
            title: "Synthetic".into(),
            artists: vec![],
            artist_links: vec![],
            album_id: None,
            album_title: None,
            duration_ms: None,
            available: true,
            cover: None,
            explicit: false,
        };
        let mut source = LoadedTracks::new(ContextKind::Search, Some(identity.clone()), 1);
        for page in 0..11 {
            source.append(
                page,
                Page {
                    items: vec![track.clone(); 50],
                    has_more: true,
                },
            );
        }
        for index in [499, 500] {
            let context = source.context(index).unwrap();
            assert!(context.source_id().unwrap().len() <= 512);
            assert!(!context.source_id().unwrap().contains(&identity));
            assert_eq!(context.current_index(), index % 500);
        }
    }
}
