// SPDX-License-Identifier: GPL-3.0-or-later
//! Linux StatusNotifierItem and its static DBusMenu.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io::Cursor,
    rc::Rc,
};

use glib::variant::ObjectPath;
use gtk::{gdk_pixbuf, gio, glib, prelude::*};

use crate::config::APP_ID;

const ITEM: &str = "org.kde.StatusNotifierItem";
const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU: &str = "com.canonical.dbusmenu";
const MENU_PATH: &str = "/MenuBar";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const DBUS: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const PLASMA: &str = "org.kde.plasmashell";
const TIMEOUT_MS: i32 = 2_000;

const ITEM_XML: &str = r#"
<node>
  <interface name="org.kde.StatusNotifierItem">
    <property name="Category" type="s" access="read"/>
    <property name="Id" type="s" access="read"/>
    <property name="Title" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="IconName" type="s" access="read"/>
    <property name="IconPixmap" type="a(iiay)" access="read"/>
    <property name="ItemIsMenu" type="b" access="read"/>
    <property name="Menu" type="o" access="read"/>
    <method name="Activate">
      <arg type="i" direction="in"/><arg type="i" direction="in"/>
    </method>
    <method name="ProvideXdgActivationToken"><arg type="s" direction="in"/></method>
  </interface>
</node>
"#;

const MENU_XML: &str = r#"
<node>
  <interface name="com.canonical.dbusmenu">
    <property name="Version" type="u" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="TextDirection" type="s" access="read"/>
    <property name="IconThemePath" type="as" access="read"/>
    <method name="GetLayout">
      <arg type="i" direction="in"/><arg type="i" direction="in"/>
      <arg type="as" direction="in"/>
      <arg type="u" direction="out"/><arg type="(ia{sv}av)" direction="out"/>
    </method>
    <method name="GetGroupProperties">
      <arg type="ai" direction="in"/><arg type="as" direction="in"/>
      <arg type="a(ia{sv})" direction="out"/>
    </method>
    <method name="GetProperty">
      <arg type="i" direction="in"/><arg type="s" direction="in"/>
      <arg type="v" direction="out"/>
    </method>
    <method name="Event">
      <arg type="i" direction="in"/><arg type="s" direction="in"/>
      <arg type="v" direction="in"/><arg type="u" direction="in"/>
    </method>
    <method name="EventGroup">
      <arg type="a(isvu)" direction="in"/><arg type="ai" direction="out"/>
    </method>
    <method name="AboutToShow">
      <arg type="i" direction="in"/><arg type="b" direction="out"/>
    </method>
    <method name="AboutToShowGroup">
      <arg type="ai" direction="in"/><arg type="ai" direction="out"/>
      <arg type="ai" direction="out"/>
    </method>
  </interface>
</node>
"#;

type Properties = BTreeMap<&'static str, glib::Variant>;
type MenuEvent = (i32, String, glib::Variant, u32);
type WindowAction = Box<dyn Fn(Option<&str>)>;
type ActivateAction = Box<dyn Fn((i32, i32), Option<&str>)>;

const ROOT: i32 = 0;
const OPEN: i32 = 1;
const QUIT: i32 = 2;
const ITEMS: [i32; 3] = [ROOT, OPEN, QUIT];

pub(super) struct Tray {
    inner: Rc<Inner>,
}

struct Inner {
    bus: RefCell<Option<gio::DBusConnection>>,
    names: RefCell<Vec<gio::SignalSubscription>>,
    watcher: RefCell<Option<Watcher>>,
    watch_plasma: bool,
    plasma_present: Cell<bool>,
    watcher_confirmed: Cell<bool>,
    available: Cell<bool>,
    activation_token: RefCell<Option<(String, String)>>,
    on_activate: ActivateAction,
    on_show: WindowAction,
    on_quit: Box<dyn Fn()>,
    on_availability_changed: Box<dyn Fn(bool)>,
}

struct Watcher {
    owner: String,
    registered: bool,
    registration: Option<glib::JoinHandle<()>>,
    refresh: Option<glib::JoinHandle<()>>,
    _signals: [gio::SignalSubscription; 2],
}

impl Drop for Watcher {
    fn drop(&mut self) {
        for task in [self.registration.take(), self.refresh.take()]
            .into_iter()
            .flatten()
        {
            task.abort();
        }
    }
}

