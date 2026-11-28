// SPDX-License-Identifier: GPL-3.0-or-later

use gtk::prelude::*;

pub(super) fn new(scale: &gtk::Scale) -> gtk::Label {
    let label = gtk::Label::builder()
        .width_chars(4)
        .xalign(1.0)
        .valign(gtk::Align::Center)
        .accessible_role(gtk::AccessibleRole::Presentation)
        .build();
    label.set_widget_name(&format!("{}-percent", scale.widget_name()));
    label.add_css_class("caption");
    label.add_css_class("numeric");
    label.add_css_class("karin-font-secondary");
    let tooltip = scale.tooltip_text();
    let update = move |scale: &gtk::Scale, label: &gtk::Label| {
        let value = format!("{:.0}%", scale.value() * 100.0);
        if label.text() != value {
            label.set_text(&value);
            scale.update_property(&[gtk::accessible::Property::ValueText(&value)]);
            if let Some(tooltip) = &tooltip {
                scale.set_tooltip_text(Some(&format!("{tooltip}: {value}")));
            }
        }
    };
    update(scale, &label);
    let readout = label.clone();
    // Value changes include renders from the shared player. Playback inputs
    // still use change-value, so updating the readout sends no audio command.
    scale.connect_value_changed(move |scale| update(scale, &readout));
    label
}
