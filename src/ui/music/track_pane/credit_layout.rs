// SPDX-License-Identifier: GPL-3.0-or-later
//! A centered, naturally wrapping line of native artist credit controls.

use gtk::{glib, prelude::*};

mod imp {
    use super::*;
    use gtk::subclass::prelude::*;

    #[derive(Default)]
    pub struct ArtistCreditLayout;

    #[glib::object_subclass]
    impl ObjectSubclass for ArtistCreditLayout {
        const NAME: &'static str = "KarinArtistCreditLayout";
        type Type = super::ArtistCreditLayout;
        type ParentType = gtk::LayoutManager;
    }

    impl ObjectImpl for ArtistCreditLayout {}

    impl LayoutManagerImpl for ArtistCreditLayout {
        fn request_mode(&self, _widget: &gtk::Widget) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(
            &self,
            widget: &gtk::Widget,
            orientation: gtk::Orientation,
            for_size: i32,
        ) -> (i32, i32, i32, i32) {
            let (minimum, natural) = if orientation == gtk::Orientation::Horizontal {
                credits(widget).fold((0, 0), |(minimum, natural), credit| {
                    let (min, nat, _, _) = credit.measure(orientation, -1);
                    (minimum.max(min), natural + nat)
                })
            } else {
                let width = if for_size < 0 { i32::MAX } else { for_size };
                lines(widget, width)
                    .iter()
                    .fold((0, 0), |(minimum, natural), line| {
                        (minimum + line.minimum_height, natural + line.natural_height)
                    })
            };
            (minimum, natural, -1, -1)
        }

        fn allocate(&self, widget: &gtk::Widget, width: i32, _height: i32, _baseline: i32) {
            let rtl = widget.direction() == gtk::TextDirection::Rtl;
            let mut top = 0;
            for line in lines(widget, width) {
                let start = (width - line.width) / 2;
                let mut offset = 0;
                for (credit, width) in line.credits {
                    let left = if rtl {
                        start + line.width - offset - width
                    } else {
                        start + offset
                    };
                    let transform = gtk::gsk::Transform::new()
                        .translate(&gtk::graphene::Point::new(left as f32, top as f32));
                    credit.allocate(width, line.natural_height, -1, Some(transform));
                    offset += width;
                }
                top += line.natural_height;
            }
        }
    }

    fn credits(widget: &gtk::Widget) -> impl Iterator<Item = gtk::Widget> {
        std::iter::successors(widget.first_child(), WidgetExt::next_sibling)
            .filter(WidgetExt::should_layout)
    }

    #[derive(Default)]
    struct Line {
        credits: Vec<(gtk::Widget, i32)>,
        width: i32,
        minimum_height: i32,
        natural_height: i32,
    }

    fn lines(widget: &gtk::Widget, width: i32) -> Vec<Line> {
        let mut lines = Vec::new();
        let mut line = Line::default();
        for credit in credits(widget) {
            let (minimum, natural, _, _) = credit.measure(gtk::Orientation::Horizontal, -1);
            let credit_width = natural.min(width).max(minimum);
            if !line.credits.is_empty() && line.width + credit_width > width {
                lines.push(std::mem::take(&mut line));
            }
            let (minimum_height, natural_height, _, _) =
                credit.measure(gtk::Orientation::Vertical, credit_width);
            line.width += credit_width;
            line.minimum_height = line.minimum_height.max(minimum_height);
            line.natural_height = line.natural_height.max(natural_height);
            line.credits.push((credit, credit_width));
        }
        if !line.credits.is_empty() {
            lines.push(line);
        }
        lines
    }
}

glib::wrapper! {
    pub struct ArtistCreditLayout(ObjectSubclass<imp::ArtistCreditLayout>)
        @extends gtk::LayoutManager;
}

impl ArtistCreditLayout {
    pub(super) fn new() -> Self {
        glib::Object::new()
    }
}