impl Tray {
    pub(super) fn new(
        on_activate: impl Fn((i32, i32), Option<&str>) + 'static,
        on_show: impl Fn(Option<&str>) + 'static,
        on_quit: impl Fn() + 'static,
        on_availability_changed: impl Fn(bool) + 'static,
    ) -> Self {
        let desktops = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        Self::start(
            is_plasma(&desktops),
            on_activate,
            on_show,
            on_quit,
            on_availability_changed,
        )
    }

    fn start(
        watch_plasma: bool,
        on_activate: impl Fn((i32, i32), Option<&str>) + 'static,
        on_show: impl Fn(Option<&str>) + 'static,
        on_quit: impl Fn() + 'static,
        on_availability_changed: impl Fn(bool) + 'static,
    ) -> Self {
        let inner = Rc::new(Inner {
            bus: RefCell::new(None),
            names: RefCell::new(Vec::new()),
            watcher: RefCell::new(None),
            watch_plasma,
            plasma_present: Cell::new(!watch_plasma),
            watcher_confirmed: Cell::new(false),
            available: Cell::new(false),
            activation_token: RefCell::new(None),
            on_activate: Box::new(on_activate),
            on_show: Box::new(on_show),
            on_quit: Box::new(on_quit),
            on_availability_changed: Box::new(on_availability_changed),
        });
        let weak = Rc::downgrade(&inner);
        glib::spawn_future_local(async move {
            // A separate connection lets us actually remove the watcher's item
            // when the preference is turned off without closing the app bus.
            let address = gio::spawn_blocking(|| {
                gio::dbus_address_get_for_bus_sync(gio::BusType::Session, gio::Cancellable::NONE)
            })
            .await;
            if let Ok(Ok(address)) = address
                && let Ok(bus) = gio::DBusConnection::for_address_future(
                    &address,
                    gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                        | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
                    None,
                )
                .await
                && let Some(inner) = weak.upgrade()
            {
                inner.export(bus);
            }
        });
        Self { inner }
    }

    pub(super) fn is_available(&self) -> bool {
        self.inner.available.get()
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.inner.names.take();
        self.inner.watcher.take();
        if let Some(bus) = self.inner.bus.take() {
            bus.close(gio::Cancellable::NONE, |_| {});
        }
    }
}

impl Inner {
    fn export(self: &Rc<Self>, bus: gio::DBusConnection) {
        bus.set_exit_on_close(false);
        if !self.export_objects(&bus) {
            bus.close(gio::Cancellable::NONE, |_| {});
            return;
        }
        self.bus.replace(Some(bus.clone()));
        let weak = Rc::downgrade(self);
        let watcher = watch_name(&bus, WATCHER, move |bus, owner| {
            if let Some(inner) = weak.upgrade() {
                if let Some(owner) = owner {
                    inner.watcher_appeared(bus, owner);
                } else {
                    inner.watcher_lost();
                }
            }
        });
        self.names.borrow_mut().push(watcher);
        if self.watch_plasma {
            // KDE's watcher runs in kded and reports a host even when the panel
            // is down. Require the panel's bus name as a separate liveness check.
            let weak = Rc::downgrade(self);
            let plasma = watch_name(&bus, PLASMA, move |_, owner| {
                if let Some(inner) = weak.upgrade() {
                    inner.plasma_present.set(owner.is_some());
                    inner.update_availability();
                }
            });
            self.names.borrow_mut().push(plasma);
        }
        let weak = Rc::downgrade(self);
        bus.connect_local("closed", false, move |_| {
            if let Some(inner) = weak.upgrade() {
                inner.watcher_lost();
            }
            None
        });
    }

    fn export_objects(self: &Rc<Self>, bus: &gio::DBusConnection) -> bool {
        let item = interface(ITEM_XML, ITEM);
        let menu = interface(MENU_XML, MENU);
        let icon = icon_pixmap();
        let inner = Rc::downgrade(self);
        let item_registration = bus
            .register_object(ITEM_PATH, &item)
            .property(move |_, _, _, _, name| item_property(name, &icon))
            .method_call(move |_, sender, _, _, method, args, invocation| {
                invocation.return_value(None);
                let Some(inner) = inner.upgrade().filter(|inner| inner.available.get()) else {
                    return;
                };
                match method {
                    "Activate" => {
                        let token = inner.take_token(sender);
                        if let Some(position) = args.get::<(i32, i32)>() {
                            (inner.on_activate)(position, token.as_deref());
                        }
                    }
                    "ProvideXdgActivationToken" => {
                        if let Some(sender) = sender
                            && let Some((token,)) = args.get::<(String,)>()
                            && !token.is_empty()
                            && token.len() <= 4_096
                        {
                            inner.activation_token.replace(Some((sender.into(), token)));
                        }
                    }
                    _ => unreachable!(),
                }
            })
            .build();
        if item_registration.is_err() {
            return false;
        }
        let inner = Rc::downgrade(self);
        bus.register_object(MENU_PATH, &menu)
            .property(|_, _, _, _, name| menu_property(name))
            .method_call(move |_, sender, _, _, method, args, invocation| {
                let Some(inner) = inner.upgrade() else {
                    invocation
                        .return_dbus_error("org.freedesktop.DBus.Error.Failed", "Tray is closed");
                    return;
                };
                inner.menu_call(sender, method, &args, invocation);
            })
            .build()
            .is_ok()
    }

