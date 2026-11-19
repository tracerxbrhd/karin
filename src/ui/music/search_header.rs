// SPDX-License-Identifier: GPL-3.0-or-later
//! Allocate the tabs, search entry and toggle in one fixed-height navigation row.
use adw::prelude::*;
use gtk::{glib, graphene, gsk, subclass::prelude::*};
use std::cell::{Cell, OnceCell};

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
            let (tabs_min, tabs_nat, ..) = c.tabs.measure(orientation, -1);
            let (toggle_min, toggle_nat, ..) = c.toggle.measure(orientation, -1);
            if orientation == gtk::Orientation::Horizontal {
                return (tabs_min + 6 + toggle_min, tabs_nat + 6 + toggle_nat, -1, -1);
            }
            let (field_min, field_nat, ..) = c.field.measure(orientation, -1);
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
            // Measure the entries themselves even when their clipping viewport
            // is child-invisible at either endpoint of the transition.
            let tabs = c
                .tabs
                .child()
                .unwrap()
                .measure(along, -1)
                .1
                .min((width - button - 6).max(0));
            let field_min = c.field.child().unwrap().measure(along, -1).0;
            let keep_tabs = width >= tabs + field_min + button + 12;
            let start = (width - tabs - 6 - button).max(0) / 2;
            let progress = self.progress.get();
            let blend = |from: i32, to: i32| {
                (f64::from(from) + f64::from(to - from) * progress).round() as i32
            };
            let tabs_x = blend(start, 0);
            let tabs_w = blend(tabs, if keep_tabs { tabs } else { 0 });
            let button_x = blend(start + tabs + 6, width - button);
            let field_x = tabs_x + tabs_w + blend(6, if keep_tabs { 6 } else { 0 });
            let field_w = (button_x - blend(0, 6) - field_x).max(0);
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
            c.tabs.set_can_focus(!self.open.get() || keep_tabs);
            allocate(c.tabs.upcast_ref(), tabs_x, tabs_w);
            allocate(
                c.field.upcast_ref(),
                field_x,
                field_w.max(i32::from(self.open.get())),
            );
            allocate(c.toggle.upcast_ref(), button_x, button);
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
        header.set_hexpand(true);
        header.set_overflow(gtk::Overflow::Hidden);
        let slot = |child: &gtk::Widget| {
            let viewport = gtk::Viewport::builder()
                .scroll_to_focus(false)
                .child(child)
                .build();
            viewport.set_parent(&header);
            viewport
        };
        let tabs = slot(tabs.upcast_ref());
        let field = slot(entry.upcast_ref());
        entry.set_can_focus(false);
        field.set_child_visible(false);
        field.set_can_focus(false);
        field.set_can_target(false);
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
        if open {
            c.field.set_child_visible(true);
        }
        c.field.set_can_focus(open);
        c.field.set_can_target(open);
        let label = if open {
            "Закрыть поиск"
        } else {
            "Поиск музыки"
        };
        c.toggle.set_tooltip_text(Some(label));
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
