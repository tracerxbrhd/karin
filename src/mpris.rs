// SPDX-License-Identifier: GPL-3.0-or-later
//! Session-bus MPRIS 2.2 adapter. Audio and queue ownership remain with the caller.
//!
//! Only the explicit display metadata below crosses D-Bus. There is deliberately
//! no field for an audio URI, OAuth credential, browser cookie or service response.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io::Read,
    path::Path,
    rc::Rc,
};

use gio::prelude::FileExt;
use glib::variant::{ObjectPath, ToVariant};
use gtk::{gio, glib};

pub const BUS_NAME: &str = "org.mpris.MediaPlayer2.io.github.tracerxbrhd.Karin";
pub const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT: &str = "org.mpris.MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

// LoopStatus and Shuffle are optional in the official MPRIS 2.2 specification
// (v2.2, Player_Node.xml). Omit them until repeat/shuffle actually work, so
// desktop clients do not display controls which this player cannot perform.
const XML: &str = r#"<node>
<interface name="org.mpris.MediaPlayer2">
 <method name="Raise"/><method name="Quit"/>
 <property name="CanQuit" type="b" access="read"/>
 <property name="Fullscreen" type="b" access="readwrite"/>
 <property name="CanSetFullscreen" type="b" access="read"/>
 <property name="CanRaise" type="b" access="read"/>
 <property name="HasTrackList" type="b" access="read"/>
 <property name="Identity" type="s" access="read"/>
 <property name="DesktopEntry" type="s" access="read"/>
 <property name="SupportedUriSchemes" type="as" access="read"/>
 <property name="SupportedMimeTypes" type="as" access="read"/>
</interface>
<interface name="org.mpris.MediaPlayer2.Player">
 <method name="Next"/><method name="Previous"/><method name="Pause"/>
 <method name="PlayPause"/><method name="Stop"/><method name="Play"/>
 <method name="Seek"><arg name="Offset" type="x" direction="in"/></method>
 <method name="SetPosition"><arg name="TrackId" type="o" direction="in"/><arg name="Position" type="x" direction="in"/></method>
 <method name="OpenUri"><arg name="Uri" type="s" direction="in"/></method>
 <signal name="Seeked"><arg name="Position" type="x"/></signal>
 <property name="PlaybackStatus" type="s" access="read"/>
 <property name="Rate" type="d" access="readwrite"/>
 <property name="Metadata" type="a{sv}" access="read"/>
 <property name="Volume" type="d" access="readwrite"/>
 <property name="Position" type="x" access="read"><annotation name="org.freedesktop.DBus.Property.EmitsChangedSignal" value="false"/></property>
 <property name="MinimumRate" type="d" access="read"/>
 <property name="MaximumRate" type="d" access="read"/>
 <property name="CanGoNext" type="b" access="read"/>
 <property name="CanGoPrevious" type="b" access="read"/>
 <property name="CanPlay" type="b" access="read"/>
 <property name="CanPause" type="b" access="read"/>
 <property name="CanSeek" type="b" access="read"/>
 <property name="CanControl" type="b" access="read"><annotation name="org.freedesktop.DBus.Property.EmitsChangedSignal" value="false"/></property>
</interface></node>"#;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Stopped,
    Playing,
    Paused,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "Stopped",
            Self::Playing => "Playing",
            Self::Paused => "Paused",
        }
    }
}

/// A local cover file, never a returned provider URL or arbitrary URI scheme.
/// The application cover cache owns its lifetime and must remove it on cleanup.
#[derive(Clone, PartialEq, Eq)]
pub struct LocalArtwork(String);

impl LocalArtwork {
    pub fn from_file(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        if !path.is_absolute()
            || !metadata.file_type().is_file()
            || !(33..=2 * 1024 * 1024).contains(&metadata.len())
        {
            return None;
        }
        // The GUI exports only its own bounded decoded-cover PNG cache. Do not
        // hand a token file, SVG, media resource or arbitrary URI to clients.
        let mut header = [0u8; 24];
        std::fs::File::open(path)
            .ok()?
            .read_exact(&mut header)
            .ok()?;
        if &header[..8] != b"\x89PNG\r\n\x1a\n"
            || header[8..12] != [0, 0, 0, 13]
            || &header[12..16] != b"IHDR"
        {
            return None;
        }
        let width = u32::from_be_bytes(header[16..20].try_into().ok()?);
        let height = u32::from_be_bytes(header[20..24].try_into().ok()?);
        if !(1..=2048).contains(&width) || !(1..=2048).contains(&height) {
            return None;
        }
        let uri = gio::File::for_path(path).uri().to_string();
        (uri.len() <= 4096 && uri.starts_with("file:///")).then_some(Self(uri))
    }
}

