// SPDX-License-Identifier: GPL-3.0-or-later
//! Vertical queue reordering within the shared, clipped queue viewport.

use super::{Controller, PlaybackContext, queue_action_is_current};
use crate::queue::OccurrenceId;
use gtk::{gdk, glib, graphene, prelude::*};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

const EDGE_SCROLL_ZONE: f64 = 32.0;
const EDGE_SCROLL_SPEED: f64 = 480.0;

pub(in crate::ui::music) struct QueueReorder {
    overlay: gtk::Overlay,
    cards: gtk::Fixed,
    gesture: gtk::GestureDrag,
    rows: RefCell<Vec<QueueRow>>,
    drag: RefCell<Option<ReorderDrag>>,
    tick: RefCell<Option<gtk::TickCallbackId>>,
}

struct QueueRow {
    row: glib::WeakRef<gtk::ListBoxRow>,
    handle: glib::WeakRef<gtk::Image>,
    occurrence: OccurrenceId,
}

struct ReorderDrag {
    generation: u64,
    context: Option<PlaybackContext>,
    source: usize,
    destination: usize,
    pointer: (f64, f64),
    scroll_origin: f64,
    grabbed: DragCard,
    displaced: BTreeMap<usize, DragCard>,
    bounds: Vec<graphene::Rect>,
    last_frame: Option<i64>,
}

struct DragCard {
    row: gtk::ListBoxRow,
    picture: gtk::Picture,
    bounds: graphene::Rect,
    y: f64,
}

impl DragCard {
    fn new(cards: &gtk::Fixed, row: gtk::ListBoxRow, bounds: graphene::Rect) -> Self {
        let image = gtk::WidgetPaintable::new(Some(&row)).current_image();
        let picture = gtk::Picture::for_paintable(&image);
        picture.set_can_shrink(false);
        picture.set_can_target(false);
        picture.update_state(&[gtk::accessible::State::Hidden(true)]);
        let y = f64::from(bounds.y());
        cards.put(&picture, f64::from(bounds.x()), y);
        row.set_opacity(0.0);
        Self {
            row,
            picture,
            bounds,
            y,
        }
    }

    fn move_to(&mut self, cards: &gtk::Fixed, y: f64) {
        if self.y != y {
            self.y = y;
            cards.move_(&self.picture, f64::from(self.bounds.x()), y);
        }
    }

    fn restore(self, cards: &gtk::Fixed) {
        self.row.set_opacity(1.0);
        cards.remove(&self.picture);
    }
}

impl QueueReorder {
    pub(in crate::ui::music) fn new(list: &gtk::ListBox) -> Self {
        let overlay = gtk::Overlay::new();
        let cards = gtk::Fixed::new();
        cards.set_widget_name("music-queue-reorder-layer");
        cards.set_can_target(false);
        let gesture = gtk::GestureDrag::new();
        gesture.set_button(gdk::BUTTON_PRIMARY);
        gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
        overlay.set_child(Some(list));
        overlay.add_overlay(&cards);
        overlay.set_measure_overlay(&cards, false);
        overlay.set_clip_overlay(&cards, true);
        Self {
            overlay,
            cards,
            gesture,
            rows: RefCell::default(),
            drag: RefCell::default(),
            tick: RefCell::default(),
        }
    }

    pub(in crate::ui::music) fn widget(&self) -> &gtk::Overlay {
        &self.overlay
    }

    pub(in crate::ui::music) fn clear_rows(&self) {
        // An outstanding press belongs to the retired presentation, even if
        // the replacement mounts a different grip at the same coordinates.
        self.gesture.reset();
        self.clear_drag();
        self.rows.borrow_mut().clear();
    }

    pub(super) fn add_row(
        &self,
        row: &gtk::ListBoxRow,
        handle: &gtk::Image,
        occurrence: OccurrenceId,
    ) {
        self.rows.borrow_mut().push(QueueRow {
            row: row.downgrade(),
            handle: handle.downgrade(),
            occurrence,
        });
    }

