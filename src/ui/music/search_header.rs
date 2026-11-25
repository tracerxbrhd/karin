// SPDX-License-Identifier: GPL-3.0-or-later
//! The two navigation presentations occupy one naturally measured native stack.
use adw::prelude::*;
use gtk::{glib, subclass::prelude::*};
use std::cell::{Cell, OnceCell};

#[derive(Debug)]
struct Children {
    slot: gtk::Stack,
    tabs: gtk::Box,
    entry: gtk::SearchEntry,
    toggle: gtk::Button,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct SearchHeader {
        pub(super) children: OnceCell<Children>,
        pub(super) open: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SearchHeader {
        const NAME: &'static str = "KarinSearchHeader";
        type Type = super::SearchHeader;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for SearchHeader {}
    impl WidgetImpl for SearchHeader {}
    impl BoxImpl for SearchHeader {}
}

glib::wrapper! {
    pub struct SearchHeader(ObjectSubclass<imp::SearchHeader>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl SearchHeader {
    pub(super) fn new(tabs: &gtk::Box, entry: &gtk::SearchEntry, toggle: &gtk::Button) -> Self {
        let header: Self = glib::Object::new();
        header.set_orientation(gtk::Orientation::Horizontal);
        // GtkStack measures both pages, so the entry occupies the same title
        // slot as the tabs. The separate leading toggle never moves on open.
        entry.set_width_request(120);
        entry.set_width_chars(1);
        entry.set_max_width_chars(1);
        let slot = gtk::Stack::builder()
            .hhomogeneous(true)
            .vhomogeneous(true)
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(220)
            .build();
        slot.set_widget_name("music-navigation-slot");
        slot.set_overflow(gtk::Overflow::Hidden);
        slot.add_named(tabs, Some("tabs"));
        slot.add_named(entry, Some("search"));
        slot.set_visible_child_name("tabs");
        entry.set_can_focus(false);
        entry.set_can_target(false);
        entry.update_state(&[gtk::accessible::State::Hidden(true)]);
        toggle.update_relation(&[gtk::accessible::Relation::Controls(&[entry.upcast_ref()])]);
        toggle.update_state(&[gtk::accessible::State::Expanded(Some(false))]);
        header.append(&slot);
        header
            .imp()
            .children
            .set(Children {
                slot,
                tabs: tabs.clone(),
                entry: entry.clone(),
                toggle: toggle.clone(),
            })
            .unwrap();
        header
    }

    pub(super) fn is_open(&self) -> bool {
        self.imp().open.get()
    }

    pub(super) fn set_open(&self, open: bool) {
        let imp = self.imp();
        if imp.open.replace(open) == open {
            return;
        }
        let c = imp.children.get().unwrap();
        c.entry.set_can_focus(open);
        c.entry.set_can_target(open);
        c.entry
            .update_state(&[gtk::accessible::State::Hidden(!open)]);
        c.tabs.set_can_focus(!open);
        c.tabs.set_can_target(!open);
        c.tabs.update_state(&[gtk::accessible::State::Hidden(open)]);
        // GtkStack owns mapping, clipping, focus invalidation and presentation.
        // Both pages have the same allocation, including throughout crossfade.
        c.slot
            .set_visible_child_name(if open { "search" } else { "tabs" });
        let label = if open {
            "Закрыть поиск"
        } else {
            "Поиск музыки"
        };
        c.toggle.set_tooltip_text(Some(label));
        c.toggle.set_icon_name(if open {
            "karin-x-symbolic"
        } else {
            "karin-search-symbolic"
        });
        c.toggle
            .update_property(&[gtk::accessible::Property::Label(label)]);
        c.toggle
            .update_state(&[gtk::accessible::State::Expanded(Some(open))]);
    }
}

pub(super) fn section_button(
    name: &str,
    icon_name: &str,
    text: &str,
    action: &str,
) -> (gtk::ToggleButton, gtk::Label) {
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    let label = gtk::Label::new(Some(text));
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.append(&icon);
    content.append(&label);
    let button = gtk::ToggleButton::builder()
        .child(&content)
        .action_name(action)
        .tooltip_text(text)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.add_css_class("music-header-tab");
    button.update_property(&[gtk::accessible::Property::Label(text)]);
    (button, label)
}
