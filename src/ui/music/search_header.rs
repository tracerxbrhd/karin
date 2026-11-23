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
        header.set_spacing(6);
        header.set_hexpand(true);
        header.set_halign(gtk::Align::Center);
        // The short entry must not enlarge the tabs' natural slot as text is
        // entered. GtkStack measures both pages, including the hidden tabs, so
        // font/translation changes automatically update the native allocation.
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
        header.append(toggle);
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