    fn grip_at(&self, scroll: &gtk::ScrolledWindow, x: f64, y: f64) -> Option<usize> {
        viewport_at(scroll, x, y)?;
        let point = graphene::Point::new(x as f32, y as f32);
        self.rows.borrow().iter().position(|entry| {
            entry.handle.upgrade().is_some_and(|grip| {
                grip.is_mapped()
                    && grip
                        .compute_bounds(scroll)
                        .is_some_and(|bounds| bounds.contains_point(&point))
            })
        })
    }

    fn drag_from(
        &self,
        this: &Controller,
        scroll: &gtk::ScrolledWindow,
        x: f64,
        y: f64,
    ) -> Option<()> {
        let source = self.grip_at(scroll, x, y)?;
        let rows = self.rows.borrow();
        if rows.len() < 2 {
            return None;
        }
        let bounds = rows
            .iter()
            .map(|entry| entry.row.upgrade()?.compute_bounds(&self.cards))
            .collect::<Option<Vec<_>>>()?;
        let row = rows[source].row.upgrade()?;
        let state = this.model.borrow();
        let context = state.queue.context().cloned();
        let generation = state.generation;
        drop(state);
        if !queue_action_is_current(this, generation, &context) {
            return None;
        }
        let grabbed = DragCard::new(&self.cards, row, bounds[source]);
        grabbed
            .picture
            .set_widget_name("music-queue-reorder-grabbed");
        grabbed.picture.add_css_class("music-queue-reorder-grabbed");
        // Preserve the grab position as the viewport scrolls. The preview's x
        // coordinate always remains the original row's x coordinate.
        let scroll_origin = grabbed.y - y - scroll.vadjustment().value();
        scroll.set_cursor_from_name(Some("grabbing"));
        if let Some(handle) = rows[source].handle.upgrade() {
            handle.set_cursor_from_name(Some("grabbing"));
        }
        self.drag.replace(Some(ReorderDrag {
            generation,
            context,
            source,
            destination: source,
            pointer: (x, y),
            scroll_origin,
            grabbed,
            displaced: BTreeMap::new(),
            bounds,
            last_frame: None,
        }));
        Some(())
    }

    fn update(&self, scroll: &gtk::ScrolledWindow, x: f64, y: f64) {
        if let Some(drag) = self.drag.borrow_mut().as_mut() {
            drag.pointer = (x, y);
            if viewport_at(scroll, x, y).is_some() {
                drag.follow_pointer(
                    &self.cards,
                    &self.rows.borrow(),
                    scroll.vadjustment().value(),
                );
            }
        }
    }

    fn clear_drag(&self) {
        if let Some(tick) = self.tick.borrow_mut().take() {
            tick.remove();
        }
        if let Some(drag) = self.drag.borrow_mut().take() {
            self.restore_drag(drag);
        }
    }

    fn restore_drag(&self, drag: ReorderDrag) {
        if let Some(scroll) = self.overlay.ancestor(gtk::ScrolledWindow::static_type()) {
            scroll.set_cursor(None);
        }
        if let Some(handle) = self.rows.borrow()[drag.source].handle.upgrade() {
            handle.set_cursor_from_name(Some("grab"));
        }
        drag.grabbed.restore(&self.cards);
        for card in drag.displaced.into_values() {
            card.restore(&self.cards);
        }
    }

    fn finish(&self, this: &Rc<Controller>, scroll: &gtk::ScrolledWindow, x: f64, y: f64) {
        self.update(scroll, x, y);
        let drop = self.drag.borrow().as_ref().and_then(|drag| {
            let from = drag.source;
            let to = drag.destination;
            let rows = self.rows.borrow();
            (viewport_at(scroll, x, y).is_some()
                && from != to
                && queue_action_is_current(this, drag.generation, &drag.context))
            .then(|| {
                (
                    rows[from].occurrence.clone(),
                    rows[to].occurrence.clone(),
                    to > from,
                )
            })
        });
        self.clear_drag();
        if let Some((from, to, after)) = drop {
            this.move_queue_occurrence(&from, &to, after);
        }
    }

