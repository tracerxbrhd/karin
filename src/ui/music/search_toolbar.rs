// SPDX-License-Identifier: GPL-3.0-or-later

use adw::prelude::*;

pub(super) fn entry(placeholder: &str, name: &str, accessible_name: &str) -> gtk::SearchEntry {
    let entry = gtk::SearchEntry::new();
    entry.set_placeholder_text(Some(placeholder));
    entry.set_widget_name(name);
    entry.set_hexpand(true);
    entry.set_width_chars(12);
    entry.update_property(&[gtk::accessible::Property::Label(accessible_name)]);
    entry
}