    fn watcher_appeared(self: &Rc<Self>, bus: &gio::DBusConnection, owner: &str) {
        let signal_handler = |interface| {
            let weak = Rc::downgrade(self);
            bus.subscribe_to_signal(
                Some(WATCHER),
                Some(interface),
                None,
                Some(WATCHER_PATH),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    if matches!(
                        signal.signal_name,
                        "StatusNotifierHostRegistered"
                            | "StatusNotifierHostUnregistered"
                            | "StatusNotifierItemRegistered"
                            | "StatusNotifierItemUnregistered"
                            | "PropertiesChanged"
                    ) && let Some(inner) = weak.upgrade()
                    {
                        inner.refresh(signal.connection, signal.sender_name);
                    }
                },
            )
        };
        self.watcher.replace(Some(Watcher {
            owner: owner.into(),
            registered: false,
            registration: None,
            refresh: None,
            _signals: [signal_handler(WATCHER), signal_handler(PROPERTIES)],
        }));
        self.watcher_confirmed.set(false);
        self.update_availability();

        let weak = Rc::downgrade(self);
        let owner = owner.to_owned();
        let bus = bus.clone();
        let registration = glib::spawn_future_local(async move {
            let result = bus
                .call_future(
                    Some(WATCHER),
                    WATCHER_PATH,
                    WATCHER,
                    "RegisterStatusNotifierItem",
                    Some(&(ITEM_PATH,).to_variant()),
                    None,
                    gio::DBusCallFlags::NO_AUTO_START,
                    TIMEOUT_MS,
                )
                .await;
            if result.is_ok()
                && let Some(inner) = weak.upgrade()
            {
                {
                    let mut state = inner.watcher.borrow_mut();
                    let Some(watcher) = state.as_mut().filter(|watcher| watcher.owner == owner)
                    else {
                        return;
                    };
                    watcher.registration.take();
                    watcher.registered = true;
                }
                inner.refresh(&bus, &owner);
            }
        });
        if let Some(watcher) = self.watcher.borrow_mut().as_mut() {
            watcher.registration = Some(registration);
        }
    }

    fn watcher_lost(&self) {
        self.watcher.take();
        self.watcher_confirmed.set(false);
        self.update_availability();
    }

    fn refresh(self: &Rc<Self>, bus: &gio::DBusConnection, owner: &str) {
        let mut state = self.watcher.borrow_mut();
        let Some(watcher) = state
            .as_mut()
            .filter(|watcher| watcher.owner == owner && watcher.registered)
        else {
            return;
        };
        if let Some(refresh) = watcher.refresh.take() {
            refresh.abort();
        }
        let Some(unique_name) = bus.unique_name() else {
            return;
        };
        let item = format!("{unique_name}{ITEM_PATH}");
        let owner = owner.to_owned();
        let bus = bus.clone();
        let weak = Rc::downgrade(self);
        watcher.refresh = Some(glib::spawn_future_local(async move {
            let result = bus
                .call_future(
                    Some(WATCHER),
                    WATCHER_PATH,
                    PROPERTIES,
                    "GetAll",
                    Some(&(WATCHER,).to_variant()),
                    None,
                    gio::DBusCallFlags::NO_AUTO_START,
                    TIMEOUT_MS,
                )
                .await;
            if let Some(inner) = weak.upgrade() {
                {
                    let mut state = inner.watcher.borrow_mut();
                    let Some(watcher) = state.as_mut().filter(|watcher| watcher.owner == owner)
                    else {
                        return;
                    };
                    watcher.refresh.take();
                }
                let available = result
                    .ok()
                    .and_then(|reply| reply.get::<(BTreeMap<String, glib::Variant>,)>())
                    .is_some_and(|(properties,)| {
                        watcher_has_item(&properties, &item, &unique_name)
                    });
                inner.watcher_confirmed.set(available);
                inner.update_availability();
            }
        }));
    }

    fn update_availability(&self) {
        let available = self.watcher_confirmed.get() && self.plasma_present.get();
        if self.available.replace(available) != available {
            if !available {
                self.activation_token.take();
            }
            (self.on_availability_changed)(available);
        }
    }

    fn take_token(&self, sender: Option<&str>) -> Option<String> {
        self.activation_token
            .take()
            .filter(|(owner, _)| Some(owner.as_str()) == sender)
            .map(|(_, token)| token)
    }

    fn menu_call(
        &self,
        sender: Option<&str>,
        method: &str,
        args: &glib::Variant,
        invocation: gio::DBusMethodInvocation,
    ) {
        let reply = match method {
            "GetLayout" => args
                .get::<(i32, i32, Vec<String>)>()
                .and_then(|(id, depth, names)| menu_node(id, depth, &names))
                .map(|node| (1u32, node).to_variant()),
            "GetGroupProperties" => args.get::<(Vec<i32>, Vec<String>)>().map(|(ids, names)| {
                let ids = if ids.is_empty() {
                    &ITEMS
                } else {
                    ids.as_slice()
                };
                let properties: Vec<_> = ids
                    .iter()
                    .filter_map(|&id| menu_properties(id, &names).map(|props| (id, props)))
                    .collect();
                (properties,).to_variant()
            }),
            "GetProperty" => args
                .get::<(i32, String)>()
                .and_then(|(id, name)| menu_properties(id, &[])?.remove(name.as_str()))
                .map(|value| (value,).to_variant()),
            "AboutToShow" => args
                .get::<(i32,)>()
                .filter(|(id,)| ITEMS.contains(id))
                .map(|_| (false,).to_variant()),
            "AboutToShowGroup" => args.get::<(Vec<i32>,)>().map(|(ids,)| {
                let invalid: Vec<_> = ids.into_iter().filter(|id| !ITEMS.contains(id)).collect();
                (Vec::<i32>::new(), invalid).to_variant()
            }),
            "Event" => {
                if let Some(event) = args
                    .get::<MenuEvent>()
                    .filter(|event| ITEMS.contains(&event.0))
                {
                    invocation.return_value(None);
                    self.menu_event(sender, event);
                    return;
                }
                None
            }
            "EventGroup" => {
                if let Some((events,)) = args.get::<(Vec<MenuEvent>,)>() {
                    let invalid: Vec<_> = events
                        .iter()
                        .filter_map(|event| (!ITEMS.contains(&event.0)).then_some(event.0))
                        .collect();
                    invocation.return_value(Some(&(invalid,).to_variant()));
                    for event in events {
                        let quits = event.0 == QUIT && event.1 == "clicked";
                        self.menu_event(sender, event);
                        if quits {
                            break;
                        }
                    }
                    return;
                }
                None
            }
            _ => unreachable!(),
        };
        if let Some(reply) = reply {
            invocation.return_value(Some(&reply));
        } else {
            invocation.return_dbus_error(
                "org.freedesktop.DBus.Error.InvalidArgs",
                "Unknown menu entry",
            );
        }
    }

    fn menu_event(&self, sender: Option<&str>, (id, event, _, _): MenuEvent) {
        if event == "clicked" && self.available.get() {
            match id {
                OPEN => {
                    let token = self.take_token(sender);
                    (self.on_show)(token.as_deref());
                }
                QUIT => (self.on_quit)(),
                _ => {}
            }
        }
    }
}

