// SPDX-License-Identifier: GPL-3.0-or-later
//! Measure the responsive footer and place it in the native ToolbarView.

use gtk::{glib, prelude::*, subclass::prelude::*};
use std::{cell::OnceCell, rc::Rc};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FooterLayout {
        pub(super) before_toolbar_allocate: OnceCell<Rc<dyn Fn(i32)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FooterLayout {
        const NAME: &'static str = "KarinFooterLayout";
        type Type = super::FooterLayout;
        type ParentType = gtk::LayoutManager;
    }

    impl ObjectImpl for FooterLayout {}

    impl LayoutManagerImpl for FooterLayout {
        fn request_mode(&self, widget: &gtk::Widget) -> gtk::SizeRequestMode {
            if self.before_toolbar_allocate.get().is_some()
                && let Some(toolbar) = widget.first_child()
            {
                return toolbar.request_mode();
            }
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
            if self.before_toolbar_allocate.get().is_some() {
                return child.measure(direction, for_size);
            }
            let (current_min, natural, ..) = child.measure(gtk::Orientation::Horizontal, -1);
            if direction == gtk::Orientation::Horizontal {
                return (0, natural, -1, -1);
            }
            // While probing a possible width, GTK requires measurements to
            // respect the minimum of the current grid arrangement.
            let width = if for_size < 0 {
                -1
            } else {
                for_size.max(current_min)
            };
            child.measure(direction, width)
        }

        fn allocate(&self, widget: &gtk::Widget, width: i32, height: i32, baseline: i32) {
            if let Some(arrange) = self.before_toolbar_allocate.get() {
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

pub(in crate::ui::music) fn allow_child_to_shrink(container: &gtk::Box) {
    container.set_layout_manager(Some(glib::Object::new::<FooterLayout>()));
}

/// Libadwaita 1.5 measures native bottom bars at width -1, even during
/// allocation. Arrange the grid at the actual incoming width before
/// ToolbarView decides how much height to give it in that very frame.
pub(in crate::ui::music) fn toolbar_frame(
    toolbar: &adw::ToolbarView,
    arrange: Rc<dyn Fn(i32)>,
) -> gtk::Box {
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let layout: FooterLayout = glib::Object::new();
    layout
        .imp()
        .before_toolbar_allocate
        .set(arrange)
        .unwrap_or_else(|_| unreachable!());
    frame.set_layout_manager(Some(layout));
    frame.append(toolbar);
    frame
}
