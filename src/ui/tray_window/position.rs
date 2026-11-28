// SPDX-License-Identifier: GPL-3.0-or-later

use adw::prelude::*;
use gtk::{gdk, glib};

pub(in crate::ui) const MINI_WIDTH: i32 = 360;
const GAP: i32 = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Left,
    Top,
    Right,
    Bottom,
}

pub(super) struct Positioning {
    #[cfg(karin_x11)]
    xwayland: Xwayland,
}

impl Positioning {
    pub(super) fn new() -> Self {
        Self {
            #[cfg(karin_x11)]
            xwayland: Xwayland::default(),
        }
    }

    pub(super) fn placement(&self, display: &gdk::Display, point: (i32, i32)) -> Placement {
        let Some(monitor) = monitor_at(display, point) else {
            return Placement::Normal;
        };
        #[cfg(karin_layer_shell)]
        if layer::is_supported(display) {
            return Placement::Layer(monitor, point);
        }
        #[cfg(karin_x11)]
        {
            let x11_display = if x11::is_x11(display) {
                Some(display.clone())
            } else {
                self.xwayland.display(display)
            };
            if let Some(x11_display) = x11_display {
                if x11_display == *display {
                    return Placement::X11(monitor, point);
                }
                if let Some(x11_monitor) = monitor_at(&x11_display, point) {
                    return Placement::X11(x11_monitor, point);
                }
                // SNI coordinates refer to the default display. Xwayland can
                // expose a different global coordinate system under scaling.
                let bounds = monitor.geometry();
                let displays = x11_display.monitors();
                for candidate in displays.iter::<gdk::Monitor>().flatten() {
                    if candidate.connector() != monitor.connector() {
                        continue;
                    }
                    let x11_bounds = candidate.geometry();
                    let offset_x = i64::from(point.0 - bounds.x()) * i64::from(x11_bounds.width())
                        / i64::from(bounds.width());
                    let offset_y = i64::from(point.1 - bounds.y()) * i64::from(x11_bounds.height())
                        / i64::from(bounds.height());
                    let projected = (
                        x11_bounds.x() + offset_x as i32,
                        x11_bounds.y() + offset_y as i32,
                    );
                    return Placement::X11(candidate, projected);
                }
            }
        }
        let _ = monitor;
        Placement::Normal
    }
}

fn monitor_at(display: &gdk::Display, (x, y): (i32, i32)) -> Option<gdk::Monitor> {
    // SNI hosts can use (0, 0) to activate without an icon position.
    if (x, y) == (0, 0) {
        return None;
    }
    display
        .monitors()
        .iter::<gdk::Monitor>()
        .flatten()
        .find(|monitor| {
            let bounds = monitor.geometry();
            (bounds.x()..bounds.x() + bounds.width()).contains(&x)
                && (bounds.y()..bounds.y() + bounds.height()).contains(&y)
        })
}

#[derive(Default)]
#[cfg(karin_x11)]
struct Xwayland(std::cell::OnceCell<Option<gdk::Display>>);

#[cfg(karin_x11)]
impl Xwayland {
    fn display(&self, default: &gdk::Display) -> Option<gdk::Display> {
        if !default.type_().name().contains("Wayland") || std::env::var_os("DISPLAY").is_none() {
            return None;
        }
        self.0.get_or_init(x11::open_display).clone()
    }
}

pub(super) enum Placement {
    Normal,
    #[cfg(karin_layer_shell)]
    Layer(gdk::Monitor, (i32, i32)),
    #[cfg(karin_x11)]
    X11(gdk::Monitor, (i32, i32)),
}

impl Placement {
    pub(super) fn display(&self) -> Option<gdk::Display> {
        match self {
            #[cfg(karin_x11)]
            Self::X11(monitor, _) => Some(monitor.display()),
            _ => None,
        }
    }

    pub(super) fn is_anchored(&self) -> bool {
        !matches!(self, Self::Normal)
    }

    pub(super) fn initialize(&self, window: &adw::ApplicationWindow) {
        #[cfg(karin_layer_shell)]
        if let Self::Layer(..) = self {
            layer::initialize(window);
        }
        let _ = window;
    }

