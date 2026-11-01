// SPDX-License-Identifier: GPL-3.0-or-later
//! Modest native starting surface over the existing real music destinations.

use super::*;

pub(super) struct HomeParts {
    pub home: gtk::Box,
    pub home_signedout: gtk::Box,
    pub home_signedin: gtk::Box,
    pub home_hero: gtk::Box,
    pub home_liked: gtk::Button,
    pub home_playlists: gtk::Button,
    pub home_search: gtk::Button,
    pub home_queue: gtk::Label,
    pub home_preview: gtk::ListBox,
    pub home_collections: gtk::FlowBox,
}

impl HomeParts {
    pub(super) fn new() -> Self {
        let home = gtk::Box::new(gtk::Orientation::Vertical, 20);
        home.set_widget_name("music-home");
        let home_signedout = gtk::Box::new(gtk::Orientation::Vertical, 16);
        home_signedout.set_margin_top(24);
        let icon = gtk::Image::from_icon_name("audio-x-generic-symbolic");
        icon.set_pixel_size(64);
        icon.set_halign(gtk::Align::Start);
        icon.add_css_class("accent");
        home_signedout.append(&icon);
        home_signedout.append(&text("Ваша музыка — в Karin", "title-1"));
        home_signedout.append(&text(
            "Войдите в Яндекс, чтобы слушать любимые треки и свои плейлисты.",
            "dim-label",
        ));
        home.append(&home_signedout);

        let home_signedin = gtk::Box::new(gtk::Orientation::Vertical, 24);
        home_signedin.set_widget_name("music-home-library");
        let introduction = gtk::Box::new(gtk::Orientation::Vertical, 6);
        introduction.set_widget_name("music-home-introduction");
        introduction.append(&text("Ваша музыка", "title-1"));
        introduction.append(&text(
            "Вернитесь к любимому или найдите новое.",
            "dim-label",
        ));
        home_signedin.append(&introduction);

        // These are ordinary existing controller actions, not another library
        // snapshot or an eager read of personalized recommendations.
        let routes = gtk::FlowBox::new();
        routes.set_widget_name("music-home-shortcuts");
        routes.set_selection_mode(gtk::SelectionMode::None);
        routes.set_focusable(false);
        routes.set_homogeneous(true);
        routes.set_min_children_per_line(1);
        routes.set_max_children_per_line(3);
        routes.set_column_spacing(12);
        routes.set_row_spacing(12);
        let home_liked = shortcut(
            "music-home-liked",
            interactions::like_icon_name(None),
            "Нравится",
            "Любимые композиции",
        );
        let home_playlists = shortcut(
            "music-home-playlists",
            "view-list-symbolic",
            "Плейлисты",
            "Ваши музыкальные подборки",
        );
        let home_search = shortcut(
            "music-home-search",
            "system-search-symbolic",
            "Поиск",
            "Треки, альбомы и исполнители",
        );
        for button in [&home_liked, &home_playlists, &home_search] {
            routes.insert(button, -1);
            if let Some(wrapper) = button.parent() {
                wrapper.set_focusable(false);
            }
        }
        home_signedin.append(&routes);

        let wave = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        wave.set_widget_name("music-wave-placeholder");
        wave.add_css_class("music-home-wave");
        wave.set_valign(gtk::Align::Start);
        let icon = gtk::Image::from_icon_name("radio-symbolic");
        icon.set_pixel_size(24);
        icon.set_valign(gtk::Align::Start);
        icon.add_css_class("dim-label");
        wave.append(&icon);
        let labels = gtk::Box::new(gtk::Orientation::Vertical, 4);
        labels.set_hexpand(true);
        labels.append(&text("Моя волна", "heading"));
        labels.append(&text("Персональная волна пока не подключена.", "dim-label"));
        wave.append(&labels);
        home_signedin.append(&wave);
        let home_queue = text("", "caption");
        home_queue.set_widget_name("music-home-queue-summary");
        home_queue.add_css_class("dim-label");
        home_queue.set_visible(false);
        home_signedin.append(&home_queue);
        home.append(&home_signedin);

        // Retain the existing renderer targets without mounting a second full
        // liked list/grid on Home. No new request or stale personal preview is
        // created; future verified sections can be added to the native Box.
        let home_hero = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        home_hero.set_widget_name("music-home-hero");
        let home_preview = list();
        home_preview.set_widget_name("music-home-liked-preview");
        let home_collections = catalog_layout::tile_grid();
        home_collections.set_widget_name("music-home-collections");
        Self {
            home,
            home_signedout,
            home_signedin,
            home_hero,
            home_liked,
            home_playlists,
            home_search,
            home_queue,
            home_preview,
            home_collections,
        }
    }
}

fn shortcut(name: &str, icon: &str, title: &str, description: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.add_css_class("music-home-shortcut");
    button.set_width_request(216);
    button.set_height_request(84);
    button.set_hexpand(true);
    button.set_tooltip_text(Some(title));
    button.update_property(&[gtk::accessible::Property::Label(title)]);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    super::super::margins(&content, 12);
    let icon = gtk::Image::from_icon_name(icon);
    icon.set_pixel_size(24);
    icon.set_valign(gtk::Align::Center);
    content.append(&icon);
    let labels = gtk::Box::new(gtk::Orientation::Vertical, 4);
    labels.set_hexpand(true);
    labels.set_valign(gtk::Align::Center);
    labels.append(&text(title, "heading"));
    let subtitle = text(description, "dim-label");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.set_wrap(false);
    subtitle.set_width_chars(1);
    subtitle.set_tooltip_text(Some(description));
    labels.append(&subtitle);
    content.append(&labels);
    button.set_child(Some(&content));
    button
}
