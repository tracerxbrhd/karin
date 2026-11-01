// SPDX-License-Identifier: GPL-3.0-or-later
//! Native catalog presentation around the existing request and service contracts.

use super::*;

pub(super) struct CatalogRow {
    row: glib::WeakRef<gtk::ListBoxRow>,
    context: PlaybackContext,
}

#[cfg(test)]
mod tests;

fn same_context_occurrence(row: &PlaybackContext, current: &PlaybackContext) -> bool {
    row.source_kind() == current.source_kind()
        && row.source_id() == current.source_id()
        && row.current_index() == current.current_index()
        && row.loaded_items_len() == current.loaded_items_len()
        && row
            .items()
            .iter()
            .take(row.loaded_items_len())
            .map(|track| &track.id)
            .eq(current
                .items()
                .iter()
                .take(current.loaded_items_len())
                .map(|track| &track.id))
}

/// A compact, keyboard-activatable song row. Metadata stays available through
/// native tooltips/accessibility while ellipsis keeps long titles from changing
/// the transport/action columns or the number of visible songs.
pub(super) fn catalog_item(title: &str, subtitle: &str) -> (gtk::ListBoxRow, gtk::Box) {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name("music-track-row");
    row.add_css_class("music-track-row");
    row.set_height_request(64);
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    body.set_margin_start(8);
    body.set_margin_end(8);
    body.set_margin_top(8);
    body.set_margin_bottom(8);
    let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
    labels.set_hexpand(true);
    labels.set_valign(gtk::Align::Center);
    let title = compact_label(title, "music-track-title");
    title.add_css_class("heading");
    labels.append(&title);
    if !subtitle.is_empty() {
        let artist = compact_label(subtitle, "music-track-artist");
        artist.add_css_class("dim-label");
        labels.append(&artist);
    }
    body.append(&labels);
    row.set_child(Some(&body));
    (row, body)
}

fn compact_label(value: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(value));
    label.set_xalign(0.0);
    label.set_wrap(false);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_width_chars(1);
    label.set_tooltip_text(Some(value));
    label.add_css_class("music-ellipsized-metadata");
    label.update_property(&[gtk::accessible::Property::Label(value)]);
    label.add_css_class(class);
    label
}

fn song_list(list: &gtk::ListBox) {
    list.remove_css_class("boxed-list");
    list.add_css_class("music-track-list");
}

pub(super) fn catalog_tile(
    loader: Option<&ArtworkLoader>,
    cover: Option<&CoverArt>,
    title: &str,
    subtitle: &str,
    artist: bool,
) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_width_request(176);
    button.set_hexpand(false);
    button.set_halign(gtk::Align::Start);
    button.set_valign(gtk::Align::Start);
    button.add_css_class("flat");
    button.add_css_class("music-catalog-tile");
    button.set_tooltip_text(Some(title));
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let art = cover_widget(loader, cover, 160);
    art.set_halign(gtk::Align::Center);
    if artist {
        art.add_css_class("music-artist-art");
    }
    content.append(&art);
    let heading = compact_label(title, "music-catalog-title");
    heading.set_wrap(true);
    heading.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    heading.set_lines(2);
    heading.set_max_width_chars(22);
    heading.add_css_class("heading");
    content.append(&heading);
    if !subtitle.is_empty() {
        let subtitle = compact_label(subtitle, "music-catalog-subtitle");
        subtitle.add_css_class("dim-label");
        content.append(&subtitle);
    }
    button.set_child(Some(&content));
    button
}

pub(super) fn tile_grid() -> gtk::FlowBox {
    let grid = gtk::FlowBox::new();
    grid.set_selection_mode(gtk::SelectionMode::None);
    grid.set_focusable(false);
    grid.set_min_children_per_line(1);
    grid.set_max_children_per_line(12);
    grid.set_column_spacing(16);
    grid.set_row_spacing(20);
    grid.set_homogeneous(true);
    grid.set_halign(gtk::Align::Start);
    grid.set_valign(gtk::Align::Start);
    grid.add_css_class("music-catalog-grid");
    grid
}

pub(super) fn append_tile(grid: &gtk::FlowBox, tile: &gtk::Button) {
    grid.insert(tile, -1);
    if let Some(wrapper) = tile.parent() {
        wrapper.set_focusable(false);
    }
}

impl Controller {
    /// Styling observes the existing queue occurrence, never controls playback.
    /// Weak rows are pruned on each refresh and cannot retain removed pages or
    /// private catalog widgets after logout.
    pub(super) fn sync_catalog_rows(&self) {
        let model = self.model.borrow();
        let current = model.account_valid.then(|| model.queue.context()).flatten();
        self.catalog_rows.borrow_mut().retain(|binding| {
            let Some(row) = binding.row.upgrade() else {
                return false;
            };
            let selected =
                current.is_some_and(|current| same_context_occurrence(&binding.context, current));
            if row.has_css_class("music-current") == selected {
                return true;
            }
            if selected {
                row.add_css_class("music-current");
            } else {
                row.remove_css_class("music-current");
            }
            row.update_property(&[gtk::accessible::Property::Description(if selected {
                "Текущий трек"
            } else {
                ""
            })]);
            true
        });
    }

    fn library_heading(
        &self,
        title: &str,
        cover: Option<&CoverArt>,
        facts: &str,
        play: Option<gtk::Button>,
    ) {
        clear_box(&self.widgets.library_hero);
        // The selector already identifies this collection. Keep its summary
        // compact; full cover heroes belong to album/playlist detail pages.
        self.widgets
            .library_hero
            .remove_css_class("music-collection-hero");
        self.widgets
            .library_hero
            .add_css_class("music-library-summary");
        self.widgets.library_hero.set_spacing(12);
        if cover.is_some() {
            self.widgets
                .library_hero
                .append(&cover_widget(self.artwork.as_ref(), cover, 36));
        }
        let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
        labels.set_hexpand(true);
        labels.set_valign(gtk::Align::Center);
        let already_named = self
            .widgets
            .library_mode
            .selected_item()
            .and_downcast::<gtk::StringObject>()
            .is_some_and(|selected| selected.string().as_str() == title);
        if !already_named {
            labels.append(&text(title, "heading"));
        }
        if !facts.is_empty() {
            let facts = text(facts, "dim-label");
            facts.add_css_class("caption");
            labels.append(&facts);
        }
        self.widgets.library_hero.append(&labels);
        if let Some(play) = play {
            play.remove_css_class("pill");
            play.set_valign(gtk::Align::Center);
            self.widgets.library_hero.append(&play);
        }
    }

