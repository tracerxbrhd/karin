// SPDX-License-Identifier: GPL-3.0-or-later
//! Native track information. Playback and catalog work stay in the controller.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;

use crate::{
    artwork::ArtworkLoader,
    music::{Artist, Track},
};

#[derive(Clone)]
pub(super) enum PaneAction {
    Close,
    Play(Track),
    Enqueue(Track),
    Details(Track),
    Album(String),
    Artist(Artist),
}

struct State {
    selected: RefCell<Option<Track>>,
    artists: RefCell<Vec<Artist>>,
    serial: Cell<u64>,
    action: Rc<dyn Fn(PaneAction)>,
}

/// One reparentable view: its owner supplies a wide sidebar or compact overlay.
/// It never derives a stream, calls an API, or owns a separate player state.
pub(super) struct TrackPane {
    pub widget: gtk::Box,
    pub close: gtk::Button,
    pub play: gtk::Button,
    pub enqueue: gtk::Button,
    pub details: gtk::Button,
    pub album: gtk::Button,
    pub artists: gtk::Box,
    state: Rc<State>,
    empty: gtk::Box,
    content: gtk::Box,
    cover: gtk::Box,
    title: gtk::Label,
    credits: gtk::Label,
    album_title: gtk::Label,
    status: gtk::Label,
    context: gtk::Label,
}