    pub(super) fn present(self, window: &adw::ApplicationWindow, token: Option<&str>) {
        match self {
            Self::Normal => super::present(window, token),
            #[cfg(karin_layer_shell)]
            Self::Layer(monitor, point) => {
                let bounds = monitor.geometry();
                let size = natural_size(window);
                layer::place(window, &monitor, geometry(&bounds, point, size), size);
                window.present();
            }
            #[cfg(karin_x11)]
            Self::X11(monitor, point) => {
                let bounds = monitor.geometry();
                let position = geometry(&bounds, point, natural_size(window));
                let target = (bounds.x() + position.x, bounds.y() + position.y);
                gtk::prelude::WidgetExt::realize(window);
                x11::move_to(window, target);
                super::present(window, token);
                let weak = window.downgrade();
                // The WM may finish applying initial placement after map.
                glib::idle_add_local_once(move || {
                    if let Some(window) = weak.upgrade().filter(|window| window.is_visible()) {
                        x11::move_to(&window, target);
                    }
                });
            }
        }
    }
}

#[cfg(any(karin_layer_shell, karin_x11))]
fn natural_size(window: &adw::ApplicationWindow) -> (i32, i32) {
    let (min_width, _, _, _) = window.measure(gtk::Orientation::Horizontal, -1);
    let width = MINI_WIDTH.max(min_width);
    let (_, height, _, _) = window.measure(gtk::Orientation::Vertical, width);
    (width, height)
}

#[derive(Debug, PartialEq, Eq)]
struct Geometry {
    x: i32,
    y: i32,
    side: Side,
}

fn geometry(bounds: &gdk::Rectangle, (x, y): (i32, i32), (w, h): (i32, i32)) -> Geometry {
    let x = x - bounds.x();
    let y = y - bounds.y();
    let width = bounds.width();
    let height = bounds.height();
    let side = [
        (Side::Bottom, height - y),
        (Side::Top, y),
        (Side::Right, width - x),
        (Side::Left, x),
    ]
    .into_iter()
    .min_by_key(|(_, distance)| *distance)
    .expect("four sides")
    .0;
    let (x, y) = match side {
        Side::Left => (x + GAP, y - h / 2),
        Side::Right => (x - GAP - w, y - h / 2),
        Side::Top => (x - w / 2, y + GAP),
        Side::Bottom => (x - w / 2, y - GAP - h),
    };
    Geometry {
        x: x.clamp(0, (width - w).max(0)),
        y: y.clamp(0, (height - h).max(0)),
        side,
    }
}

#[cfg(karin_layer_shell)]
mod layer {
    use glib::translate::{ToGlibPtr, from_glib};

    use super::*;

    const LEFT: i32 = 0;
    const RIGHT: i32 = 1;
    const TOP: i32 = 2;
    const BOTTOM: i32 = 3;
    const TOP_LAYER: i32 = 2;
    const KEYBOARD_ON_DEMAND: i32 = 2;

    // A direct dependency loads the upstream Wayland integration before GTK's
    // transitive libwayland-client. Linking via dlopen after GTK starts cannot
    // support gtk4-layer-shell.
    #[link(name = "gtk4-layer-shell")]
    unsafe extern "C" {
        fn gtk_layer_is_supported() -> glib::ffi::gboolean;
        fn gtk_layer_init_for_window(window: *mut gtk::ffi::GtkWindow);
        fn gtk_layer_set_namespace(
            window: *mut gtk::ffi::GtkWindow,
            namespace: *const std::ffi::c_char,
        );
        fn gtk_layer_set_layer(window: *mut gtk::ffi::GtkWindow, layer: i32);
        fn gtk_layer_set_monitor(
            window: *mut gtk::ffi::GtkWindow,
            monitor: *mut gdk::ffi::GdkMonitor,
        );
        fn gtk_layer_set_anchor(
            window: *mut gtk::ffi::GtkWindow,
            edge: i32,
            enabled: glib::ffi::gboolean,
        );
        fn gtk_layer_set_margin(window: *mut gtk::ffi::GtkWindow, edge: i32, margin: i32);
        fn gtk_layer_set_exclusive_zone(window: *mut gtk::ffi::GtkWindow, zone: i32);
        fn gtk_layer_set_keyboard_mode(window: *mut gtk::ffi::GtkWindow, mode: i32);
        fn gtk_layer_get_protocol_version() -> u32;
    }

