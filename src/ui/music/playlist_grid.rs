// SPDX-License-Identifier: GPL-3.0-or-later
//! Adaptive playlist tiles. The controller remains the source of catalog truth.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;

use crate::{artwork::ArtworkLoader, music::Playlist};

#[derive(Clone)]
pub(super) enum PlaylistAction {
    Open(Playlist),
    Create,
    Retry,
}

struct State {
    generation: Cell<u64>,
    create_allowed: Cell<bool>,
    action: Rc<dyn Fn(PlaylistAction)>,
}

pub(super) struct PlaylistGrid {
    pub widget: gtk::Box,
    pub tiles: gtk::FlowBox,
    pub retry: gtk::Button,
    state: Rc<State>,
    loader: Option<ArtworkLoader>,
    feedback: gtk::Box,
    spinner: gtk::Spinner,
    message: gtk::Label,
    create_tile: RefCell<Option<gtk::glib::WeakRef<gtk::FlowBoxChild>>>,
    cover_size: Rc<Cell<i32>>,
}

impl PlaylistGrid {
    pub fn new(loader: Option<ArtworkLoader>, action: Rc<dyn Fn(PlaylistAction)>) -> Self {
        let state = Rc::new(State {
            generation: Cell::new(0),
            create_allowed: Cell::new(false),
            action,
        });
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        widget.set_widget_name("music-playlist-grid");
        widget.add_css_class("karin-playlist-grid");
        let tiles = gtk::FlowBox::new();
        tiles.set_widget_name("music-playlist-tiles");
        tiles.set_selection_mode(gtk::SelectionMode::None);
        tiles.set_focusable(false);
        tiles.set_min_children_per_line(1);
        tiles.set_max_children_per_line(12);
        tiles.set_column_spacing(16);
        tiles.set_row_spacing(24);
        tiles.set_homogeneous(true);
        tiles.set_halign(gtk::Align::Start);
        tiles.set_hexpand(false);
        tiles.set_valign(gtk::Align::Start);
        widget.append(&tiles);
        let feedback = gtk::Box::new(gtk::Orientation::Vertical, 10);
        feedback.set_halign(gtk::Align::Start);
        let spinner = gtk::Spinner::new();
        spinner.set_halign(gtk::Align::Start);
        spinner.set_visible(false);
        feedback.append(&spinner);
        let message = gtk::Label::new(None);
        message.set_widget_name("music-playlist-grid-status");
        message.set_xalign(0.0);
        message.set_wrap(true);
        message.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        message.set_width_chars(1);
        message.add_css_class("dim-label");
        feedback.append(&message);
        let retry = gtk::Button::with_label("Повторить загрузку");
        retry.set_widget_name("music-playlist-grid-retry");
        retry.set_halign(gtk::Align::Start);
        retry.set_visible(false);
        retry.connect_clicked({
            let state = Rc::clone(&state);
            move |_| (state.action)(PlaylistAction::Retry)
        });
        feedback.append(&retry);
        widget.append(&feedback);
        // Use the actual section viewport, not the FlowBox's natural width:
        // one sparse tile must not turn itself into a full-width cover. Resize
        // the already mounted thumbnails without rebuilding tiles or refetching
        // art. The only closure reference into the widget tree is weak.
        let cover_size = Rc::new(Cell::new(160));
        let weak_tiles = tiles.downgrade();
        widget.add_tick_callback({
            let cover_size = Rc::clone(&cover_size);
            move |host, _| {
                if host.width() <= 0 {
                    return gtk::glib::ControlFlow::Continue;
                }
                let size = cover_size_for_width(host.width());
                if cover_size.replace(size) != size
                    && let Some(tiles) = weak_tiles.upgrade()
                {
                    resize_tiles(&tiles, size);
                }
                gtk::glib::ControlFlow::Continue
            }
        });
        let result = Self {
            widget,
            tiles,
            retry,
            state,
            loader,
            feedback,
            spinner,
            message,
            create_tile: RefCell::new(None),
            cover_size,
        };
        result.set_playlists(&[], false);
        result
    }

