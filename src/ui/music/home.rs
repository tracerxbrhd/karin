// SPDX-License-Identifier: GPL-3.0-or-later
//! Real library-led starting surface, assembled without demo catalog content.

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

        let home_signedin = gtk::Box::new(gtk::Orientation::Vertical, 18);
        home_signedin.set_widget_name("music-home-library");
        let home_hero = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        home_hero.set_widget_name("music-home-hero");
        home_hero.add_css_class("music-home-hero");
        home_signedin.append(&home_hero);

        let collection_section = gtk::Box::new(gtk::Orientation::Vertical, 10);
        collection_section.set_widget_name("music-home-playlists");
        let (heading, home_playlists) = section_heading("Ваши плейлисты", "Все плейлисты");
        collection_section.append(&heading);
        let home_collections = catalog_layout::tile_grid();
        home_collections.set_max_children_per_line(8);
        home_collections.set_widget_name("music-home-collections");
        home_collections.add_css_class("music-home-collections");
        home_collections.set_visible(false);
        collection_section.append(&home_collections);
        let liked_section = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let (heading, home_liked) = section_heading("Из любимого", "Все треки");
        home_liked.set_widget_name("music-home-liked");
        liked_section.append(&heading);
        let home_preview = list();
        home_preview.set_widget_name("music-home-liked-preview");
        set_empty(&home_preview, "Загрузка коллекции…");
        liked_section.append(&home_preview);
        home_signedin.append(&liked_section);
        home_signedin.append(&collection_section);

        // Search is a primary rail destination. Preserve the existing action
        // object for navigation callbacks without another oversized shortcut.
        let home_search = gtk::Button::new();
        home_search.set_visible(false);
        let home_queue = text("", "caption");
        home_queue.set_visible(false);
        home_signedin.append(&home_queue);
        home.append(&home_signedin);
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

fn section_heading(title: &str, action: &str) -> (gtk::Box, gtk::Button) {
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = text(title, "title-3");
    title.set_hexpand(true);
    title.set_valign(gtk::Align::Center);
    header.append(&title);
    let button = gtk::Button::with_label(action);
    button.add_css_class("flat");
    button.set_halign(gtk::Align::End);
    header.append(&button);
    (header, button)
}
