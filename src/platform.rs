// SPDX-License-Identifier: GPL-3.0-or-later
//! Desktop capability inspection only. No credentials are read or stored.

use glib::variant::ToVariant;
use gtk::{gio, glib};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformReport {
    pub session_bus: bool,
    pub secret_service_running: bool,
}

/// Run on a worker thread. Looking up a bus name does not activate a keyring.
pub fn inspect_session() -> PlatformReport {
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return PlatformReport {
            session_bus: false,
            secret_service_running: false,
        };
    };
    let secret_service_running = bus
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&("org.freedesktop.secrets",).to_variant()),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            2_000,
            gio::Cancellable::NONE,
        )
        .ok()
        .and_then(|reply| reply.get::<(bool,)>())
        .is_some_and(|(running,)| running);
    PlatformReport {
        session_bus: true,
        secret_service_running,
    }
}

/// Reserved identity, not an advertised MPRIS service in v0.0.
pub const MPRIS_BUS_NAME: &str = "org.mpris.MediaPlayer2.Karin";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_mpris_name_is_a_valid_bus_name() {
        assert!(gio::dbus_is_name(MPRIS_BUS_NAME));
    }
}