#[derive(Clone, PartialEq)]
pub struct Metadata {
    /// Nonzero, opaque, per-current-track generation. Never a service track ID.
    pub track_id: u64,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub length_us: Option<i64>,
    pub artwork: Option<LocalArtwork>,
}

#[derive(Clone, PartialEq)]
pub struct Snapshot {
    pub status: Status,
    pub metadata: Option<Metadata>,
    pub position_us: i64,
    pub volume: f64,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_seek: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_control: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            status: Status::Stopped,
            metadata: None,
            position_us: 0,
            volume: 0.7,
            can_play: false,
            can_pause: false,
            can_seek: false,
            can_go_next: false,
            can_go_previous: false,
            can_control: true,
        }
    }
}

impl Snapshot {
    fn normalized(mut self) -> Self {
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0.0, 1.0)
        } else {
            0.7
        };
        if self.metadata.as_ref().is_some_and(|m| m.track_id == 0) {
            self.metadata = None;
        }
        if let Some(metadata) = self.metadata.as_mut() {
            metadata.title = bounded_text(&metadata.title, 512);
            metadata.artists = metadata
                .artists
                .iter()
                .take(32)
                .map(|a| bounded_text(a, 256))
                .collect();
            metadata.album = metadata.album.as_ref().map(|a| bounded_text(a, 512));
            metadata.length_us = metadata.length_us.filter(|length| *length >= 0);
            self.position_us = self.position_us.max(0);
            if let Some(length) = metadata.length_us {
                self.position_us = self.position_us.min(length);
            }
        } else {
            self.status = Status::Stopped;
            self.position_us = 0;
            self.can_play = false;
            self.can_pause = false;
            self.can_seek = false;
            self.can_go_next = false;
            self.can_go_previous = false;
        }
        if !self.can_control {
            self.can_play = false;
            self.can_pause = false;
            self.can_seek = false;
            self.can_go_next = false;
            self.can_go_previous = false;
        }
        self
    }

    fn properties(&self) -> BTreeMap<String, glib::Variant> {
        BTreeMap::from([
            ("PlaybackStatus".into(), self.status.as_str().to_variant()),
            ("Rate".into(), 1.0f64.to_variant()),
            ("Metadata".into(), metadata_variant(self.metadata.as_ref())),
            ("Volume".into(), self.volume.to_variant()),
            ("Position".into(), self.position_us.to_variant()),
            ("MinimumRate".into(), 1.0f64.to_variant()),
            ("MaximumRate".into(), 1.0f64.to_variant()),
            ("CanGoNext".into(), self.can_go_next.to_variant()),
            ("CanGoPrevious".into(), self.can_go_previous.to_variant()),
            ("CanPlay".into(), self.can_play.to_variant()),
            ("CanPause".into(), self.can_pause.to_variant()),
            ("CanSeek".into(), self.can_seek.to_variant()),
            ("CanControl".into(), self.can_control.to_variant()),
        ])
    }
}

fn bounded_text(value: &str, bound: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(bound)
        .collect()
}

fn track_path(track_id: u64) -> ObjectPath {
    ObjectPath::try_from(format!("/org/mpris/MediaPlayer2/Track/t{track_id}"))
        .expect("numeric opaque track path")
}

fn metadata_variant(metadata: Option<&Metadata>) -> glib::Variant {
    let mut values = BTreeMap::<String, glib::Variant>::new();
    if let Some(metadata) = metadata {
        values.insert(
            "mpris:trackid".into(),
            track_path(metadata.track_id).to_variant(),
        );
        values.insert("xesam:title".into(), metadata.title.to_variant());
        values.insert("xesam:artist".into(), metadata.artists.to_variant());
        if let Some(album) = &metadata.album {
            values.insert("xesam:album".into(), album.to_variant());
        }
        if let Some(length) = metadata.length_us {
            values.insert("mpris:length".into(), length.to_variant());
        }
        if let Some(artwork) = &metadata.artwork {
            values.insert("mpris:artUrl".into(), artwork.0.to_variant());
        }
    }
    values.to_variant()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Raise,
    Quit,
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    Seek(i64),
    SetPosition { track_id: u64, position_us: i64 },
    SetVolume(f64),
}

struct Inner {
    snapshot: RefCell<Snapshot>,
    callback: Rc<dyn Fn(Command)>,
    connection: RefCell<Option<gio::DBusConnection>>,
    registrations: RefCell<Vec<gio::RegistrationId>>,
    owner: RefCell<Option<gio::OwnerId>>,
    owned: Cell<bool>,
    closed: Cell<bool>,
}

