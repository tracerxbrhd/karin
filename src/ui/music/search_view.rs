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
                                for album in result.albums {
                                    this.album_row(&this.widgets.search_rows, album);
                                }
                            }
                            SearchKind::Artists => {
                                for artist in result.artists {
                                    this.artist_row(&this.widgets.search_rows, artist);
                                }
                            }
                            SearchKind::Playlists => {
                                for playlist in result.playlists {
                                    this.search_playlist_row(&this.widgets.search_rows, playlist);
                                }
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
            for album in section.items.into_iter().take(5) {
                self.album_row(&rows, album);
            }
        }
        if let Some((rows, section)) =
            self.search_section(SearchKind::Artists, result.artists, None)
        {
            for artist in section.items.into_iter().take(5) {
                self.artist_row(&rows, artist);
            }
        }
        if let Some((rows, section)) =
            self.search_section(SearchKind::Playlists, result.playlists, None)
        {
            for playlist in section.items.into_iter().take(5) {
                self.search_playlist_row(&rows, playlist);
            }
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
            .is_ok_and(|s| s.items.is_empty() && s.omitted == 0)
        {
            return None;
        }
        let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
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

    fn search_playlist_row(self: &Rc<Self>, rows: &gtk::ListBox, playlist: SearchPlaylist) {
        let subtitle = playlist
            .track_count
            .map(track_count_text)
            .unwrap_or_else(|| "Плейлист".into());
        let (row, body) = item(&playlist.title, &subtitle);
        body.prepend(&cover_widget(
            self.artwork.as_ref(),
            playlist.cover.as_ref(),
            48,
        ));
        body.append(&gtk::Image::from_icon_name("go-next-symbolic"));
        self.row_action(&row, RowAction::SearchPlaylist(playlist));
        rows.append(&row);
    }

    pub(super) fn open_search_playlist(self: &Rc<Self>, playlist: SearchPlaylist, page: u32) {
        let id = playlist.id.clone();
        self.detail_loading(&playlist.title, "Загрузка плейлиста…");
        self.run(
            Operation::Detail,
            move |service, token| service.playlist_tracks(&token, &id, page),
            move |this, result| match result {
                Ok(data) => {
                    this.detail_header(
                        &playlist.title,
                        &format!("Плейлист · страница {}", page + 1),
                    );
                    this.detail_cover(playlist.cover.as_ref());
                    let rows = list();
                    this.tracks_from(
                        &rows,
                        data.items,
                        ContextKind::Playlist,
                        Some(format!("{}:{}", playlist.id.owner, playlist.id.kind)),
                        page > 0 || data.has_more,
                        0..usize::MAX,
                    );
                    this.widgets.detail.append(&rows);
                    let pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    for (label, target, visible) in [
                        ("Предыдущая страница", page.saturating_sub(1), page > 0),
                        ("Следующая страница", page.saturating_add(1), data.has_more),
                    ] {
                        if visible {
                            let button = gtk::Button::with_label(label);
                            let weak = Rc::downgrade(this);
                            let selected = playlist.clone();
                            button.connect_clicked(move |_| {
                                if let Some(this) = weak.upgrade() {
                                    this.open_search_playlist(selected.clone(), target);
                                }
                            });
                            pages.append(&button);
                        }
                    }
                    this.widgets.detail.append(&pages);
                }
                Err(error) => this.detail_error(error, move |this| {
                    this.open_search_playlist(playlist.clone(), page)
                }),
            },
        );
    }
}

fn section_title(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Tracks => "Треки",
        SearchKind::Albums => "Альбомы",
        SearchKind::Artists => "Исполнители",
        SearchKind::Playlists => "Плейлисты",
    }
}
