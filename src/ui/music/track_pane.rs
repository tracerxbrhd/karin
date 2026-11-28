// SPDX-License-Identifier: GPL-3.0-or-later
//! Native track information. Playback and catalog work stay in the controller.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use gtk::gio;

use super::wrap_row::{RowAlignment, WrapRow};

use crate::{
    artwork::ArtworkLoader,
    music::{Artist, Track, wave::WaveSource},
};

#[derive(Clone)]
pub(super) enum PaneAction {
    Close,
    BackInfo,
    Play(Track),
    Wave(Track),
    Enqueue(Track),
    EnqueueNext(Track),
    AddToPlaylist(Track),
    CopyLink(Track),
    Lyrics(Track),
    OpenMenu(Track),
    DeliveryProbe(Track),
    Album(String),
    Artist(Artist),
    Extras(super::track_extras::ExtrasAction),
}

struct State {
    selected: RefCell<Option<Track>>,
    playback_status: RefCell<String>,
    context: RefCell<Option<String>>,
    compact: Cell<bool>,
    navigation_enabled: Cell<bool>,
    mode: Cell<super::workspace::PanelMode>,
    serial: Cell<u64>,
    action: Rc<dyn Fn(PaneAction)>,
}

/// One view: its owner supplies the adaptive, explicitly closable presentation.
/// It never derives a stream, calls an API, or owns a separate player state.
pub(super) struct TrackPane {
    pub widget: gtk::Box,
    pub close: gtk::Button,
    pub(super) back_info: gtk::Button,
    pub play: gtk::Button,
    pub enqueue: gtk::Button,
    pub menu: gtk::MenuButton,
    pub album: gtk::Button,
    pub cache_indicator: gtk::Box,
    pub(super) extras: super::track_extras::ExtrasView,
    header: gtk::CenterBox,
    pub(super) heading: gtk::Label,
    pages: gtk::Stack,
    state: Rc<State>,
    empty: gtk::Box,
    body: gtk::Box,
    content: gtk::Box,
    cover: gtk::Box,
    metadata: gtk::Box,
    transport: gtk::Box,
    #[cfg(test)]
    scroll: gtk::ScrolledWindow,
    art_size: Cell<i32>,
    title: gtk::Label,
    credits: gtk::Box,
    album_title: gtk::Label,
    status: gtk::Label,
    next_action: gio::SimpleAction,
    wave_action: gio::SimpleAction,
    enqueue_action: gio::SimpleAction,
    playlist_action: gio::SimpleAction,
    copy_link_action: gio::SimpleAction,
    lyrics_action: gio::SimpleAction,
    delivery_action: gio::SimpleAction,
    delivery_enabled: Cell<bool>,
    reactions: gtk::Box,
}

