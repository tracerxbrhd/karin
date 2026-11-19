// SPDX-License-Identifier: GPL-3.0-or-later
//! Measure the footer for its available width without imposing the desktop
//! grid's minimum on the window before the compact allocation can run.

use gtk::{glib, prelude::*, subclass::prelude::*};
use std::{cell::OnceCell, rc::Rc};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FooterLayout {
        pub(super) arrange: OnceCell<Rc<dyn Fn(i32)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FooterLayout {
        const NAME: &'static str = "KarinFooterLayout";
        type Type = super::FooterLayout;
        type ParentType = gtk::LayoutManager;
    }

    impl ObjectImpl for FooterLayout {}

    impl LayoutManagerImpl for FooterLayout {
        fn request_mode(&self, _: &gtk::Widget) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(
            &self,
            widget: &gtk::Widget,
            direction: gtk::Orientation,
            for_size: i32,
        ) -> (i32, i32, i32, i32) {
            let Some(child) = widget.first_child() else {
                return (0, 0, -1, -1);
            };
            let (current_min, natural, ..) = child.measure(gtk::Orientation::Horizontal, -1);
            if direction == gtk::Orientation::Horizontal {
                return (0, natural, -1, -1);
            }
            // During a shrink the grid still has the desktop attachments until
            // allocate(). GTK only accepts a height-for-width request at or
            // above that layout's minimum. The changed attachments queue their
            // own height measurement at the newly allocated compact width.
            let width = if for_size < 0 {
                -1
            } else {
                for_size.max(current_min)
            };
            child.measure(direction, width)
        }

        fn allocate(&self, widget: &gtk::Widget, width: i32, height: i32, baseline: i32) {
            if let Some(arrange) = self.arrange.get() {
                arrange(width);
            }
            if let Some(child) = widget.first_child() {
                child.allocate(width, height, baseline, None);
            }
        }
    }
}

glib::wrapper! {
    pub struct FooterLayout(ObjectSubclass<imp::FooterLayout>) @extends gtk::LayoutManager;
}

pub(super) fn install(footer: &gtk::Box, arrange: Rc<dyn Fn(i32)>) {
    let layout: FooterLayout = glib::Object::new();
    layout
        .imp()
        .arrange
        .set(arrange)
        .unwrap_or_else(|_| unreachable!());
    footer.set_layout_manager(Some(layout));
}