impl TrackPane {
    pub fn new(action: Rc<dyn Fn(PaneAction)>) -> Self {
        let state = Rc::new(State {
            selected: RefCell::new(None),
            artists: RefCell::new(Vec::new()),
            serial: Cell::new(0),
            action,
        });
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.set_widget_name("music-track-pane");
        widget.set_vexpand(true);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        super::super::margins(&header, 12);
        let heading = label("О треке", "heading");
        heading.set_hexpand(true);
        header.append(&heading);
        let close = button("window-close-symbolic", "Закрыть сведения о треке");
        close.set_widget_name("music-track-pane-close");
        close.add_css_class("flat");
        close.connect_clicked({
            let state = Rc::clone(&state);
            move |_| (state.action)(PaneAction::Close)
        });
        header.append(&close);
        widget.append(&header);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        super::super::margins(&body, 16);
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let icon = gtk::Image::from_icon_name("audio-x-generic-symbolic");
        icon.set_pixel_size(64);
        icon.add_css_class("dim-label");
        icon.set_halign(gtk::Align::Start);
        empty.append(&icon);
        empty.append(&label("Выберите трек", "title-2"));
        empty.append(&label(
            "Здесь появятся сведения о выбранной композиции.",
            "dim-label",
        ));
        body.append(&empty);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        content.set_visible(false);
        let cover = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        cover.set_halign(gtk::Align::Center);
        cover.set_widget_name("music-track-pane-cover");
        content.append(&cover);
        let metadata = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let title = label("", "title-2");
        title.set_widget_name("music-track-pane-title");
        title.set_selectable(true);
        metadata.append(&title);
        let credits = label("", "dim-label");
        credits.set_selectable(true);
        credits.set_widget_name("music-track-pane-artist");
        metadata.append(&credits);
        let album_title = label("", "caption");
        album_title.set_visible(false);
        metadata.append(&album_title);
        content.append(&metadata);
        let status = label("", "heading");
        status.set_widget_name("music-track-pane-state");
        status.set_visible(false);
        content.append(&status);
        let context = label("", "caption");
        context.set_widget_name("music-track-pane-context");
        context.set_visible(false);
        content.append(&context);

        let transport = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let play = gtk::Button::with_label("Воспроизвести");
        play.set_widget_name("music-track-pane-play");
        play.add_css_class("suggested-action");
        play.set_hexpand(true);
        play.set_sensitive(false);
        play.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                let track = state.selected.borrow().clone();
                if let Some(track) = track {
                    (state.action)(PaneAction::Play(track));
                }
            }
        });
        transport.append(&play);
        let enqueue = button("list-add-symbolic", "Добавить трек в очередь");
        enqueue.set_widget_name("music-track-pane-enqueue");
        enqueue.set_sensitive(false);
        enqueue.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                let track = state.selected.borrow().clone();
                if let Some(track) = track {
                    (state.action)(PaneAction::Enqueue(track));
                }
            }
        });
        transport.append(&enqueue);
        content.append(&transport);
        let details = gtk::Button::with_label("Подробности и действия");
        details.set_widget_name("music-track-pane-details");
        details.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                let track = state.selected.borrow().clone();
                if let Some(track) = track {
                    (state.action)(PaneAction::Details(track));
                }
            }
        });
        content.append(&details);
        let album = gtk::Button::new();
        album.set_widget_name("music-track-pane-album");
        album.set_visible(false);
        album.add_css_class("flat");
        album.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                let id = state
                    .selected
                    .borrow()
                    .as_ref()
                    .and_then(|track| track.album_id.clone());
                if let Some(id) = id {
                    (state.action)(PaneAction::Album(id));
                }
            }
        });
        content.append(&album);
        let artists = gtk::Box::new(gtk::Orientation::Vertical, 6);
        artists.set_widget_name("music-track-pane-artist-links");
        artists.set_visible(false);
        content.append(&artists);
        body.append(&content);
        let viewport = gtk::Viewport::builder()
            .child(&body)
            .scroll_to_focus(true)
            .build();
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .child(&viewport)
            .build();
        scroll.set_widget_name("music-track-pane-scroll");
        widget.append(&scroll);
        Self {
            widget,
            close,
            play,
            enqueue,
            details,
            album,
            artists,
            state,
            empty,
            content,
            cover,
            title,
            credits,
            album_title,
            status,
            context,
        }
    }

    pub fn selected(&self) -> Option<Track> {
        self.state.selected.borrow().clone()
    }

    /// Rebuilding only on metadata change avoids cancelling a cover every tick.
    pub fn set_track(&self, track: Option<&Track>, loader: Option<&ArtworkLoader>) {
        if self.state.selected.borrow().as_ref() == track {
            return;
        }
        self.state
            .serial
            .set(self.state.serial.get().wrapping_add(1));
        self.state.selected.replace(track.cloned());
        self.state.artists.borrow_mut().clear();
        clear(&self.cover);
        clear(&self.artists);
        self.artists.set_visible(false);
        self.empty.set_visible(track.is_none());
        self.content.set_visible(track.is_some());
        self.play.set_sensitive(false);
        self.enqueue.set_sensitive(false);
        self.status.set_visible(false);
        self.context.set_visible(false);
        let Some(track) = track else {
            self.title.set_text("");
            self.credits.set_text("");
            self.album_title.set_text("");
            self.album.set_child(gtk::Widget::NONE);
            self.album.set_visible(false);
            return;
        };
        let image = super::cover_widget(loader, track.cover.as_ref(), 224);
        image.set_halign(gtk::Align::Center);
        self.cover.append(&image);
        self.title.set_text(&track.title);
        self.credits.set_text(&track.artists.join(", "));
        self.credits.set_visible(!track.artists.is_empty());
        self.album_title
            .set_text(track.album_title.as_deref().unwrap_or(""));
        self.album_title.set_visible(track.album_title.is_some());
        self.album.set_visible(track.album_id.is_some());
        if track.album_id.is_some() {
            let title = track.album_title.as_deref().unwrap_or("Открыть альбом");
            let row = link_row("media-optical-symbolic", title);
            self.album.set_child(Some(&row));
            self.album.set_tooltip_text(Some("Открыть альбом"));
            self.album
                .update_property(&[gtk::accessible::Property::Label(&format!(
                    "Открыть альбом: {title}"
                ))]);
        }
        self.set_artists(&track.artist_links);
    }

    /// Status text/capabilities come from the existing player/session owner.
    pub fn set_playback(&self, status: &str, can_play: bool, can_enqueue: bool) {
        let (selected, available) = {
            let selected = self.state.selected.borrow();
            (
                selected.is_some(),
                selected.as_ref().is_some_and(|track| track.available),
            )
        };
        self.status.set_text(status);
        self.status.set_visible(selected && !status.is_empty());
        self.play.set_sensitive(can_play && available);
        self.enqueue.set_sensitive(can_enqueue && selected);
    }

    pub fn set_context(&self, context: Option<&str>) {
        self.context.set_text(context.unwrap_or(""));
        self.context.set_visible(
            self.state.selected.borrow().is_some() && context.is_some_and(|text| !text.is_empty()),
        );
    }

    pub fn set_navigation_enabled(&self, enabled: bool) {
        self.details.set_sensitive(enabled);
        self.album.set_sensitive(enabled);
        self.artists.set_sensitive(enabled);
    }

    /// Caller must supply verified catalog identities; names alone never link.
    pub fn set_artists(&self, artists: &[Artist]) {
        let artists = &artists[..artists.len().min(32)];
        if self.state.artists.borrow().as_slice() == artists {
            return;
        }
        self.state.artists.replace(artists.to_vec());
        self.state
            .serial
            .set(self.state.serial.get().wrapping_add(1));
        clear(&self.artists);
        let serial = self.state.serial.get();
        if self.state.selected.borrow().is_none() {
            self.artists.set_visible(false);
            return;
        }
        for artist in artists.iter().take(32) {
            let button = gtk::Button::new();
            button.add_css_class("flat");
            button.set_widget_name("music-track-pane-artist-link");
            button.set_child(Some(&link_row("avatar-default-symbolic", &artist.name)));
            button.set_tooltip_text(Some("Открыть исполнителя"));
            button.update_property(&[gtk::accessible::Property::Label(&format!(
                "Открыть исполнителя: {}",
                artist.name
            ))]);
            let artist = artist.clone();
            let state = Rc::clone(&self.state);
            button.connect_clicked(move |_| {
                if state.serial.get() == serial {
                    (state.action)(PaneAction::Artist(artist.clone()));
                }
            });
            self.artists.append(&button);
        }
        self.artists.set_visible(!artists.is_empty());
    }
}

fn label(value: &str, style: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(value));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_width_chars(1);
    label.add_css_class(style);
    label
}

fn button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    button
}

fn link_row(icon: &str, title: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&gtk::Image::from_icon_name(icon));
    let name = label(title, "body");
    name.set_hexpand(true);
    row.append(&name);
    row.append(&gtk::Image::from_icon_name("go-next-symbolic"));
    row
}

fn clear(parent: &gtk::Box) {
    while let Some(child) = parent.first_child() {
        parent.remove(&child);
    }
}

#[cfg(test)]
mod tests;
