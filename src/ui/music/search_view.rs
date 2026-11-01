// SPDX-License-Identifier: GPL-3.0-or-later
//! One search journey with independently usable typed result sections.
use super::*;
use crate::music::{SearchPlaylist, SearchSection, UnifiedSearchResults};

impl Controller {
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
        if !self.model.borrow().account_valid || self.model.borrow().closed {
            return;
        }
        let query = self.widgets.search.text().trim().to_owned();
        let scope = self.model.borrow().search_scope;
        self.track_feeds
            .borrow_mut()
            .retain(|feed| feed.rows != self.widgets.search_rows);
        self.reset_search_sections();
        self.clear_search_feedback();
        clear_list(&self.widgets.search_rows);
        if query.is_empty() {
            set_empty(
                &self.widgets.search_rows,
                "Что хотите послушать? Введите название или исполнителя.",
            );
            return;
        }
        self.widgets.stack.set_visible_child_name("search");
        self.widgets.search_next.set_visible(false);
        self.widgets.search_previous.set_visible(false);
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
                move |service, token| service.search(&token, &query, kind, page),
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
                move |service, token| service.search_all(&token, &query),
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
            self.search_tile_action(&tile, RowAction::Album(album.id));
            catalog_layout::append_tile(&grid, &tile);
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
            self.search_tile_action(&tile, RowAction::Artist(artist));
            catalog_layout::append_tile(&grid, &tile);
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
        self.open_search_playlist_policy(playlist, false);
    }

    fn open_search_playlist_policy(self: &Rc<Self>, playlist: SearchPlaylist, force: bool) {
        self.detail_loading(&playlist.title, "Загрузка плейлиста…");
        let refresh = playlist.clone();
        self.detail_refresh.replace(Some(Rc::new(move |this| {
            this.open_search_playlist_policy(refresh.clone(), true)
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