    pub(super) fn is_supported(display: &gdk::Display) -> bool {
        Some(display.clone()) == gdk::Display::default()
            && unsafe { from_glib(gtk_layer_is_supported()) }
    }

    pub(super) fn initialize(window: &adw::ApplicationWindow) {
        let window: &gtk::Window = window.upcast_ref();
        // This window is still unrealized, as required by gtk_init_for_window.
        unsafe {
            let window = window.to_glib_none().0;
            gtk_layer_init_for_window(window);
            gtk_layer_set_namespace(window, c"karin-mini-player".as_ptr());
            gtk_layer_set_layer(window, TOP_LAYER);
            gtk_layer_set_exclusive_zone(window, -1);
            if gtk_layer_get_protocol_version() >= 4 {
                gtk_layer_set_keyboard_mode(window, KEYBOARD_ON_DEMAND);
            }
        }
    }

    pub(super) fn place(
        window: &adw::ApplicationWindow,
        monitor: &gdk::Monitor,
        placement: Geometry,
        size: (i32, i32),
    ) {
        let bounds = monitor.geometry();
        let right = (bounds.width() - placement.x - size.0).max(0);
        let bottom = (bounds.height() - placement.y - size.1).max(0);
        let anchor_right = placement.side == Side::Right;
        let anchor_bottom = placement.side == Side::Bottom;
        let window: &gtk::Window = window.upcast_ref();
        // All pointers are borrowed for the duration of these GTK main-thread calls.
        unsafe {
            let window = window.to_glib_none().0;
            gtk_layer_set_monitor(window, monitor.to_glib_none().0);
            for (edge, enabled, margin) in [
                (LEFT, !anchor_right, placement.x),
                (RIGHT, anchor_right, right),
                (TOP, !anchor_bottom, placement.y),
                (BOTTOM, anchor_bottom, bottom),
            ] {
                gtk_layer_set_anchor(window, edge, i32::from(enabled));
                gtk_layer_set_margin(window, edge, if enabled { margin } else { 0 });
            }
        }
    }
}

#[cfg(karin_x11)]
#[path = "x11.rs"]
mod x11;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_popover_inside_monitor_near_the_selected_tray_edge() {
        let bounds = gdk::Rectangle::new(-1920, 0, 1920, 1080);
        let size = (360, 242);
        let bottom = geometry(&bounds, (-225, 1060), size);
        assert_eq!(
            (bottom.x, bottom.y, bottom.side as u8),
            (1515, 804, Side::Bottom as u8)
        );
        let left = geometry(&bounds, (-1900, 300), size);
        assert_eq!(
            (left.x, left.y, left.side as u8),
            (34, 179, Side::Left as u8)
        );
        let right = geometry(&bounds, (-20, 900), size);
        assert_eq!(
            (right.x, right.y, right.side as u8),
            (1526, 779, Side::Right as u8)
        );
        let top = geometry(&bounds, (-500, 8), size);
        assert_eq!((top.x, top.y, top.side as u8), (1240, 22, Side::Top as u8));
    }

    #[test]
    fn clamps_along_the_panel_and_handles_an_output_smaller_than_the_popup() {
        assert_eq!(
            geometry(
                &gdk::Rectangle::new(1920, 100, 1920, 1080),
                (3820, 1170),
                (360, 242)
            ),
            Geometry {
                x: 1560,
                y: 814,
                side: Side::Bottom
            },
        );
        let tiny = geometry(&gdk::Rectangle::new(0, 0, 250, 200), (125, 198), (360, 242));
        assert_eq!((tiny.x, tiny.y), (0, 0));
    }
}
