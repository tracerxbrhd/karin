// SPDX-License-Identifier: GPL-3.0-or-later
//! One search journey with independently usable typed result sections.
use super::*;
use crate::music::{SearchPlaylist, SearchSection, UnifiedSearchResults};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(350);

#[derive(Default)]
pub(super) struct SearchInput {
    origin: RefCell<Option<SearchOrigin>>,
    restoring_detail: Cell<bool>,
    debounce: RefCell<Option<glib::SourceId>>,
    unsent: Cell<bool>,
    cancellation: RefCell<Option<Arc<AtomicBool>>>,
}

#[derive(Clone)]
struct SearchOrigin {
    page: String,
    scroll: f64,
    detail: Option<DetailRefresh>,
    detail_parent: String,
    detail_parent_scroll: f64,
    detail_history: navigation::DetailHistory,
    detail_loading: bool,
}

impl Controller {
    pub(super) fn connect_search_entry(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.widgets.nav_search.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                if this.widgets.search_header.is_open() {
                    this.close_global_search(true);
                } else {
                    this.open_global_search();
                }
            }
        });
        if let Some(window) = self.window.upgrade() {
            let keys = gtk::EventControllerKey::new();
            keys.set_propagation_phase(gtk::PropagationPhase::Capture);
            let weak = Rc::downgrade(self);
            keys.connect_key_pressed(move |_, key, _, _| {
                if key == gtk::gdk::Key::Escape
                    && let Some(this) = weak.upgrade()
                    && this.search_header_active()
                    && let Some(window) = this.window.upgrade()
                    && window.visible_dialog().is_none()
                    && !gtk::prelude::GtkWindowExt::focus(&window)
                        .is_some_and(|focus| focus.ancestor(gtk::Popover::static_type()).is_some())
                {
                    this.close_global_search(true);
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            window.add_controller(keys);

            let click = gtk::GestureClick::new();
            click.set_button(gtk::gdk::BUTTON_PRIMARY);
            click.set_propagation_limit(gtk::PropagationLimit::SameNative);
            click.set_propagation_phase(gtk::PropagationPhase::Capture);
            // Observe both ends without claiming the sequence. An action can
            // change the page before release and a drag can leave an entry.
            let began_on_backdrop = Rc::new(Cell::new(false));
            let began = began_on_backdrop.clone();
            let weak = Rc::downgrade(self);
            click.connect_pressed(move |_, _, x, y| {
                began.set(
                    weak.upgrade()
                        .is_some_and(|this| this.search_backdrop_at(x, y)),
                );
            });
            let began = began_on_backdrop.clone();
            click.connect_stopped(move |_| began.set(false));
            let weak = Rc::downgrade(self);
            click.connect_released(move |_, _, x, y| {
                if began_on_backdrop.replace(false)
                    && let Some(this) = weak.upgrade()
                    && this.search_backdrop_at(x, y)
                {
                    this.close_global_search(true);
                }
            });
            window.add_controller(click);
        }
        let weak = Rc::downgrade(self);
        // A replacement can emit "changed" twice, once for the temporary
        // empty buffer. Observe the completed edit so only an actual clear
        // restores the previous route.
        self.widgets.search.connect_text_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.search_entry_changed();
            }
        });
        let weak = Rc::downgrade(self);
        self.widgets.search.connect_activate(move |_| {
            if let Some(this) = weak.upgrade() {
                this.search(0);
            }
        });
    }

    fn search_backdrop_at(&self, x: f64, y: f64) -> bool {
        if !self.search_header_active() {
            return false;
        }
        let Some(window) = self.window.upgrade() else {
            return false;
        };
        if window.visible_dialog().is_some() {
            return false;
        }
        let Some(target) = window.pick(x, y, gtk::PickFlags::DEFAULT) else {
            return false;
        };
        // A pick can return the image, label or padding inside a control.
        // Walk out to the window so those descendants inherit its action.
        for widget in std::iter::successors(Some(target), |w| w.parent()) {
            if widget.is::<gtk::Window>() {
                break;
            }
            if widget == *self.widgets.search.upcast_ref::<gtk::Widget>()
                || widget == *self.widgets.player_revealer.upcast_ref::<gtk::Widget>()
                || widget.is::<gtk::Actionable>()
                || widget.is::<gtk::Editable>()
                || widget.is::<gtk::MenuButton>()
                || widget.is::<gtk::Popover>()
                || widget.is::<gtk::Scrollbar>()
                || widget.is::<gtk::ListBoxRow>()
                || widget.is::<gtk::FlowBoxChild>()
            {
                return false;
            }
            // Scroll containers themselves take keyboard focus even when the
            // picked pixel belongs to the empty space beside or below a list.
            if widget.is_focusable()
                && !widget.is::<gtk::ScrolledWindow>()
                && !widget.is::<gtk::Viewport>()
                && !widget.is::<gtk::ListBox>()
                && !widget.is::<gtk::FlowBox>()
            {
                return false;
            }
        }
        true
    }

    fn search_origin(&self) -> SearchOrigin {
        let page = self
            .widgets
            .stack
            .visible_child_name()
            .unwrap_or_else(|| "home".into());
        SearchOrigin {
            scroll: self.music_scroll(),
            detail: (page == "detail")
                .then(|| self.detail_refresh.borrow().clone())
                .flatten(),
            detail_parent: self.model.borrow().detail_origin.clone(),
            detail_parent_scroll: self.navigation_history.detail_origin_scroll.get(),
            detail_history: if page == "detail" {
                self.navigation_history.details.borrow().clone()
            } else {
                Vec::new()
            },
            detail_loading: page == "detail" && self.requests.borrow().active(Operation::Detail),
            page: page.to_string(),
        }
    }

    pub(super) fn search_return_section(&self) -> Option<(String, f64)> {
        self.search_input.origin.borrow().as_ref().map(|origin| {
            if origin.page == "detail" {
                (origin.detail_parent.clone(), origin.detail_parent_scroll)
            } else {
                (origin.page.clone(), origin.scroll)
            }
        })
    }

    pub(super) fn open_global_search(self: &Rc<Self>) {
        if !self.widgets.search.is_sensitive() {
            return;
        }
        if !self.widgets.search_header.is_open() {
            self.search_input.origin.replace(Some(self.search_origin()));
            self.widgets.search.set_can_focus(true);
            self.widgets.search_header.set_open(true);
            self.sync_music_back();
        }
        self.widgets.search.grab_focus();
    }

    fn restore_search_origin(self: &Rc<Self>, origin: &SearchOrigin) {
        let current = self.widgets.stack.visible_child_name();
        if current.as_deref() != Some("search")
            && !(current.as_deref() == Some("detail")
                && self.model.borrow().detail_origin == "search")
        {
            return;
        }
        self.widgets.stack.set_visible_child_name(&origin.page);
        if let Some(detail) = &origin.detail {
            // Another result may have replaced the sole detail widget. Reopen
            // the precise original entity with the ordinary cached read.
            let displaced = self
                .detail_refresh
                .borrow()
                .as_ref()
                .is_none_or(|current| !Rc::ptr_eq(current, detail));
            self.model
                .borrow_mut()
                .detail_origin
                .clone_from(&origin.detail_parent);
            self.navigation_history
                .detail_origin_scroll
                .set(origin.detail_parent_scroll);
            self.navigation_history
                .details
                .replace(origin.detail_history.clone());
            if displaced || origin.detail_loading {
                self.search_input.restoring_detail.set(true);
                detail(self, false);
                self.search_input.restoring_detail.set(false);
            }
        }
        self.restore_music_scroll(origin.scroll);
    }

    pub(super) fn entering_collection_detail(self: &Rc<Self>) {
        if !self.widgets.search_header.is_open() || self.search_input.restoring_detail.get() {
            return;
        }
        let route = self.widgets.stack.visible_child_name();
        let from_results = route.as_deref() == Some("search")
            || (route.as_deref() == Some("detail")
                && self.model.borrow().detail_origin == "search");
        if !from_results {
            self.close_global_search(false);
        }
    }

    pub(super) fn retire_global_search(&self) {
        self.search_input.origin.take();
        // Move focus before disabling the GtkText subtree. GTK otherwise
        // queues a focus fallback that can run *after* Ctrl+F reopens it.
        if self.window.upgrade().is_some_and(|window| {
            gtk::prelude::GtkWindowExt::focus(&window).is_some_and(|focus| {
                focus == *self.widgets.search.upcast_ref::<gtk::Widget>()
                    || focus.is_ancestor(&self.widgets.search)
            })
        }) {
            self.widgets.nav_search.grab_focus();
        }
        self.widgets.search.set_can_focus(false);
        self.widgets.search_header.set_open(false);
        self.sync_music_back();
    }

    pub(super) fn close_global_search(self: &Rc<Self>, restore: bool) {
        if !self.widgets.search_header.is_open() {
            return;
        }
        let origin = self.search_input.origin.take();
        self.cancel_search_input();
        self.requests.borrow_mut().invalidate(Operation::Search);
        self.retire_global_search();
        self.widgets.search.set_text("");
        if restore {
            if let Some(origin) = origin {
                self.restore_search_origin(&origin);
            }
            self.widgets.nav_search.grab_focus();
        }
    }

    fn cancel_search_debounce(&self) {
        if let Some(source) = self.search_input.debounce.take() {
            source.remove();
        }
    }

    pub(super) fn cancel_search_input(&self) {
        self.cancel_search_debounce();
        self.search_input.unsent.set(false);
        if let Some(cancellation) = self.search_input.cancellation.take() {
            cancellation.store(true, Ordering::Release);
        }
    }

    pub(super) fn search_cancellation(&self) -> Option<Arc<AtomicBool>> {
        self.search_input.cancellation.borrow().clone()
    }

    fn search_header_active(&self) -> bool {
        self.music_surface_visible.get() && self.widgets.search_header.is_open()
    }

    pub(super) fn sync_search_debounce_route(self: &Rc<Self>) {
        if !self.search_header_active() {
            self.cancel_search_debounce();
            return;
        }
        if !self.search_input.unsent.get() || self.search_input.debounce.borrow().is_some() {
            return;
        }
        let generation = self.model.borrow().generation;
        let weak = Rc::downgrade(self);
        self.search_input
            .debounce
            .replace(Some(glib::timeout_add_local_once(
                SEARCH_DEBOUNCE,
                move || {
                    let Some(this) = weak.upgrade() else { return };
                    this.search_input.debounce.take();
                    let ready = {
                        let model = this.model.borrow();
                        model.account_valid && !model.closed && model.generation == generation
                    };
                    if ready && this.search_input.unsent.get() && this.search_header_active() {
                        this.search(0);
                    }
                },
            )));
    }

    fn search_entry_changed(self: &Rc<Self>) {
        let already_cleared = self.search_input.unsent.get();
        self.cancel_search_input();
        self.requests.borrow_mut().invalidate(Operation::Search);
        self.sync_busy();
        self.track_feeds
            .borrow_mut()
            .retain(|feed| feed.rows != self.widgets.search_rows);
        let ready = {
            let mut model = self.model.borrow_mut();
            model.view_generation += 1;
            model.search_page = 0;
            model.search_scope = None;
            model.account_valid && !model.closed
        };
        let has_query = !self.widgets.search.text().trim().is_empty();
        self.render_catalog_refresh();
        if !already_cleared || !has_query {
            self.reset_search_sections();
            self.clear_search_feedback();
            clear_list(&self.widgets.search_rows);
            self.widgets.search_next.set_visible(false);
            self.widgets.search_previous.set_visible(false);
        }
        if ready && has_query {
            self.search_input.unsent.set(true);
            self.sync_search_debounce_route();
        } else if ready {
            self.show_empty_search();
            let origin = self.search_input.origin.borrow().clone();
            if let Some(origin) = origin {
                self.restore_search_origin(&origin);
            }
        }
    }

    fn show_empty_search(&self) {
        set_empty(
            &self.widgets.search_rows,
            "Что хотите послушать? Введите название или исполнителя.",
        );
    }

    pub(super) fn reset_search_sections(&self) {
        // Reparent the reusable track/section list before removing old sections.
        if let Some(parent) = self.widgets.search_rows.parent().and_downcast::<gtk::Box>() {
            parent.remove(&self.widgets.search_rows);
        }
        clear_box(&self.widgets.search_sections);
        self.widgets
            .search_sections
            .append(&self.widgets.search_rows);
        self.widgets.search_all.set_visible(false);
        self.widgets.search_scope_title.set_visible(false);
    }

    pub(super) fn show_search_section(self: &Rc<Self>, kind: SearchKind) {
        self.model.borrow_mut().search_scope = Some(kind);
        self.search(0);
    }

    pub(super) fn search(self: &Rc<Self>, page: u32) {
        if !self.search_header_active()
            || !self.model.borrow().account_valid
            || self.model.borrow().closed
        {
            return;
        }
        self.cancel_search_input();
        let query = self.widgets.search.text().trim().to_owned();
        let scope = self.model.borrow().search_scope;
        self.track_feeds
            .borrow_mut()
            .retain(|feed| feed.rows != self.widgets.search_rows);
        self.reset_search_sections();
        self.clear_search_feedback();
        clear_list(&self.widgets.search_rows);
        self.widgets.search_next.set_visible(false);
        self.widgets.search_previous.set_visible(false);
        if query.is_empty() {
            self.show_empty_search();
            return;
        }
        // Capture a detail load at departure: it may have completed while
        // the user was typing, or still need resuming after navigation.
        if self.widgets.stack.visible_child_name().as_deref() == Some("detail")
            && self.model.borrow().detail_origin != "search"
        {
            self.search_input.origin.replace(Some(self.search_origin()));
        }
        self.widgets.stack.set_visible_child_name("search");
        let cancellation = Arc::new(AtomicBool::new(false));
        self.search_input
            .cancellation
            .replace(Some(cancellation.clone()));
        let loading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let spinner = gtk::Spinner::new();
        spinner.set_spinning(true);
        loading.append(&spinner);
        loading.append(&text("Ищем музыку…", "dim-label"));
        self.widgets.search_feedback.append(&loading);
        self.widgets.search_feedback.set_visible(true);
        if let Some(kind) = scope {
            self.widgets.search_all.set_visible(true);
            self.widgets
                .search_scope_title
                .set_text(section_title(kind));
            self.widgets.search_scope_title.set_visible(true);
            if kind == SearchKind::Tracks {
                let feed = self.new_track_feed(
                    continuous_loading::FeedSource::Search(query),
                    ContextKind::Search,
                    None,
                    &self.widgets.search_rows,
                    &self.widgets.search_feedback,
                );
                self.widgets.search_feedback.set_visible(true);
                self.request_track_feed(&feed, false);
                return;
            }
            self.run(
                Operation::Search,
                move |service, token| {
                    service.search_cancellable(&token, &query, kind, page, Some(&cancellation))
                },
                move |this, result| match result {
                    Ok(result) => {
                        this.clear_search_feedback();
                        this.model.borrow_mut().search_page = page;
                        match kind {
                            SearchKind::Tracks => this.tracks_from(
                                &this.widgets.search_rows,
                                result.tracks,
                                ContextKind::Search,
                                None,
                                page > 0 || result.has_more || result.omitted_tracks > 0,
                                0..usize::MAX,
                            ),
                            SearchKind::Albums => {
                                this.search_album_tiles(&this.widgets.search_rows, result.albums);
                            }
                            SearchKind::Artists => {
                                this.search_artist_tiles(&this.widgets.search_rows, result.artists);
                            }
                            SearchKind::Playlists => {
                                this.search_playlist_tiles(
                                    &this.widgets.search_rows,
                                    result.playlists,
                                );
                            }
                        }
                        if this.widgets.search_rows.first_child().is_none() {
                            set_empty(
                                &this.widgets.search_rows,
                                "Ничего не найдено. Попробуйте другой запрос.",
                            );
                        }
                        if result.omitted_tracks > 0 {
                            this.widgets.search_feedback.append(&text(
                                "Часть найденных треков сейчас недоступна.",
                                "dim-label",
                            ));
                            this.widgets.search_feedback.set_visible(true);
                        }
                        this.widgets.search_next.set_visible(result.has_more);
                        this.widgets.search_previous.set_visible(page > 0);
                    }
                    Err(error) => this.search_error(error, page),
                },
            );
        } else {
            self.run(
                Operation::Search,
                move |service, token| {
                    service.search_all_cancellable(&token, &query, Some(&cancellation))
                },
                |this, result| match result {
                    Ok(result) => this.render_unified_search(result),
                    Err(error) => this.search_error(error, 0),
                },
            );
        }
    }

    fn render_unified_search(self: &Rc<Self>, result: UnifiedSearchResults) {
        self.clear_search_feedback();
        self.model.borrow_mut().search_page = 0;
        self.reset_search_sections();
        if let Some(parent) = self.widgets.search_rows.parent().and_downcast::<gtk::Box>() {
            parent.remove(&self.widgets.search_rows);
        }
        let tracks = self.search_section(
            SearchKind::Tracks,
            result.tracks,
            Some(self.widgets.search_rows.clone()),
        );
        if let Some((rows, section)) = tracks {
            self.tracks_from(
                &rows,
                section.items,
                ContextKind::Search,
                None,
                section.has_more || section.omitted > 0,
                0..5,
            );
        }
        if let Some((rows, section)) = self.search_section(SearchKind::Albums, result.albums, None)
        {
            self.search_album_tiles(&rows, section.items.into_iter().take(5).collect());
        }
        if let Some((rows, section)) =
            self.search_section(SearchKind::Artists, result.artists, None)
        {
            self.search_artist_tiles(&rows, section.items.into_iter().take(5).collect());
        }
        if let Some((rows, section)) =
            self.search_section(SearchKind::Playlists, result.playlists, None)
        {
            self.search_playlist_tiles(&rows, section.items.into_iter().take(5).collect());
        }
        if self.widgets.search_sections.first_child().is_none() {
            self.widgets.search_sections.append(&text(
                "Ничего не найдено. Попробуйте другой запрос.",
                "dim-label",
            ));
        }
        write_catalog_line(catalog_line(
            Operation::Search,
            CatalogPhase::Result,
            "success",
            Some(result.http_status),
            None,
        ));
    }

    fn search_section<T>(
        self: &Rc<Self>,
        kind: SearchKind,
        result: Result<SearchSection<T>, MusicError>,
        reuse: Option<gtk::ListBox>,
    ) -> Option<(gtk::ListBox, SearchSection<T>)> {
        if result
            .as_ref()
            .is_ok_and(|s| s.items.is_empty() && s.omitted == 0 && !s.has_more)
        {
            return None;
        }
        let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
        section.add_css_class("music-search-section");
        section.set_widget_name(match kind {
            SearchKind::Tracks => "music-results-tracks",
            SearchKind::Albums => "music-results-albums",
            SearchKind::Artists => "music-results-artists",
            SearchKind::Playlists => "music-results-playlists",
        });
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = text(section_title(kind), "title-3");
        title.set_hexpand(true);
        header.append(&title);
        section.append(&header);
        self.widgets.search_sections.append(&section);
        match result {
            Ok(data) => {
                if data.items.len() > 5 || data.has_more {
                    let all = gtk::Button::with_label("Показать всё");
                    all.add_css_class("flat");
                    all.set_widget_name("music-search-show-all");
                    all.update_property(&[gtk::accessible::Property::Label(&format!(
                        "Все результаты: {}",
                        section_title(kind)
                    ))]);
                    let weak = Rc::downgrade(self);
                    all.connect_clicked(move |_| {
                        if let Some(this) = weak.upgrade() {
                            this.show_search_section(kind);
                        }
                    });
                    header.append(&all);
                }
                if data.omitted > 0 {
                    section.append(&text("Часть результатов недоступна.", "dim-label"));
                }
                let rows = reuse.unwrap_or_else(list);
                clear_list(&rows);
                section.append(&rows);
                if data.items.is_empty() {
                    set_empty(&rows, "Сейчас нет доступных результатов в этом разделе.");
                }
                Some((rows, data))
            }
            Err(error) => {
                catalog_diagnostic(Operation::Search, Some(error));
                let failure = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let message = text("Не удалось загрузить этот раздел.", "dim-label");
                message.set_hexpand(true);
                failure.append(&message);
                let retry = gtk::Button::with_label("Повторить");
                let weak = Rc::downgrade(self);
                retry.connect_clicked(move |_| {
                    if let Some(this) = weak.upgrade() {
                        this.show_search_section(kind);
                    }
                });
                failure.append(&retry);
                section.append(&failure);
                None
            }
        }
    }

    fn search_album_tiles(self: &Rc<Self>, rows: &gtk::ListBox, albums: Vec<Album>) {
        if albums.is_empty() {
            return;
        }
        let grid = search_grid(rows);
        for album in albums {
            let tile = catalog_layout::catalog_tile(
                self.artwork.as_ref(),
                album.cover.as_ref(),
                &album.title,
                &album.artists.join(", "),
                false,
            );
            tile.set_widget_name("music-search-album-tile");
            tile.update_property(&[gtk::accessible::Property::Label(&format!(
                "Открыть альбом: {}",
                album.title
            ))]);
            let link = crate::music::public_links::album(&album.id);
            self.search_tile_action(&tile, RowAction::Album(album.id));
            catalog_layout::append_tile(
                &grid,
                &sharing::tile_with_link_menu(&tile, "Действия с альбомом", link),
            );
        }
    }

    fn search_artist_tiles(self: &Rc<Self>, rows: &gtk::ListBox, artists: Vec<Artist>) {
        if artists.is_empty() {
            return;
        }
        let grid = search_grid(rows);
        for artist in artists {
            let tile = catalog_layout::catalog_tile(
                self.artwork.as_ref(),
                artist.cover.as_ref(),
                &artist.name,
                "Исполнитель",
                true,
            );
            tile.set_widget_name("music-search-artist-tile");
            tile.update_property(&[gtk::accessible::Property::Label(&format!(
                "Открыть исполнителя: {}",
                artist.name
            ))]);
            let link = crate::music::public_links::artist(&artist.id);
            self.search_tile_action(&tile, RowAction::Artist(artist));
            catalog_layout::append_tile(
                &grid,
                &sharing::tile_with_link_menu(&tile, "Действия с исполнителем", link),
            );
        }
    }

    fn search_playlist_tiles(self: &Rc<Self>, rows: &gtk::ListBox, playlists: Vec<SearchPlaylist>) {
        if playlists.is_empty() {
            return;
        }
        let grid = search_grid(rows);
        for playlist in playlists {
            let subtitle = playlist
                .track_count
                .map(track_count_text)
                .unwrap_or_else(|| "Плейлист".into());
            let tile = catalog_layout::catalog_tile(
                self.artwork.as_ref(),
                playlist.cover.as_ref(),
                &playlist.title,
                &subtitle,
                false,
            );
            tile.set_widget_name("music-search-playlist-tile");
            tile.update_property(&[gtk::accessible::Property::Label(&format!(
                "Открыть плейлист: {}",
                playlist.title
            ))]);
            self.search_tile_action(&tile, RowAction::SearchPlaylist(playlist));
            catalog_layout::append_tile(&grid, &tile);
        }
    }

    fn search_tile_action(self: &Rc<Self>, tile: &gtk::Button, action: RowAction) {
        let generation = self.model.borrow().generation;
        let weak = Rc::downgrade(self);
        tile.connect_clicked(move |_| {
            let Some(this) = weak.upgrade() else { return };
            let model = this.model.borrow();
            if model.closed || !model.account_valid || model.generation != generation {
                return;
            }
            drop(model);
            match &action {
                RowAction::Album(id) => this.open_album(id.clone()),
                RowAction::Artist(artist) => this.open_artist(artist.clone(), 0),
                RowAction::SearchPlaylist(playlist) => {
                    this.open_search_playlist(playlist.clone(), 0)
                }
                _ => {}
            }
        });
    }

    pub(super) fn open_search_playlist(self: &Rc<Self>, playlist: SearchPlaylist, _page: u32) {
        self.remember_detail();
        self.open_search_playlist_policy(playlist, false);
    }

    fn open_search_playlist_policy(self: &Rc<Self>, playlist: SearchPlaylist, force: bool) {
        self.detail_loading(&playlist.title, "Загрузка плейлиста…");
        let refresh = playlist.clone();
        self.detail_refresh
            .replace(Some(Rc::new(move |this, force| {
                this.open_search_playlist_policy(refresh.clone(), force)
            })));
        self.detail_header(
            &playlist.title,
            &playlist
                .track_count
                .map(track_count_text)
                .map_or_else(|| "Плейлист".into(), |count| format!("Плейлист · {count}")),
        );
        self.detail_cover(playlist.cover.as_ref());
        let rows = list();
        let status = gtk::Box::new(gtk::Orientation::Vertical, 6);
        self.widgets.detail.append(&rows);
        self.widgets.detail.append(&status);
        let id = Some(format!("{}:{}", playlist.id.owner, playlist.id.kind));
        let feed = self.new_track_feed(
            continuous_loading::FeedSource::SearchPlaylist(playlist),
            ContextKind::Playlist,
            id,
            &rows,
            &status,
        );
        self.detail_primary(Some(self.feed_play_button(&feed)));
        self.request_track_feed(&feed, force);
    }
}

fn search_grid(rows: &gtk::ListBox) -> gtk::FlowBox {
    rows.remove_css_class("boxed-list");
    rows.add_css_class("music-catalog-results");
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_selectable(false);
    row.set_focusable(false);
    row.add_css_class("music-catalog-grid-row");
    let grid = catalog_layout::tile_grid();
    row.set_child(Some(&grid));
    rows.append(&row);
    grid
}

fn section_title(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Tracks => "Треки",
        SearchKind::Albums => "Альбомы",
        SearchKind::Artists => "Исполнители",
        SearchKind::Playlists => "Плейлисты",
    }
}
