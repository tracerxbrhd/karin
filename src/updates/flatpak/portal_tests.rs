// SPDX-License-Identifier: GPL-3.0-or-later
//! The stock wire contract against an isolated session-bus service fixture.

use std::cell::Cell;

use super::*;

const PORTAL_XML: &str = r#"<node><interface name="org.freedesktop.portal.Flatpak">
    <method name="CreateUpdateMonitor"><arg type="a{sv}" direction="in"/><arg type="o" direction="out"/></method>
    <property name="version" type="u" access="read"/>
</interface></node>"#;
const MONITOR_XML: &str = r#"<node><interface name="org.freedesktop.portal.Flatpak.UpdateMonitor">
    <method name="Close"/><method name="Update"><arg type="s" direction="in"/><arg type="a{sv}" direction="in"/></method>
    <signal name="UpdateAvailable"><arg type="a{sv}"/></signal>
    <signal name="Progress"><arg type="a{sv}"/></signal>
</interface></node>"#;

#[derive(Default)]
struct Calls {
    installs: Cell<usize>,
    closes: Cell<usize>,
    monitors: RefCell<Vec<gio::RegistrationId>>,
}

fn emit_offer(
    bus: &gio::DBusConnection,
    to: Option<&str>,
    path: &str,
    running: &str,
    local: &str,
    remote: &str,
) {
    let report = Dict::from([
        ("running-commit".into(), running.to_variant()),
        ("local-commit".into(), local.to_variant()),
        ("remote-commit".into(), remote.to_variant()),
    ]);
    bus.emit_signal(
        to,
        path,
        MONITOR,
        "UpdateAvailable",
        Some(&(report,).to_variant()),
    )
    .unwrap();
}

fn register_portal(
    connection: &gio::DBusConnection,
    calls: &Rc<Calls>,
    running: &str,
    target: &str,
) -> gio::RegistrationId {
    let portal = gio::DBusNodeInfo::for_xml(PORTAL_XML)
        .unwrap()
        .lookup_interface(PORTAL)
        .unwrap();
    let monitor = gio::DBusNodeInfo::for_xml(MONITOR_XML)
        .unwrap()
        .lookup_interface(MONITOR)
        .unwrap();
    let calls = calls.clone();
    let running = running.to_owned();
    let target = target.to_owned();
    connection
        .register_object(ROOT, &portal)
        .property(|_, _, _, _, _| 2u32.to_variant())
        .method_call(move |bus, from, _, _, method, parameters, invocation| {
            assert_eq!(method, "CreateUpdateMonitor");
            let from = from.unwrap();
            let (opts,) = parameters.get::<(Dict,)>().unwrap();
            assert_eq!(opts.len(), 1);
            let token = opts["handle_token"].str().unwrap();
            let sender = from.strip_prefix(':').unwrap().replace('.', "_");
            let path = format!("{ROOT}/update_monitor/{sender}/{token}");
            let monitor_calls = calls.clone();
            let registration = bus
                .register_object(&path, &monitor)
                .method_call(
                    move |bus, from, path, _, method, args, request| match method {
                        "Close" => {
                            monitor_calls.closes.set(monitor_calls.closes.get() + 1);
                            request.return_value(None);
                        }
                        "Update" => {
                            let (parent, options) = args.get::<(String, Dict)>().unwrap();
                            assert_eq!(parent, "x11:1234");
                            assert!(options.is_empty());
                            let attempt = monitor_calls.installs.get();
                            monitor_calls.installs.set(attempt + 1);
                            let data = match attempt {
                                0 => Dict::from([
                                    ("status".into(), 3u32.to_variant()),
                                    (
                                        "error".into(),
                                        "org.freedesktop.DBus.Error.NotSupported".to_variant(),
                                    ),
                                ]),
                                1 => Dict::from([("status".into(), 1u32.to_variant())]),
                                _ => Dict::from([("status".into(), 0u32.to_variant())]),
                            };
                            bus.emit_signal(
                                from,
                                path,
                                MONITOR,
                                "Progress",
                                Some(&(data,).to_variant()),
                            )
                            .unwrap();
                            if attempt == 1 {
                                // Terminal signal before a delayed method error.
                                request.return_dbus_error(
                                    "org.freedesktop.DBus.Error.NoReply",
                                    "fixture",
                                );
                            } else {
                                request.return_value(None);
                            }
                        }
                        _ => panic!("unsupported update fixture request"),
                    },
                )
                .build()
                .unwrap();
            calls.monitors.borrow_mut().push(registration);
            // A unicast signal may arrive before the creation call returns.
            emit_offer(&bus, Some(from), &path, &running, &running, &target);
            let path = ObjectPath::try_from(path).unwrap();
            invocation.return_value(Some(&(path,).to_variant()));
        })
        .build()
        .unwrap()
}

