// SPDX-License-Identifier: GPL-3.0-or-later
//! A bounded overview of the account's existing library views.

use super::*;

mod marquee;

const TRACK_PREVIEW: usize = 6;
const PLAYLIST_PREVIEW: usize = 8;
const COLUMN_MIN_WIDTH: i32 = 304;
const COLUMN_GAP: i32 = 12;

#[derive(PartialEq, Eq)]
enum Section<T> {
    Hidden,
    Loading,
    Error,
    Ready(T),
}

pub(super) struct CollectionOverview {
    pub widget: gtk::Box,
    pub open_liked: gtk::Button,
    pub open_playlists: gtk::Button,
    liked_rows: gtk::Box,
    liked_columns: [gtk::ListBox; 2],
    liked_scroll: gtk::ScrolledWindow,
    liked_status: gtk::Label,
    liked_spinner: gtk::Spinner,
    liked_retry: gtk::Button,
    playlist_owned: gtk::ToggleButton,
    playlist_liked: gtk::ToggleButton,
    playlist_grid: playlist_grid::PlaylistGrid,
    track_state: RefCell<Section<Page<Track>>>,
    playlist_state: RefCell<Section<Vec<Playlist>>>,
}

impl CollectionOverview {
    pub(super) fn new(
        artwork: Option<ArtworkLoader>,
        playlist_action: Rc<dyn Fn(playlist_grid::PlaylistAction)>,
    ) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 28);
        widget.set_widget_name("music-collection-overview");
        let heading = text("Моя коллекция", "title-1");
        heading.set_margin_top(4);
        widget.append(&heading);

        let liked = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let open_liked = section_link(
            "Мне нравится",
            "karin-heart-symbolic",
            "music-collection-open-liked",
            "win.music-library",
        );
        let liked_header = gtk::CenterBox::new();
        liked_header.set_start_widget(Some(&open_liked));
        let liked_rows = gtk::Box::new(gtk::Orientation::Horizontal, COLUMN_GAP);
        liked_rows.set_widget_name("music-collection-liked-rows");
        liked_rows.set_homogeneous(true);
        let liked_columns = [list(), list()];
        for (index, column) in liked_columns.iter().enumerate() {
            column.set_widget_name(if index == 0 {
                "music-collection-liked-first-column"
            } else {
                "music-collection-liked-second-column"
            });
            liked_rows.append(column);
        }
        let viewport = gtk::Viewport::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        viewport.set_scroll_to_focus(true);
        viewport.set_child(Some(&liked_rows));
        let liked_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .child(&viewport)
            .build();
        liked_scroll.set_widget_name("music-collection-liked-scroll");
        liked_scroll.set_visible(false);
        liked_header.set_end_widget(Some(&preview_arrows(&liked_scroll)));
        liked.append(&liked_header);
        liked.append(&liked_scroll);
        let liked_feedback = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let liked_status = text("", "dim-label");
        liked_status.set_widget_name("music-collection-liked-status");
        liked_status.set_visible(false);
        let liked_spinner = gtk::Spinner::new();
        liked_spinner.set_halign(gtk::Align::Start);
        liked_spinner.set_visible(false);
        let liked_retry = gtk::Button::with_label("Повторить загрузку");
        liked_retry.set_widget_name("music-collection-liked-retry");
        liked_retry.set_halign(gtk::Align::Start);
        liked_retry.set_visible(false);
        liked_feedback.append(&liked_status);
        liked_feedback.append(&liked_spinner);
        liked_feedback.append(&liked_retry);
        liked.append(&liked_feedback);
        widget.append(&liked);

        let playlists = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let open_playlists = section_link(
            "Мои плейлисты",
            "karin-playlist-symbolic",
            "music-collection-open-playlists",
            "win.music-playlists",
        );
        playlists.append(&open_playlists);
        let playlist_options = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        playlist_options.set_halign(gtk::Align::Start);
        playlist_options.add_css_class("linked");
        let playlist_owned = gtk::ToggleButton::with_label("Созданные");
        playlist_owned.set_widget_name("music-collection-playlists-owned");
        let playlist_liked = gtk::ToggleButton::with_label("Понравившиеся");
        playlist_liked.set_widget_name("music-collection-playlists-liked");
        playlist_liked.set_group(Some(&playlist_owned));
        playlist_owned.set_active(true);
        playlist_options.append(&playlist_owned);
        playlist_options.append(&playlist_liked);
        playlists.append(&playlist_options);
        let playlist_grid = playlist_grid::PlaylistGrid::new(artwork, playlist_action);
        playlist_grid
            .widget
            .set_widget_name("music-collection-playlist-grid");
        playlist_grid.clear();
        playlists.append(&playlist_grid.widget);
        widget.append(&playlists);
        Self {
            widget,
            open_liked,
            open_playlists,
            liked_rows,
            liked_columns,
            liked_scroll,
            liked_status,
            liked_spinner,
            liked_retry,
            playlist_owned,
            playlist_liked,
            playlist_grid,
            track_state: RefCell::new(Section::Hidden),
            playlist_state: RefCell::new(Section::Hidden),
        }
    }

    pub(super) fn is_showing_liked_playlists(&self) -> bool {
        self.playlist_liked.is_active()
    }
}