    pub fn set_playlists(&self, playlists: &[Playlist], allow_create: bool) {
        self.reset();
        let generation = self.state.generation.get();
        if allow_create {
            self.state.create_allowed.set(true);
            let button = self.create_tile(generation);
            self.append_tile(&button);
            if let Some(child) = button.parent().and_downcast::<gtk::FlowBoxChild>() {
                self.create_tile.replace(Some(child.downgrade()));
            }
        }
        for playlist in playlists {
            let button = self.playlist_tile(playlist, generation);
            self.append_tile(&button);
        }
        self.tiles
            .set_visible(allow_create || !playlists.is_empty());
        self.feedback.set_visible(playlists.is_empty());
        if playlists.is_empty() {
            self.message.set_text(if allow_create {
                "Плейлистов пока нет. Создайте первую подборку."
            } else {
                "Плейлистов пока нет."
            });
        }
    }

    pub fn set_loading(&self) {
        self.reset();
        self.feedback.set_visible(true);
        self.message.set_text("Загрузка плейлистов…");
        self.spinner.set_visible(true);
        self.spinner.set_spinning(true);
    }

    pub fn set_error(&self) {
        self.reset();
        self.feedback.set_visible(true);
        self.message
            .set_text("Не удалось загрузить плейлисты. Попробуйте ещё раз.");
        self.retry.set_visible(true);
    }

    pub fn clear(&self) {
        self.reset();
        self.feedback.set_visible(false);
    }

    /// Remove only a refused capability. Existing playlist actions keep their
    /// generation, while an already queued old Create event is rejected.
    pub fn hide_create(&self) {
        self.state.create_allowed.set(false);
        let child = self
            .create_tile
            .borrow_mut()
            .take()
            .and_then(|child| child.upgrade());
        let Some(child) = child else {
            return;
        };
        self.tiles.remove(&child);
        if self.tiles.first_child().is_none() {
            self.tiles.set_visible(false);
            self.feedback.set_visible(true);
            self.message.set_text("Плейлистов пока нет.");
        }
    }

    fn reset(&self) {
        self.state
            .generation
            .set(self.state.generation.get().wrapping_add(1));
        self.state.create_allowed.set(false);
        self.create_tile.borrow_mut().take();
        while let Some(child) = self.tiles.first_child() {
            self.tiles.remove(&child);
        }
        self.tiles.set_visible(false);
        self.spinner.set_spinning(false);
        self.spinner.set_visible(false);
        self.retry.set_visible(false);
        self.message.set_text("");
    }

    fn append_tile(&self, button: &gtk::Button) {
        self.tiles.insert(button, -1);
        if let Some(parent) = button.parent() {
            // The actual Button is the one keyboard/mouse activation target.
            // Do not add a second tab stop on FlowBox's wrapper child.
            parent.set_focusable(false);
        }
    }

    fn playlist_tile(&self, playlist: &Playlist, generation: u64) -> gtk::Button {
        let size = self.cover_size.get();
        let button = tile_button("music-playlist-tile", size);
        let title = if playlist.title.trim().is_empty() {
            "Плейлист"
        } else {
            &playlist.title
        };
        button.set_tooltip_text(Some(title));
        let content = tile_content();
        let cover = super::cover_widget(self.loader.as_ref(), playlist.cover.as_ref(), 196);
        cover.set_widget_name("music-playlist-tile-cover");
        resize_cover(&cover, size);
        cover.add_css_class("karin-playlist-cover");
        cover.set_halign(gtk::Align::Center);
        content.append(&cover);
        content.append(&tile_title(title, size));
        let count = track_count(playlist.track_count);
        let subtitle = gtk::Label::new(Some(&count));
        subtitle.set_xalign(0.0);
        subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
        subtitle.set_width_chars(1);
        subtitle.set_tooltip_text(Some(&count));
        subtitle.add_css_class("caption");
        subtitle.add_css_class("dim-label");
        content.append(&subtitle);
        button.set_child(Some(&content));
        button.update_property(&[gtk::accessible::Property::Label(&format!(
            "Открыть плейлист: {} · {count}",
            title
        ))]);
        let playlist = playlist.clone();
        let state = Rc::clone(&self.state);
        button.connect_clicked(move |_| {
            if state.generation.get() == generation {
                (state.action)(PlaylistAction::Open(playlist.clone()));
            }
        });
        button
    }