fn is_plasma(desktops: &str) -> bool {
    desktops.split(':').any(|desktop| {
        desktop.eq_ignore_ascii_case("KDE") || desktop.eq_ignore_ascii_case("Plasma")
    })
}

fn watch_name(
    bus: &gio::DBusConnection,
    name: &'static str,
    changed: impl Fn(&gio::DBusConnection, Option<&str>) + 'static,
) -> gio::SignalSubscription {
    let changed = Rc::new(changed);
    let seen_signal = Rc::new(Cell::new(false));
    let callback = changed.clone();
    let saw_signal = seen_signal.clone();
    let subscription = bus.subscribe_to_signal(
        Some(DBUS),
        Some(DBUS),
        Some("NameOwnerChanged"),
        Some(DBUS_PATH),
        Some(name),
        gio::DBusSignalFlags::NONE,
        move |signal| {
            if let Some((_, _, owner)) = signal.parameters.get::<(String, String, String)>() {
                saw_signal.set(true);
                callback(
                    signal.connection,
                    (!owner.is_empty()).then_some(owner.as_str()),
                );
            }
        },
    );
    let bus = bus.clone();
    glib::spawn_future_local(async move {
        let result = bus
            .call_future(
                Some(DBUS),
                DBUS_PATH,
                DBUS,
                "GetNameOwner",
                Some(&(name,).to_variant()),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                TIMEOUT_MS,
            )
            .await;
        if !seen_signal.get() {
            let owner = result.ok().and_then(|reply| reply.get::<(String,)>());
            changed(&bus, owner.as_ref().map(|(owner,)| owner.as_str()));
        }
    });
    subscription
}