    fn start_ticks(&self, this: &Rc<Controller>, scroll: &gtk::ScrolledWindow) {
        let weak = Rc::downgrade(this);
        self.tick
            .replace(Some(scroll.add_tick_callback(move |scroll, frame| {
                let Some(this) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let reorder = &this.widgets.queue_reorder;
                let mut state = reorder.drag.borrow_mut();
                let Some(drag) = state.as_mut() else {
                    return glib::ControlFlow::Break;
                };
                if !drag.grabbed.row.is_mapped()
                    || !queue_action_is_current(&this, drag.generation, &drag.context)
                {
                    let drag = state.take().expect("active queue drag");
                    reorder.restore_drag(drag);
                    reorder.tick.borrow_mut().take();
                    return glib::ControlFlow::Break;
                }

                let time = frame.frame_time();
                let elapsed = drag.last_frame.replace(time).map_or(0.016, |last| {
                    (time - last).clamp(0, 50_000) as f64 / 1_000_000.0
                });
                let adjustment = scroll.vadjustment();
                let current = adjustment.value();
                if let Some(viewport) = viewport_at(scroll, drag.pointer.0, drag.pointer.1) {
                    let speed = drag.edge_speed(&viewport, current);
                    let next = current + speed * elapsed;
                    if speed != 0.0 {
                        adjustment.set_value(next);
                        drag.follow_pointer(
                            &reorder.cards,
                            &reorder.rows.borrow(),
                            adjustment.value(),
                        );
                    }
                }
                drag.move_neighbors(
                    &reorder.cards,
                    elapsed,
                    scroll.settings().is_gtk_enable_animations(),
                );
                glib::ControlFlow::Continue
            })));
    }
}

impl ReorderDrag {
    fn follow_pointer(&mut self, cards: &gtk::Fixed, rows: &[QueueRow], scroll: f64) {
        let height = f64::from(self.grabbed.bounds.height());
        let lower = f64::from(self.bounds[0].y());
        let upper = f64::from(
            self.bounds
                .last()
                .expect("live drag has rows")
                .bottom_right()
                .y(),
        ) - height;
        let y = (self.scroll_origin + scroll + self.pointer.1).clamp(lower, upper.max(lower));
        self.grabbed.move_to(cards, y);
        let destination = self
            .bounds
            .windows(2)
            .take_while(|pair| {
                let midpoint = f64::from((pair[0].center().y() + pair[1].center().y()) / 2.0);
                y + height / 2.0 >= midpoint
            })
            .count();
        if std::mem::replace(&mut self.destination, destination) == destination {
            return;
        }
        let first = self.source.min(destination);
        let last = self.source.max(destination);
        for (index, entry) in rows.iter().enumerate().take(last + 1).skip(first) {
            if self.source != index
                && let Some(row) = entry.row.upgrade()
            {
                self.displaced.entry(index).or_insert_with(|| {
                    let card = DragCard::new(cards, row, self.bounds[index]);
                    card.picture.add_css_class("music-queue-reorder-neighbor");
                    card
                });
            }
        }
        if let Some(last) = cards.last_child()
            && last != self.grabbed.picture
        {
            self.grabbed.picture.insert_after(cards, Some(&last));
        }
    }

    fn move_neighbors(&mut self, cards: &gtk::Fixed, elapsed: f64, animate: bool) {
        let progress = if animate {
            1.0 - (-24.0 * elapsed).exp()
        } else {
            1.0
        };
        let height = f64::from(self.grabbed.bounds.height());
        let mut finished = Vec::new();
        for (index, card) in &mut self.displaced {
            let offset = if (self.destination..self.source).contains(index) {
                height
            } else if (self.source + 1..=self.destination).contains(index) {
                -height
            } else {
                0.0
            };
            let goal = f64::from(card.bounds.y()) + offset;
            let y = if (goal - card.y).abs() < 0.5 {
                goal
            } else {
                card.y + (goal - card.y) * progress
            };
            card.move_to(cards, y);
            if offset == 0.0 && y == goal {
                finished.push(*index);
            }
        }
        for index in finished {
            self.displaced
                .remove(&index)
                .expect("finished neighbor")
                .restore(cards);
        }
    }

