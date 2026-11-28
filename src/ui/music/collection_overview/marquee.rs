// SPDX-License-Identifier: GPL-3.0-or-later
//! Reveal clipped preview metadata without changing the row's allocation.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk::{glib, prelude::*};

const START_DELAY_MS: f64 = 280.0;
const END_HOLD_MS: f64 = 900.0;
const PIXELS_PER_MS: f64 = 0.055;

struct Line {
    normal: gtk::Label,
    runner: gtk::Fixed,
    full: gtk::Label,
}

impl Line {
    fn new(normal: gtk::Label) -> Option<Self> {
        let parent = normal.parent().and_downcast::<gtk::Box>()?;
        let previous = normal.prev_sibling();
        let clip = gtk::Overlay::new();
        clip.set_hexpand(true);
        parent.remove(&normal);
        clip.set_child(Some(&normal));
        parent.insert_child_after(&clip, previous.as_ref());

        let full = gtk::Label::new(Some(&normal.text()));
        for class in normal.css_classes() {
            full.add_css_class(&class);
        }
        full.set_xalign(0.0);
        full.update_state(&[gtk::accessible::State::Hidden(true)]);
        let runner = gtk::Fixed::new();
        runner.set_widget_name("music-collection-marquee-runner");
        runner.set_can_target(false);
        runner.put(&full, 0.0, 0.0);
        clip.add_overlay(&runner);
        clip.set_measure_overlay(&runner, false);
        clip.set_clip_overlay(&runner, true);
        runner.set_visible(false);
        Some(Self {
            normal,
            runner,
            full,
        })
    }

    fn distance(&self) -> f64 {
        if !self.normal.layout().is_ellipsized() {
            return 0.0;
        }
        let layout = self.normal.layout().copy();
        layout.set_width(-1);
        f64::from((layout.pixel_size().0 - self.normal.width()).max(0))
    }

    fn show_offset(&self, offset: f64, distance: f64) {
        if distance <= 0.0 {
            self.reset();
            return;
        }
        let offset = offset.min(distance);
        let x = if self.normal.direction() == gtk::TextDirection::Rtl {
            offset - distance
        } else {
            -offset
        };
        self.normal.set_opacity(0.0);
        self.runner.set_visible(true);
        self.runner.move_(&self.full, x, 0.0);
    }

    fn reset(&self) {
        self.normal.set_opacity(1.0);
        self.runner.set_visible(false);
    }
}

struct Marquee {
    lines: Vec<Line>,
    hovered: Cell<bool>,
    keyboard_focus: Cell<bool>,
    tick: RefCell<Option<gtk::TickCallbackId>>,
}

impl Marquee {
    fn update(self: &Rc<Self>, row: &gtk::Widget) {
        if !self.hovered.get() && !self.keyboard_focus.get() {
            self.stop();
        } else if row.is_mapped()
            && row.settings().is_gtk_enable_animations()
            && self.tick.borrow().is_none()
            && self.lines.iter().any(|line| line.distance() > 0.0)
        {
            let start = Cell::new(None);
            let weak = Rc::downgrade(self);
            let tick = row.add_tick_callback(move |row, clock| {
                let Some(this) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let time = clock.frame_time();
                let started = start.get().unwrap_or_else(|| {
                    start.set(Some(time));
                    time
                });
                let elapsed_ms = (time - started) as f64 / 1000.0 - START_DELAY_MS;
                let animations = row.settings().is_gtk_enable_animations();
                let mut distance = 0.0_f64;
                if elapsed_ms >= 0.0 && animations {
                    for line in &this.lines {
                        let extent = line.distance();
                        distance = distance.max(extent);
                        line.show_offset(elapsed_ms * PIXELS_PER_MS, extent);
                    }
                }
                if !animations
                    || (elapsed_ms >= 0.0
                        && (distance <= 0.0
                            || elapsed_ms >= distance / PIXELS_PER_MS + END_HOLD_MS))
                {
                    this.tick.borrow_mut().take();
                    this.reset();
                    return glib::ControlFlow::Break;
                }
                glib::ControlFlow::Continue
            });
            self.tick.replace(Some(tick));
        }
    }

    fn reset(&self) {
        for line in &self.lines {
            line.reset();
        }
    }

    fn stop(&self) {
        if let Some(tick) = self.tick.borrow_mut().take() {
            tick.remove();
        }
        self.reset();
    }
}

pub(super) fn install(row: &gtk::ListBoxRow) {
    let lines = ["music-track-title", "music-track-artist"]
        .into_iter()
        .filter_map(|class| metadata(row.upcast_ref(), class))
        .filter_map(Line::new)
        .collect();
    let state = Rc::new(Marquee {
        lines,
        hovered: Cell::new(false),
        keyboard_focus: Cell::new(false),
        tick: RefCell::new(None),
    });
    let motion = gtk::EventControllerMotion::new();
    motion.connect_enter({
        let state = Rc::clone(&state);
        move |event, _, _| {
            if let Some(row) = event.widget() {
                state.hovered.set(true);
                state.update(&row);
            }
        }
    });
    motion.connect_leave({
        let state = Rc::clone(&state);
        move |event| {
            if let Some(row) = event.widget() {
                state.hovered.set(false);
                state.update(&row);
            }
        }
    });
    row.add_controller(motion);

    let focus = gtk::EventControllerFocus::new();
    focus.connect_enter({
        let state = Rc::clone(&state);
        move |event| {
            if let Some(row) = event.widget()
                && row
                    .root()
                    .and_downcast::<gtk::Window>()
                    .is_some_and(|window| window.gets_focus_visible())
            {
                state.keyboard_focus.set(true);
                state.update(&row);
            }
        }
    });
    focus.connect_leave({
        let state = Rc::clone(&state);
        move |event| {
            if let Some(row) = event.widget() {
                state.keyboard_focus.set(false);
                state.update(&row);
            }
        }
    });
    row.add_controller(focus);
    row.connect_unmap(move |_| {
        state.hovered.set(false);
        state.keyboard_focus.set(false);
        state.stop();
    });
}

fn metadata(widget: &gtk::Widget, class: &str) -> Option<gtk::Label> {
    if widget.has_css_class(class) {
        return widget.clone().downcast().ok();
    }
    std::iter::successors(widget.first_child(), gtk::Widget::next_sibling)
        .find_map(|child| metadata(&child, class))
}