impl TrackPane {
    pub fn new(action: Rc<dyn Fn(PaneAction)>) -> Self {
        let state = Rc::new(State {
            selected: RefCell::new(None),
            playback_status: RefCell::new(String::new()),
            context: RefCell::new(None),
            compact: Cell::new(false),
            navigation_enabled: Cell::new(false),
            mode: Cell::new(super::workspace::PanelMode::TrackInfo),
            serial: Cell::new(0),
            action,
        });
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.set_widget_name("music-side-panel-surface");
        widget.add_css_class("karin-track-pane");
        widget.set_vexpand(true);
        let header = gtk::CenterBox::new();
        super::super::margins(&header, 6);
        header.set_margin_start(12);
        header.set_margin_end(12);
        header.add_css_class("karin-track-pane-header");
        let back_info = button("karin-chevron-left-symbolic", "Назад к сведениям о треке");
        back_info.set_widget_name("music-lyrics-back-info");
        back_info.add_css_class("flat");
        back_info.add_css_class("circular");
        back_info.set_visible(false);
        back_info.connect_clicked({
            let state = Rc::clone(&state);
            move |_| (state.action)(PaneAction::BackInfo)
        });
        header.set_start_widget(Some(&back_info));
        let heading = gtk::Label::new(Some("О треке"));
        heading.set_widget_name("music-panel-heading");
        heading.add_css_class("heading");
        heading.set_xalign(0.5);
        heading.set_margin_start(8);
        heading.set_margin_end(8);
        header.set_center_widget(Some(&heading));
        let close = button("karin-x-symbolic", "Закрыть сведения о треке");
        close.set_widget_name("music-panel-close");
        close.add_css_class("flat");
        close.add_css_class("circular");
        close.connect_clicked({
            let state = Rc::clone(&state);
            move |_| (state.action)(PaneAction::Close)
        });
        header.set_end_widget(Some(&close));
        widget.append(&header);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        super::super::margins(&body, 20);
        body.set_margin_top(12);
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let icon = gtk::Image::from_icon_name("karin-music-symbolic");
        icon.set_pixel_size(48);
        icon.add_css_class("dim-label");
        icon.set_halign(gtk::Align::Center);
        empty.append(&icon);
        let empty_title = label("Выберите трек", "title-2");
        empty_title.set_xalign(0.5);
        empty.append(&empty_title);
        body.append(&empty);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
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
        title.set_xalign(0.5);
        title.set_justify(gtk::Justification::Center);
        title.set_widget_name("music-track-pane-title");
        title.set_selectable(true);
        let title_line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        title_line.set_halign(gtk::Align::Center);
        let cache_indicator = super::cache_presence::indicator("music-track-pane-cache");
        title_line.append(&title);
        title_line.append(&cache_indicator);
        metadata.append(&title_line);
        let credits = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        credits.set_widget_name("music-track-pane-artist");
        credits.set_layout_manager(Some(WrapRow::new(RowAlignment::Center, 0, 0)));
        credits.set_visible(false);
        metadata.append(&credits);
        let album_title = label("", "caption");
        album_title.set_xalign(0.5);
        album_title.set_justify(gtk::Justification::Center);
        album_title.set_visible(false);
        metadata.append(&album_title);
        content.append(&metadata);
        let status = label("", "heading");
        status.set_xalign(0.5);
        status.set_justify(gtk::Justification::Center);
        status.set_widget_name("music-track-pane-state");
        status.add_css_class("karin-font-secondary");
        status.set_visible(false);
        let transport = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        transport.set_halign(gtk::Align::Center);
        transport.set_widget_name("music-track-pane-actions");
        transport.add_css_class("karin-track-pane-actions");
        let play = button("karin-player-play-symbolic", "Воспроизвести трек");
        play.set_widget_name("music-track-pane-play");
        play.add_css_class("suggested-action");
        play.add_css_class("circular");
        play.set_size_request(44, 44);
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
        let enqueue = button("karin-plus-symbolic", "Добавить трек в очередь");
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
        let menu = gtk::MenuButton::builder()
            .icon_name("karin-dots-vertical-symbolic")
            .build();
        menu.set_widget_name("music-track-pane-menu");
        menu.set_tooltip_text(Some("Действия с треком"));
        menu.update_property(&[gtk::accessible::Property::Label("Действия с треком")]);
        menu.add_css_class("flat");
        menu.add_css_class("circular");
        menu.set_sensitive(false);
        let group = gio::SimpleActionGroup::new();
        let next_action = track_action(&group, &state, "next", PaneAction::EnqueueNext);
        let wave_action = track_action(&group, &state, "wave", PaneAction::Wave);
        let enqueue_action = track_action(&group, &state, "enqueue", PaneAction::Enqueue);
        let playlist_action = track_action(&group, &state, "playlist", PaneAction::AddToPlaylist);
        let copy_link_action = track_action(&group, &state, "copy-link", PaneAction::CopyLink);
        let lyrics_action = track_action(&group, &state, "lyrics", PaneAction::Lyrics);
        let delivery_action = track_action(&group, &state, "delivery", PaneAction::DeliveryProbe);
        menu.insert_action_group("track-pane", Some(&group));
        let popover = gtk::PopoverMenu::from_model(Some(&menu_model(false)));
        let reactions = gtk::Box::new(gtk::Orientation::Vertical, 4);
        reactions.set_widget_name("music-track-pane-reactions");
        assert!(popover.add_child(&reactions, "track-reactions"));
        popover.connect_map({
            let state = Rc::clone(&state);
            move |_| {
                let selected = state.selected.borrow().clone();
                if let Some(track) = selected {
                    (state.action)(PaneAction::OpenMenu(track));
                }
            }
        });
        menu.set_popover(Some(&popover));
        transport.append(&menu);
        let album = gtk::Button::new();
        album.set_widget_name("music-track-pane-album");
        album.set_visible(false);
        album.add_css_class("flat");
        album.add_css_class("karin-track-pane-link");
        album.set_halign(gtk::Align::Center);
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
        content.append(&transport);
        content.append(&status);
        let extras = super::track_extras::ExtrasView::new(Rc::new({
            let state = Rc::clone(&state);
            move |action| (state.action)(PaneAction::Extras(action))
        }));
        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        separator.set_margin_top(4);
        separator.set_margin_bottom(4);
        content.append(&separator);
        content.append(&extras.widget);
        body.append(&content);
        // The same musical cluster remains bounded and centered when the
        // pinned sidebar becomes a dedicated compact information view.
        let cluster = adw::Clamp::builder()
            .maximum_size(384)
            .tightening_threshold(320)
            .child(&body)
            .build();
        cluster.set_widget_name("music-track-pane-cluster");
        let viewport = gtk::Viewport::builder()
            .child(&cluster)
            .scroll_to_focus(true)
            .build();
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .child(&viewport)
            .build();
        scroll.set_widget_name("music-track-pane-scroll");
        let pages = gtk::Stack::new();
        pages.set_widget_name("music-panel-content");
        pages.set_hhomogeneous(false);
        pages.set_vhomogeneous(false);
        pages.set_transition_type(gtk::StackTransitionType::Crossfade);
        pages.set_transition_duration(100);
        pages.add_named(&scroll, Some("track"));
        pages.add_named(&extras.lyrics_widget, Some("lyrics"));
        pages.set_vexpand(true);
        widget.append(&pages);
        Self {
            widget,
            close,
            back_info,
            play,
            enqueue,
            menu,
            album,
            cache_indicator,
            extras,
            header,
            heading,
            pages,
            state,
            empty,
            body,
            content,
            cover,
            metadata,
            transport,
            #[cfg(test)]
            scroll,
            art_size: Cell::new(224),
            title,
            credits,
            album_title,
            status,
            next_action,
            wave_action,
            enqueue_action,
            playlist_action,
            copy_link_action,
            lyrics_action,
            delivery_action,
            delivery_enabled: Cell::new(false),
            reactions,
        }
    }

