// SPDX-License-Identifier: GPL-3.0-or-later
//! Compact presentation of the application's existing music session.

use std::{rc::Rc, time::Duration};

use adw::prelude::*;
use gtk::{gdk, glib};

use super::music::{MiniCommand, MiniSnapshot, mini_time, repeat_button, set_order_presentation};

pub(super) enum Action {
    Playback(MiniCommand),
    OpenMain,
}

pub(super) struct MiniPlayer {
    pub(super) window: adw::ApplicationWindow,
    pub(super) reactions_slot: gtk::Box,
    pub(super) is_anchored: bool,
    artwork: gtk::Picture,
    title: gtk::Label,
    artist: gtk::Label,
    previous: gtk::Button,
    play: gtk::Button,
    play_icon: gtk::Image,
    loading: gtk::Spinner,
    next: gtk::Button,
    shuffle: gtk::ToggleButton,
    repeat: gtk::Button,
    progress: gtk::Scale,
    time: gtk::Label,
    volume: gtk::Scale,
}

impl MiniPlayer {
    pub(super) fn new(
        app: &adw::Application,
        display: &gdk::Display,
        is_anchored: bool,
        on_action: impl Fn(Action) + 'static,
    ) -> Self {
        super::style::install_for_display(display);
        let on_action = Rc::new(on_action);
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .display(display)
            .decorated(!is_anchored)
            .title("Karin Mini")
            .icon_name(super::APP_ICON)
            .default_width(super::tray_window::MINI_WIDTH)
            .resizable(false)
            .build();
        window.set_widget_name("karin-mini-player");
        super::style::attach(&window);

        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        let open = gtk::Button::with_label("Открыть Karin");
        open.set_widget_name("karin-mini-open-main");
        open.add_css_class("flat");
        describe(&open, "Открыть основное окно Karin");
        header.pack_start(&open);
        if is_anchored {
            window.add_css_class("karin-mini-popup");
            header.set_show_start_title_buttons(false);
            header.set_show_end_title_buttons(false);
            let close = gtk::Button::from_icon_name("window-close-symbolic");
            close.set_widget_name("karin-mini-close");
            close.add_css_class("flat");
            describe(&close, "Закрыть мини-плеер");
            let weak = window.downgrade();
            close.connect_clicked(move |_| {
                if let Some(window) = weak.upgrade() {
                    window.set_visible(false);
                }
            });
            header.pack_end(&close);
        }
        toolbar.add_top_bar(&header);
        window.set_content(Some(&toolbar));

        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.add_css_class("karin-font-primary");
        super::margins(&content, 14);
        let track = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let cover = gtk::Overlay::builder()
            .width_request(56)
            .height_request(56)
            .valign(gtk::Align::Center)
            .overflow(gtk::Overflow::Hidden)
            .build();
        cover.add_css_class("card");
        let fallback = gtk::Image::from_icon_name("karin-music-symbolic");
        fallback.set_pixel_size(24);
        fallback.add_css_class("dim-label");
        cover.set_child(Some(&fallback));
        let artwork = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .can_shrink(true)
            .visible(false)
            .build();
        artwork.set_widget_name("karin-mini-artwork");
        cover.add_overlay(&artwork);
        cover.set_measure_overlay(&artwork, false);
        track.append(&cover);

        let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
        labels.set_hexpand(true);
        labels.set_valign(gtk::Align::Center);
        let title = track_label("karin-mini-title", "title-4");
        let artist = track_label("karin-mini-artist", "dim-label");
        labels.append(&title);
        labels.append(&artist);
        track.append(&labels);
        content.append(&track);

        let timeline = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let progress = scale("karin-mini-progress", "Позиция воспроизведения");
        let time = gtk::Label::new(None);
        time.set_widget_name("karin-mini-time");
        time.add_css_class("caption");
        time.add_css_class("numeric");
        timeline.append(&progress);
        timeline.append(&time);
        content.append(&timeline);

        let transport = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        transport.add_css_class("music-transport");
        transport.set_halign(gtk::Align::Center);
        let shuffle = gtk::ToggleButton::new();
        shuffle.set_widget_name("karin-mini-shuffle");
        shuffle.set_icon_name("karin-arrows-shuffle-symbolic");
        shuffle.set_size_request(38, 38);
        shuffle.set_valign(gtk::Align::Center);
        shuffle.add_css_class("flat");
        let previous = transport_button(
            "karin-mini-previous",
            "karin-player-skip-back-symbolic",
            "Предыдущий трек",
        );
        let play = gtk::Button::new();
        play.set_widget_name("karin-mini-play");
        play.set_size_request(44, 44);
        play.set_valign(gtk::Align::Center);
        play.add_css_class("circular");
        play.add_css_class("suggested-action");
        let play_surface = gtk::Overlay::new();
        let play_icon = gtk::Image::from_icon_name("karin-player-play-symbolic");
        play_icon.set_pixel_size(16);
        play_surface.set_child(Some(&play_icon));
        let loading = gtk::Spinner::builder()
            .width_request(16)
            .height_request(16)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .can_target(false)
            .visible(false)
            .build();
        play_surface.add_overlay(&loading);
        play_surface.set_measure_overlay(&loading, false);
        play.set_child(Some(&play_surface));
        let next = transport_button(
            "karin-mini-next",
            "karin-player-skip-forward-symbolic",
            "Следующий трек",
        );
        let repeat = repeat_button();
        repeat.set_widget_name("karin-mini-repeat");
        repeat.set_size_request(38, 38);
        repeat.set_valign(gtk::Align::Center);
        repeat.add_css_class("flat");
        transport.append(&shuffle);
        transport.append(&previous);
        transport.append(&play);
        transport.append(&next);
        transport.append(&repeat);
        content.append(&transport);

        let extra = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let reactions_slot = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        reactions_slot.set_widget_name("karin-mini-reactions");
        reactions_slot.set_visible(false);
        extra.append(&reactions_slot);
        let loudness = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        loudness.set_width_request(120);
        loudness.set_hexpand(true);
        loudness.set_valign(gtk::Align::Center);
        let speaker = gtk::Image::from_icon_name("karin-volume-symbolic");
        speaker.add_css_class("dim-label");
        speaker.set_pixel_size(16);
        let volume = scale("karin-mini-volume", "Громкость");
        loudness.append(&speaker);
        loudness.append(&volume);
        extra.append(&loudness);
        content.append(&extra);
        toolbar.set_content(Some(&content));

        for (button, command) in [
            (&previous, MiniCommand::Previous),
            (&play, MiniCommand::PlayPause),
            (&next, MiniCommand::Next),
            (&repeat, MiniCommand::Repeat),
        ] {
            let on_action = Rc::clone(&on_action);
            button.connect_clicked(move |_| on_action(Action::Playback(command)));
        }
        let action = Rc::clone(&on_action);
        shuffle.connect_clicked(move |button| {
            action(Action::Playback(MiniCommand::Shuffle(button.is_active())));
        });
        let action = Rc::clone(&on_action);
        progress.connect_change_value(move |slider, _, value| {
            // GTK's native range handles pointer and keyboard input. Rendering
            // from the player only updates the adjustment and emits no seek.
            if slider.is_sensitive()
                && value.is_finite()
                && let Ok(position) =
                    Duration::try_from_secs_f64(value.clamp(0.0, slider.adjustment().upper()))
            {
                action(Action::Playback(MiniCommand::Seek(position)));
            }
            glib::Propagation::Proceed
        });
        let action = Rc::clone(&on_action);
        volume.connect_change_value(move |slider, _, value| {
            if slider.is_sensitive() && value.is_finite() {
                action(Action::Playback(MiniCommand::Volume(value.clamp(0.0, 1.0))));
            }
            glib::Propagation::Proceed
        });
        open.connect_clicked(move |_| on_action(Action::OpenMain));

        let shortcuts = gtk::ShortcutController::new();
        let weak = window.downgrade();
        shortcuts.set_scope(gtk::ShortcutScope::Managed);
        shortcuts.add_shortcut(gtk::Shortcut::new(
            Some(gtk::KeyvalTrigger::new(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
            )),
            Some(gtk::CallbackAction::new(move |_, _| {
                if let Some(window) = weak.upgrade() {
                    window.close();
                }
                glib::Propagation::Stop
            })),
        ));
        window.add_controller(shortcuts);

        let view = Self {
            window,
            reactions_slot,
            is_anchored,
            artwork,
            title,
            artist,
            previous,
            play,
            play_icon,
            loading,
            next,
            shuffle,
            repeat,
            progress,
            time,
            volume,
        };
        view.render(&MiniSnapshot::default());
        view
    }