    fn create_tile(&self, generation: u64) -> gtk::Button {
        let size = self.cover_size.get();
        let button = tile_button("music-playlist-create", size);
        button.add_css_class("karin-playlist-create");
        button.set_tooltip_text(Some("Создать плейлист"));
        button.update_property(&[gtk::accessible::Property::Label("Создать плейлист")]);
        let content = tile_content();
        let icon = gtk::Image::from_icon_name("list-add-symbolic");
        icon.set_pixel_size(40);
        icon.set_size_request(size, size);
        icon.add_css_class("karin-playlist-cover");
        icon.set_halign(gtk::Align::Center);
        icon.add_css_class("accent");
        content.append(&icon);
        content.append(&tile_title("Создать плейлист", size));
        button.set_child(Some(&content));
        let state = Rc::clone(&self.state);
        button.connect_clicked(move |_| {
            if state.generation.get() == generation && state.create_allowed.get() {
                (state.action)(PlaylistAction::Create);
            }
        });
        button
    }
}

// Insets include the button/content padding and native FlowBoxChild wrapper.
// Leave explicit room for them when fitting two small-window columns.
const TILE_HORIZONTAL_BUDGET: i32 = 40;

fn cover_size_for_width(width: i32) -> i32 {
    match width {
        1100.. => 196,
        600.. => 160,
        300.. => ((width - 16) / 2 - TILE_HORIZONTAL_BUDGET).clamp(96, 160),
        _ => (width - TILE_HORIZONTAL_BUDGET).clamp(64, 160),
    }
}

fn resize_cover(cover: &gtk::Widget, size: i32) {
    cover.set_size_request(size, size);
    if let Some(fallback) = cover
        .downcast_ref::<gtk::Overlay>()
        .and_then(gtk::Overlay::child)
        .and_downcast::<gtk::Image>()
    {
        fallback.set_pixel_size((size / 2).max(16));
    } else if let Some(image) = cover.downcast_ref::<gtk::Image>()
        && cover.widget_name() == "music-playlist-tile-cover"
    {
        image.set_pixel_size((size / 2).max(16));
    }
}

fn resize_tiles(tiles: &gtk::FlowBox, size: i32) {
    let mut child = tiles.first_child();
    while let Some(wrapper) = child {
        child = wrapper.next_sibling();
        if let Some(button) = wrapper
            .downcast_ref::<gtk::FlowBoxChild>()
            .and_then(gtk::FlowBoxChild::child)
            .and_downcast::<gtk::Button>()
            && let Some(content) = button.child()
            && let Some(cover) = content.first_child()
        {
            button.set_width_request(size + 32);
            resize_cover(&cover, size);
            if let Some(title) = cover.next_sibling().and_downcast::<adw::Clamp>() {
                title.set_maximum_size(size);
                title.set_tightening_threshold(size);
            }
        }
    }
}

fn tile_button(name: &str, size: i32) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_widget_name(name);
    button.set_width_request(size + 32);
    button.set_hexpand(false);
    button.set_halign(gtk::Align::Start);
    button.set_valign(gtk::Align::Start);
    button.add_css_class("flat");
    button.add_css_class("karin-playlist-tile");
    button
}

fn tile_content() -> gtk::Box {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    super::super::margins(&content, 8);
    content
}

fn tile_title(title: &str, size: i32) -> adw::Clamp {
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_lines(2);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_width_chars(1);
    label.set_tooltip_text(Some(title));
    label.add_css_class("music-ellipsized-metadata");
    label.update_property(&[gtk::accessible::Property::Label(title)]);
    label.add_css_class("heading");
    // A character-count natural width can exceed the smaller cover even with
    // wrapping enabled. FlowBox then chooses one oversized column although
    // two cover-sized tiles fit the viewport. Bound native natural width in
    // actual logical pixels without changing font size, wrapping or semantics.
    let clamp = adw::Clamp::builder()
        .maximum_size(size)
        .tightening_threshold(size)
        .child(&label)
        .build();
    clamp.set_widget_name("music-playlist-title-clamp");
    clamp
}

fn track_count(count: u64) -> String {
    let word = if (11..=14).contains(&(count % 100)) {
        "треков"
    } else {
        match count % 10 {
            1 => "трек",
            2..=4 => "трека",
            _ => "треков",
        }
    };
    format!("{count} {word}")
}

#[cfg(test)]
mod tests;
