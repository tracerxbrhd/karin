// SPDX-License-Identifier: GPL-3.0-or-later
//! Expanded presentation of the shared playback session.

use std::{cell::Cell, rc::Rc, time::Duration};

use adw::prelude::*;
use gtk::glib;

use super::{
    icon_button,
    mini::{MiniCommand, MiniSnapshot},
    mini_time, player_bar,
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Section {
    #[default]
    Player,
    Info,
    Lyrics,
    Queue,
}

impl Section {
    const ALL: [Self; 4] = [Self::Player, Self::Info, Self::Lyrics, Self::Queue];

    fn tab(self) -> (&'static str, &'static str) {
        match self {
            Self::Player => ("player", "Плеер"),
            Self::Info => ("info", "О треке"),
            Self::Lyrics => ("lyrics", "Текст"),
            Self::Queue => ("queue", "Очередь"),
        }
    }
}

pub(super) enum Action {
    Back,
    Section(Section),
    Playback(MiniCommand),
}

pub(super) struct NowPlaying {
    pub(super) widget: gtk::Box,
    pub(super) panel_host: gtk::Box,
    pub(super) reactions_slot: gtk::Box,
    back: gtk::Button,
    tabs: [gtk::ToggleButton; 4],
    player: gtk::ScrolledWindow,
    section: Rc<Cell<Section>>,
    compact: Rc<Cell<bool>>,
    artwork: gtk::Picture,
    title: gtk::Label,
    artist: gtk::Label,
    status: gtk::Label,
    position: gtk::Label,
    duration: gtk::Label,
    progress: gtk::Scale,
    previous: gtk::Button,
    play: gtk::Button,
    next: gtk::Button,
    shuffle: gtk::ToggleButton,
    repeat: gtk::Button,
    volume: gtk::Scale,
}

impl NowPlaying {
    pub(super) fn new(on_action: impl Fn(Action) + 'static) -> Self {
        let on_action = Rc::new(on_action);
        let section = Rc::new(Cell::new(Section::Player));
        let compact = Rc::new(Cell::new(false));
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.set_widget_name("music-now-playing");
        widget.add_css_class("music-now-playing");
        widget.add_css_class("karin-font-primary");
        widget.set_hexpand(true);
        widget.set_vexpand(true);

        let header = gtk::CenterBox::new();
        header.add_css_class("music-now-playing-header");
        header.set_margin_start(12);
        header.set_margin_end(12);
        header.set_margin_top(6);
        let (back_content, _) = super::search_header::back_button_content();
        let back = gtk::Button::builder()
            .child(&back_content)
            .tooltip_text("Вернуться к музыке")
            .has_frame(false)
            .build();
        back.set_widget_name("music-now-playing-back");
        back.add_css_class("flat");
        back.add_css_class("music-back-button");
        back.update_property(&[gtk::accessible::Property::Label("Назад")]);
        let start_controls = gtk::WindowControls::new(gtk::PackType::Start);
        start_controls.set_widget_name("music-now-playing-window-controls-start");
        super::super::bind_window_decoration_layout(&start_controls);
        let start = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        start.append(&start_controls);
        start.append(&back);
        header.set_start_widget(Some(&start));
        let heading = gtk::Label::new(Some("Сейчас играет"));
        heading.add_css_class("heading");
        header.set_center_widget(Some(&heading));
        let end_controls = gtk::WindowControls::new(gtk::PackType::End);
        end_controls.set_widget_name("music-now-playing-window-controls-end");
        super::super::bind_window_decoration_layout(&end_controls);
        header.set_end_widget(Some(&end_controls));
        let handle = gtk::WindowHandle::new();
        handle.set_widget_name("music-now-playing-window-handle");
        handle.set_child(Some(&header));
        widget.append(&handle);

        let tab_row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        tab_row.set_halign(gtk::Align::Center);
        tab_row.add_css_class("music-now-playing-tabs");
        tab_row.set_widget_name("music-now-playing-tabs");
        let tabs = Section::ALL.map(|section| {
            let (id, label) = section.tab();
            let button = gtk::ToggleButton::with_label(label);
            button.set_widget_name(&format!("music-now-playing-{id}"));
            button.add_css_class("flat");
            tab_row.append(&button);
            let action = Rc::clone(&on_action);
            // `toggled` also fires on set_section. Only a native activation
            // of the button should request a controller route change.
            button.connect_clicked(move |_| action(Action::Section(section)));
            button
        });
        for button in &tabs[1..] {
            button.set_group(Some(&tabs[0]));
        }
        tabs[0].set_active(true);
        widget.append(&tab_row);

        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let width_layout = adw::BreakpointBin::new();
        let height_layout = adw::BreakpointBin::new();
        for layout in [&width_layout, &height_layout] {
            layout.set_size_request(256, 160);
        }
        let body_clamp = adw::Clamp::builder()
            .maximum_size(1080)
            .tightening_threshold(1000)
            .child(&body)
            .build();
        // Each bin applies one breakpoint. Keep the width and height rules
        // independent so a short, narrow window still uses the compact pane.
        height_layout.set_child(Some(&body_clamp));
        width_layout.set_child(Some(&height_layout));
        width_layout.set_vexpand(true);
        widget.append(&width_layout);

        let cover = gtk::Overlay::new();
        cover.add_css_class("music-now-playing-cover");
        cover.set_overflow(gtk::Overflow::Hidden);
        let fallback = gtk::Image::from_icon_name("karin-music-symbolic");
        fallback.set_pixel_size(54);
        cover.set_child(Some(&fallback));
        let artwork = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .can_shrink(true)
            .visible(false)
            .build();
        artwork.set_widget_name("music-now-playing-artwork");
        artwork.update_property(&[gtk::accessible::Property::Label("Обложка текущего трека")]);
        cover.add_overlay(&artwork);
        cover.set_measure_overlay(&artwork, false);
        let cover_frame = gtk::AspectFrame::new(0.5, 0.5, 1.0, false);
        cover_frame.set_height_request(376);
        cover_frame.set_child(Some(&cover));
        cover_frame.set_widget_name("music-now-playing-cover");

        let title = track_label("music-now-playing-title", "music-now-playing-title", 2);
        let artist = track_label("music-now-playing-artist", "dim-label", 2);
        let metadata = gtk::Box::new(gtk::Orientation::Vertical, 3);
        metadata.append(&title);
        metadata.append(&artist);

        let progress = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
        progress.set_widget_name("music-now-playing-progress");
        progress.set_draw_value(false);
        progress.update_property(&[gtk::accessible::Property::Label("Позиция воспроизведения")]);
        let position = time_label("music-now-playing-position");
        let duration = time_label("music-now-playing-duration");
        let times = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        position.set_hexpand(true);
        position.set_xalign(0.0);
        times.append(&position);
        times.append(&duration);
        let timeline = gtk::Box::new(gtk::Orientation::Vertical, 0);
        timeline.add_css_class("music-now-playing-timeline");
        timeline.append(&progress);
        timeline.append(&times);

        let previous = transport_button(
            "music-now-playing-previous",
            "karin-player-skip-back-symbolic",
            "Предыдущий трек",
        );
        let play = gtk::Button::new();
        play.set_widget_name("music-now-playing-play");
        play.set_size_request(58, 58);
        play.set_valign(gtk::Align::Center);
        play.add_css_class("circular");
        play.add_css_class("suggested-action");
        let play_face = gtk::Overlay::new();
        let play_icon = gtk::Image::from_icon_name("karin-player-play-symbolic");
        play_icon.set_pixel_size(22);
        play_face.set_child(Some(&play_icon));
        let loading = gtk::Spinner::builder()
            .width_request(20)
            .height_request(20)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .can_target(false)
            .visible(false)
            .build();
        play_face.add_overlay(&loading);
        play.set_child(Some(&play_face));
        let next = transport_button(
            "music-now-playing-next",
            "karin-player-skip-forward-symbolic",
            "Следующий трек",
        );
        let shuffle = gtk::ToggleButton::new();
        shuffle.set_widget_name("music-now-playing-shuffle");
        shuffle.set_icon_name("karin-arrows-shuffle-symbolic");
        shuffle.set_size_request(44, 44);
        shuffle.set_valign(gtk::Align::Center);
        shuffle.add_css_class("flat");
        shuffle.add_css_class("circular");
        let repeat = player_bar::repeat_button();
        repeat.set_widget_name("music-now-playing-repeat");
        repeat.set_size_request(44, 44);
        repeat.set_valign(gtk::Align::Center);
        repeat.add_css_class("flat");
        repeat.add_css_class("circular");
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        controls.set_halign(gtk::Align::Center);
        controls.add_css_class("music-transport");
        controls.append(&shuffle);
        controls.append(&previous);
        controls.append(&play);
        controls.append(&next);
        controls.append(&repeat);

        let reactions_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        reactions_slot.set_widget_name("music-now-playing-reactions");
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        volume.set_widget_name("music-now-playing-volume");
        volume.set_draw_value(false);
        volume.set_width_request(96);
        volume.set_hexpand(true);
        volume.set_tooltip_text(Some("Громкость"));
        volume.update_property(&[gtk::accessible::Property::Label("Громкость")]);
        let volume_readout = crate::ui::volume_readout::new(&volume);
        let speaker = gtk::Image::from_icon_name("karin-volume-symbolic");
        speaker.set_pixel_size(16);
        speaker.add_css_class("dim-label");
        let loudness = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        loudness.append(&speaker);
        loudness.append(&volume);
        loudness.append(&volume_readout);
        let additional = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        additional.add_css_class("music-now-playing-additional-controls");
        additional.set_halign(gtk::Align::Center);
        additional.append(&reactions_slot);
        additional.append(&loudness);

        let status = track_label("music-now-playing-status", "dim-label", 2);
        status.add_css_class("caption");
        let column = gtk::Box::new(gtk::Orientation::Vertical, 10);
        column.set_margin_top(10);
        column.set_margin_bottom(14);
        column.set_margin_start(18);
        column.set_margin_end(18);
        column.set_valign(gtk::Align::Center);
        column.append(&cover_frame);
        column.append(&metadata);
        column.append(&timeline);
        column.append(&controls);
        column.append(&additional);
        column.append(&status);
        let clamp = adw::Clamp::builder()
            .maximum_size(456)
            .tightening_threshold(362)
            .child(&column)
            .build();
        let viewport = gtk::Viewport::builder()
            .child(&clamp)
            .scroll_to_focus(true)
            .build();
        let player = gtk::ScrolledWindow::builder()
            .child(&viewport)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        player.set_widget_name("music-now-playing-transport-view");
        body.append(&player);

        let panel_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        panel_host.set_widget_name("music-now-playing-panel");
        panel_host.add_css_class("music-now-playing-panel");
        panel_host.set_vexpand(true);
        body.append(&panel_host);

        // A narrower pane gives the active detail the full body. At wider
        // sizes, transport stays in view next to the same real TrackPane.
        let narrow = breakpoint(adw::BreakpointConditionLengthType::MaxWidth, 740.0);
        narrow.connect_apply({
            let (compact, section) = (Rc::clone(&compact), Rc::clone(&section));
            let (player, panel) = (player.downgrade(), panel_host.downgrade());
            let tabs = tabs.each_ref().map(gtk::ToggleButton::downgrade);
            move |_| {
                compact.set(true);
                if let (Some(player), Some(panel)) = (player.upgrade(), panel.upgrade()) {
                    let section = section.get();
                    let tab = tabs[section as usize].upgrade();
                    show_section(&player, &panel, section, true, tab.as_ref());
                }
            }
        });
        narrow.connect_unapply({
            let (compact, section) = (Rc::clone(&compact), Rc::clone(&section));
            let (player, panel) = (player.downgrade(), panel_host.downgrade());
            move |_| {
                compact.set(false);
                if let (Some(player), Some(panel)) = (player.upgrade(), panel.upgrade()) {
                    show_section(&player, &panel, section.get(), false, None);
                }
            }
        });
        width_layout.add_breakpoint(narrow);
        // Install the tighter rules last, giving the playback buttons room
        // even when the track title and artist each wrap onto two lines.
        for (height, artwork) in [(625.0, 272_i32), (525.0, 202), (460.0, 144)] {
            let short = breakpoint(adw::BreakpointConditionLengthType::MaxHeight, height);
            short.add_setter(&cover_frame, "height-request", Some(&artwork.to_value()));
            height_layout.add_breakpoint(short);
        }

        for (button, command) in [
            (&previous, MiniCommand::Previous),
            (&play, MiniCommand::PlayPause),
            (&next, MiniCommand::Next),
            (&repeat, MiniCommand::Repeat),
        ] {
            let action = Rc::clone(&on_action);
            button.connect_clicked(move |_| action(Action::Playback(command)));
        }
        let seek_action = Rc::clone(&on_action);
        progress.connect_change_value(move |slider, _, value| {
            if slider.is_sensitive()
                && value.is_finite()
                && let Ok(position) =
                    Duration::try_from_secs_f64(value.clamp(0.0, slider.adjustment().upper()))
            {
                seek_action(Action::Playback(MiniCommand::Seek(position)));
            }
            glib::Propagation::Proceed
        });
        let shuffle_action = Rc::clone(&on_action);
        shuffle.connect_clicked(move |button| {
            shuffle_action(Action::Playback(MiniCommand::Shuffle(button.is_active())));
        });
        let volume_action = Rc::clone(&on_action);
        volume.connect_change_value(move |slider, _, value| {
            if slider.is_sensitive() && value.is_finite() {
                volume_action(Action::Playback(MiniCommand::Volume(value.clamp(0.0, 1.0))));
            }
            glib::Propagation::Proceed
        });
        back.connect_clicked(move |_| on_action(Action::Back));

        let view = Self {
            widget,
            panel_host,
            reactions_slot,
            back,
            tabs,
            player,
            section,
            compact,
            artwork,
            title,
            artist,
            status,
            position,
            duration,
            progress,
            previous,
            play,
            next,
            shuffle,
            repeat,
            volume,
        };
        view.set_section(Section::Player);
        view.render(&MiniSnapshot::default(), None);
        view
    }

    pub(super) fn set_section(&self, section: Section) {
        self.section.set(section);
        let tab = &self.tabs[section as usize];
        tab.set_active(true);
        show_section(
            &self.player,
            &self.panel_host,
            section,
            self.compact.get(),
            Some(tab),
        );
    }

    pub(super) fn focus_back(&self) {
        self.back.grab_focus();
    }

    pub(super) fn is_compact(&self) -> bool {
        self.compact.get()
    }

    pub(super) fn render(&self, snapshot: &MiniSnapshot, status: Option<&str>) {
        let (title, missing) = snapshot
            .title
            .as_deref()
            .filter(|name| !name.is_empty())
            .map_or(("Трек не выбран", true), |name| (name, false));
        if self.title.text() != title {
            self.title.set_text(title);
            self.title.set_tooltip_text((!missing).then_some(title));
        }
        let artist = snapshot.artist.as_deref().unwrap_or_default();
        set_track_label(&self.artist, artist);
        self.artist.set_visible(!artist.is_empty());
        let status = status.filter(|status| !status.is_empty());
        set_track_label(&self.status, status.unwrap_or_default());
        self.status.set_visible(status.is_some());
        let cover = snapshot.cover.as_ref().map(|texture| texture.upcast_ref());
        if self.artwork.paintable().as_ref() != cover {
            self.artwork.set_paintable(cover);
        }
        self.artwork.set_visible(cover.is_some());

        player_bar::set_play_presentation(&self.play, snapshot.playing, snapshot.loading);
        let label = if snapshot.loading {
            "Загрузка трека"
        } else if snapshot.playing {
            "Пауза"
        } else if status.is_some() && snapshot.can_play_pause {
            "Повторить воспроизведение"
        } else {
            "Воспроизвести"
        };
        if self.play.tooltip_text().as_deref() != Some(label) {
            self.play.set_tooltip_text(Some(label));
            self.play
                .update_property(&[gtk::accessible::Property::Label(label)]);
        }
        self.previous.set_sensitive(snapshot.can_previous);
        self.play.set_sensitive(snapshot.can_play_pause);
        self.next.set_sensitive(snapshot.can_next);
        player_bar::set_order_presentation(
            &self.shuffle,
            &self.repeat,
            snapshot.shuffle,
            snapshot.repeat,
            snapshot.can_change_order,
        );
        self.volume.set_sensitive(snapshot.can_volume);
        self.volume.set_value(snapshot.volume);

        let duration = snapshot.duration.filter(|duration| !duration.is_zero());
        let position = snapshot
            .position
            .map(|position| duration.map_or(position, |duration| position.min(duration)));
        self.progress
            .set_sensitive(snapshot.can_seek && duration.is_some());
        self.progress.adjustment().configure(
            position
                .zip(duration)
                .map_or(0.0, |(position, _)| position.as_secs_f64()),
            0.0,
            duration.map_or(1.0, |duration| duration.as_secs_f64()),
            1.0,
            5.0,
            0.0,
        );
        let elapsed = position.map_or_else(|| "—".into(), mini_time);
        let total = duration.map_or_else(|| "—".into(), mini_time);
        if self.position.text() != elapsed || self.duration.text() != total {
            self.position.set_text(&elapsed);
            self.duration.set_text(&total);
            self.progress
                .update_property(&[gtk::accessible::Property::ValueText(&format!(
                    "{elapsed} / {total}"
                ))]);
        }
    }
}

fn breakpoint(kind: adw::BreakpointConditionLengthType, size: f64) -> adw::Breakpoint {
    adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        kind,
        size,
        adw::LengthUnit::Px,
    ))
}