fn preview_arrows(scroll: &gtk::ScrolledWindow) -> gtk::Box {
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    controls.set_widget_name("music-collection-liked-arrows");
    controls.set_valign(gtk::Align::Center);
    controls.set_visible(false);
    let left = preview_arrow("‹", "Прокрутить избранное влево", "left");
    let right = preview_arrow("›", "Прокрутить избранное вправо", "right");
    controls.append(&left);
    controls.append(&right);
    let adjustment = scroll.hadjustment();
    for (button, forward) in [(&left, false), (&right, true)] {
        let adjustment = adjustment.clone();
        button.connect_clicked(move |_| {
            let endpoint = if forward {
                adjustment.upper() - adjustment.page_size()
            } else {
                adjustment.lower()
            };
            adjustment.set_value(endpoint);
        });
    }
    let update = {
        let (controls, left, right) = (controls.downgrade(), left.downgrade(), right.downgrade());
        let scroll = scroll.downgrade();
        move |adjustment: &gtk::Adjustment| {
            if let (Some(controls), Some(left), Some(right), Some(scroll)) = (
                controls.upgrade(),
                left.upgrade(),
                right.upgrade(),
                scroll.upgrade(),
            ) {
                let lower = adjustment.lower();
                let upper = adjustment.upper() - adjustment.page_size();
                controls.set_visible(scroll.is_visible() && upper - lower > 1.0);
                left.set_sensitive(adjustment.value() - lower > 1.0);
                right.set_sensitive(upper - adjustment.value() > 1.0);
            }
        }
    };
    adjustment.connect_changed(update.clone());
    adjustment.connect_value_changed(update.clone());
    scroll.connect_visible_notify(move |scroll| update(&scroll.hadjustment()));
    controls
}

fn preview_arrow(glyph: &str, description: &str, direction: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_widget_name(&format!("music-collection-liked-arrow-{direction}"));
    button.set_size_request(36, 36);
    button.add_css_class("circular");
    let label = gtk::Label::new(Some(glyph));
    label.add_css_class("title-3");
    button.set_child(Some(&label));
    button.set_tooltip_text(Some(description));
    button.update_property(&[gtk::accessible::Property::Label(description)]);
    button
}

fn section_link(title: &str, icon: &str, name: &str, action: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_widget_name(name);
    button.set_halign(gtk::Align::Start);
    button.add_css_class("flat");
    button.set_action_name(Some(action));
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let art = gtk::Image::from_icon_name(icon);
    art.set_pixel_size(22);
    art.set_size_request(40, 40);
    art.add_css_class("music-liked-cover");
    content.append(&art);
    let title_label = text(title, "title-2");
    content.append(&title_label);
    let next = gtk::Label::new(Some("›"));
    next.add_css_class("dim-label");
    next.add_css_class("title-3");
    content.append(&next);
    button.set_child(Some(&content));
    button.update_property(&[gtk::accessible::Property::Label(title)]);
    button
}