    fn edge_speed(&self, viewport: &graphene::Rect, scroll: f64) -> f64 {
        let y = self.pointer.1;
        let top = f64::from(viewport.y());
        let bottom = f64::from(viewport.bottom_right().y());
        let position = self.scroll_origin + scroll + y;
        if y < top + EDGE_SCROLL_ZONE && position > f64::from(self.bounds[0].y()) {
            -EDGE_SCROLL_SPEED * (top + EDGE_SCROLL_ZONE - y) / EDGE_SCROLL_ZONE
        } else if y > bottom - EDGE_SCROLL_ZONE
            && position < f64::from(self.bounds.last().expect("live drag has rows").y())
        {
            EDGE_SCROLL_SPEED * (y - (bottom - EDGE_SCROLL_ZONE)) / EDGE_SCROLL_ZONE
        } else {
            0.0
        }
    }
}

fn viewport_at(scroll: &gtk::ScrolledWindow, x: f64, y: f64) -> Option<graphene::Rect> {
    scroll
        .child()?
        .compute_bounds(scroll)
        .filter(|bounds| bounds.contains_point(&graphene::Point::new(x as f32, y as f32)))
}

impl Controller {
    pub(in crate::ui::music) fn connect_queue_reorder(
        self: &Rc<Self>,
        window: &adw::ApplicationWindow,
    ) {
        let scroll = self
            .widgets
            .queue_reorder
            .widget()
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_downcast::<gtk::ScrolledWindow>()
            .expect("queue has a viewport");
        let gesture = &self.widgets.queue_reorder.gesture;
        let weak = Rc::downgrade(self);
        gesture.connect_drag_begin(move |gesture, x, y| {
            if let Some(this) = weak.upgrade()
                && let Some(scroll) = gesture.widget().and_downcast::<gtk::ScrolledWindow>()
                && this.widgets.queue_reorder.grip_at(&scroll, x, y).is_some()
            {
                return;
            }
            gesture.set_state(gtk::EventSequenceState::Denied);
        });
        let weak = Rc::downgrade(self);
        gesture.connect_drag_update(move |gesture, dx, dy| {
            let (Some(this), Some(scroll), Some((x, y))) = (
                weak.upgrade(),
                gesture.widget().and_downcast::<gtk::ScrolledWindow>(),
                gesture.start_point(),
            ) else {
                return;
            };
            let reorder = &this.widgets.queue_reorder;
            if reorder.drag.borrow().is_none() {
                let threshold =
                    f64::from(scroll.settings().property::<i32>("gtk-dnd-drag-threshold"));
                if dy.abs() <= threshold {
                    return;
                }
                if reorder.drag_from(&this, &scroll, x, y).is_none()
                    || !gesture.set_state(gtk::EventSequenceState::Claimed)
                {
                    reorder.clear_drag();
                    gesture.set_state(gtk::EventSequenceState::Denied);
                    return;
                }
                reorder.start_ticks(&this, &scroll);
            }
            reorder.update(&scroll, x + dx, y + dy);
        });
        let weak = Rc::downgrade(self);
        gesture.connect_drag_end(move |gesture, dx, dy| {
            if let (Some(this), Some(scroll), Some((x, y))) = (
                weak.upgrade(),
                gesture.widget().and_downcast::<gtk::ScrolledWindow>(),
                gesture.start_point(),
            ) {
                this.widgets
                    .queue_reorder
                    .finish(&this, &scroll, x + dx, y + dy);
            }
        });
        let weak = Rc::downgrade(self);
        gesture.connect_cancel(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.widgets.queue_reorder.clear_drag();
            }
        });
        let weak = Rc::downgrade(self);
        scroll.connect_unmap(move |_| {
            if let Some(this) = weak.upgrade() {
                this.widgets.queue_reorder.clear_drag();
            }
        });
        scroll.add_controller(gesture.clone());

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        let gesture = gesture.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(this) = weak.upgrade()
                && this.widgets.queue_reorder.drag.borrow().is_some()
            {
                this.widgets.queue_reorder.clear_drag();
                gesture.reset();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(keys);
    }
}
