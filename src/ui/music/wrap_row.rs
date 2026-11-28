// SPDX-License-Identifier: GPL-3.0-or-later
//! Native rows that wrap at the combined natural widths of their children.

use std::cell::Cell;

use gtk::{glib, prelude::*, subclass::prelude::*};

#[derive(Clone, Copy, Default)]
pub(super) enum RowAlignment {
    #[default]
    Start,
    Center,
}

#[derive(Clone, Copy, Default)]
struct Options {
    alignment: RowAlignment,
    column_spacing: i32,
    row_spacing: i32,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct WrapRow {
        pub(super) options: Cell<Options>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WrapRow {
        const NAME: &'static str = "KarinWrapRow";
        type Type = super::WrapRow;
        type ParentType = gtk::LayoutManager;
    }

    impl ObjectImpl for WrapRow {}

    impl LayoutManagerImpl for WrapRow {
        fn request_mode(&self, _widget: &gtk::Widget) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(
            &self,
            widget: &gtk::Widget,
            orientation: gtk::Orientation,
            for_size: i32,
        ) -> (i32, i32, i32, i32) {
            let options = self.options.get();
            let (minimum, natural) = if orientation == gtk::Orientation::Horizontal {
                children(widget).enumerate().fold(
                    (0_i32, 0_i32),
                    |(minimum, natural), (index, child)| {
                        let (min, nat, _, _) = child.measure(orientation, -1);
                        let spacing = if index == 0 {
                            0
                        } else {
                            options.column_spacing
                        };
                        (
                            minimum.max(min),
                            natural.saturating_add(spacing).saturating_add(nat),
                        )
                    },
                )
            } else {
                let width = if for_size < 0 { i32::MAX } else { for_size };
                lines(widget, width, options.column_spacing)
                    .iter()
                    .enumerate()
                    .fold((0_i32, 0_i32), |(minimum, natural), (index, line)| {
                        let spacing = if index == 0 { 0 } else { options.row_spacing };
                        (
                            minimum
                                .saturating_add(spacing)
                                .saturating_add(line.minimum_height),
                            natural
                                .saturating_add(spacing)
                                .saturating_add(line.natural_height),
                        )
                    })
            };
            (minimum, natural, -1, -1)
        }

        fn allocate(&self, widget: &gtk::Widget, width: i32, _height: i32, _baseline: i32) {
            let options = self.options.get();
            let rtl = widget.direction() == gtk::TextDirection::Rtl;
            let mut top = 0;
            for line in lines(widget, width, options.column_spacing) {
                let start = match options.alignment {
                    RowAlignment::Center => (width - line.width) / 2,
                    RowAlignment::Start if rtl => width - line.width,
                    RowAlignment::Start => 0,
                };
                let mut offset = 0;
                for (child, width) in line.children {
                    let left = if rtl {
                        start + line.width - offset - width
                    } else {
                        start + offset
                    };
                    let transform = gtk::gsk::Transform::new()
                        .translate(&gtk::graphene::Point::new(left as f32, top as f32));
                    child.allocate(width, line.natural_height, -1, Some(transform));
                    offset += width + options.column_spacing;
                }
                top += line.natural_height + options.row_spacing;
            }
        }
    }

    fn children(widget: &gtk::Widget) -> impl Iterator<Item = gtk::Widget> {
        std::iter::successors(widget.first_child(), WidgetExt::next_sibling)
            .filter(WidgetExt::should_layout)
    }

    #[derive(Default)]
    struct Line {
        children: Vec<(gtk::Widget, i32)>,
        width: i32,
        minimum_height: i32,
        natural_height: i32,
    }

    fn lines(widget: &gtk::Widget, width: i32, column_spacing: i32) -> Vec<Line> {
        let mut lines = Vec::new();
        let mut line = Line::default();
        for child in children(widget) {
            let (minimum, natural, _, _) = child.measure(gtk::Orientation::Horizontal, -1);
            let child_width = natural.min(width).max(minimum);
            if !line.children.is_empty() {
                if line
                    .width
                    .saturating_add(column_spacing)
                    .saturating_add(child_width)
                    > width
                {
                    lines.push(std::mem::take(&mut line));
                } else {
                    line.width += column_spacing;
                }
            }
            let (minimum_height, natural_height, _, _) =
                child.measure(gtk::Orientation::Vertical, child_width);
            line.width += child_width;
            line.minimum_height = line.minimum_height.max(minimum_height);
            line.natural_height = line.natural_height.max(natural_height);
            line.children.push((child, child_width));
        }
        if !line.children.is_empty() {
            lines.push(line);
        }
        lines
    }
}

glib::wrapper! {
    pub struct WrapRow(ObjectSubclass<imp::WrapRow>)
        @extends gtk::LayoutManager;
}

impl WrapRow {
    pub(super) fn new(alignment: RowAlignment, column_spacing: i32, row_spacing: i32) -> Self {
        let layout = glib::Object::new::<Self>();
        layout.imp().options.set(Options {
            alignment,
            column_spacing,
            row_spacing,
        });
        layout
    }
}
