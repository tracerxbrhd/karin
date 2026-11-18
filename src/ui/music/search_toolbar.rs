// SPDX-License-Identifier: GPL-3.0-or-later

use adw::prelude::*;

/// Keep the entry and any trailing actions aligned at the leading edge.
pub(super) fn build(
    placeholder: &str,
    name: &str,
    accessible_name: &str,
) -> (adw::Clamp, gtk::Box, gtk::SearchEntry) {
    let entry = gtk::SearchEntry::new();
    entry.set_placeholder_text(Some(placeholder));
    entry.set_widget_name(name);
    entry.set_hexpand(true);
    entry.set_width_chars(12);
    entry.set_max_width_chars(64);
    entry.update_property(&[gtk::accessible::Property::Label(accessible_name)]);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_widget_name("music-search-controls");
    row.append(&entry);

    let clamp = adw::Clamp::builder()
        .maximum_size(560)
        .tightening_threshold(560)
        .halign(gtk::Align::Start)
        .child(&row)
        .build();
    clamp.set_widget_name("music-search-toolbar");
    (clamp, row, entry)
}
