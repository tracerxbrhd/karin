// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use super::*;

#[test]
fn tray_desktop_and_menu_protocol_shapes() {
    assert!(is_plasma("KDE"));
    assert!(is_plasma("other:Plasma"));
    assert!(!is_plasma("KDE-compatible:GNOME"));
    let reply = (1u32, menu_node(ROOT, -1, &[]).unwrap()).to_variant();
    assert_eq!(reply.type_().as_str(), "(u(ia{sv}av))");
    let children = reply.child_value(1).child_value(2);
    assert_eq!(children.n_children(), 2);
    for (index, action) in [OPEN, QUIT].into_iter().enumerate() {
        let child = children.child_value(index).as_variant().unwrap();
        assert_eq!(child.child_value(0).get::<i32>(), Some(action));
    }
    let icon = icon_pixmap();
    assert_eq!(icon.type_().as_str(), "a(iiay)");
    assert_eq!(icon.n_children(), 1);
}

const WATCHER_XML: &str = r#"
<node>
  <interface name="org.kde.StatusNotifierWatcher">
    <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
    <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
    <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
  </interface>
</node>
"#;

async fn connect() -> gio::DBusConnection {
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
    gio::DBusConnection::for_address_future(
        &address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
    )
    .await
    .unwrap()
}

async fn call(
    bus: &gio::DBusConnection,
    (name, path, interface): (&str, &str, &str),
    method: &str,
    args: glib::Variant,
) -> glib::Variant {
    bus.call_future(
        Some(name),
        path,
        interface,
        method,
        Some(&args),
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        TIMEOUT_MS,
    )
    .await
    .unwrap()
}

async fn name(bus: &gio::DBusConnection, name: &str, own: bool) {
    let (method, args) = if own {
        ("RequestName", (name, 4u32).to_variant())
    } else {
        ("ReleaseName", (name,).to_variant())
    };
    let reply = call(bus, (DBUS, DBUS_PATH, DBUS), method, args).await;
    assert_eq!(reply.get::<(u32,)>(), Some((1,)));
}

async fn eventually(condition: impl Fn() -> bool) {
    for _ in 0..300 {
        if condition() {
            return;
        }
        glib::timeout_future(Duration::from_millis(10)).await;
    }
    panic!("tray observation timed out");
}

#[test]
#[ignore = "requires an isolated D-Bus session with dbus-run-session"]
fn native_tray_registration_menu_and_host_lifecycle() {
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            context.block_on(async {
                let watcher = connect().await;
                let panel = connect().await;
                name(&watcher, WATCHER, true).await;
                name(&panel, PLASMA, true).await;
                let sender = Rc::new(RefCell::new(None::<String>));
                let accepted = Rc::new(Cell::new(false));
                let host = Rc::new(Cell::new(true));
                let reads = Rc::new(Cell::new(0));
                let observed_sender = sender.clone();
                let item_sender = sender.clone();
                let item_accepted = accepted.clone();
                let host_state = host.clone();
                let property_reads = reads.clone();
                let registration = watcher
                    .register_object(WATCHER_PATH, &interface(WATCHER_XML, WATCHER))
                    .method_call(move |_, sender, _, _, method, args, invocation| {
                        assert_eq!(method, "RegisterStatusNotifierItem");
                        assert_eq!(args.get::<(String,)>(), Some((ITEM_PATH.into(),)));
                        observed_sender.replace(sender.map(str::to_owned));
                        invocation.return_value(None);
                    })
                    .property(move |_, _, _, _, property| match property {
                        "IsStatusNotifierHostRegistered" => host_state.get().to_variant(),
                        "RegisteredStatusNotifierItems" => {
                            property_reads.set(property_reads.get() + 1);
                            let item = item_sender
                                .borrow()
                                .as_ref()
                                .filter(|_| item_accepted.get())
                                .map(|sender| format!("{sender}{ITEM_PATH}"));
                            item.into_iter().collect::<Vec<_>>().to_variant()
                        }
                        _ => unreachable!(),
                    })
                    .build()
                    .unwrap();
                let availability = Rc::new(RefCell::new(Vec::new()));
                let activations = Rc::new(RefCell::new(Vec::new()));
                let seen = availability.clone();
                let clicked = activations.clone();
                let opened = activations.clone();
                let quit = activations.clone();
                let tray = Tray::start(
                    true,
                    move |token| {
                        clicked
                            .borrow_mut()
                            .push(("activate", token.map(str::to_owned)))
                    },
                    move |token| opened.borrow_mut().push(("open", token.map(str::to_owned))),
                    move || quit.borrow_mut().push(("quit", None)),
                    move |available| seen.borrow_mut().push(available),
                );
                eventually(|| reads.get() > 0).await;
                let item = sender.borrow().clone().unwrap();
                assert!(!tray.is_available());
                accepted.set(true);
                watcher
                    .emit_signal(
                        None,
                        WATCHER_PATH,
                        WATCHER,
                        "StatusNotifierItemRegistered",
                        Some(&(format!("{item}{ITEM_PATH}"),).to_variant()),
                    )
                    .unwrap();
                eventually(|| tray.is_available()).await;

                let menu = (item.as_str(), MENU_PATH, MENU);
                let icon = (item.as_str(), ITEM_PATH, ITEM);
                let layout = call(
                    &panel,
                    menu,
                    "GetLayout",
                    (ROOT, -1, Vec::<String>::new()).to_variant(),
                )
                .await;
                assert_eq!(layout.type_().as_str(), "(u(ia{sv}av))");
                call(
                    &panel,
                    icon,
                    "ProvideXdgActivationToken",
                    ("test-token",).to_variant(),
                )
                .await;
                call(&panel, icon, "Activate", (10i32, 20i32).to_variant()).await;
                for id in [OPEN, QUIT] {
                    call(
                        &panel,
                        menu,
                        "Event",
                        (id, "clicked", false.to_variant(), 0u32).to_variant(),
                    )
                    .await;
                }
                assert_eq!(
                    *activations.borrow(),
                    [
                        ("activate", Some("test-token".into())),
                        ("open", None),
                        ("quit", None)
                    ]
                );

                name(&panel, PLASMA, false).await;
                eventually(|| !tray.is_available()).await;
                name(&panel, PLASMA, true).await;
                eventually(|| tray.is_available()).await;

                host.set(false);
                watcher
                    .emit_signal(
                        None,
                        WATCHER_PATH,
                        WATCHER,
                        "StatusNotifierHostUnregistered",
                        None,
                    )
                    .unwrap();
                eventually(|| !tray.is_available()).await;
                host.set(true);
                watcher
                    .emit_signal(
                        None,
                        WATCHER_PATH,
                        WATCHER,
                        "StatusNotifierHostRegistered",
                        None,
                    )
                    .unwrap();
                eventually(|| tray.is_available()).await;

                name(&watcher, WATCHER, false).await;
                eventually(|| !tray.is_available()).await;
                name(&watcher, WATCHER, true).await;
                eventually(|| tray.is_available()).await;
                assert_eq!(
                    *availability.borrow(),
                    [true, false, true, false, true, false, true]
                );
                drop(tray);
                watcher.unregister_object(registration).unwrap();
            })
        })
        .unwrap();
}