    /// Queue and information share this exact surface and header.
    pub(super) fn attach_queue(&self, queue: &impl IsA<gtk::Widget>) {
        self.pages.add_named(queue, Some("queue"));
    }

    pub(super) fn set_mode(&self, mode: super::workspace::PanelMode) {
        use super::workspace::PanelMode;
        if self.state.mode.replace(mode) == mode {
            return;
        }
        self.back_info.set_visible(mode == PanelMode::Lyrics);
        self.heading.set_text(match mode {
            PanelMode::Lyrics => "Текст песни",
            PanelMode::Queue => "Очередь",
            PanelMode::Closed | PanelMode::TrackInfo => "О треке",
        });
        self.pages.set_visible_child_name(match mode {
            PanelMode::Lyrics => "lyrics",
            PanelMode::Queue => "queue",
            PanelMode::Closed | PanelMode::TrackInfo => "track",
        });
        self.update_header();
    }

    pub fn selected(&self) -> Option<Track> {
        self.state.selected.borrow().clone()
    }

    pub(super) fn set_delivery_enabled(&self, enabled: bool) {
        if self.delivery_enabled.replace(enabled) != enabled
            && let Some(popover) = self.menu.popover().and_downcast::<gtk::PopoverMenu>()
        {
            self.menu.popdown();
            popover.remove_child(&self.reactions);
            popover.set_menu_model(Some(&menu_model(enabled)));
            assert!(popover.add_child(&self.reactions, "track-reactions"));
        }
        self.delivery_action.set_enabled(
            enabled && self.state.navigation_enabled.get() && self.selected().is_some(),
        );
    }

