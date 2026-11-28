// SPDX-License-Identifier: GPL-3.0-or-later
//! A selectable native line with space for its largest wrapped typography.

use super::diagnostics;

use std::cell::{Cell, OnceCell};

use adw::prelude::*;
use gtk::{glib, graphene, pango, subclass::prelude::*};

const ACTIVE_SCALE: f64 = 1.28;
const RESTING_OPACITY: f64 = 0.76;
const EXPANDED_RESTING_OPACITY: f64 = 0.60;

#[derive(Debug)]
struct Labels {
    measure: gtk::Label,
    text: gtk::Label,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct LyricRow {
        pub(super) labels: OnceCell<Labels>,
        pub(super) strength: Cell<f64>,
        pub(super) synchronized: Cell<bool>,
        pub(super) expanded: Cell<bool>,
        pub(super) painted_phase: Cell<Option<diagnostics::Paint>>,
        pub(super) variable_face: Cell<Option<(u32, bool)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LyricRow {
        const NAME: &'static str = "KarinLyricRow";
        type Type = super::LyricRow;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for LyricRow {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for LyricRow {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, size: i32) -> (i32, i32, i32, i32) {
            self.labels.get().map_or((0, 0, -1, -1), |labels| {
                labels.measure.measure(orientation, size)
            })
        }

        fn size_allocate(&self, width: i32, height: i32, _: i32) {
            if let Some(labels) = self.labels.get() {
                labels.measure.allocate(width, height, -1, None);
                labels.text.allocate(width, height, -1, None);
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let Some(labels) = self.labels.get() else {
                return;
            };
            let text = &labels.text;
            let row = self.obj();
            if let Some(spread) = row.static_ink_offset(text, self.strength.get()) {
                // Consume the child's pending GTK snapshot as well. Otherwise
                // it stays dirty and later native selection, focus and theme
                // invalidations can stop at the child on GTK 4.14.
                row.snapshot_child(text, &gtk::Snapshot::new());
                row.snapshot_static_text(snapshot, text, spread);
            } else {
                row.snapshot_child(text, snapshot);
            }
            if self.synchronized.get() && !text.text().is_empty() {
                let phase = diagnostics::Paint::for_strength(self.strength.get());
                let previous = self.painted_phase.replace(Some(phase));
                // Ordinary TEXT and newly bound resting lines carry no lyric
                // handoff. Preserve the budget for actual size changes.
                if previous != Some(phase)
                    && (phase != diagnostics::Paint::Resting || previous.is_some())
                {
                    diagnostics::painted(phase);
                }
            }
        }
    }
}

glib::wrapper! {
    pub struct LyricRow(ObjectSubclass<imp::LyricRow>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl LyricRow {
    pub(super) fn new() -> Self {
        let slot: Self = glib::Object::new();
        slot.set_margin_start(2);
        slot.set_margin_end(2);

        // This unpainted child follows theme, scale, direction and column width.
        let measure = super::wrap("", "body");
        measure.set_can_target(false);
        measure.update_state(&[gtk::accessible::State::Hidden(true)]);
        measure.set_parent(&slot);
        let text = super::wrap("", "body");
        text.set_selectable(true);
        text.set_parent(&slot);
        slot.imp().labels.set(Labels { measure, text }).unwrap();
        slot
    }

    pub(super) fn bind(&self, value: &str, synchronized: bool, strength: f64) -> gtk::Label {
        let Labels { measure, text } = self.imp().labels.get().unwrap();
        self.imp().variable_face.set(None);
        self.imp().synchronized.set(synchronized);
        self.imp().painted_phase.set(None);
        let (class, previous) = if synchronized {
            ("karin-lyrics-line", "karin-lyrics-stanza")
        } else {
            ("karin-lyrics-stanza", "karin-lyrics-line")
        };
        for label in [measure, text] {
            label.remove_css_class(previous);
            label.add_css_class(class);
            label.set_text(value);
        }
        if synchronized {
            measure.set_attributes(Some(&typography(1.0, true, self.imp().expanded.get())));
            emphasize(text, strength);
        } else {
            self.imp().strength.set(0.);
            measure.set_attributes(None);
            text.set_attributes(None);
            text.set_opacity(1.);
        }
        text.clone()
    }

    pub(super) fn set_expanded(&self, expanded: bool) {
        if self.imp().expanded.replace(expanded) == expanded {
            return;
        }
        let Labels { measure, text } = self.imp().labels.get().unwrap();
        for label in [measure, text] {
            label.set_xalign(if expanded { 0.5 } else { 0.0 });
            label.set_justify(if expanded {
                gtk::Justification::Center
            } else {
                gtk::Justification::Left
            });
        }
        if self.imp().synchronized.get() {
            measure.set_attributes(Some(&typography(1.0, true, expanded)));
            emphasize(text, self.imp().strength.get());
        }
    }

    pub(super) fn unbind(&self) -> gtk::Label {
        let Labels { measure, text } = self.imp().labels.get().unwrap();
        measure.set_text("");
        text.select_region(0, 0);
        text.set_text("");
        text.clone()
    }
}

fn typography(strength: f64, set_weight_axis: bool, expanded: bool) -> pango::AttrList {
    let scale = 1.0 + (ACTIVE_SCALE - 1.0) * strength;
    let attrs = pango::AttrList::new();
    if set_weight_axis {
        let (resting_weight, active_weight) = if expanded {
            (600.0, 840.0)
        } else {
            (400.0, 700.0)
        };
        let weight = (resting_weight + (active_weight - resting_weight) * strength).round() as u16;
        let mut font = pango::FontDescription::new();
        font.set_variations(Some(&format!("wght={weight}")));
        attrs.insert(pango::AttrFontDesc::new(&font));
    }
    attrs.insert(pango::AttrFloat::new_scale(scale));
    attrs
}

impl LyricRow {
    fn has_variable_face(&self, text: &gtk::Label) -> bool {
        // The Pango context serial changes with theme fonts, DPI and settings.
        // A new bound string can also select a different fallback family.
        let serial = text.pango_context().serial();
        let cache = &self.imp().variable_face;
        if let Some((last, variable)) = cache.get()
            && last == serial
        {
            return variable;
        }
        let variable = text
            .layout()
            .iter()
            .run_readonly()
            .and_then(|run| {
                let font = run.item().analysis().font();
                let name = font.describe().family()?;
                // An interpolated variable face need not have FC_STYLE.
                // Pango 1.56's face lookup asserts on that valid pattern;
                // variable capability belongs to the actual family instead.
                font.font_map()?
                    .list_families()
                    .into_iter()
                    .find(|family| family.name() == name)
            })
            .is_some_and(|family| family.is_variable());
        cache.set(Some((serial, variable)));
        variable
    }

    // A family with only regular/bold faces changes advances abruptly at the
    // midpoint. Increase the ink of its existing glyphs continuously instead.
    // Preserve GTK's unmodified selection background, caret and focus outline.
    fn static_ink_offset(&self, text: &gtk::Label, strength: f64) -> Option<f32> {
        if strength <= 0.0
            || text.has_focus()
            || text.selection_bounds().is_some()
            || self.has_variable_face(text)
        {
            return None;
        }
        let run = text.layout().iter().run_readonly()?;
        let font = run.item().analysis().font();
        let em = font.describe_with_absolute_size().size() as f32 / pango::SCALE as f32;
        Some(em * 0.06 * strength as f32)
    }

    fn snapshot_static_text(&self, snapshot: &gtk::Snapshot, text: &gtk::Label, spread: f32) {
        let (x, y) = text.layout_offsets();
        let Some(origin) = text.compute_point(self, &graphene::Point::new(x as f32, y as f32))
        else {
            return;
        };
        let mut color = text.color();
        let opacity = f64::from(color.alpha()) * text.opacity();
        color.set_alpha(1.0);
        let glyphs = gtk::Snapshot::new();
        glyphs.append_layout(&text.layout(), &color);
        let Some(ink) = glyphs.to_node() else { return };

        // Union subpixel samples of the shaped strokes before applying the
        // theme foreground alpha once. Overlapping translucent copies would
        // paint dark double edges, particularly in light themes at 2×.
        let steps = (spread * text.scale_factor() as f32 * 2.0)
            .ceil()
            .clamp(1.0, 16.0) as usize;
        let step = graphene::Point::new(spread / steps as f32, 0.);
        snapshot.push_opacity(opacity);
        snapshot.save();
        snapshot.translate(&graphene::Point::new(origin.x() - spread / 2.0, origin.y()));
        snapshot.append_node(&ink);
        for _ in 0..steps {
            snapshot.translate(&step);
            snapshot.append_node(&ink);
        }
        snapshot.restore();
        snapshot.pop();
    }
}

pub(super) fn emphasize(label: &gtk::Label, strength: f64) {
    let slot = label.parent().and_downcast::<LyricRow>().unwrap();
    slot.imp().strength.set(strength);
    // Keep the resting and active endpoints valid across live font changes.
    // Static-only fonts need no new variation description on each frame.
    let axis = strength == 1.0 || (strength > 0.0 && slot.has_variable_face(label));
    label.set_attributes(Some(&typography(strength, axis, slot.imp().expanded.get())));
    update_contrast(label, strength);
}

pub(super) fn update_contrast(label: &gtk::Label, strength: f64) {
    let row = label.parent().and_downcast::<LyricRow>().unwrap();
    let resting = if row.imp().expanded.get() {
        EXPANDED_RESTING_OPACITY
    } else {
        RESTING_OPACITY
    };
    let opacity = if adw::StyleManager::default().is_high_contrast() {
        1.0
    } else {
        resting + (1.0 - resting) * strength
    };
    label.set_opacity(opacity);
    // The static font's shaped ink is owned by the row's own render node.
    row.queue_draw();
}
