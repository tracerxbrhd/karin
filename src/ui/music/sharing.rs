// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[cfg(test)]
mod tests;

pub(super) fn track_link(track: &Track) -> Option<String> {
    crate::music::public_links::track(&track.id, track.album_id.as_deref())
}

pub(super) fn copy_link(widget: &impl IsA<gtk::Widget>, link: &str) {
    widget.clipboard().set_text(link);
    if let Some(overlay) = widget
        .ancestor(adw::ToastOverlay::static_type())
        .and_downcast::<adw::ToastOverlay>()
    {
        overlay.add_toast(
            adw::Toast::builder()
                .title("Ссылка скопирована")
                .timeout(2)
                .build(),
        );
    }
}

pub(super) fn copy_menu_item(link: Option<String>, popover: &gtk::Popover) -> gtk::Button {
    let button = gtk::Button::with_label("Копировать ссылку");
    button.set_widget_name("music-copy-link");
    button.add_css_class("flat");
    button.set_sensitive(link.is_some());
    let popover = popover.downgrade();
    button.connect_clicked(move |button| {
        if let Some(link) = &link {
            copy_link(button, link);
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
        }
    });
    button
}

fn link_menu(label: &str, link: Option<String>) -> gtk::MenuButton {
    let menu = gtk::MenuButton::builder()
        .icon_name("karin-dots-vertical-symbolic")
        .tooltip_text(label)
        .has_frame(false)
        .build();
    menu.set_widget_name("music-entity-menu");
    menu.update_property(&[gtk::accessible::Property::Label(label)]);
    let popover = gtk::Popover::new();
    let copy = copy_menu_item(link, &popover);
    super::super::margins(&copy, 6);
    popover.set_child(Some(&copy));
    menu.set_popover(Some(&popover));
    menu
}

pub(super) fn tile_with_link_menu(
    tile: &gtk::Button,
    label: &str,
    link: Option<String>,
) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(tile));
    let menu = link_menu(label, link);
    menu.set_halign(gtk::Align::End);
    menu.set_valign(gtk::Align::Start);
    menu.set_margin_end(8);
    menu.set_margin_top(8);
    menu.add_css_class("circular");
    // A standard button surface keeps the affordance readable over artwork.
    menu.set_has_frame(true);
    overlay.add_overlay(&menu);
    catalog_layout::bind_context_menu(tile, &menu);
    overlay
}

impl Controller {
    pub(super) fn detail_actions_with_link(
        &self,
        play: gtk::Button,
        label: &str,
        link: Option<String>,
    ) {
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.add_css_class("music-collection-actions");
        actions.append(&play);
        actions.append(&link_menu(label, link));
        self.place_detail_actions(&actions);
    }
}
