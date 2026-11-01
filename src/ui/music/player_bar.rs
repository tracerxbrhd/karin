// SPDX-License-Identifier: GPL-3.0-or-later
//! One native footer around the existing controller; no playback ownership.

use super::*;

pub(super) struct PlayerBarParts {
    pub player_bar: gtk::Box,
    pub player_revealer: gtk::Revealer,
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
        // The zero-height host still receives the real allocated width while
        // its player content is hidden. Responsive layout must observe it,
        // rather than wait for the inner content's first visible frame.
        let footer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        footer.set_widget_name("music-player-bar");
        let player_bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        player_bar.set_widget_name("music-player-content");
        // A closed vertical GtkRevealer still measures its child's horizontal
        // request. Conceal this inner widget at the closed endpoint as well,
        // so an idle desktop arrangement cannot constrain a compact window.
        player_bar.set_visible(false);
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
        columns.set_row_spacing(4);
        columns.set_hexpand(true);
        player_main.append(&columns);
        player_bar.append(&player_main);

        let metadata = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        metadata.set_widget_name("music-player-metadata");
        metadata.set_hexpand(true);
        metadata.set_valign(gtk::Align::Center);
        metadata.add_css_class("music-player-metadata");
        let current_track = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        current_track.set_widget_name("music-current-track-content");
        let current_art = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        current_art.set_valign(gtk::Align::Center);
        current_track.append(&current_art);
        let current_art_button = gtk::Button::builder()
            .has_frame(false)
            .child(&current_track)
            .tooltip_text("Показать сведения о текущем треке")
            .sensitive(false)
            .build();
        current_art_button.add_css_class("flat");
        current_art_button.add_css_class("music-current-track-button");
        current_art_button.set_halign(gtk::Align::Start);
        current_art_button.set_hexpand(false);
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
        current.set_max_width_chars(26);
        current_artist.set_max_width_chars(26);
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
        current_track.append(&labels);
        let current_like_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        current_like_slot.set_widget_name("music-current-like-slot");
        current_like_slot.set_visible(false);
        metadata.append(&current_like_slot);
        columns.attach(&metadata, 0, 0, 2, 1);

        let middle = gtk::Box::new(gtk::Orientation::Vertical, 2);
        middle.set_valign(gtk::Align::Center);
        let transport = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        transport.set_halign(gtk::Align::Center);
        transport.add_css_class("music-transport");
        transport.set_widget_name("music-transport");
        let shuffle = gtk::ToggleButton::new();
        shuffle.set_icon_name("media-playlist-shuffle-symbolic");
        shuffle.set_widget_name("music-shuffle");
        shuffle.set_tooltip_text(Some("Перемешать очередь"));
        shuffle.update_property(&[gtk::accessible::Property::Label("Перемешать очередь")]);
        shuffle.add_css_class("flat");
        shuffle.set_size_request(44, 44);
        let previous = icon_button("media-skip-backward-symbolic", "Предыдущий трек");
        previous.set_widget_name("music-previous");
        let play = gtk::Button::new();
        play.set_valign(gtk::Align::Center);
        play.set_tooltip_text(Some("Воспроизвести выбранный трек"));
        play.update_property(&[gtk::accessible::Property::Label(
            "Воспроизвести выбранный трек",
        )]);
        let play_surface = gtk::Overlay::new();
        let play_icon = gtk::Image::from_icon_name("media-playback-start-symbolic");
        play_icon.set_pixel_size(16);
        play_surface.set_child(Some(&play_icon));
        let play_loading = gtk::Spinner::new();
        play_loading.set_widget_name("music-play-loading");
        play_loading.set_size_request(16, 16);
        play_loading.set_halign(gtk::Align::Center);
        play_loading.set_valign(gtk::Align::Center);
        play_loading.set_can_target(false);
        play_loading.set_visible(false);
        play_surface.add_overlay(&play_loading);
        play_surface.set_measure_overlay(&play_loading, false);
        play.set_child(Some(&play_surface));
        play.set_widget_name("music-play");
        play.set_sensitive(false);
        play.add_css_class("circular");
        play.add_css_class("suggested-action");
        play.set_size_request(48, 48);
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
        for control in [&previous, &next, &repeat] {
            control.add_css_class("flat");
            control.set_size_request(44, 44);
        }
        transport.append(&shuffle);
        transport.append(&previous);
        transport.append(&play);
        transport.append(&next);
        transport.append(&repeat);
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
        let timeline_clamp = adw::Clamp::builder()
            .maximum_size(420)
            .tightening_threshold(240)
            .child(&timeline)
            .build();
        timeline_clamp.set_widget_name("music-player-timeline-clamp");
        player_bar.append(&timeline_clamp);
        columns.attach(&middle, 0, 1, 3, 1);

        let utilities = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        utilities.set_halign(gtk::Align::End);
        utilities.set_valign(gtk::Align::Center);
        let queue = icon_button("view-list-symbolic", "Показать очередь");
        queue.set_widget_name("music-queue-toggle");
        queue.set_action_name(Some("win.music-queue"));
        queue.add_css_class("flat");
        queue.set_size_request(44, 44);
        utilities.append(&queue);