    pub(super) fn set_reaction_controls(&self, controls: &gtk::Box) {
        clear(&self.reactions);
        self.reactions.append(controls);
    }

    pub fn set_compact(&self, compact: bool) {
        if self.state.compact.replace(compact) == compact {
            return;
        }
        self.update_header();
    }

    fn update_header(&self) {
        let compact = self.state.compact.get();
        let lyrics = self.state.mode.get() == super::workspace::PanelMode::Lyrics;
        let use_back = compact && !lyrics;
        let close_at_end = !use_back;
        if self.header.end_widget().is_some() != close_at_end {
            let close_focused = self.close.has_focus();
            if use_back {
                self.header.set_end_widget(gtk::Widget::NONE);
                self.header.set_start_widget(Some(&self.close));
            } else {
                self.header.set_start_widget(Some(&self.back_info));
                self.header.set_end_widget(Some(&self.close));
            }
            if close_focused {
                self.close.grab_focus();
            }
        }
        // The compact native HeaderBar names this explicit subview. Keep one
        // moving Back control here, without repeating its title below it.
        self.heading.set_visible(!compact);
        self.close.set_icon_name(if use_back {
            "karin-chevron-left-symbolic"
        } else {
            "karin-x-symbolic"
        });
        let label = if use_back {
            "Назад к музыке"
        } else {
            "Закрыть правую панель"
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
        self.menu.popdown();
        clear(&self.reactions);
        let cache_track_changed = self.state.selected.borrow().as_ref().map(|track| &track.id)
            != track.map(|track| &track.id);
        self.state.selected.replace(track.cloned());
        self.update_credits();
        self.menu
            .set_sensitive(track.is_some() && self.state.navigation_enabled.get());
        self.extras.set_track(track, loader);
        if cache_track_changed {
            self.cache_indicator.set_visible(false);
        }
        clear(&self.cover);
        self.empty.set_visible(track.is_none());
        self.content.set_visible(track.is_some());
        self.play.set_sensitive(false);
        self.enqueue.set_sensitive(false);
        self.next_action.set_enabled(false);
        self.wave_action.set_enabled(
            self.state.navigation_enabled.get() && track.and_then(WaveSource::from_track).is_some(),
        );
        self.enqueue_action.set_enabled(false);
        self.playlist_action
            .set_enabled(track.is_some() && self.state.navigation_enabled.get());
        self.copy_link_action.set_enabled(
            self.state.navigation_enabled.get()
                && track.and_then(super::sharing::track_link).is_some(),
        );
        self.lyrics_action
            .set_enabled(track.is_some() && self.state.navigation_enabled.get());
        self.delivery_action.set_enabled(
            track.is_some() && self.state.navigation_enabled.get() && self.delivery_enabled.get(),
        );
        self.status.set_visible(false);
        self.state.playback_status.borrow_mut().clear();
        self.state.context.replace(None);
        self.update_description();
        let Some(track) = track else {
            self.title.set_text("");
            self.title.set_tooltip_text(None);
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
        self.resize_existing_art();
        self.title.set_text(&track.title);
        self.title.set_tooltip_text(Some(&track.title));
        self.album_title
            .set_text(track.album_title.as_deref().unwrap_or(""));
        // The album appears once: a verified link when an ID is available,
        // otherwise honest display metadata with no inferred navigation.
        self.album_title
            .set_visible(track.album_id.is_none() && track.album_title.is_some());
        self.album.set_visible(track.album_id.is_some());
        if track.album_id.is_some() {
            let title = track.album_title.as_deref().unwrap_or("Открыть альбом");
            let row = link_row("karin-disc-symbolic", title);
            self.album.set_child(Some(&row));
            self.album.set_tooltip_text(Some("Открыть альбом"));
            self.album
                .update_property(&[gtk::accessible::Property::Label(&format!(
                    "Открыть альбом: {title}"
                ))]);
        }
    }

    /// Budget artwork against the actual panel viewport, retaining natural
    /// text/action sizes. Long metadata still scrolls once the art reaches its
    /// useful minimum. Resizing never recreates a thumbnail or starts a fetch.
    pub(super) fn set_available_size(&self, width: i32, height: i32) {
        if width <= 0 || height <= 0 || self.state.selected.borrow().is_none() {
            return;
        }
        let inner_width =
            (width.min(384) - self.body.margin_start() - self.body.margin_end()).max(1);
        let natural_height = |widget: &gtk::Widget, width| {
            let (minimum, natural, _, _) = widget.measure(gtk::Orientation::Vertical, width);
            minimum.max(natural)
        };
        let header = natural_height(self.header.upcast_ref(), width);
        let mut non_art_height = 0;
        let mut gaps = 0;
        for widget in [
            self.metadata.upcast_ref::<gtk::Widget>(),
            self.transport.upcast_ref(),
            self.status.upcast_ref(),
        ] {
            if widget.is_visible() {
                non_art_height += natural_height(widget, inner_width);
                // Cover precedes each visible non-art child in this Box.
                gaps += 1;
            }
        }
        let available = height
            - header
            - self.body.margin_top()
            - self.body.margin_bottom()
            - non_art_height
            - gaps * self.content.spacing();
        let maximum = inner_width.clamp(96, 224).min((height * 3 / 10).max(96));
        let size = available.clamp(96, maximum);
        if self.art_size.replace(size) != size {
            self.resize_existing_art();
        }
    }

    fn resize_existing_art(&self) {
        let Some(image) = self.cover.first_child() else {
            return;
        };
        let size = self.art_size.get();
        image.set_size_request(size, size);
        let fallback = image
            .downcast_ref::<gtk::Overlay>()
            .and_then(gtk::Overlay::child)
            .and_downcast::<gtk::Image>()
            .or_else(|| image.clone().downcast::<gtk::Image>().ok());
        if let Some(fallback) = fallback {
            fallback.set_pixel_size((size / 2).max(16));
        }
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
        self.enqueue_action.set_enabled(can_enqueue && selected);
        self.next_action.set_enabled(can_enqueue && available);
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
        self.state.navigation_enabled.set(enabled);
        if !enabled {
            self.next_action.set_enabled(false);
            self.enqueue_action.set_enabled(false);
        }
        self.menu
            .set_sensitive(enabled && self.selected().is_some());
        self.wave_action.set_enabled(
            enabled
                && self
                    .state
                    .selected
                    .borrow()
                    .as_ref()
                    .and_then(WaveSource::from_track)
                    .is_some(),
        );
        self.playlist_action
            .set_enabled(enabled && self.selected().is_some());
        self.copy_link_action.set_enabled(
            enabled
                && self
                    .state
                    .selected
                    .borrow()
                    .as_ref()
                    .and_then(super::sharing::track_link)
                    .is_some(),
        );
        self.lyrics_action
            .set_enabled(enabled && self.selected().is_some());
        self.delivery_action
            .set_enabled(enabled && self.delivery_enabled.get() && self.selected().is_some());
        self.album.set_sensitive(enabled);
        let mut child = self.credits.first_child();
        while let Some(credit) = child {
            child = credit.next_sibling();
            if let Some(button) = credit.downcast_ref::<gtk::Button>() {
                button.set_sensitive(enabled);
            }
        }
    }

    fn update_credits(&self) {
        let serial = self.state.serial.get().wrapping_add(1);
        self.state.serial.set(serial);
        clear(&self.credits);
        let credits = self
            .state
            .selected
            .borrow()
            .as_ref()
            .map(artist_credits)
            .unwrap_or_default();
        self.credits.set_visible(!credits.is_empty());
        let count = credits.len();
        for (index, (name, artist)) in credits.into_iter().enumerate() {
            let display = if index + 1 == count {
                name.clone()
            } else {
                format!("{name}, ")
            };
            let credit = label(&display, "karin-font-secondary");
            credit.set_valign(gtk::Align::Center);
            if let Some(artist) = artist {
                let attributes = gtk::pango::AttrList::new();
                let mut underline =
                    gtk::pango::AttrInt::new_underline(gtk::pango::Underline::Single);
                underline.set_end_index(u32::try_from(name.len()).unwrap_or(u32::MAX));
                attributes.insert(underline);
                credit.set_attributes(Some(&attributes));
                let button = gtk::Button::new();
                button.set_child(Some(&credit));
                button.set_widget_name("music-track-pane-artist-link");
                button.add_css_class("flat");
                button.add_css_class("karin-track-pane-artist-link");
                button.set_valign(gtk::Align::Center);
                button.set_tooltip_text(Some("Открыть исполнителя"));
                button.update_property(&[gtk::accessible::Property::Label(&format!(
                    "Открыть исполнителя: {name}"
                ))]);
                button.set_sensitive(self.state.navigation_enabled.get());
                let state = Rc::clone(&self.state);
                button.connect_clicked(move |_| {
                    if state.serial.get() == serial && state.navigation_enabled.get() {
                        (state.action)(PaneAction::Artist(artist.clone()));
                    }
                });
                self.credits.append(&button);
            } else {
                credit.add_css_class("dim-label");
                credit.set_selectable(true);
                self.credits.append(&credit);
            }
        }
    }
}

const ARTIST_LINK_LIMIT: usize = 32;

fn artist_credits(track: &Track) -> Vec<(String, Option<Artist>)> {
    let verified = track.artist_links.iter().take(ARTIST_LINK_LIMIT);
    let mut included = [false; ARTIST_LINK_LIMIT];
    let mut credits = Vec::with_capacity(track.artists.len());
    for name in &track.artists {
        // The service's display credits supply the order. Consume each
        // verified identity once, even when a name occurs more than once.
        let artist = verified
            .clone()
            .enumerate()
            .find(|(index, artist)| !included[*index] && artist.name == *name);
        if let Some((index, _)) = artist {
            included[index] = true;
        }
        credits.push((name.clone(), artist.map(|(_, artist)| artist.clone())));
    }
    // Older cached tracks can carry a verified artist without its display
    // credit. Keep that identity browsable after the ordered credits.
    credits.extend(
        verified
            .enumerate()
            .filter(|(index, _)| !included[*index])
            .map(|(_, artist)| (artist.name.clone(), Some(artist.clone()))),
    );
    credits
}

fn track_action(
    group: &gio::SimpleActionGroup,
    state: &Rc<State>,
    name: &str,
    command: fn(Track) -> PaneAction,
) -> gio::SimpleAction {
    let action = gio::SimpleAction::new(name, None);
    action.set_enabled(false);
    action.connect_activate({
        let state = Rc::clone(state);
        move |_, _| {
            let selected = state.selected.borrow().clone();
            if let Some(track) = selected {
                (state.action)(command(track));
            }
        }
    });
    group.add_action(&action);
    action
}

fn menu_model(delivery: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    let queue = gio::Menu::new();
    queue.append(
        Some(super::radio::TRACK_WAVE_LABEL),
        Some("track-pane.wave"),
    );
    queue.append(Some("Воспроизвести следующим"), Some("track-pane.next"));
    queue.append(Some("Добавить в очередь"), Some("track-pane.enqueue"));
    queue.append(Some("Добавить в плейлист"), Some("track-pane.playlist"));
    menu.append_section(None, &queue);
    let information = gio::Menu::new();
    information.append(Some("Копировать ссылку"), Some("track-pane.copy-link"));
    information.append(Some("Показать текст песни"), Some("track-pane.lyrics"));
    if delivery {
        information.append(
            Some("Проверить доставку аудио"),
            Some("track-pane.delivery"),
        );
    }
    menu.append_section(None, &information);
    let item = gio::MenuItem::new(None, None);
    item.set_attribute_value("custom", Some(&"track-reactions".to_variant()));
    menu.append_item(&item);
    menu
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
