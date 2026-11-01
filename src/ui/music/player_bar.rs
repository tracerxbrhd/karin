// SPDX-License-Identifier: GPL-3.0-or-later
//! One native footer around the existing controller; no playback ownership.

use super::*;

pub(super) struct PlayerBarParts {
    pub player_bar: gtk::Box,
    pub player_main: gtk::Box,
    pub current: gtk::Label,
    pub current_artist: gtk::Label,
    pub current_art: gtk::Box,
    pub current_art_button: gtk::Button,
    pub current_like_slot: gtk::Box,
    pub position: gtk::Label,
    pub player_status: gtk::Label,
    pub play: gtk::Button,
    pub stop: gtk::Button,
    pub previous: gtk::Button,
    pub next: gtk::Button,
    pub queue: gtk::Button,
    pub shuffle: gtk::ToggleButton,
    pub repeat: gtk::Button,
    pub progress: gtk::Scale,
    pub volume: gtk::Scale,
}

impl PlayerBarParts {
    pub(super) fn new() -> Self {
        let player_bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        player_bar.set_widget_name("music-player-bar");
        player_bar.add_css_class("music-player");
        player_bar.set_margin_start(12);
        player_bar.set_margin_end(12);
        player_bar.set_margin_top(8);
        player_bar.set_margin_bottom(8);
        // Start with the compact minimum so a first 420px allocation is never
        // forced wider by the desktop timeline before the initial frame.
        player_bar.add_css_class("music-player-compact");

        let player_main = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        player_main.set_widget_name("music-player-main");
        player_main.add_css_class("music-player-main");
        // Equal native grid columns keep transport centered even when a long
        // title's natural width exceeds its wing. GtkCenterBox may shift its
        // center in that case instead of reducing the side allocation.
        let columns = gtk::Grid::new();
        columns.set_column_homogeneous(true);
        columns.set_column_spacing(8);
        columns.set_hexpand(true);
        player_main.append(&columns);
        player_bar.append(&player_main);

        let metadata = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        metadata.set_halign(gtk::Align::Start);
        metadata.set_valign(gtk::Align::Center);
        metadata.add_css_class("music-player-metadata");
        let current_art = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let current_art_button = gtk::Button::builder()
            .has_frame(false)
            .child(&current_art)
            .tooltip_text("Показать сведения о текущем треке")
            .sensitive(false)
            .build();
        current_art_button.add_css_class("flat");
        current_art_button.set_widget_name("music-current-info");
        current_art_button.update_property(&[gtk::accessible::Property::Label(
            "Показать сведения о текущем треке",
        )]);
        metadata.append(&current_art_button);
        let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
        labels.set_valign(gtk::Align::Center);
        let current = metadata_label("Трек не выбран", "music-current-title");
        current.add_css_class("music-player-title");
        let current_artist = metadata_label("Выберите трек", "music-current-artist");
        current.set_max_width_chars(14);
        current_artist.set_max_width_chars(14);
        current_artist.add_css_class("music-player-artist");
        current_artist.add_css_class("dim-label");
        let player_status = metadata_label("", "music-player-status");
        player_status.add_css_class("caption");
        // Unlike personal metadata, a fixed error/loading caption must remain
        // readable in full. Keep it outside the homogeneous grid: wrapping in
        // a wing can produce a GTK height-for-width min/natural-size conflict.
        player_status.set_wrap(true);
        player_status.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        player_status.set_lines(-1);
        player_status.set_ellipsize(gtk::pango::EllipsizeMode::None);
        player_status.set_margin_start(4);
        player_status.set_margin_end(4);
        player_status.set_margin_bottom(4);
        player_status.set_visible(false);
        labels.append(&current);
        labels.append(&current_artist);
        // The bottom bar grows upward only while feedback is visible; the
        // transport and timeline keep their existing bottom anchoring.
        player_bar.prepend(&player_status);
        metadata.append(&labels);
        let current_like_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        current_like_slot.set_widget_name("music-current-like-slot");
        current_like_slot.set_visible(false);
        columns.attach(&metadata, 0, 0, 1, 1);

        let middle = gtk::Box::new(gtk::Orientation::Vertical, 2);
        middle.set_valign(gtk::Align::Center);
        let transport = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        transport.set_halign(gtk::Align::Center);
        transport.add_css_class("music-transport");
        transport.set_widget_name("music-transport");
        let shuffle = gtk::ToggleButton::new();
        shuffle.set_icon_name("media-playlist-shuffle-symbolic");
        shuffle.set_widget_name("music-shuffle");
        shuffle.set_tooltip_text(Some("Перемешать очередь"));
        shuffle.update_property(&[gtk::accessible::Property::Label("Перемешать очередь")]);
        shuffle.add_css_class("flat");
        let previous = icon_button("media-skip-backward-symbolic", "Предыдущий трек");
        previous.set_widget_name("music-previous");
        let play = icon_button(
            "media-playback-start-symbolic",
            "Воспроизвести выбранный трек",
        );
        play.set_widget_name("music-play");
        play.set_sensitive(false);
        play.add_css_class("circular");
        play.add_css_class("suggested-action");
        play.set_size_request(42, 42);
        let next = icon_button("media-skip-forward-symbolic", "Следующий трек");
        next.set_widget_name("music-next");
        let repeat = gtk::Button::new();
        repeat.set_valign(gtk::Align::Center);
        let repeat_surface = gtk::Overlay::new();
        let repeat_icon = gtk::Image::from_icon_name("media-playlist-repeat-symbolic");
        repeat_icon.set_pixel_size(16);
        repeat_surface.set_child(Some(&repeat_icon));
        let repeat_one = gtk::Label::new(Some("1"));
        repeat_one.set_widget_name("music-repeat-one-badge");
        repeat_one.add_css_class("music-repeat-one-badge");
        repeat_one.set_halign(gtk::Align::End);
        repeat_one.set_valign(gtk::Align::Start);
        repeat_one.set_can_target(false);
        repeat_one.set_visible(false);
        repeat_surface.add_overlay(&repeat_one);
        repeat_surface.set_measure_overlay(&repeat_one, false);
        repeat.set_child(Some(&repeat_surface));
        repeat.set_tooltip_text(Some("Повтор выключен"));
        repeat.update_property(&[gtk::accessible::Property::Label("Повтор выключен")]);
        repeat.set_widget_name("music-repeat");
        repeat.add_css_class("flat");
        for control in [&previous, &next] {
            control.add_css_class("flat");
        }
        transport.append(&previous);
        transport.append(&play);
        transport.append(&next);
        middle.append(&transport);

        let timeline = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        timeline.add_css_class("music-player-timeline");
        timeline.set_widget_name("music-player-timeline");
        let progress = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        progress.set_draw_value(false);
        progress.set_hexpand(true);
        progress.set_sensitive(false);
        progress.set_widget_name("music-progress");
        progress.set_tooltip_text(Some("Позиция воспроизведения"));
        progress.update_property(&[gtk::accessible::Property::Label("Позиция воспроизведения")]);
        let position = text("— / —", "caption");
        position.set_wrap(false);
        position.set_hexpand(false);
        position.set_widget_name("music-position");
        timeline.append(&progress);
        timeline.append(&position);
        player_bar.append(&timeline);
        columns.attach(&middle, 1, 0, 1, 1);

        let utilities = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        utilities.set_halign(gtk::Align::End);
        utilities.set_valign(gtk::Align::Center);
        let modes = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .tooltip_text("Режимы воспроизведения и «Нравится»")
            .build();
        modes.set_widget_name("music-playback-modes");
        modes.set_direction(gtk::ArrowType::Up);
        modes.add_css_class("flat");
        modes.update_property(&[gtk::accessible::Property::Label(
            "Режимы воспроизведения и «Нравится»",
        )]);
        let mode_popover = gtk::Popover::new();
        mode_popover.set_position(gtk::PositionType::Top);
        let mode_controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        super::super::margins(&mode_controls, 8);
        mode_controls.append(&shuffle);
        mode_controls.append(&repeat);
        mode_controls.append(&current_like_slot);
        mode_popover.set_child(Some(&mode_controls));
        modes.set_popover(Some(&mode_popover));
        utilities.append(&modes);

        let queue = icon_button("view-list-symbolic", "Показать очередь");
        queue.set_widget_name("music-queue-toggle");
        queue.set_action_name(Some("win.music-queue"));
        queue.add_css_class("flat");
        utilities.append(&queue);

        let volume_menu = gtk::MenuButton::builder()
            .icon_name("audio-volume-high-symbolic")
            .tooltip_text("Громкость")
            .build();
        volume_menu.set_widget_name("music-volume-menu");
        volume_menu.set_direction(gtk::ArrowType::Up);
        volume_menu.add_css_class("flat");
        volume_menu.update_property(&[gtk::accessible::Property::Label("Громкость")]);
        let volume_popover = gtk::Popover::new();
        volume_popover.set_position(gtk::PositionType::Top);
        volume_menu.set_popover(Some(&volume_popover));
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        volume.set_draw_value(false);
        volume.set_value(0.7);
        volume.set_width_request(140);
        volume.set_sensitive(false);
        volume.set_widget_name("music-volume");
        volume.set_tooltip_text(Some("Громкость"));
        volume.update_property(&[gtk::accessible::Property::Label(
            "Громкость воспроизведения",
        )]);
        super::super::margins(&volume, 12);
        volume_popover.set_child(Some(&volume));
        utilities.append(&volume_menu);
        columns.attach(&utilities, 2, 0, 1, 1);

        // Stop remains an internal/system command. Keeping its existing object
        // preserves controller/test hooks without placing it in the music UI.
        let stop = icon_button("media-playback-stop-symbolic", "Остановить воспроизведение");
        stop.set_widget_name("music-stop");
        stop.set_visible(false);
        stop.set_sensitive(false);

        // Only the same real controls move into the native modes popover. The
        // footer owns no duplicate mode state or playback model. Weak captures
        // let the ordinary window-close cleanup retire every native widget.
        let weak_transport = transport.downgrade();
        let weak_mode_controls = mode_controls.downgrade();
        let weak_modes = modes.downgrade();
        let weak_shuffle = shuffle.downgrade();
        let weak_repeat = repeat.downgrade();
        let weak_previous = previous.downgrade();
        let weak_middle = middle.downgrade();
        let weak_timeline = timeline.downgrade();
        let weak_current = current.downgrade();
        let weak_artist = current_artist.downgrade();
        let weak_metadata = metadata.downgrade();
        let weak_like_slot = current_like_slot.downgrade();
        let compact_state = Cell::new(Some(true));
        player_bar.add_tick_callback(move |bar, _| {
            let compact = bar.width() < 720;
            if bar.width() <= 0 || compact_state.replace(Some(compact)) == Some(compact) {
                return glib::ControlFlow::Continue;
            }
            let (
                Some(transport),
                Some(controls),
                Some(modes),
                Some(shuffle),
                Some(repeat),
                Some(previous),
                Some(middle),
                Some(timeline),
                Some(current),
                Some(artist),
                Some(metadata),
                Some(like_slot),
            ) = (
                weak_transport.upgrade(),
                weak_mode_controls.upgrade(),
                weak_modes.upgrade(),
                weak_shuffle.upgrade(),
                weak_repeat.upgrade(),
                weak_previous.upgrade(),
                weak_middle.upgrade(),
                weak_timeline.upgrade(),
                weak_current.upgrade(),
                weak_artist.upgrade(),
                weak_metadata.upgrade(),
                weak_like_slot.upgrade(),
            )
            else {
                return glib::ControlFlow::Break;
            };
            if compact {
                transport.remove(&shuffle);
                transport.remove(&repeat);
                controls.append(&shuffle);
                controls.append(&repeat);
                metadata.remove(&like_slot);
                controls.append(&like_slot);
                middle.remove(&timeline);
                timeline.set_width_request(-1);
                bar.append(&timeline);
                current.set_max_width_chars(14);
                artist.set_max_width_chars(14);
                metadata.set_spacing(4);
                bar.add_css_class("music-player-compact");
            } else {
                controls.remove(&shuffle);
                controls.remove(&repeat);
                transport.insert_child_after(&shuffle, gtk::Widget::NONE);
                transport.reorder_child_after(&previous, Some(&shuffle));
                transport.append(&repeat);
                controls.remove(&like_slot);
                metadata.append(&like_slot);
                bar.remove(&timeline);
                timeline.set_width_request(-1);
                middle.append(&timeline);
                current.set_max_width_chars(26);
                artist.set_max_width_chars(26);
                metadata.set_spacing(10);
                bar.remove_css_class("music-player-compact");
            }
            modes.set_visible(compact);
            glib::ControlFlow::Continue
        });

        Self {
            player_bar,
            player_main,
            current,
            current_artist,
            current_art,
            current_art_button,
            current_like_slot,
            position,
            player_status,
            play,
            stop,
            previous,
            next,
            queue,
            shuffle,
            repeat,
            progress,
            volume,
        }
    }
}

/// Only one existing label changes visibility. Playback mode remains queue
/// state, and the permanent overlay never changes the transport's measurement.
pub(super) fn set_repeat_one_badge(button: &gtk::Button, one: bool) {
    if let Some(badge) = repeat_one_badge(button) {
        badge.set_visible(one);
    }
}

pub(super) fn repeat_one_badge(button: &gtk::Button) -> Option<gtk::Label> {
    button
        .child()?
        .downcast::<gtk::Overlay>()
        .ok()?
        .last_child()?
        .downcast()
        .ok()
}

fn metadata_label(value: &str, name: &str) -> gtk::Label {
    let label = text(value, "");
    label.set_widget_name(name);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_lines(1);
    label.set_wrap(false);
    label.set_width_chars(1);
    label.set_max_width_chars(26);
    label
}
