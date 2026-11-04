// SPDX-License-Identifier: GPL-3.0-or-later
//! A clipped, full-size native surface translated by Libadwaita's animation.
//!
//! GtkRevealer reallocates its child during a slide. Wrapped labels and a
//! scrolled viewport consequently do not move like their fixed-size artwork.
//! This host always allocates the *whole* surface at its final size and applies
//! one transform to it. It owns geometry only, never panel intent or music.

use std::cell::{Cell, RefCell};
use std::sync::LazyLock;

use adw::prelude::*;
use gtk::{glib, graphene, gsk, subclass::prelude::*};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct SlidingPanel {
        pub child: RefCell<Option<gtk::Widget>>,
        pub progress: Cell<f64>,
        pub requested: Cell<bool>,
        pub compact: Cell<bool>,
        pub animation: RefCell<Option<adw::TimedAnimation>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SlidingPanel {
        const NAME: &'static str = "KarinSlidingMusicPanel";
        type Type = super::SlidingPanel;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for SlidingPanel {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: LazyLock<Vec<glib::ParamSpec>> = LazyLock::new(|| {
                vec![
                    glib::ParamSpecDouble::builder("progress")
                        .minimum(0.0)
                        .maximum(1.0)
                        .read_only()
                        .build(),
                    glib::ParamSpecBoolean::builder("compact-presentation")
                        .read_only()
                        .build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _: usize, spec: &glib::ParamSpec) -> glib::Value {
            match spec.name() {
                "progress" => self.progress.get().to_value(),
                "compact-presentation" => self.compact.get().to_value(),
                _ => unreachable!("unknown panel property"),
            }
        }

        fn dispose(&self) {
            if let Some(animation) = self.animation.borrow_mut().take() {
                animation.pause();
            }
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for SlidingPanel {
        fn focus(&self, direction: gtk::DirectionType) -> bool {
            let child = self.child.borrow().clone();
            self.requested.get() && child.is_some_and(|child| child.child_focus(direction))
        }

        fn contains(&self, x: f64, y: f64) -> bool {
            let obj = self.obj();
            x >= (1.0 - self.progress.get()) * f64::from(obj.width())
                && x < f64::from(obj.width())
                && y >= 0.0
                && y < f64::from(obj.height())
        }

        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            // The overlay supplies the available height and the responsive
            // width-request. Offscreen children must not enlarge the window.
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _: i32) {
            if let Some(child) = self.child.borrow().as_ref() {
                let offset = ((1.0 - self.progress.get()) * f64::from(width)) as f32;
                let transform = gsk::Transform::new().translate(&graphene::Point::new(offset, 0.0));
                child.allocate(width, height, -1, Some(transform));
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            snapshot.push_clip(&graphene::Rect::new(
                0.0,
                0.0,
                obj.width() as f32,
                obj.height() as f32,
            ));
            if self.progress.get() > 0.0
                && let Some(child) = self.child.borrow().as_ref()
            {
                obj.snapshot_child(child, snapshot);
            }
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct SlidingPanel(ObjectSubclass<imp::SlidingPanel>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl SlidingPanel {
    pub(in crate::ui) fn new(child: &impl IsA<gtk::Widget>) -> Self {
        let panel: Self = glib::Object::new();
        panel.set_overflow(gtk::Overflow::Hidden);
        child.set_parent(&panel);
        child.set_child_visible(false);
        panel.imp().child.replace(Some(child.clone().upcast()));
        let weak = panel.downgrade();
        let target = adw::CallbackAnimationTarget::new(move |progress| {
            if let Some(panel) = weak.upgrade() {
                panel.imp().progress.set(progress.clamp(0.0, 1.0));
                if let Some(child) = panel.imp().child.borrow().as_ref() {
                    child.set_child_visible(panel.reveals_child() || progress > 0.0);
                }
                panel.queue_allocate();
                panel.queue_draw();
                panel.notify("progress");
            }
        });
        let animation = adw::TimedAnimation::new(&panel, 0.0, 1.0, 240, target);
        animation.set_easing(adw::Easing::EaseOutCubic);
        panel.imp().animation.replace(Some(animation));
        panel
    }

    #[cfg(test)]
    pub(in crate::ui) fn surface(&self) -> Option<gtk::Widget> {
        self.imp().child.borrow().clone()
    }

    pub(in crate::ui) fn set_compact_presentation(&self, compact: bool) {
        if self.imp().compact.replace(compact) != compact {
            self.notify("compact-presentation");
        }
    }

    pub(in crate::ui) fn progress(&self) -> f64 {
        self.imp().progress.get()
    }

    pub(in crate::ui) fn reveals_child(&self) -> bool {
        self.imp().requested.get()
    }

    pub(in crate::ui) fn is_child_revealed(&self) -> bool {
        self.progress() >= 1.0 - f64::EPSILON
    }

    #[cfg(test)]
    pub(in crate::ui) fn transition_duration(&self) -> u32 {
        self.imp()
            .animation
            .borrow()
            .as_ref()
            .map_or(0, |a| a.duration())
    }

    pub(in crate::ui) fn set_reveal_child(&self, open: bool) {
        if self.imp().requested.replace(open) == open {
            return;
        }
        if open && let Some(child) = self.imp().child.borrow().as_ref() {
            child.set_child_visible(true);
        }
        if let Some(animation) = self.imp().animation.borrow().as_ref() {
            animation.pause();
            animation.set_value_from(self.progress());
            animation.set_value_to(if open { 1.0 } else { 0.0 });
            animation.play();
        }
    }

    /// Retiring a session hides private content synchronously; diagnostic
    /// suspension may use the same operation while retaining controller intent.
    pub(in crate::ui) fn hide_immediately(&self) {
        if !self.reveals_child() && self.progress() == 0.0 {
            return;
        }
        if let Some(animation) = self.imp().animation.borrow().as_ref() {
            animation.pause();
        }
        self.imp().requested.set(false);
        self.imp().progress.set(0.0);
        if let Some(child) = self.imp().child.borrow().as_ref() {
            child.set_child_visible(false);
        }
        self.queue_allocate();
        self.queue_draw();
        self.notify("progress");
    }
}
