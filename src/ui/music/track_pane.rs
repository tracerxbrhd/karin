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
    playback_status: RefCell<String>,
    context: RefCell<Option<String>>,
    compact: Cell<bool>,
    serial: Cell<u64>,
    action: Rc<dyn Fn(PaneAction)>,
}

/// One view: its owner supplies the adaptive, explicitly closable presentation.
/// It never derives a stream, calls an API, or owns a separate player state.
pub(super) struct TrackPane {
    pub widget: gtk::Box,
    pub close: gtk::Button,
    pub play: gtk::Button,
    pub enqueue: gtk::Button,
    pub details: gtk::Button,
    pub album: gtk::Button,
    pub artists: gtk::Box,
    header: gtk::Box,
    heading: gtk::Label,
    state: Rc<State>,
    empty: gtk::Box,
    content: gtk::Box,
    cover: gtk::Box,
    title: gtk::Label,
    credits: gtk::Label,
    album_title: gtk::Label,
    status: gtk::Label,
}

impl TrackPane {
    pub fn new(action: Rc<dyn Fn(PaneAction)>) -> Self {
        let state = Rc::new(State {
            selected: RefCell::new(None),
            artists: RefCell::new(Vec::new()),
            playback_status: RefCell::new(String::new()),
            context: RefCell::new(None),
            compact: Cell::new(false),
            serial: Cell::new(0),
            action,
        });
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.set_widget_name("music-track-pane");
        widget.add_css_class("karin-track-pane");
        widget.set_vexpand(true);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        super::super::margins(&header, 6);
        header.add_css_class("karin-track-pane-header");
        let heading = label("О треке", "heading");
        heading.set_hexpand(true);
        header.append(&heading);
        let close = button("window-close-symbolic", "Закрыть сведения о треке");
        close.set_widget_name("music-track-pane-close");
        close.add_css_class("flat");
        close.add_css_class("circular");
        close.connect_clicked({
            let state = Rc::clone(&state);
            move |_| (state.action)(PaneAction::Close)
        });
        header.append(&close);
        widget.append(&header);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        super::super::margins(&body, 20);
        body.set_margin_top(12);
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let icon = gtk::Image::from_icon_name("audio-x-generic-symbolic");
        icon.set_pixel_size(48);
        icon.add_css_class("dim-label");
        icon.set_halign(gtk::Align::Start);
        empty.append(&icon);
        empty.append(&label("Выберите трек", "title-2"));
        body.append(&empty);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        content.add_css_class("karin-track-pane-content");
        content.set_visible(false);
        let cover = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        cover.set_halign(gtk::Align::Center);
        cover.set_widget_name("music-track-pane-cover");
        cover.add_css_class("karin-track-pane-cover");
        content.append(&cover);
        let metadata = gtk::Box::new(gtk::Orientation::Vertical, 6);
        metadata.add_css_class("karin-track-pane-metadata");
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
        let transport = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        transport.set_widget_name("music-track-pane-actions");
        transport.add_css_class("karin-track-pane-actions");
        let play = button("media-playback-start-symbolic", "Воспроизвести трек");
        play.set_widget_name("music-track-pane-play");
        play.add_css_class("suggested-action");
        play.add_css_class("circular");
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
        enqueue.add_css_class("flat");
        enqueue.add_css_class("circular");
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
        let details = button("view-more-symbolic", "Все действия с треком");
        details.set_widget_name("music-track-pane-details");
        details.add_css_class("flat");
        details.add_css_class("circular");
        details.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                let track = state.selected.borrow().clone();
                if let Some(track) = track {
                    (state.action)(PaneAction::Details(track));
                }
            }
        });
        transport.append(&details);
        let album = gtk::Button::new();
        album.set_widget_name("music-track-pane-album");
        album.set_visible(false);
        album.add_css_class("flat");
        album.add_css_class("karin-track-pane-link");
        album.set_halign(gtk::Align::Start);
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
        metadata.append(&album);
        let artists = gtk::Box::new(gtk::Orientation::Vertical, 0);
        artists.set_widget_name("music-track-pane-artist-links");
        artists.set_visible(false);
        metadata.insert_child_after(&artists, Some(&title));
        content.append(&transport);
        content.append(&status);
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
            header,
            heading,
            state,
            empty,
            content,
            cover,
            title,
            credits,
            album_title,
            status,
        }
    }

    pub fn selected(&self) -> Option<Track> {
        self.state.selected.borrow().clone()
    }

    pub fn set_compact(&self, compact: bool) {
        if self.state.compact.replace(compact) == compact {
            return;
        }
        if compact {
            self.header
                .reorder_child_after(&self.close, gtk::Widget::NONE);
        } else {
            self.header
                .reorder_child_after(&self.close, Some(&self.heading));
        }
        self.close.set_icon_name(if compact {
            "go-previous-symbolic"
        } else {
            "window-close-symbolic"
        });
        let label = if compact {
            "Назад к музыке"
        } else {
            "Закрыть сведения о треке"
        };
        self.close.set_tooltip_text(Some(label));
        self.close
            .update_property(&[gtk::accessible::Property::Label(label)]);
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
        self.state.playback_status.borrow_mut().clear();
        self.state.context.replace(None);
        self.update_description();
        let Some(track) = track else {
            self.title.set_text("");
            self.title.set_tooltip_text(None);
            self.credits.set_text("");
            self.album_title.set_text("");
            self.album.set_child(gtk::Widget::NONE);
            self.album.set_visible(false);
            self.album.set_tooltip_text(None);
            self.album
                .update_property(&[gtk::accessible::Property::Label("")]);
            return;
        };
        let image = super::cover_widget(loader, track.cover.as_ref(), 256);
        image.set_halign(gtk::Align::Center);
        self.cover.append(&image);
        self.title.set_text(&track.title);
        self.title.set_tooltip_text(Some(&track.title));
        self.credits.set_text(&track.artists.join(", "));
        self.credits.set_visible(!track.artists.is_empty());
        self.album_title
            .set_text(track.album_title.as_deref().unwrap_or(""));
        // The album appears once: a verified link when an ID is available,
        // otherwise honest display metadata with no inferred navigation.
        self.album_title
            .set_visible(track.album_id.is_none() && track.album_title.is_some());
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
        self.state.playback_status.replace(status.into());
        self.status.set_text(status);
        self.status
            .set_visible(selected && !available && !status.is_empty());
        self.play.set_sensitive(can_play && available);
        self.enqueue.set_sensitive(can_enqueue && selected);
        self.update_description();
    }

    pub fn set_context(&self, context: Option<&str>) {
        self.state
            .context
            .replace(context.filter(|text| !text.is_empty()).map(str::to_owned));
        self.update_description();
    }

    fn update_description(&self) {
        let description = if self.state.selected.borrow().is_some() {
            [
                self.state.playback_status.borrow().clone(),
                self.state.context.borrow().clone().unwrap_or_default(),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
        } else {
            String::new()
        };
        self.widget
            .update_property(&[gtk::accessible::Property::Description(&description)]);
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
            button.add_css_class("karin-track-pane-link");
            button.set_halign(gtk::Align::Start);
            button.set_widget_name("music-track-pane-artist-link");
            button.set_child(Some(&label(&artist.name, "body")));
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
        let unlinked = self
            .state
            .selected
            .borrow()
            .as_ref()
            .map(|track| {
                track
                    .artists
                    .iter()
                    .filter(|name| !artists.iter().any(|artist| &artist.name == *name))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.credits.set_text(&unlinked.join(", "));
        self.credits.set_visible(!unlinked.is_empty());
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
    row
}

fn clear(parent: &gtk::Box) {
    while let Some(child) = parent.first_child() {
        parent.remove(&child);
    }
}

#[cfg(test)]
mod tests;