fn interface(xml: &str, name: &str) -> gio::DBusInterfaceInfo {
    gio::DBusNodeInfo::for_xml(xml)
        .expect("built-in tray introspection is valid")
        .lookup_interface(name)
        .expect("built-in tray interface exists")
}

fn watcher_has_item(
    properties: &BTreeMap<String, glib::Variant>,
    item: &str,
    unique_name: &str,
) -> bool {
    properties
        .get("IsStatusNotifierHostRegistered")
        .and_then(glib::Variant::get::<bool>)
        == Some(true)
        && properties
            .get("RegisteredStatusNotifierItems")
            .and_then(glib::Variant::get::<Vec<String>>)
            .is_some_and(|items| items.iter().any(|name| name == item || name == unique_name))
}

fn item_property(name: &str, icon: &glib::Variant) -> glib::Variant {
    match name {
        "Category" => "ApplicationStatus".to_variant(),
        "Id" | "IconName" => APP_ID.to_variant(),
        "Title" => "Karin".to_variant(),
        "Status" => "Active".to_variant(),
        "IconPixmap" => icon.clone(),
        "ItemIsMenu" => false.to_variant(),
        "Menu" => ObjectPath::try_from(MENU_PATH).unwrap().to_variant(),
        _ => unreachable!(),
    }
}

fn icon_pixmap() -> glib::Variant {
    let png =
        include_bytes!("../../packaging/icons/hicolor/32x32/apps/io.github.tracerxbrhd.Karin.png");
    let icon = gdk_pixbuf::Pixbuf::from_read(Cursor::new(png))
        .ok()
        .filter(|icon| icon.has_alpha() && icon.n_channels() == 4)
        .map(|icon| {
            let width = icon.width();
            let height = icon.height();
            let bytes = icon.read_pixel_bytes();
            let mut argb = Vec::with_capacity((width * height * 4) as usize);
            for y in 0..height {
                let start = (y * icon.rowstride()) as usize;
                let end = start + (width * 4) as usize;
                for rgba in bytes[start..end].as_chunks::<4>().0 {
                    argb.extend_from_slice(&[rgba[3], rgba[0], rgba[1], rgba[2]]);
                }
            }
            (width, height, argb)
        });
    icon.into_iter().collect::<Vec<_>>().to_variant()
}

fn menu_property(name: &str) -> glib::Variant {
    match name {
        "Version" => 4u32.to_variant(),
        "Status" => "normal".to_variant(),
        "TextDirection" => "ltr".to_variant(),
        "IconThemePath" => Vec::<String>::new().to_variant(),
        _ => unreachable!(),
    }
}

fn menu_properties(id: i32, names: &[String]) -> Option<Properties> {
    let (name, value) = match id {
        ROOT => ("children-display", "submenu"),
        OPEN => ("label", "Открыть Karin"),
        QUIT => ("label", "Выйти из Karin"),
        _ => return None,
    };
    let mut properties = Properties::new();
    if names.is_empty() || names.iter().any(|requested| requested == name) {
        properties.insert(name, value.to_variant());
    }
    Some(properties)
}

fn menu_node(
    id: i32,
    depth: i32,
    names: &[String],
) -> Option<(i32, Properties, Vec<glib::Variant>)> {
    let properties = menu_properties(id, names)?;
    let children = if id == ROOT && depth != 0 {
        [OPEN, QUIT]
            .into_iter()
            .filter_map(|id| menu_node(id, 0, names).map(|child| child.to_variant()))
            .collect()
    } else {
        Vec::new()
    };
    Some((id, properties, children))
}

#[cfg(test)]
mod tests;