    pub(super) fn render(&self, snapshot: &MiniSnapshot) {
        let title = snapshot
            .title
            .as_deref()
            .filter(|title| !title.is_empty())
            .unwrap_or("Трек не выбран");
        set_track_label(&self.title, title);
        let artist = snapshot.artist.as_deref().unwrap_or_default();
        set_track_label(&self.artist, artist);
        self.artist.set_visible(!artist.is_empty());
        let cover = snapshot.cover.as_ref().map(|cover| cover.upcast_ref());
        if self.artwork.paintable().as_ref() != cover {
            self.artwork.set_paintable(cover);
        }
        self.artwork.set_visible(cover.is_some());

        let (icon, label) = if snapshot.playing {
            ("karin-player-pause-symbolic", "Пауза")
        } else if snapshot.loading {
            ("karin-player-play-symbolic", "Загрузка трека")
        } else {
            ("karin-player-play-symbolic", "Воспроизвести")
        };
        if self.play_icon.icon_name().as_deref() != Some(icon) {
            self.play_icon.set_icon_name(Some(icon));
        }
        self.play_icon
            .set_opacity(if snapshot.loading { 0.0 } else { 1.0 });
        self.loading.set_spinning(snapshot.loading);
        self.loading.set_visible(snapshot.loading);
        if self.play.tooltip_text().as_deref() != Some(label) {
            describe(&self.play, label);
        }
        self.previous.set_sensitive(snapshot.can_previous);
        self.play.set_sensitive(snapshot.can_play_pause);
        self.next.set_sensitive(snapshot.can_next);
        set_order_presentation(
            &self.shuffle,
            &self.repeat,
            snapshot.shuffle,
            snapshot.repeat,
            snapshot.can_change_order,
        );

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
        let time = format!("{} / {}", time_text(position), time_text(duration));
        if self.time.text() != time {
            self.time.set_text(&time);
            self.progress
                .update_property(&[gtk::accessible::Property::ValueText(&time)]);
        }

        self.volume.set_sensitive(snapshot.can_volume);
        if (0.0..=1.0).contains(&snapshot.volume) {
            self.volume.set_value(snapshot.volume);
            let value = format!("{:.0}%", snapshot.volume * 100.0);
            let tooltip = format!("Громкость: {value}");
            if self.volume.tooltip_text().as_deref() != Some(tooltip.as_str()) {
                self.volume.set_tooltip_text(Some(&tooltip));
                self.volume
                    .update_property(&[gtk::accessible::Property::ValueText(&value)]);
            }
        }
    }
}

fn describe(widget: &impl IsA<gtk::Widget>, label: &str) {
    let widget = widget.upcast_ref::<gtk::Widget>();
    widget.set_tooltip_text(Some(label));
    widget.update_property(&[gtk::accessible::Property::Label(label)]);
}

fn transport_button(name: &str, icon: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_widget_name(name);
    button.set_size_request(38, 38);
    button.set_valign(gtk::Align::Center);
    button.add_css_class("flat");
    describe(&button, label);
    button
}

fn scale(name: &str, label: &str) -> gtk::Scale {
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
    scale.set_widget_name(name);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    describe(&scale, label);
    scale
}

fn track_label(name: &str, style: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .xalign(0.0)
        .single_line_mode(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(32)
        .build();
    label.set_widget_name(name);
    label.add_css_class(style);
    label
}

fn set_track_label(label: &gtk::Label, text: &str) {
    if label.text() != text {
        label.set_text(text);
        label.set_tooltip_text((!text.is_empty()).then_some(text));
    }
}

fn time_text(time: Option<Duration>) -> String {
    time.map_or_else(|| "—".into(), mini_time)
}
