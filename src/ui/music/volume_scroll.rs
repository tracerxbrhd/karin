// SPDX-License-Identifier: GPL-3.0-or-later
//! Native footer input delegates to the one existing volume and panel owners.

use super::*;

const VOLUME_STEP: f64 = 0.05;
const SURFACE_PIXELS_PER_STEP: f64 = 40.0;

/// Gesture remainder only, never a second volume value. High-resolution wheel
/// and touchpad events accumulate before one five-percentage-point change.
#[derive(Default)]
struct VolumeScroll {
    remainder: f64,
    unit: Option<gtk::gdk::ScrollUnit>,
}

impl VolumeScroll {
    fn reset(&mut self) {
        self.remainder = 0.0;
        self.unit = None;
    }

    fn apply(&mut self, volume: f64, dy: f64, unit: gtk::gdk::ScrollUnit) -> Option<f64> {
        if !volume.is_finite() || !dy.is_finite() || dy == 0.0 {
            return None;
        }
        if self.unit != Some(unit)
            || (self.remainder != 0.0 && self.remainder.signum() != dy.signum())
        {
            self.remainder = 0.0;
        }
        self.unit = Some(unit);
        let distance = if unit == gtk::gdk::ScrollUnit::Surface {
            SURFACE_PIXELS_PER_STEP
        } else {
            1.0
        };
        // One unusually large platform event cannot jump through the entire
        // range; subsequent genuine events still contribute normally.
        self.remainder += (dy / distance).clamp(-1.0, 1.0);
        if self.remainder.abs() < 1.0 - 1e-9 {
            return None;
        }
        let direction = self.remainder.signum();
        self.remainder -= direction;
        let next = (volume - direction * VOLUME_STEP).clamp(0.0, 1.0);
        (next != volume).then_some(next)
    }
}

impl Controller {
    pub(super) fn connect_footer_extras(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.widgets.lyrics.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.toggle_current_lyrics();
            }
        });

        let remainder = Rc::new(RefCell::new(VolumeScroll::default()));
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
        let gesture = Rc::clone(&remainder);
        scroll.connect_scroll_begin(move |_| gesture.borrow_mut().reset());
        let gesture = Rc::clone(&remainder);
        scroll.connect_scroll_end(move |_| gesture.borrow_mut().reset());
        let weak = Rc::downgrade(self);
        let gesture = Rc::clone(&remainder);
        scroll.connect_scroll(move |controller, _, dy| {
            if let Some(this) = weak.upgrade() {
                let available = !this.model.borrow().closed && this.widgets.volume.is_sensitive();
                if available {
                    // Release the gesture borrow before GTK renders the shared
                    // model; a synchronous leave signal may reset this state.
                    let volume =
                        gesture
                            .borrow_mut()
                            .apply(this.music_volume(), dy, controller.unit());
                    if let Some(volume) = volume {
                        this.set_music_volume(volume);
                    }
                }
            }
            // A volume gesture belongs to this target even at 0/100%; it must
            // neither open the popup nor scroll a collection behind the footer.
            glib::Propagation::Stop
        });
        self.widgets.volume_menu.add_controller(scroll);
        let motion = gtk::EventControllerMotion::new();
        motion.connect_leave(move |_| remainder.borrow_mut().reset());
        self.widgets.volume_menu.add_controller(motion);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::gdk::ScrollUnit;

    #[test]
    fn wheel_is_five_percentage_points_and_clamps_without_redundant_commands() {
        let mut gesture = VolumeScroll::default();
        assert_eq!(gesture.apply(0.5, -1.0, ScrollUnit::Wheel), Some(0.55));
        assert_eq!(gesture.apply(0.55, 1.0, ScrollUnit::Wheel), Some(0.5));
        assert_eq!(gesture.apply(0.98, -1.0, ScrollUnit::Wheel), Some(1.0));
        assert_eq!(gesture.apply(1.0, -1.0, ScrollUnit::Wheel), None);
        assert_eq!(gesture.apply(0.02, 1.0, ScrollUnit::Wheel), Some(0.0));
        assert_eq!(gesture.apply(0.0, 1.0, ScrollUnit::Wheel), None);
        assert_eq!(gesture.apply(0.5, f64::NAN, ScrollUnit::Wheel), None);
        assert_eq!(gesture.apply(0.5, f64::INFINITY, ScrollUnit::Wheel), None);
    }

    #[test]
    fn smooth_events_accumulate_and_reversal_new_gesture_and_unit_reset() {
        let mut gesture = VolumeScroll::default();
        for _ in 0..3 {
            assert_eq!(gesture.apply(0.5, -10.0, ScrollUnit::Surface), None);
        }
        assert_eq!(gesture.apply(0.5, -10.0, ScrollUnit::Surface), Some(0.55));
        assert_eq!(gesture.apply(0.55, 20.0, ScrollUnit::Surface), None);
        // Opposite movement retires the half-step rather than surprising the
        // user with a delayed change in the old direction.
        assert_eq!(gesture.apply(0.55, -20.0, ScrollUnit::Surface), None);
        gesture.reset();
        assert_eq!(gesture.apply(0.55, -20.0, ScrollUnit::Surface), None);
        assert_eq!(gesture.apply(0.55, 0.5, ScrollUnit::Wheel), None);
        assert_eq!(gesture.apply(0.55, 0.5, ScrollUnit::Wheel), Some(0.5));
        assert_eq!(
            gesture.apply(0.5, -10000.0, ScrollUnit::Surface),
            Some(0.55)
        );
        assert_eq!(gesture.apply(0.55, -0.25, ScrollUnit::Wheel), None);
    }
}

#[cfg(test)]
mod native_tests;
