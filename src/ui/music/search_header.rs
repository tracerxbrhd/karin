// SPDX-License-Identifier: GPL-3.0-or-later
//! The tabs and global search entry share one naturally measured navigation slot.
use adw::prelude::*;
use gtk::{glib, graphene, gsk, subclass::prelude::*};
use std::cell::{Cell, OnceCell};

const TOGGLE_GAP: i32 = 6;

#[derive(Debug)]
struct Children {
    tabs: gtk::Viewport,
    field: gtk::Viewport,
    toggle: gtk::Button,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct SearchHeader {
        pub(super) children: OnceCell<Children>,
        pub(super) animation: OnceCell<adw::TimedAnimation>,
        pub(super) open: Cell<bool>,
        pub(super) progress: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SearchHeader {
        const NAME: &'static str = "KarinSearchHeader";
        type Type = super::SearchHeader;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for SearchHeader {
        fn dispose(&self) {
            if let Some(animation) = self.animation.get() {
                animation.pause();
            }
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for SearchHeader {
        fn measure(&self, orientation: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            let Some(c) = self.children.get() else {
                return (0, 0, -1, -1);
            };
            // The viewport can be unmapped while its child still determines
            // the slot's request (including font, locale and theme changes).
            let (tabs_min, tabs_nat, ..) = c.tabs.child().unwrap().measure(orientation, -1);
            let (toggle_min, toggle_nat, ..) = c.toggle.measure(orientation, -1);
            if orientation == gtk::Orientation::Horizontal {
                return (
                    tabs_min + TOGGLE_GAP + toggle_min,
                    tabs_nat + TOGGLE_GAP + toggle_nat,
                    -1,
                    -1,
                );
            }
            let (field_min, field_nat, ..) = c.field.child().unwrap().measure(orientation, -1);
            (
                tabs_min.max(field_min).max(toggle_min),
                tabs_nat.max(field_nat).max(toggle_nat),
                -1,
                -1,
            )
        }

        fn size_allocate(&self, width: i32, height: i32, _: i32) {
            let Some(c) = self.children.get() else { return };
            let along = gtk::Orientation::Horizontal;
            let button = c.toggle.measure(along, -1).1.min(width);
            let slot = (width - button - TOGGLE_GAP).max(0);
            let field = (f64::from(slot) * self.progress.get()).round() as i32;
            let tabs = slot - field;
            let allocate = |child: &gtk::Widget, x: i32, w: i32| {
                child.set_child_visible(w > 0);
                if w > 0 {
                    let x = if self.obj().direction() == gtk::TextDirection::Rtl {
                        width - x - w
                    } else {
                        x
                    };
                    let transform =
                        gsk::Transform::new().translate(&graphene::Point::new(x as f32, 0.0));
                    child.allocate(w, height, -1, Some(transform));
                }
            };
            // Slide the whole strip behind the group's leading clip. Its
            // labels keep the same allocation throughout a transition.
            allocate(c.tabs.upcast_ref(), -field, if tabs > 0 { slot } else { 0 });
            // Map the entry even on the first animation frame so Ctrl+F
            // followed immediately by typing reaches the same GtkText.
            allocate(
                c.field.upcast_ref(),
                tabs,
                field.max(i32::from(self.open.get() && slot > 0)),
            );
            allocate(c.toggle.upcast_ref(), width - button, button);
        }
    }
}

glib::wrapper! {
    pub struct SearchHeader(ObjectSubclass<imp::SearchHeader>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl SearchHeader {
    pub(super) fn new(tabs: &gtk::Box, entry: &gtk::SearchEntry, toggle: &gtk::Button) -> Self {
        let header: Self = glib::Object::new();
        // The toolbar expands; GTK aligns our natural-width group inside it
        // and only reduces its allocation when the window actually needs it.
        header.set_hexpand(true);
        header.set_halign(gtk::Align::Center);
        header.set_overflow(gtk::Overflow::Hidden);
        // GTK propagates resize requests between size-group members, including
        // changes to the unmapped tabs while the field occupies their slot.
        // Binding height leaves horizontal measurement entirely to the tabs.
        let row_height = gtk::SizeGroup::new(gtk::SizeGroupMode::Vertical);
        row_height.add_widget(tabs);
        row_height.add_widget(entry);
        let slot = |child: &gtk::Widget| {
            let viewport = gtk::Viewport::builder()
                .scroll_to_focus(false)
                .child(child)
                .build();
            viewport.set_parent(&header);
            viewport
        };
        let tabs = slot(tabs.upcast_ref());
        entry.set_width_chars(1);
        let field = slot(entry.upcast_ref());
        entry.set_can_focus(false);
        field.set_child_visible(false);
        field.set_can_focus(false);
        field.set_can_target(false);
        field.update_state(&[gtk::accessible::State::Hidden(true)]);
        toggle.update_relation(&[gtk::accessible::Relation::Controls(&[entry.upcast_ref()])]);
        toggle.update_state(&[gtk::accessible::State::Expanded(Some(false))]);
        toggle.set_parent(&header);
        header
            .imp()
            .children
            .set(Children {
                tabs,
                field,
                toggle: toggle.clone(),
            })
            .unwrap();
        let weak = header.downgrade();
        let target = adw::CallbackAnimationTarget::new(move |value| {
            if let Some(header) = weak.upgrade() {
                header.imp().progress.set(value);
                let children = header.imp().children.get().unwrap();
                children.tabs.set_opacity(1.0 - value);
                header.queue_allocate();
            }
        });
        let animation = adw::TimedAnimation::new(&header, 0.0, 1.0, 220, target);
        animation.set_easing(adw::Easing::EaseOutCubic);
        header.imp().animation.set(animation).unwrap();
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
        let (incoming, outgoing) = if open {
            (&c.field, &c.tabs)
        } else {
            (&c.tabs, &c.field)
        };
        incoming.set_child_visible(true);
        incoming.set_can_focus(true);
        incoming.update_state(&[gtk::accessible::State::Hidden(false)]);
        outgoing.set_can_focus(false);
        outgoing.update_state(&[gtk::accessible::State::Hidden(true)]);
        // A tab still under the pointer during the slide can finish its
        // navigation. The clipped allocations give it a separate hit area.
        c.field.set_can_target(open);
        let label = if open {
            "Закрыть поиск"
        } else {
            "Поиск музыки"
        };
        c.toggle.set_tooltip_text(Some(label));
        c.toggle.set_icon_name(if open {
            "window-close-symbolic"
        } else {
            "system-search-symbolic"
        });
        c.toggle
            .update_property(&[gtk::accessible::Property::Label(label)]);
        c.toggle
            .update_state(&[gtk::accessible::State::Expanded(Some(open))]);
        let animation = imp.animation.get().unwrap();
        animation.pause();
        animation.set_value_from(imp.progress.get());
        animation.set_value_to(if open { 1.0 } else { 0.0 });
        animation.play();
    }
}