/// GTK-main-context adapter. Bus acquisition is asynchronous and unavailable
/// session buses never prevent the independent music engine from operating.
pub struct Mpris {
    inner: Rc<Inner>,
}

impl Mpris {
    pub fn new(callback: Rc<dyn Fn(Command)>) -> Self {
        let inner = Rc::new(Inner {
            snapshot: RefCell::new(Snapshot::default()),
            callback,
            connection: RefCell::new(None),
            registrations: RefCell::new(Vec::new()),
            owner: RefCell::new(None),
            owned: Cell::new(false),
            closed: Cell::new(false),
        });
        let acquired = Rc::downgrade(&inner);
        let named = acquired.clone();
        let lost = acquired.clone();
        let owner = gio::bus_own_name(
            gio::BusType::Session,
            BUS_NAME,
            gio::BusNameOwnerFlags::DO_NOT_QUEUE,
            move |connection, _| {
                if let Some(inner) = acquired.upgrade()
                    && !inner.closed.get()
                    && !inner.register(&connection)
                {
                    inner.close();
                }
            },
            move |_, _| {
                if let Some(inner) = named.upgrade()
                    && !inner.closed.get()
                {
                    inner.owned.set(inner.registrations.borrow().len() == 2);
                }
            },
            move |_, _| {
                if let Some(inner) = lost.upgrade() {
                    inner.owned.set(false);
                    inner.unregister();
                }
            },
        );
        *inner.owner.borrow_mut() = Some(owner);
        Self { inner }
    }

    pub fn is_available(&self) -> bool {
        self.inner.owned.get() && !self.inner.closed.get()
    }

    pub fn publish(&self, snapshot: Snapshot) {
        if self.inner.closed.get() {
            return;
        }
        let snapshot = snapshot.normalized();
        let old = self.inner.snapshot.borrow().properties();
        let mut changed = snapshot.properties();
        // MPRIS explicitly excludes Position from PropertiesChanged. Clients
        // interpolate using Rate; discontinuities are signalled by Seeked.
        // CanControl is an intrinsic capability and also has no change signal.
        changed.retain(|key, value| {
            key != "Position" && key != "CanControl" && old.get(key) != Some(&*value)
        });
        *self.inner.snapshot.borrow_mut() = snapshot;
        if !changed.is_empty() && self.is_available() {
            self.inner.emit(
                PROPERTIES,
                "PropertiesChanged",
                &(PLAYER, changed, Vec::<String>::new()).to_variant(),
            );
        }
    }

    /// Call only after the existing player has accepted a seek and reported its
    /// actual resulting position, not merely when a control request is queued.
    pub fn seeked(&self, position_us: i64) {
        let snapshot = self.inner.snapshot.borrow();
        if self.is_available() && snapshot.can_seek {
            let mut position = position_us.max(0);
            if let Some(length) = snapshot.metadata.as_ref().and_then(|m| m.length_us) {
                position = position.min(length);
            }
            drop(snapshot);
            self.inner.emit(PLAYER, "Seeked", &(position,).to_variant());
        }
    }

    pub fn shutdown(&self) {
        if self.inner.closed.get() {
            return;
        }
        let volume = self.inner.snapshot.borrow().volume;
        self.publish(Snapshot {
            volume,
            ..Snapshot::default()
        });
        self.inner.close();
    }
}

impl Drop for Mpris {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Inner {
    fn register(self: &Rc<Self>, connection: &gio::DBusConnection) -> bool {
        self.unregister();
        *self.connection.borrow_mut() = Some(connection.clone());
        let Ok(node) = gio::DBusNodeInfo::for_xml(XML) else {
            return false;
        };
        for interface in [ROOT, PLAYER] {
            let Some(info) = node.lookup_interface(interface) else {
                return false;
            };
            let methods = Rc::downgrade(self);
            let properties = methods.clone();
            let setters = methods.clone();
            let registration = connection
                .register_object(OBJECT_PATH, &info)
                .method_call(move |_, _, _, _, method, parameters, invocation| {
                    if let Some(inner) = methods.upgrade()
                        && !inner.closed.get()
                    {
                        inner.call(interface, method, &parameters, invocation);
                    } else {
                        invocation.return_dbus_error(
                            "org.freedesktop.DBus.Error.Failed",
                            "Player is closed",
                        );
                    }
                })
                .property(move |_, _, _, _, name| {
                    properties
                        .upgrade()
                        .map(|inner| inner.property(interface, name))
                        .unwrap_or_else(|| false.to_variant())
                })
                .set_property(move |_, _, _, _, name, value| {
                    setters.upgrade().is_some_and(|inner| {
                        !inner.closed.get() && inner.set_property(interface, name, &value)
                    })
                })
                .build();
            match registration {
                Ok(id) => self.registrations.borrow_mut().push(id),
                Err(_) => {
                    self.unregister();
                    return false;
                }
            }
        }
        true
    }