    fn render_home_collections(self: &Rc<Self>) {
        while let Some(child) = self.widgets.home_collections.first_child() {
            self.widgets.home_collections.remove(&child);
        }
        let (valid, generation, playlists) = {
            let model = self.model.borrow();
            (
                model.account_valid && !model.closed,
                model.generation,
                model
                    .playlists
                    .as_ref()
                    .and_then(|page| page.as_ref().ok())
                    .map(|page| page.items.clone()),
            )
        };
        let has_collections = valid && playlists.as_ref().is_some_and(|p| !p.is_empty());
        self.widgets.home_collections.set_visible(has_collections);
        if let Some(section) = self.widgets.home_collections.parent() {
            section.set_visible(has_collections);
        }
        if !valid {
            return;
        }
        for playlist in playlists.unwrap_or_default().into_iter().take(6) {
            let count = track_count_text(playlist.track_count);
            let tile = catalog_tile(
                self.artwork.as_ref(),
                playlist.cover.as_ref(),
                &playlist.title,
                &count,
                false,
            );
            tile.set_widget_name("music-home-playlist-tile");
            tile.update_property(&[gtk::accessible::Property::Label(&format!(
                "Открыть плейлист: {}",
                playlist.title
            ))]);
            let weak = Rc::downgrade(self);
            tile.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    let model = this.model.borrow();
                    if model.closed || !model.account_valid || model.generation != generation {
                        return;
                    }
                    drop(model);
                    this.open_playlist(playlist.clone(), 0);
                }
            });
            append_tile(&self.widgets.home_collections, &tile);
        }
    }

    pub(super) fn collection_play(
        self: &Rc<Self>,
        tracks: Vec<Track>,
        kind: ContextKind,
        source: Option<String>,
        partial: bool,
    ) -> Option<gtk::Button> {
        let index = tracks.iter().position(|track| track.available)?;
        let context = PlaybackContext::new(kind, source, tracks, index, partial).ok()?;
        let track = &context.items()[index];
        let button = self.context_play_button(track, false, context.clone());
        button.set_label("Слушать");
        button.add_css_class("pill");
        Some(button)
    }

    pub(super) fn detail_primary(self: &Rc<Self>, play: Option<gtk::Button>) {
        let Some(play) = play else { return };
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.add_css_class("music-collection-actions");
        actions.append(&play);
        self.place_detail_actions(&actions);
    }

    /// Artwork, facts and the actual collection action form one compact hero.
    /// Reuse the same buttons rather than reserving an extra full-width row
    /// below the cover. Wrapped metadata can still grow naturally on compact.
    fn place_detail_actions(&self, actions: &gtk::Box) {
        if let Some(hero) = self
            .widgets
            .detail
            .first_child()
            .and_then(|back| back.next_sibling())
            .and_downcast::<gtk::Box>()
        {
            let mut child = hero.first_child();
            while let Some(widget) = child {
                if widget.widget_name() == "music-detail-heading"
                    && let Some(heading) = widget.downcast_ref::<gtk::Box>()
                {
                    heading.append(actions);
                    return;
                }
                child = widget.next_sibling();
            }
        }
        self.widgets.detail.append(actions);
    }

    pub(super) fn show_library(self: &Rc<Self>) {
        if !self.model.borrow().account_valid {
            return;
        }
        self.requests.borrow_mut().invalidate(Operation::Library);
        if self.widgets.library_mode.selected() == 2 {
            self.load_library(0);
            return;
        }
        let playlists = self.widgets.library_mode.selected() == 1;
        if playlists {
            let cached = self.model.borrow().playlists.clone();
            if let Some(result) = cached {
                self.render_playlists(result, 0);
                return;
            }
        } else {
            let cached = self.model.borrow().liked.clone();
            if let Some(result) = cached {
                self.render_liked(result, 0);
                return;
            }
        }
        self.load_library(0);
    }

    pub(super) fn render_playlists(
        self: &Rc<Self>,
        result: Result<Page<Playlist>, MusicError>,
        page: u32,
    ) {
        self.widgets.library_rows.set_visible(false);
        self.playlist_grid.widget.set_visible(true);
        match result {
            Ok(result) => {
                let title = if self.widgets.library_mode.selected() == 2 {
                    "Понравившиеся плейлисты"
                } else {
                    "Мои плейлисты"
                };
                let facts = if page > 0 {
                    format!("Плейлистов: {} · Страница {}", result.items.len(), page + 1)
                } else {
                    format!("Плейлистов: {}", result.items.len())
                };
                self.library_heading(title, None, &facts, None);
                self.render_home_collections();
                self.model.borrow_mut().library_page = page;
                self.model.borrow_mut().library_displaying =
                    Some((self.widgets.library_mode.selected(), page));
                self.playlist_grid.set_playlists(
                    &result.items,
                    self.widgets.library_mode.selected() == 1
                        && self.can_create_playlist.get()
                        && self.model.borrow().account_valid,
                );
                self.widgets.library_next.set_visible(result.has_more);
                self.widgets.library_previous.set_visible(page > 0);
                self.refresh_catalog_notice();
                if self.widgets.stack.visible_child_name().as_deref() == Some("library") {
                    self.widgets.status.set_title(&format!(
                        "{} · страница {}",
                        if self.widgets.library_mode.selected() == 2 {
                            "Понравившиеся плейлисты"
                        } else {
                            "Мои плейлисты"
                        },
                        page + 1
                    ));
                    self.widgets.status.set_subtitle("");
                }
            }
            Err(error) => {
                self.library_heading("Плейлисты", None, "", None);
                self.model.borrow_mut().library_displaying = None;
                self.playlist_grid.set_error();
                self.show_error(error);
            }
        }
    }

    pub(super) fn render_home_preview(self: &Rc<Self>) {
        self.render_home_collections();
        let (valid, result) = {
            let model = self.model.borrow();
            (model.account_valid, model.liked.clone())
        };
        clear_box(&self.widgets.home_hero);
        if !valid {
            set_empty(
                &self.widgets.home_preview,
                "Любимые треки появятся после входа.",
            );
            return;
        }
        match result {
            Some(Ok(page)) if !page.items.is_empty() => {
                let art = cover_widget(
                    self.artwork.as_ref(),
                    page.items.first().and_then(|track| track.cover.as_ref()),
                    96,
                );
                art.add_css_class("music-home-hero-art");
                self.widgets.home_hero.append(&art);
                let labels = gtk::Box::new(gtk::Orientation::Vertical, 6);
                labels.set_hexpand(true);
                labels.set_valign(gtk::Align::Center);
                labels.append(&text("Любимые треки", "title-1"));
                let mut facts = track_count_text(page.items.len() as u64);
                if page.has_more {
                    facts.push_str(" · Часть коллекции");
                }
                labels.append(&text(&facts, "dim-label"));
                if let Some(play) = self.collection_play(
                    page.items.clone(),
                    ContextKind::LikedTracks,
                    None,
                    page.has_more,
                ) {
                    play.set_widget_name("music-home-play-liked");
                    play.set_halign(gtk::Align::Start);
                    labels.append(&play);
                }
                self.widgets.home_hero.append(&labels);
                self.tracks_from(
                    &self.widgets.home_preview,
                    page.items,
                    ContextKind::LikedTracks,
                    None,
                    page.has_more,
                    0..3,
                );
            }
            Some(Ok(_)) => set_empty(&self.widgets.home_preview, "Пока нет любимых треков."),
            Some(Err(error)) => {
                let (title, _) = music_error_message(error);
                set_empty(&self.widgets.home_preview, title);
            }
            None => set_empty(&self.widgets.home_preview, "Загрузка коллекции…"),
        }
        self.refresh_catalog_notice();
    }

    pub(super) fn render_liked(
        self: &Rc<Self>,
        result: Result<Page<Track>, MusicError>,
        page: u32,
    ) {
        self.widgets.library_rows.set_visible(true);
        self.playlist_grid.widget.set_visible(false);
        match result {
            Ok(result) => {
                let play = self.collection_play(
                    result.items.clone(),
                    ContextKind::LikedTracks,
                    None,
                    page > 0 || result.has_more,
                );
                let cover = result.items.first().and_then(|track| track.cover.as_ref());
                let mut facts = track_count_text(result.items.len() as u64);
                if page > 0 || result.has_more {
                    facts.push_str(" · Часть коллекции");
                }
                self.library_heading("Любимые треки", cover, &facts, play);
                self.model.borrow_mut().library_page = page;
                self.model.borrow_mut().library_displaying = Some((0, page));
                self.tracks_from(
                    &self.widgets.library_rows,
                    result.items,
                    ContextKind::LikedTracks,
                    None,
                    page > 0 || result.has_more,
                    0..usize::MAX,
                );
                self.widgets.library_next.set_visible(result.has_more);
                self.widgets.library_previous.set_visible(page > 0);
                self.refresh_catalog_notice();
                if self.widgets.stack.visible_child_name().as_deref() == Some("library") {
                    self.widgets
                        .status
                        .set_title(&format!("Понравившиеся треки · страница {}", page + 1));
                    self.widgets.status.set_subtitle("");
                }
            }
            Err(error) => {
                self.library_heading("Любимые треки", None, "", None);
                self.model.borrow_mut().library_displaying = None;
                set_empty(
                    &self.widgets.library_rows,
                    "Не удалось загрузить треки. Повторите обновление.",
                );
                self.show_error(error);
            }
        }
        if page == 0 {
            self.render_home_preview();
        }
    }

    pub(super) fn load_initial_library(self: &Rc<Self>) {
        self.load_initial_library_policy(false);
    }

    pub(super) fn load_initial_library_policy(self: &Rc<Self>, force: bool) {
        self.render_home_preview();
        let catalog = self.cached_catalog();
        self.run(
            Operation::InitialLibrary,
            move |service, token| {
                let liked = if let Some(catalog) = &catalog {
                    catalog.liked_tracks(&service, &token, 0, force)
                } else {
                    service.liked_tracks(&token, 0).map(catalog_view::network)
                };
                let playlists = if let Some(catalog) = &catalog {
                    catalog.playlists(&service, &token, 0, force)
                } else {
                    service.playlists(&token, 0).map(catalog_view::network)
                };
                Ok((liked, playlists))
            },
            move |this, result| match result {
                Ok((liked, playlists)) => {
                    let retired = liked
                        .as_ref()
                        .ok()
                        .and_then(|reply| reply.lease.as_ref())
                        .is_some_and(|lease| !lease.is_current())
                        || playlists
                            .as_ref()
                            .ok()
                            .and_then(|reply| reply.lease.as_ref())
                            .is_some_and(|lease| !lease.is_current());
                    if retired && !force {
                        this.load_initial_library_policy(true);
                        return;
                    }
                    let liked_origin = liked
                        .as_ref()
                        .map_or(crate::catalog_cache::Provenance::Network, |reply| {
                            reply.provenance
                        });
                    let playlists_origin = playlists
                        .as_ref()
                        .map_or(crate::catalog_cache::Provenance::Network, |reply| {
                            reply.provenance
                        });
                    let liked_cache_error = liked
                        .as_ref()
                        .is_ok_and(|reply| reply.cache_error.is_some());
                    let playlists_cache_error = playlists
                        .as_ref()
                        .is_ok_and(|reply| reply.cache_error.is_some());
                    let liked = liked.and_then(|reply| {
                        if reply
                            .lease
                            .as_ref()
                            .is_some_and(|lease| !lease.is_current())
                        {
                            Err(MusicError::Network)
                        } else {
                            Ok(reply.value)
                        }
                    });
                    let playlists = playlists.and_then(|reply| {
                        if reply
                            .lease
                            .as_ref()
                            .is_some_and(|lease| !lease.is_current())
                        {
                            Err(MusicError::Network)
                        } else {
                            Ok(reply.value)
                        }
                    });
                    if liked
                        .as_ref()
                        .err()
                        .into_iter()
                        .chain(playlists.as_ref().err())
                        .any(|error| {
                            matches!(
                                error,
                                MusicError::AccessDenied | MusicError::Unauthenticated
                            )
                        })
                    {
                        this.logout();
                        this.show_error(MusicError::AccessDenied);
                        return;
                    }
                    let mut model = this.model.borrow_mut();
                    let save_liked = model.liked.is_none();
                    let save_playlists = model.playlists.is_none();
                    if save_liked {
                        model.liked = Some(liked);
                    }
                    if save_playlists {
                        model.playlists = Some(playlists);
                    }
                    let render = !model.library_touched;
                    drop(model);
                    if save_liked {
                        this.remember_library_origin(0, 0, liked_origin, liked_cache_error);
                    }
                    if save_playlists {
                        this.remember_library_origin(1, 0, playlists_origin, playlists_cache_error);
                    }
                    if render {
                        this.show_library();
                    }
                    this.render_home_preview();
                }
                Err(error) => this.show_error(error),
            },
        );
    }

    pub(super) fn load_library(self: &Rc<Self>, page: u32) {
        self.load_library_policy(page, false);
    }

    pub(super) fn load_library_refresh(self: &Rc<Self>, page: u32) {
        self.load_library_policy(page, true);
    }

    pub(super) fn load_library_policy(self: &Rc<Self>, page: u32, force: bool) {
        if !self.model.borrow().account_valid || self.model.borrow().closed {
            return;
        }
        self.model.borrow_mut().library_touched = true;
        self.model.borrow_mut().library_displaying = None;
        self.widgets.status.set_title("Загрузка библиотеки…");
        self.widgets.status.set_subtitle("");
        self.widgets.cache_notice.set_revealed(false);
        let mode = self.widgets.library_mode.selected();
        self.widgets.library_rows.set_visible(mode == 0);
        self.playlist_grid.widget.set_visible(mode != 0);
        if mode == 0 {
            set_empty(&self.widgets.library_rows, "Загрузка библиотеки…");
        } else {
            self.playlist_grid.set_loading();
        }
        self.widgets.library_next.set_visible(false);
        self.widgets.library_previous.set_visible(false);
        if mode == 2 {
            self.run_catalog(
                Operation::Library,
                Some((mode, page)),
                force,
                move |service, token, catalog, force| match catalog {
                    Some(catalog) => catalog.liked_playlists(&service, &token, page, force),
                    None => service
                        .liked_playlists(&token, page)
                        .map(catalog_view::network),
                },
                move |this, result| this.render_playlists(result, page),
            );
        } else if mode == 1 {
            self.run_catalog(
                Operation::Library,
                Some((mode, page)),
                force,
                move |service, token, catalog, force| match catalog {
                    Some(catalog) => catalog.playlists(&service, &token, page, force),
                    None => service.playlists(&token, page).map(catalog_view::network),
                },
                move |this, result| {
                    if page == 0 {
                        this.model.borrow_mut().playlists = Some(result.clone());
                    }
                    this.render_playlists(result, page);
                },
            );
        } else {
            self.run_catalog(
                Operation::Library,
                Some((mode, page)),
                force,
                move |service, token, catalog, force| match catalog {
                    Some(catalog) => catalog.liked_tracks(&service, &token, page, force),
                    None => service
                        .liked_tracks(&token, page)
                        .map(catalog_view::network),
                },
                move |this, result| {
                    if page == 0 {
                        this.model.borrow_mut().liked = Some(result.clone());
                    }
                    this.render_liked(result, page);
                },
            );
        }
    }

    pub(super) fn clear_search_feedback(&self) {
        clear_box(&self.widgets.search_feedback);
        self.widgets.search_feedback.set_visible(false);
    }

    pub(super) fn search_error(self: &Rc<Self>, error: MusicError, page: u32) {
        self.clear_search_feedback();
        clear_list(&self.widgets.search_rows);
        if matches!(
            error,
            MusicError::AccessDenied | MusicError::Unauthenticated
        ) {
            self.show_error(error);
            return;
        }
        let message = friendly_search_error(error);
        let group = gtk::Box::new(gtk::Orientation::Vertical, 8);
        group.set_widget_name("music-search-error");
        group.add_css_class("music-inline-error");
        group.append(&text("Не удалось выполнить поиск", "heading"));
        group.append(&text(message, "dim-label"));
        let retry = gtk::Button::with_label("Повторить");
        retry.set_widget_name("music-search-retry");
        retry.set_halign(gtk::Align::Start);
        retry.add_css_class("suggested-action");
        let weak = Rc::downgrade(self);
        retry.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.search(page);
            }
        });
        group.append(&retry);
        self.widgets.search_feedback.append(&group);
        // Diagnostics are explicitly disclosed, collapsed and independently
        // selectable. The everyday error contains no protocol vocabulary.
        let details = gtk::Expander::new(Some("Сведения для диагностики"));
        details.set_widget_name("music-search-diagnostics");
        details.set_expanded(false);
        let support = text(
            &format!(
                "Категория: {}\nЭтап: {}\nФорма: {}\nHTTP: {}",
                error.category(),
                error.diagnostic_stage().unwrap_or("none"),
                error.diagnostic_shape().unwrap_or("none"),
                error
                    .http_status()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".into()),
            ),
            "monospace",
        );
        support.set_selectable(true);
        details.set_child(Some(&support));
        self.widgets.search_feedback.append(&details);
        self.widgets.search_feedback.set_visible(true);
    }

    pub(super) fn tracks_from(
        self: &Rc<Self>,
        list: &gtk::ListBox,
        tracks: Vec<Track>,
        kind: ContextKind,
        source: Option<String>,
        partial: bool,
        range: std::ops::Range<usize>,
    ) {
        song_list(list);
        clear_list(list);
        if tracks.is_empty() {
            set_empty(list, "Треков пока нет.");
            return;
        }
        let Ok(context) = PlaybackContext::new(kind, source, tracks, range.start, partial) else {
            set_empty(
                list,
                "Этот источник превышает размер очереди. Откройте отдельную страницу.",
            );
            return;
        };
        for (index, track) in context
            .items()
            .iter()
            .enumerate()
            .skip(range.start)
            .take(range.end.saturating_sub(range.start))
        {
            if let Some(selected) = context.with_current_index(index) {
                self.track_row_context(list, track.clone(), selected);
            }
        }
    }

    pub(super) fn track_row_context(
        self: &Rc<Self>,
        list: &gtk::ListBox,
        track: Track,
        context: PlaybackContext,
    ) {
        song_list(list);
        let subtitle = track.artists.join(", ");
        let (row, body) = catalog_item(&track.title, &subtitle);
        row.set_tooltip_text(Some(if track.available {
            "Воспроизвести трек"
        } else {
            "Трек недоступен по данным сервиса"
        }));
        row.update_property(&[gtk::accessible::Property::Label(&format!(
            "Воспроизвести: {}",
            track.title
        ))]);
        row.set_activatable(track.available);
        body.prepend(&cover_widget(
            self.artwork.as_ref(),
            track.cover.as_ref(),
            48,
        ));
        if track.explicit {
            let explicit = gtk::Label::new(Some("E"));
            explicit.add_css_class("caption");
            explicit.add_css_class("dim-label");
            explicit.set_tooltip_text(Some("Ненормативная лексика"));
            explicit.update_property(&[gtk::accessible::Property::Label("Ненормативная лексика")]);
            body.append(&explicit);
        }
        if !track.available {
            row.add_css_class("music-unavailable");
        }
        if let Some(duration) = track.duration_ms {
            let label = text(&format_duration(duration), "caption");
            label.add_css_class("dim-label");
            label.add_css_class("music-track-duration");
            label.set_hexpand(false);
            label.set_wrap(false);
            body.append(&label);
        }
        let play = self.context_play_button(&track, true, context.clone());
        play.add_css_class("flat");
        body.append(&play);
        body.append(&self.track_menu_context(&track, Some(context.clone())));
        self.catalog_rows
            .borrow_mut()
            .retain(|binding| binding.row.upgrade().is_some());
        self.catalog_rows.borrow_mut().push(CatalogRow {
            row: row.downgrade(),
            context: context.clone(),
        });
        self.row_action(&row, RowAction::Track(context));
        row.set_activatable(track.available);
        list.append(&row);
        self.sync_catalog_rows();
    }

    pub(super) fn row_action(self: &Rc<Self>, row: &gtk::ListBoxRow, action: RowAction) {
        row.set_activatable(true);
        let group = gio::SimpleActionGroup::new();
        let activate = gio::SimpleAction::new("activate", None);
        let weak = Rc::downgrade(self);
        let generation = self.model.borrow().generation;
        let queue_snapshot = if matches!(action, RowAction::Queue(_)) {
            self.model.borrow().queue.context().cloned()
        } else {
            None
        };
        activate.connect_activate(move |_, _| {
            let Some(this) = weak.upgrade() else { return };
            if this.model.borrow().closed
                || !this.model.borrow().account_valid
                || this.model.borrow().generation != generation
            {
                return;
            }
            match &action {
                RowAction::Track(context) => this.play_music_context(context.clone()),
                RowAction::SearchPlaylist(playlist) => {
                    this.open_search_playlist(playlist.clone(), 0)
                }
                RowAction::Album(id) => this.open_album(id.clone()),
                RowAction::Artist(artist) => this.open_artist(artist.clone(), 0),
                RowAction::Queue(index) => {
                    let current = this.model.borrow().queue.context() == queue_snapshot.as_ref();
                    if current {
                        this.play_music_queue(*index);
                    }
                }
            }
        });
        group.add_action(&activate);
        // Do not set the row's GtkActionable action-name: the list's actual
        // user-activation signal is the single dispatch path, for mouse and
        // keyboard alike. Nested buttons retain their independent actions.
        row.insert_action_group("catalog-row", Some(&group));
    }

    pub(super) fn detail_loading(self: &Rc<Self>, title: &str, _subtitle: &str) {
        self.detail_cache.set(None);
        self.widgets.cache_notice.set_revealed(false);
        self.widgets.cache_hint.set_visible(false);
        self.requests.borrow_mut().invalidate(Operation::LikeRead);
        if let Some(origin) = self.widgets.stack.visible_child_name()
            && origin != "detail"
        {
            self.model.borrow_mut().detail_origin = origin.to_string();
        }
        self.cancel_delivery();
        self.requests.borrow_mut().invalidate(Operation::Delivery);
        self.detail_header(title, "");
        self.widgets.status.set_title("Загрузка выбранных данных…");
        self.widgets.status.set_subtitle("");
        let loading = adw::StatusPage::builder().title("Загрузка…").build();
        let spinner = gtk::Spinner::new();
        spinner.set_spinning(true);
        spinner.set_halign(gtk::Align::Center);
        loading.set_child(Some(&spinner));
        self.widgets.detail.append(&loading);
    }

    pub(super) fn detail_header(self: &Rc<Self>, title: &str, subtitle: &str) {
        clear_box(&self.widgets.detail);
        let origin = self.model.borrow().detail_origin.clone();
        let label = match origin.as_str() {
            "search" => "Назад к поиску",
            "queue" => "Назад к очереди",
            "home" => "Назад на главную",
            _ => "Назад в библиотеку",
        };
        let back = icon_button("go-previous-symbolic", label);
        back.set_widget_name("music-detail-back");
        back.add_css_class("flat");
        back.set_halign(gtk::Align::Start);
        let weak = Rc::downgrade(self);
        back.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.back_from_detail();
            }
        });
        self.widgets.detail.append(&back);
        let hero = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        hero.set_widget_name("music-detail-hero");
        hero.add_css_class("music-collection-hero");
        let labels = gtk::Box::new(gtk::Orientation::Vertical, 6);
        labels.set_widget_name("music-detail-heading");
        labels.set_hexpand(true);
        labels.set_valign(gtk::Align::Center);
        let title = text(title, "title-1");
        title.add_css_class("music-collection-title");
        labels.append(&title);
        let facts = text(subtitle, "dim-label");
        facts.set_widget_name("music-detail-facts");
        facts.add_css_class("music-collection-facts");
        facts.set_visible(!subtitle.is_empty());
        labels.append(&facts);
        hero.append(&labels);
        self.widgets.detail.append(&hero);
        self.widgets.stack.set_visible_child_name("detail");
    }

    pub(super) fn back_from_detail(&self) {
        if self.widgets.stack.visible_child_name().as_deref() != Some("detail") {
            return;
        }
        self.cancel_delivery();
        self.requests.borrow_mut().invalidate(Operation::Detail);
        self.model.borrow_mut().view_generation += 1;
        let origin = self.model.borrow().detail_origin.clone();
        self.widgets.stack.set_visible_child_name(&origin);
        match origin.as_str() {
            "search" => {
                self.widgets.search.grab_focus();
            }
            "queue" => {
                if let Some(row) = self.widgets.queue_rows.first_child() {
                    row.grab_focus();
                }
            }
            "home" => {
                self.widgets.home_liked.grab_focus();
            }
            _ => {
                self.widgets.library_mode.grab_focus();
            }
        }
    }

    pub(super) fn detail_cover(&self, cover: Option<&CoverArt>) {
        let Some(hero) = self
            .widgets
            .detail
            .first_child()
            .and_then(|back| back.next_sibling())
            .and_downcast::<gtk::Box>()
        else {
            return;
        };
        if let Some(previous) = hero.first_child()
            && previous.widget_name() == "music-detail-cover"
        {
            hero.remove(&previous);
        }
        let art = cover_widget(self.artwork.as_ref(), cover, 112);
        art.set_widget_name("music-detail-cover");
        hero.prepend(&art);
    }

    pub(super) fn track_menu_context(
        self: &Rc<Self>,
        track: &Track,
        context: Option<PlaybackContext>,
    ) -> gtk::MenuButton {
        let generation = self.model.borrow().generation;
        let button = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .build();
        button.add_css_class("flat");
        button.set_valign(gtk::Align::Center);
        button.set_tooltip_text(Some("Действия с треком"));
        button.update_property(&[gtk::accessible::Property::Label("Действия с треком")]);
        let popover = gtk::Popover::new();
        let actions = gtk::Box::new(gtk::Orientation::Vertical, 4);
        super::super::margins(&actions, 6);
        let detail = gtk::Button::with_label("Открыть сведения о треке");
        detail.add_css_class("flat");
        let weak = Rc::downgrade(self);
        let close = popover.downgrade();
        let id = track.id.clone();
        let detail_context = context.clone();
        detail.connect_clicked(move |_| {
            if let Some(popover) = close.upgrade() {
                popover.popdown();
            }
            if let Some(this) = weak.upgrade().filter(|this| {
                let model = this.model.borrow();
                model.generation == generation && model.account_valid && !model.closed
            }) {
                this.open_track_context(id.clone(), detail_context.clone());
            }
        });
        actions.append(&detail);
        let play = gtk::Button::with_label("Воспроизвести");
        play.add_css_class("flat");
        play.set_sensitive(track.available);
        let weak = Rc::downgrade(self);
        let close = popover.downgrade();
        let selected = track.clone();
        play.connect_clicked(move |_| {
            if let Some(popover) = close.upgrade() {
                popover.popdown();
            }
            if let Some(this) = weak.upgrade().filter(|this| {
                let model = this.model.borrow();
                model.generation == generation && model.account_valid && !model.closed
            }) {
                if let Some(context) = context.clone() {
                    this.play_music_context(context);
                } else {
                    this.play_music_track(selected.clone());
                }
            }
        });
        actions.append(&play);
        let add = gtk::Button::with_label("Добавить в очередь");
        add.add_css_class("flat");
        let weak = Rc::downgrade(self);
        let close = popover.downgrade();
        let queued = track.clone();
        add.connect_clicked(move |_| {
            if let Some(popover) = close.upgrade() {
                popover.popdown();
            }
            if let Some(this) = weak.upgrade().filter(|this| {
                let model = this.model.borrow();
                model.generation == generation && model.account_valid && !model.closed
            }) {
                this.enqueue(queued.clone());
            }
        });
        actions.append(&add);
        if let Some(id) = &track.album_id {
            let album = gtk::Button::with_label("Открыть альбом");
            album.add_css_class("flat");
            let weak = Rc::downgrade(self);
            let close = popover.downgrade();
            let id = id.clone();
            album.connect_clicked(move |_| {
                if let Some(popover) = close.upgrade() {
                    popover.popdown();
                }
                if let Some(this) = weak.upgrade().filter(|this| {
                    let model = this.model.borrow();
                    model.generation == generation && model.account_valid && !model.closed
                }) {
                    this.open_album(id.clone());
                }
            });
            actions.append(&album);
        }
        popover.set_child(Some(&actions));
        button.set_popover(Some(&popover));
        button
    }

    pub(super) fn detail_error(
        self: &Rc<Self>,
        error: MusicError,
        retry: impl Fn(&Rc<Self>) + 'static,
    ) {
        if matches!(
            error,
            MusicError::AccessDenied | MusicError::Unauthenticated
        ) {
            self.show_error(error);
            return;
        }
        // Keep the native Back and collection hero, replacing only the body.
        // Header internals are named instead of coupled to label positions.
        let mut content = self.widgets.detail.first_child();
        for _ in 0..2 {
            content = content.and_then(|child| child.next_sibling());
        }
        while let Some(child) = content {
            content = child.next_sibling();
            self.widgets.detail.remove(&child);
        }
        let (title, message) = music_error_message(error);
        let failure = adw::StatusPage::builder()
            .title(title)
            .description(message)
            .icon_name("dialog-warning-symbolic")
            .build();
        let button = gtk::Button::with_label("Повторить");
        button.set_halign(gtk::Align::Center);
        button.add_css_class("suggested-action");
        let generation = self.model.borrow().generation;
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade().filter(|this| {
                let model = this.model.borrow();
                model.generation == generation && model.account_valid && !model.closed
            }) {
                retry(&this);
            }
        });
        failure.set_child(Some(&button));
        self.widgets.detail.append(&failure);
    }

    pub(super) fn open_track_context(
        self: &Rc<Self>,
        id: String,
        context: Option<PlaybackContext>,
    ) {
        self.open_track_policy(id, context, false);
    }

    pub(super) fn open_track_policy(
        self: &Rc<Self>,
        id: String,
        context: Option<PlaybackContext>,
        force: bool,
    ) {
        let retry_context = context.clone();
        self.detail_loading("Сведения о треке", "Загрузка трека…");
        let info_epoch = self.info_target_epoch();
        let refresh_id = id.clone();
        let refresh_context = context.clone();
        self.detail_refresh.replace(Some(Rc::new(move |this| {
            this.open_track_policy(refresh_id.clone(), refresh_context.clone(), true)
        })));
        let retry_id = id.clone();
        self.run_catalog(
            Operation::Detail,
            None,
            force,
            move |service, token, catalog, force| match catalog {
                Some(catalog) => catalog.track(&service, &token, &id, force),
                None => service.track(&token, &id).map(catalog_view::network),
            },
            move |this, result| match result {
                Ok(track) => {
                    this.select_track_info_if_current(info_epoch, track.clone(), context.clone());
                    let mut facts = track.artists.join(", ");
                    if let Some(duration) = track.duration_ms.map(format_duration) {
                        if !facts.is_empty() {
                            facts.push_str(" · ");
                        }
                        facts.push_str(&duration);
                    }
                    this.detail_header(&track.title, &facts);
                    this.detail_cover(track.cover.as_ref());
                    if !track.available {
                        this.widgets
                            .detail
                            .append(&text("Сервис пометил трек как недоступный.", "dim-label"));
                    }
                    let button = if let Some(context) = context.clone() {
                        this.context_play_button(&track, false, context)
                    } else {
                        this.track_play_button(&track, false)
                    };
                    button.set_label("Слушать");
                    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    actions.add_css_class("music-collection-actions");
                    actions.append(&button);
                    let add = icon_button("list-add-symbolic", "Добавить в очередь");
                    add.add_css_class("flat");
                    let weak = Rc::downgrade(this);
                    let queued = track.clone();
                    add.connect_clicked(move |_| {
                        if let Some(this) = weak.upgrade() {
                            this.enqueue(queued.clone());
                        }
                    });
                    actions.append(&add);
                    actions.append(&this.track_menu_context(&track, context.clone()));
                    this.place_detail_actions(&actions);
                    this.widgets
                        .detail
                        .append(&this.like_controls(track.id.clone()));
                    if this.model.borrow().delivery_diagnostic {
                        this.delivery_action(track.id.clone());
                    }
                    if let Some(album_id) = track.album_id {
                        let album = gtk::Button::with_label(
                            track.album_title.as_deref().unwrap_or("Открыть альбом"),
                        );
                        album.set_halign(gtk::Align::Start);
                        if let Some(label) = album.child().and_downcast::<gtk::Label>() {
                            label.set_wrap(true);
                            label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                        }
                        let weak = Rc::downgrade(this);
                        album.connect_clicked(move |_| {
                            if let Some(this) = weak.upgrade() {
                                this.open_album(album_id.clone());
                            }
                        });
                        this.widgets.detail.append(&album);
                    }
                    this.widgets.status.set_title("Сведения о треке получены.");
                }
                Err(error) => this.detail_error(error, move |this| {
                    this.open_track_context(retry_id.clone(), retry_context.clone())
                }),
            },
        );
    }

    pub(super) fn open_album(self: &Rc<Self>, id: String) {
        self.open_album_policy(id, false, 0);
    }

    pub(super) fn open_album_policy(self: &Rc<Self>, id: String, force: bool, page: usize) {
        self.detail_loading("Альбом", "Загрузка альбома…");
        let refresh_id = id.clone();
        self.detail_refresh.replace(Some(Rc::new(move |this| {
            this.open_album_policy(refresh_id.clone(), true, page)
        })));
        let retry_id = id.clone();
        self.run_catalog(
            Operation::Detail,
            None,
            force,
            move |service, token, catalog, force| match catalog {
                Some(catalog) => catalog.album(&service, &token, &id, force),
                None => service.album(&token, &id).map(catalog_view::network),
            },
            move |this, result| match result {
                Ok(album) => {
                    this.render_album(Rc::new(album), page);
                    this.widgets.status.set_title("Альбом получен.");
                }
                Err(error) => this.detail_error(error, move |this| {
                    this.open_album_policy(retry_id.clone(), true, page)
                }),
            },
        );
    }

    pub(super) fn render_album(self: &Rc<Self>, album: Rc<Album>, page: usize) {
        const ROWS: usize = 50;
        let page = page.min(album.tracks.len().saturating_sub(1) / ROWS);
        let id = album.id.clone();
        self.detail_refresh.replace(Some(Rc::new(move |this| {
            this.open_album_policy(id.clone(), true, page)
        })));
        self.detail_header(&album.title, &album.artists.join(", "));
        self.detail_cover(album.cover.as_ref());
        let rows = list();
        let offset = page.saturating_mul(ROWS);
        let (play_tracks, partial) = if album.tracks.len() <= 500 {
            (album.tracks.clone(), false)
        } else {
            (
                album
                    .tracks
                    .iter()
                    .skip(offset)
                    .take(ROWS)
                    .cloned()
                    .collect(),
                true,
            )
        };
        self.detail_primary(self.collection_play(
            play_tracks,
            ContextKind::Album,
            Some(album.id.clone()),
            partial,
        ));
        if album.tracks.len() <= 500 {
            self.tracks_from(
                &rows,
                album.tracks.clone(),
                ContextKind::Album,
                Some(album.id.clone()),
                false,
                offset..offset.saturating_add(ROWS),
            );
        } else {
            self.tracks_from(
                &rows,
                album
                    .tracks
                    .iter()
                    .skip(offset)
                    .take(ROWS)
                    .cloned()
                    .collect(),
                ContextKind::Album,
                Some(album.id.clone()),
                true,
                0..ROWS,
            );
        }
        self.widgets.detail.append(&rows);
        let pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        if page > 0 {
            let back = gtk::Button::with_label("Предыдущие");
            let weak = Rc::downgrade(self);
            let album = album.clone();
            back.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.render_album(album.clone(), page - 1);
                }
            });
            pages.append(&back);
        }
        if offset.saturating_add(ROWS) < album.tracks.len() {
            let next = gtk::Button::with_label("Следующие");
            let weak = Rc::downgrade(self);
            next.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.render_album(album.clone(), page + 1);
                }
            });
            pages.append(&next);
        }
        self.widgets.detail.append(&pages);
    }

    pub(super) fn open_playlist(self: &Rc<Self>, playlist: Playlist, page: u32) {
        self.open_playlist_policy(playlist, page, false);
    }

    pub(super) fn open_playlist_policy(
        self: &Rc<Self>,
        playlist: Playlist,
        page: u32,
        force: bool,
    ) {
        let id = playlist.id.clone();
        self.detail_loading(
            &playlist.title,
            &format!("Загрузка плейлиста · страница {}", page + 1),
        );
        let refresh_playlist = playlist.clone();
        self.detail_refresh.replace(Some(Rc::new(move |this| {
            this.open_playlist_policy(refresh_playlist.clone(), page, true)
        })));
        self.detail_cover(playlist.cover.as_ref());
        self.run_catalog(
            Operation::Detail,
            None,
            force,
            move |service, token, catalog, force| match catalog {
                Some(catalog) => catalog.playlist_tracks(&service, &token, &id, page, force),
                None => service
                    .playlist_tracks(&token, &id, page)
                    .map(catalog_view::network),
            },
            move |this, result| match result {
                Ok(result) => {
                    this.detail_header(
                        &playlist.title,
                        &format!("Плейлист · {}", track_count_text(playlist.track_count)),
                    );
                    this.detail_cover(playlist.cover.as_ref());
                    this.detail_primary(this.collection_play(
                        result.items.clone(),
                        ContextKind::Playlist,
                        Some(format!("{}:{}", playlist.id.owner, playlist.id.kind)),
                        page > 0 || result.has_more,
                    ));
                    let rows = list();
                    this.tracks_from(
                        &rows,
                        result.items,
                        ContextKind::Playlist,
                        Some(format!("{}:{}", playlist.id.owner, playlist.id.kind)),
                        page > 0 || result.has_more,
                        0..usize::MAX,
                    );
                    this.widgets.detail.append(&rows);
                    let pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    if page > 0 {
                        let previous = gtk::Button::with_label("Предыдущая страница");
                        let weak = Rc::downgrade(this);
                        let playlist = playlist.clone();
                        previous.connect_clicked(move |_| {
                            if let Some(this) = weak.upgrade() {
                                this.open_playlist(playlist.clone(), page - 1);
                            }
                        });
                        pages.append(&previous);
                    }
                    if result.has_more {
                        let next = gtk::Button::with_label("Следующая страница");
                        next.set_halign(gtk::Align::Start);
                        let weak = Rc::downgrade(this);
                        next.connect_clicked(move |_| {
                            if let Some(this) = weak.upgrade() {
                                this.open_playlist(playlist.clone(), page.saturating_add(1));
                            }
                        });
                        pages.append(&next);
                    }
                    this.widgets.detail.append(&pages);
                    this.widgets.status.set_title("Плейлист получен.");
                    this.widgets.status.set_subtitle("");
                }
                Err(error) => this.detail_error(error, move |this| {
                    this.open_playlist(playlist.clone(), page)
                }),
            },
        );
    }

    pub(super) fn open_artist(self: &Rc<Self>, artist: Artist, page: u32) {
        self.open_artist_policy(artist, page, false);
    }

    pub(super) fn open_artist_policy(self: &Rc<Self>, artist: Artist, page: u32, force: bool) {
        self.detail_loading(&artist.name, "Загрузка треков исполнителя…");
        let refresh_artist = artist.clone();
        self.detail_refresh.replace(Some(Rc::new(move |this| {
            this.open_artist_policy(refresh_artist.clone(), page, true)
        })));
        self.detail_cover(artist.cover.as_ref());
        let id = artist.id.clone();
        self.run_catalog(
            Operation::Detail,
            None,
            force,
            move |service, token, catalog, force| match catalog {
                Some(catalog) => catalog.artist_tracks(&service, &token, &id, page, force),
                None => service
                    .artist_tracks(&token, &id, page)
                    .map(catalog_view::network),
            },
            move |this, result| match result {
                Ok(result) => {
                    this.detail_header(&artist.name, "Исполнитель");
                    this.detail_cover(artist.cover.as_ref());
                    this.detail_primary(this.collection_play(
                        result.items.clone(),
                        ContextKind::Artist,
                        Some(artist.id.clone()),
                        page > 0 || result.has_more,
                    ));
                    let rows = list();
                    this.tracks_from(
                        &rows,
                        result.items,
                        ContextKind::Artist,
                        Some(artist.id.clone()),
                        page > 0 || result.has_more,
                        0..usize::MAX,
                    );
                    this.widgets.detail.append(&rows);
                    let pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    if page > 0 {
                        let previous = gtk::Button::with_label("Предыдущая страница");
                        let weak = Rc::downgrade(this);
                        let artist = artist.clone();
                        previous.connect_clicked(move |_| {
                            if let Some(this) = weak.upgrade() {
                                this.open_artist(artist.clone(), page - 1);
                            }
                        });
                        pages.append(&previous);
                    }
                    if result.has_more {
                        let next = gtk::Button::with_label("Следующая страница");
                        let weak = Rc::downgrade(this);
                        next.connect_clicked(move |_| {
                            if let Some(this) = weak.upgrade() {
                                this.open_artist(artist.clone(), page.saturating_add(1));
                            }
                        });
                        pages.append(&next);
                    }
                    this.widgets.detail.append(&pages);
                    this.widgets.status.set_title("Треки исполнителя получены.");
                    this.widgets.status.set_subtitle("");
                }
                Err(error) => {
                    this.detail_error(error, move |this| this.open_artist(artist.clone(), page))
                }
            },
        );
    }
}