        let volume_menu = gtk::MenuButton::builder()
            .icon_name("audio-volume-high-symbolic")
            .tooltip_text("Громкость")
            .build();
        volume_menu.set_widget_name("music-volume-menu");
        volume_menu.set_direction(gtk::ArrowType::Up);
        volume_menu.add_css_class("flat");
        volume_menu.set_size_request(44, 44);
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
        let layout = columns.layout_manager().expect("native player grid layout");
        let metadata_cell = layout
            .layout_child(&metadata)
            .downcast::<gtk::GridLayoutChild>()
            .expect("native metadata grid cell");
        let transport_cell = layout
            .layout_child(&middle)
            .downcast::<gtk::GridLayoutChild>()
            .expect("native transport grid cell");

        // Stop remains an internal/system command. Keeping its existing object
        // preserves controller/test hooks without placing it in the music UI.
        let stop = icon_button("media-playback-stop-symbolic", "Остановить воспроизведение");
        stop.set_widget_name("music-stop");
        stop.set_visible(false);
        stop.set_sensitive(false);

        // All five existing transport controls fit one compact native group.
        // Responsive changes move only its timeline and grid attachment; the
        // same focused mode widgets remain mounted, with one playback model.
        let weak_middle = middle.downgrade();
        let weak_timeline = timeline_clamp.downgrade();
        let weak_columns = columns.downgrade();
        let weak_metadata_cell = metadata_cell.downgrade();
        let weak_transport_cell = transport_cell.downgrade();
        let weak_content = player_bar.downgrade();
        let compact_state = Cell::new(Some(true));
        let layout_player = Rc::new(move |width: i32| {
            let compact = width < 720;
            if width <= 0 || compact_state.replace(Some(compact)) == Some(compact) {
                return;
            }
            let (
                Some(bar),
                Some(middle),
                Some(timeline),
                Some(columns),
                Some(metadata_cell),
                Some(transport_cell),
            ) = (
                weak_content.upgrade(),
                weak_middle.upgrade(),
                weak_timeline.upgrade(),
                weak_columns.upgrade(),
                weak_metadata_cell.upgrade(),
                weak_transport_cell.upgrade(),
            )
            else {
                return;
            };
            if compact {
                metadata_cell.set_column_span(2);
                transport_cell.set_column(0);
                transport_cell.set_row(1);
                transport_cell.set_column_span(3);
                columns.set_row_spacing(4);
                middle.remove(&timeline);
                bar.append(&timeline);
                bar.add_css_class("music-player-compact");
            } else {
                metadata_cell.set_column_span(1);
                transport_cell.set_column(1);
                transport_cell.set_row(0);
                transport_cell.set_column_span(1);
                columns.set_row_spacing(0);
                bar.remove(&timeline);
                middle.append(&timeline);
                bar.remove_css_class("music-player-compact");
            }
        });
        let weak_bar = player_bar.downgrade();
        let layout = Rc::clone(&layout_player);
        footer.add_tick_callback(move |host, _| {
            let Some(bar) = weak_bar.upgrade() else {
                return glib::ControlFlow::Break;
            };
            layout(host.width() - bar.margin_start() - bar.margin_end());
            glib::ControlFlow::Continue
        });

        // No queue selection or placeholder makes a player appear. The
        // existing controller reveals this one native surface only for an
        // actual play request/runtime; a paused active track retains it.
        let player_revealer = gtk::Revealer::builder()
            .child(&player_bar)
            .transition_type(gtk::RevealerTransitionType::SlideUp)
            .transition_duration(240)
            .reveal_child(false)
            .build();
        player_revealer.set_widget_name("music-player-revealer");
        let weak_host = footer.downgrade();
        let weak_bar = player_bar.downgrade();
        player_revealer.connect_reveal_child_notify(move |revealer| {
            if !revealer.reveals_child() {
                return;
            }
            if let (Some(host), Some(bar)) = (weak_host.upgrade(), weak_bar.upgrade()) {
                // The same idempotent arrangement runs before revealing a
                // previously unmapped child, using only actual host geometry.
                layout_player(host.width() - bar.margin_start() - bar.margin_end());
                bar.set_visible(true);
            }
        });
        let weak_bar = player_bar.downgrade();
        player_revealer.connect_child_revealed_notify(move |revealer| {
            // A Stop during an opening still animates back from its current
            // position. Keep every closing frame until GTK reaches zero.
            if !revealer.reveals_child()
                && !revealer.is_child_revealed()
                && let Some(bar) = weak_bar.upgrade()
            {
                bar.set_visible(false);
            }
        });
        footer.append(&player_revealer);

        Self {
            player_bar: footer,
            player_revealer,
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

/// Normal source resolution/preparation uses the existing transport's native
/// spinner. Its stable icon remains measured, so loading -> playing does not
/// change the footer's height during the reveal. Real errors keep their row.
pub(super) fn set_play_presentation(button: &gtk::Button, playing: bool, loading: bool) {
    let Some(surface) = button.child().and_downcast::<gtk::Overlay>() else {
        return;
    };
    if let Some(icon) = surface.child().and_downcast::<gtk::Image>() {
        let name = if playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        };
        if icon.icon_name().as_deref() != Some(name) {
            icon.set_icon_name(Some(name));
        }
        icon.set_opacity(if loading { 0.0 } else { 1.0 });
    }
    if let Some(spinner) = play_loading_spinner(button) {
        spinner.set_spinning(loading);
        spinner.set_visible(loading);
    }
}

pub(super) fn play_loading_spinner(button: &gtk::Button) -> Option<gtk::Spinner> {
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