impl Controller {
    pub(super) fn connect_collection_overview(self: &Rc<Self>) {
        self.collection_overview
            .open_liked
            .set_tooltip_text(Some("Открыть все избранные треки"));
        let weak = Rc::downgrade(self);
        self.collection_overview
            .liked_retry
            .connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.refresh_collection_liked();
                }
            });
        for button in [
            &self.collection_overview.playlist_owned,
            &self.collection_overview.playlist_liked,
        ] {
            let weak = Rc::downgrade(self);
            button.connect_toggled(move |button| {
                if button.is_active()
                    && let Some(this) = weak.upgrade()
                {
                    this.render_collection_overview();
                    this.refresh_catalog_notice();
                }
            });
        }
    }

    pub(super) fn collection_read_completed(self: &Rc<Self>, operation: Operation) {
        if !matches!(
            operation,
            Operation::InitialLibrary
                | Operation::Library
                | Operation::PlaylistSections
                | Operation::OwnedPlaylistsMore
                | Operation::LikedPlaylistsMore
                | Operation::Mutation
                | Operation::PlaylistMutation
        ) {
            return;
        }
        match self.widgets.stack.visible_child_name().as_deref() {
            Some("collection") => self.show_collection_overview(),
            Some("playlists") if operation == Operation::InitialLibrary => {
                self.show_playlist_sections();
            }
            _ => {}
        }
    }

    pub(super) fn show_collection_overview(self: &Rc<Self>) {
        self.render_collection_overview();
        self.refresh_catalog_notice();
        let (need_liked, need_owned, need_saved) = {
            let model = self.model.borrow();
            if !model.account_valid || model.closed {
                return;
            }
            (
                model.liked.is_none(),
                model.playlists.is_none(),
                model.liked_playlists.is_none(),
            )
        };
        let (initial_busy, liked_busy, playlists_busy, changing_library) = self.collection_busy();
        if changing_library || initial_busy {
            return;
        }
        if need_liked && !liked_busy {
            self.load_liked_tracks(false);
        }
        if (need_owned || need_saved) && !playlists_busy {
            self.load_playlist_sections_mask(false, Some((need_owned, need_saved)));
        }
    }

    fn collection_busy(&self) -> (bool, bool, bool, bool) {
        let requests = self.requests.borrow();
        (
            requests.occupied(Operation::InitialLibrary),
            requests.occupied(Operation::Library),
            requests.occupied(Operation::PlaylistSections),
            requests.occupied(Operation::Mutation)
                || requests.occupied(Operation::PlaylistMutation),
        )
    }

    fn refresh_collection_liked(self: &Rc<Self>) {
        let (initial_busy, liked_busy, _, changing_library) = self.collection_busy();
        if !initial_busy && !liked_busy && !changing_library {
            self.load_liked_tracks(true);
            self.render_collection_overview();
        }
    }

    pub(super) fn refresh_collection(self: &Rc<Self>) {
        self.load_liked_tracks(true);
        self.load_playlist_sections(true);
        self.render_collection_overview();
    }

    pub(super) fn handle_collection_playlist_action(
        self: &Rc<Self>,
        action: playlist_grid::PlaylistAction,
    ) {
        if self.widgets.stack.visible_child_name().as_deref() != Some("collection") {
            return;
        }
        match action {
            playlist_grid::PlaylistAction::Retry => {
                let (initial_busy, _, playlists_busy, changing_library) = self.collection_busy();
                if !initial_busy && !playlists_busy && !changing_library {
                    let liked = self.collection_overview.is_showing_liked_playlists();
                    self.load_playlist_sections_mask(true, Some((!liked, liked)));
                }
            }
            _ => self.handle_playlist_action(action),
        }
    }

    pub(super) fn render_collection_overview(self: &Rc<Self>) {
        let overview = &self.collection_overview;
        let liked_playlists = overview.is_showing_liked_playlists();
        let (_, liked_busy, _, _) = self.collection_busy();
        let (tracks, playlists) = {
            let model = self.model.borrow();
            if model.account_valid && !model.closed {
                if self.widgets.stack.visible_child_name().as_deref() != Some("collection") {
                    return;
                }
                let tracks = match &model.liked {
                    Some(Ok(page)) => Section::Ready(page.clone()),
                    Some(Err(_)) if !liked_busy => Section::Error,
                    _ => Section::Loading,
                };
                let playlist_page = if liked_playlists {
                    &model.liked_playlists
                } else {
                    &model.playlists
                };
                let playlists = if self.playlist_refreshing.get()[usize::from(liked_playlists)] {
                    Section::Loading
                } else {
                    match playlist_page {
                        Some(Ok(page)) => Section::Ready(
                            page.items.iter().take(PLAYLIST_PREVIEW).cloned().collect(),
                        ),
                        Some(Err(_)) => Section::Error,
                        None => Section::Loading,
                    }
                };
                (tracks, playlists)
            } else {
                (Section::Hidden, Section::Hidden)
            }
        };

        if *overview.track_state.borrow() != tracks {
            overview
                .liked_spinner
                .set_spinning(tracks == Section::Loading);
            overview
                .liked_spinner
                .set_visible(tracks == Section::Loading);
            overview.liked_retry.set_visible(tracks == Section::Error);
            let status = match &tracks {
                Section::Hidden => "",
                Section::Loading => "Загрузка избранных треков…",
                Section::Error => "Не удалось загрузить избранные треки.",
                Section::Ready(page) if page.items.is_empty() => "В избранном пока нет треков.",
                Section::Ready(_) => "",
            };
            overview.liked_status.set_text(status);
            overview.liked_status.set_visible(!status.is_empty());
            overview.liked_scroll.set_visible(false);
            for column in &overview.liked_columns {
                clear_list(column);
            }
            overview.liked_scroll.hadjustment().set_value(0.0);
            if let Section::Ready(page) = &tracks
                && let Ok(snapshot) = PlaybackContext::new(
                    ContextKind::LikedTracks,
                    None,
                    page.items.clone(),
                    0,
                    page.has_more,
                )
            {
                let generation = self.model.borrow().generation;
                let preview_len = page.items.len().min(TRACK_PREVIEW);
                let column_len = preview_len.div_ceil(2);
                let two_columns = preview_len > 1;
                overview.liked_columns[1].set_visible(two_columns);
                overview.liked_rows.set_width_request(if two_columns {
                    COLUMN_MIN_WIDTH * 2 + COLUMN_GAP
                } else {
                    -1
                });
                for (index, track) in page.items.iter().take(TRACK_PREVIEW).enumerate() {
                    let weak = Rc::downgrade(self);
                    let snapshot = snapshot.clone();
                    let resolve = Rc::new(move || {
                        let this = weak.upgrade()?;
                        if this.model.borrow().generation != generation {
                            return None;
                        }
                        this.liked_overview_context(snapshot.items(), index)
                            .or_else(|| snapshot.with_current_index(index))
                    });
                    let row = self.track_row_source(
                        &overview.liked_columns[index / column_len],
                        track.clone(),
                        resolve,
                    );
                    marquee::install(&row);
                }
                overview.liked_scroll.set_visible(true);
            }
            overview.track_state.replace(tracks);
        }
        if *overview.playlist_state.borrow() != playlists {
            match &playlists {
                Section::Hidden => overview.playlist_grid.clear(),
                Section::Loading => overview.playlist_grid.set_loading(),
                Section::Error => overview.playlist_grid.set_error(),
                Section::Ready(items) => overview.playlist_grid.set_playlists(items, false),
            }
            overview.playlist_state.replace(playlists);
        }
    }
}
