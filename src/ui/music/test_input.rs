// SPDX-License-Identifier: GPL-3.0-or-later
//! Pointer coordinates for real X11 tests across native popup boundaries.

use super::tests::pump_until;
use super::*;

#[derive(Clone, Copy)]
pub(super) enum Target {
    Center,
    Leading,
    DisclosureHeading,
}

/// Bounds in the main window's widget/application pixels. GTK 4.14 cannot
/// compute widget bounds directly across GtkNative boundaries. Compute within
/// the target's own native, then translate through its actual popup ancestry.
fn geometry(window: &adw::ApplicationWindow, widget: &gtk::Widget) -> Option<(f64, f64, f64, f64)> {
    if !widget.is_mapped() || widget.width() <= 0 || widget.height() <= 0 {
        return None;
    }
    let native = widget.native()?;
    let mut surface = native.surface()?;
    let target = window.surface()?;
    if !surface.is_mapped() || !target.is_mapped() {
        return None;
    }
    let bounds = widget.compute_bounds(native.upcast_ref::<gtk::Widget>())?;
    // Native.get_surface_transform is SURFACE -> WIDGET, so invert it when
    // moving the own-native widget point into that native's surface.
    // https://docs.gtk.org/gtk4/method.Native.get_surface_transform.html
    let (dx, dy) = native.surface_transform();
    let mut x = f64::from(bounds.x()) - dx;
    let mut y = f64::from(bounds.y()) - dy;
    for _ in 0..8 {
        if surface == target {
            // Return to the target window's widget coordinate frame, matching
            // compute_bounds(window) and the established XTest window inputs.
            // For a shared native, source/target transforms cancel exactly.
            let (target_dx, target_dy) = window.surface_transform();
            return Some((
                x + target_dx,
                y + target_dy,
                f64::from(bounds.width()),
                f64::from(bounds.height()),
            ));
        }
        // Popup positions are relative to their parent SURFACE, in application
        // pixels. This bounded ancestor translation is the relevant safe case
        // of gdk_surface_translate_coordinates. gtk-rs 0.10's manual wrapper
        // returns only its bool and discards the translated x/y output.
        // https://docs.gtk.org/gdk4/method.Popup.get_position_x.html
        let popup = surface.dynamic_cast::<gtk::gdk::Popup>().ok()?;
        x += f64::from(popup.position_x());
        y += f64::from(popup.position_y());
        surface = popup.parent()?;
    }
    None
}

pub(super) fn wait_for_layout(window: &adw::ApplicationWindow, widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref();
    pump_until(|| geometry(window, widget).is_some());
    let stable = Rc::new(Cell::new(0));
    let previous = Rc::new(RefCell::new(None));
    let ready = Rc::new(Cell::new(false));
    let painted = ready.clone();
    let host = window.downgrade();
    widget.add_tick_callback(move |widget, _| {
        let Some(window) = host.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let Some((x, y, width, height)) = geometry(&window, widget) else {
            return glib::ControlFlow::Continue;
        };
        // Include popup position, not merely its stable internal button layout:
        // native popup placement can finish after the widget's map notification.
        let current = (x.to_bits(), y.to_bits(), width.to_bits(), height.to_bits());
        if previous.borrow().as_ref() == Some(&current) {
            stable.set(stable.get() + 1);
        } else {
            stable.set(1);
        }
        *previous.borrow_mut() = Some(current);
        if stable.get() >= 2 {
            painted.set(true);
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
    pump_until(|| ready.get());
}

pub(super) fn pointer(
    window: &adw::ApplicationWindow,
    widget: &impl IsA<gtk::Widget>,
    target: Target,
) -> (i32, i32) {
    wait_for_layout(window, widget);
    let (left, top, width, height) = geometry(window, widget.as_ref())
        .expect("mapped input target shares the main native surface ancestry");
    let x = left
        + match target {
            Target::Center => width / 2.0,
            Target::Leading | Target::DisclosureHeading => width.min(160.0) / 2.0,
        };
    let y = top
        + match target {
            Target::DisclosureHeading => height.min(20.0) / 2.0,
            _ => height / 2.0,
        };
    assert!(
        x.is_finite()
            && y.is_finite()
            && x >= 0.0
            && x < f64::from(window.width())
            && y >= 0.0
            && y < f64::from(window.height()),
        "input target is outside the main widget"
    );
    let scale = f64::from(window.scale_factor());
    let native = widget.as_ref().native().expect("mapped target native");
    let own = widget
        .as_ref()
        .compute_bounds(native.upcast_ref::<gtk::Widget>());
    let direct = widget.as_ref().compute_bounds(window);
    eprintln!(
        "Karin synthetic pointer: widget={} name={} native={} native-transform={:?} root-transform={:?} own={own:?} direct={direct:?} window-point=({x},{y}) scale={scale}",
        widget.as_ref().type_().name(),
        widget.as_ref().widget_name(),
        native.type_().name(),
        native.surface_transform(),
        window.surface_transform(),
    );
    ((x * scale).round() as i32, (y * scale).round() as i32)
}