    fn unregister(&self) {
        if let Some(connection) = self.connection.borrow_mut().take() {
            for registration in self.registrations.borrow_mut().drain(..) {
                let _ = connection.unregister_object(registration);
            }
        }
    }

    fn close(&self) {
        if self.closed.replace(true) {
            return;
        }
        self.owned.set(false);
        if let Some(owner) = self.owner.borrow_mut().take() {
            gio::bus_unown_name(owner);
        }
        self.unregister();
    }

    fn emit(&self, interface: &str, signal: &str, parameters: &glib::Variant) {
        if let Some(connection) = self.connection.borrow().as_ref() {
            let _ = connection.emit_signal(None, OBJECT_PATH, interface, signal, Some(parameters));
        }
    }

    fn property(&self, interface: &str, name: &str) -> glib::Variant {
        if interface == PLAYER {
            return self
                .snapshot
                .borrow()
                .properties()
                .remove(name)
                .unwrap_or_else(|| false.to_variant());
        }
        match name {
            "CanQuit" | "CanRaise" => true.to_variant(),
            "Fullscreen" | "CanSetFullscreen" | "HasTrackList" => false.to_variant(),
            "Identity" => crate::config::APP_NAME.to_variant(),
            "DesktopEntry" => crate::config::APP_ID.to_variant(),
            "SupportedUriSchemes" | "SupportedMimeTypes" => Vec::<String>::new().to_variant(),
            _ => false.to_variant(),
        }
    }

    fn call(
        &self,
        interface: &str,
        method: &str,
        parameters: &glib::Variant,
        invocation: gio::DBusMethodInvocation,
    ) {
        if interface == ROOT {
            let command = match method {
                "Raise" => Command::Raise,
                "Quit" => Command::Quit,
                _ => {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        "Unsupported player method",
                    );
                    return;
                }
            };
            (self.callback)(command);
            invocation.return_value(None);
            return;
        }
        if method == "OpenUri" {
            invocation.return_dbus_error(
                "org.mpris.MediaPlayer2.Player.Error.NotSupported",
                "External media URIs are not supported",
            );
            return;
        }
        let command = {
            let snapshot = self.snapshot.borrow();
            match method {
                "Next" if snapshot.can_go_next => Some(Command::Next),
                "Previous" if snapshot.can_go_previous => Some(Command::Previous),
                "Play" if snapshot.can_play => Some(Command::Play),
                "Pause" if snapshot.can_pause => Some(Command::Pause),
                "Stop" if snapshot.can_control => Some(Command::Stop),
                "PlayPause" if snapshot.can_pause => Some(Command::PlayPause),
                "PlayPause" | "Stop" => {
                    invocation.return_dbus_error(
                        "org.mpris.MediaPlayer2.Player.Error.NotSupported",
                        "Playback control is unavailable",
                    );
                    return;
                }
                "Seek" if snapshot.can_seek => parameters
                    .get::<(i64,)>()
                    .map(|(offset,)| Command::Seek(offset)),
                "SetPosition" if snapshot.can_seek => parameters
                    .get::<(ObjectPath, i64)>()
                    .and_then(|(path, position)| {
                        let metadata = snapshot.metadata.as_ref()?;
                        if path != track_path(metadata.track_id)
                            || position < 0
                            || metadata.length_us.is_some_and(|length| position > length)
                        {
                            return None;
                        }
                        Some(Command::SetPosition {
                            track_id: metadata.track_id,
                            position_us: position,
                        })
                    }),
                _ => None,
            }
        };
        if let Some(command) = command {
            (self.callback)(command);
        }
        invocation.return_value(None);
    }

    fn set_property(&self, interface: &str, name: &str, value: &glib::Variant) -> bool {
        if interface == ROOT {
            return name == "Fullscreen" && value.get::<bool>() == Some(false);
        }
        let (can_control, can_pause) = {
            let snapshot = self.snapshot.borrow();
            (snapshot.can_control, snapshot.can_pause)
        };
        if !can_control {
            return false;
        }
        match name {
            "Volume" => {
                let Some(value) = value.get::<f64>().filter(|value| value.is_finite()) else {
                    return false;
                };
                (self.callback)(Command::SetVolume(value.clamp(0.0, 1.0)));
                true
            }
            "Rate" => {
                let Some(rate) = value.get::<f64>() else {
                    return false;
                };
                if rate == 0.0 && can_pause {
                    (self.callback)(Command::Pause);
                    true
                } else {
                    rate == 1.0
                }
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests;
