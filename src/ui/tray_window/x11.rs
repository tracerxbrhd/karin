// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::{CString, c_char, c_int, c_long, c_ulong, c_void};
use std::os::unix::ffi::OsStrExt;

use glib::translate::{ToGlibPtr, from_glib_full};

use super::*;

const CLIENT_MESSAGE: c_int = 33;
const STATIC_GRAVITY: c_long = 10;
const POSITION_MASK: c_long = (1 << 8) | (1 << 9);
const NORMAL_APPLICATION: c_long = 1 << 12;
const SUBSTRUCTURE_MASK: c_long = (1 << 19) | (1 << 20);

#[repr(C)]
#[derive(Clone, Copy)]
struct ClientMessage {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut c_void,
    window: c_ulong,
    message_type: c_ulong,
    format: c_int,
    data: [c_long; 5],
}

#[repr(C)]
union Event {
    client: ClientMessage,
    padding: [c_long; 24],
}

unsafe extern "C" {
    fn gdk_x11_display_open(name: *const c_char) -> *mut gdk::ffi::GdkDisplay;
    fn gdk_x11_display_get_xdisplay(display: *mut gdk::ffi::GdkDisplay) -> *mut c_void;
    fn gdk_x11_display_get_xrootwindow(display: *mut gdk::ffi::GdkDisplay) -> c_ulong;
    fn gdk_x11_surface_get_xid(surface: *mut gdk::ffi::GdkSurface) -> c_ulong;
}

#[link(name = "X11")]
unsafe extern "C" {
    fn XInternAtom(display: *mut c_void, name: *const c_char, only_if_exists: c_int) -> c_ulong;
    fn XMoveWindow(display: *mut c_void, xid: c_ulong, x: c_int, y: c_int) -> c_int;
    fn XSendEvent(
        display: *mut c_void,
        window: c_ulong,
        propagate: c_int,
        event_mask: c_long,
        event: *mut Event,
    ) -> c_int;
    fn XFlush(display: *mut c_void) -> c_int;
    fn XGetWindowProperty(
        display: *mut c_void,
        window: c_ulong,
        property: c_ulong,
        offset: c_long,
        length: c_long,
        delete: c_int,
        requested_type: c_ulong,
        actual_type: *mut c_ulong,
        format: *mut c_int,
        items: *mut c_ulong,
        bytes_after: *mut c_ulong,
        values: *mut *mut u8,
    ) -> c_int;
    fn XFree(data: *mut c_void) -> c_int;
}

pub(super) fn is_x11(display: &gdk::Display) -> bool {
    display.type_().name() == "GdkX11Display"
}

pub(super) fn open_display() -> Option<gdk::Display> {
    let name = std::env::var_os("DISPLAY")?;
    let name = CString::new(name.as_bytes()).ok()?;
    // This GDK factory returns ownership of a secondary display.
    unsafe { from_glib_full(gdk_x11_display_open(name.as_ptr())) }
}

pub(super) fn move_to(window: &adw::ApplicationWindow, (x, y): (i32, i32)) {
    let display = gtk::prelude::WidgetExt::display(window);
    let Some(surface) = window.surface().filter(|_| is_x11(&display)) else {
        return;
    };
    let scale = surface.scale_factor();
    let (x, y) = (x.saturating_mul(scale), y.saturating_mul(scale));
    // GDK owns this X11 connection and surface. The event and Xlib's
    // native-long union are used only by this main-thread call.
    unsafe {
        let display = display.to_glib_none().0;
        let xid = gdk_x11_surface_get_xid(surface.to_glib_none().0);
        let root = gdk_x11_display_get_xrootwindow(display);
        let display = gdk_x11_display_get_xdisplay(display);
        let atom = XInternAtom(display, c"_NET_MOVERESIZE_WINDOW".as_ptr(), 1);
        if window.is_mapped() && supports_move(display, root, atom) {
            let mut event = Event {
                client: ClientMessage {
                    type_: CLIENT_MESSAGE,
                    serial: 0,
                    send_event: 1,
                    display,
                    window: xid,
                    message_type: atom,
                    format: 32,
                    data: [
                        STATIC_GRAVITY | POSITION_MASK | NORMAL_APPLICATION,
                        x.into(),
                        y.into(),
                        0,
                        0,
                    ],
                },
            };
            XSendEvent(display, root, 0, SUBSTRUCTURE_MASK, &mut event);
        } else {
            XMoveWindow(display, xid, x, y);
        }
        XFlush(display);
    }
}

unsafe fn supports_move(display: *mut c_void, root: c_ulong, move_atom: c_ulong) -> bool {
    const XA_ATOM: c_ulong = 4;
    const SUCCESS: c_int = 0;
    if move_atom == 0 {
        return false;
    }
    let mut status_type = 0;
    let mut format = 0;
    let mut items = 0;
    let mut remaining = 0;
    let mut values = std::ptr::null_mut();
    unsafe {
        let supported = XInternAtom(display, c"_NET_SUPPORTED".as_ptr(), 1);
        if supported == 0 {
            return false;
        }
        let status = XGetWindowProperty(
            display,
            root,
            supported,
            0,
            4096,
            0,
            XA_ATOM,
            &mut status_type,
            &mut format,
            &mut items,
            &mut remaining,
            &mut values,
        );
        if values.is_null() {
            return false;
        }
        let supported = status == SUCCESS
            && status_type == XA_ATOM
            && format == 32
            && std::slice::from_raw_parts(values.cast::<c_ulong>(), items as usize)
                .contains(&move_atom);
        XFree(values.cast());
        supported
    }
}