fn show_section(
    player: &gtk::ScrolledWindow,
    panel: &gtk::Box,
    section: Section,
    compact: bool,
    tab: Option<&gtk::ToggleButton>,
) {
    let detail = section != Section::Player;
    let player_visible = !detail || !compact;
    let hidden = if !detail {
        Some(panel.upcast_ref::<gtk::Widget>())
    } else if !player_visible {
        Some(player.upcast_ref())
    } else {
        None
    };
    if let (Some(tab), Some(hidden)) = (tab, hidden)
        && hidden
            .root()
            .and_then(|root| root.focus())
            .is_some_and(|focus| focus == *hidden || focus.is_ancestor(hidden))
    {
        tab.grab_focus();
    }
    panel.set_width_request(if compact { -1 } else { 350 });
    panel.set_hexpand(compact || section == Section::Lyrics);
    panel.set_visible(detail);
    player.set_visible(player_visible);
}

fn transport_button(name: &str, icon: &str, label: &str) -> gtk::Button {
    let button = icon_button(icon, label);
    button.set_widget_name(name);
    button.set_size_request(44, 44);
    button.add_css_class("flat");
    button.add_css_class("circular");
    button
}

fn track_label(name: &str, class: &str, lines: i32) -> gtk::Label {
    let label = gtk::Label::builder()
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(lines)
        .justify(gtk::Justification::Center)
        .build();
    label.set_widget_name(name);
    label.add_css_class(class);
    label
}

fn set_track_label(label: &gtk::Label, text: &str) {
    if label.text() != text {
        label.set_text(text);
        label.set_tooltip_text((!text.is_empty()).then_some(text));
    }
}

fn time_label(name: &str) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_widget_name(name);
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    label.add_css_class("numeric");
    label
}