async fn eventually(mut condition: impl FnMut() -> bool) {
    for _ in 0..400 {
        if condition() {
            return;
        }
        glib::timeout_future(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for the update monitor");
}

#[test]
#[ignore = "requires a fresh isolated session bus via dbus-run-session"]
fn native_flatpak_update_portal_monitor_and_transaction_protocol() {
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            context.block_on(async {
                let address = gio::dbus_address_get_for_bus_sync(
                    gio::BusType::Session,
                    gio::Cancellable::NONE,
                )
                .unwrap();
                let connection = || {
                    gio::DBusConnection::for_address_sync(
                        &address,
                        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
                        None,
                        gio::Cancellable::NONE,
                    )
                    .unwrap()
                };
                let service = connection();
                let client = connection();
                let target = Deployment {
                    version: semver::Version::new(0, 8, 3),
                    source_commit: "d".repeat(40),
                    ostree_commit: "b".repeat(64),
                    app_ref: format!("app/{}/{}/master", config::APP_ID, std::env::consts::ARCH),
                    key_fingerprint: KEY_PIN.trim().to_owned(),
                };
                let running = "a".repeat(64);
                let daemon = |message, arguments: glib::Variant| {
                    service
                        .call_sync(
                            Some(DBUS),
                            BUS_OBJECT,
                            DBUS,
                            message,
                            Some(&arguments),
                            None,
                            gio::DBusCallFlags::NONE,
                            CALL_TIMEOUT_MS,
                            gio::Cancellable::NONE,
                        )
                        .unwrap()
                };
                assert_eq!(
                    daemon("NameHasOwner", (PORTAL,).to_variant()).get::<(bool,)>(),
                    Some((false,)),
                    "fixture requires an empty session bus"
                );
                assert_eq!(
                    daemon("RequestName", (PORTAL, 4u32).to_variant()).get::<(u32,)>(),
                    Some((1,))
                );
                let calls = Rc::new(Calls::default());
                let root = register_portal(&service, &calls, &running, &target.ostree_commit);
                let open = |events: Rc<RefCell<Vec<PortalEvent>>>| {
                    FlatpakMonitor::connect(
                        client.clone(),
                        Instance {
                            app_ref: target.app_ref.clone(),
                            commit: running.clone(),
                        },
                        move |event| events.borrow_mut().push(event),
                    )
                };
                let events = Rc::new(RefCell::new(Vec::new()));
                let monitor = open(events.clone()).await.unwrap();
                eventually(|| monitor.availability(&target) == Availability::Install).await;
                assert_eq!(monitor.install(&target, "x11:1234").await, Ok(()));
                eventually(|| {
                    monitor.availability(&target)
                        == Availability::Waiting(WaitReason::SystemManagerRequired)
                })
                .await;
                assert_eq!(monitor.install(&target, "").await, Err(PortalError::Stale));
                emit_offer(
                    &service,
                    client.unique_name().as_deref(),
                    &monitor.path,
                    &running,
                    &target.ostree_commit,
                    &target.ostree_commit,
                );
                eventually(|| monitor.availability(&target) == Availability::Restart).await;
                monitor.close();
                monitor.close();
                eventually(|| calls.closes.get() == 1).await;

                // A fresh monitor after cancellation/reconnect owns its own reports.
                let second = open(events.clone()).await.unwrap();
                eventually(|| second.availability(&target) == Availability::Install).await;
                assert_eq!(second.install(&target, "x11:1234").await, Ok(()));
                eventually(|| second.availability(&target) == Availability::Restart).await;
                assert!(
                    events
                        .borrow()
                        .iter()
                        .any(|event| matches!(event, PortalEvent::Progress(Progress::Empty)))
                );
                let installing = open(events.clone()).await.unwrap();
                eventually(|| installing.availability(&target) == Availability::Install).await;
                assert_eq!(installing.install(&target, "x11:1234").await, Ok(()));
                eventually(|| {
                    events.borrow().iter().any(|event| {
                        matches!(event, PortalEvent::Progress(Progress::Running(None)))
                    })
                })
                .await;
                installing.close();
                eventually(|| calls.closes.get() == 2).await;
                let finished_late = Dict::from([("status".into(), 2u32.to_variant())]);
                service
                    .emit_signal(
                        client.unique_name().as_deref(),
                        &installing.path,
                        MONITOR,
                        "Progress",
                        Some(&(finished_late,).to_variant()),
                    )
                    .unwrap();
                assert_eq!(calls.installs.get(), 3);
                assert_eq!(
                    daemon("ReleaseName", (PORTAL,).to_variant()).get::<(u32,)>(),
                    Some((1,))
                );
                eventually(|| !second.is_open()).await;
                assert_eq!(
                    second.availability(&target),
                    Availability::Waiting(WaitReason::Unavailable)
                );
                {
                    let events = events.borrow();
                    assert_eq!(
                        events
                            .iter()
                            .filter(|event| matches!(event, PortalEvent::Disconnected))
                            .count(),
                        1
                    );
                    assert!(
                        !events
                            .iter()
                            .any(|event| matches!(event, PortalEvent::Progress(Progress::Done)))
                    );
                }
                assert_eq!(second.install(&target, "").await, Err(PortalError::Stale));
                service.unregister_object(root).unwrap();
                for id in calls.monitors.take() {
                    service.unregister_object(id).unwrap();
                }
            })
        })
        .unwrap();
}
