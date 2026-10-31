// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn stored_tokens_are_bounded_opaque_and_redacted() {
    let value = Zeroizing::new(b"synthetic-private-access-token".to_vec());
    let token = restored_token(value).unwrap();
    assert_eq!(format!("{token:?}"), "AccessToken([redacted])");
    for invalid in [
        Vec::new(),
        b"contains\nheader".to_vec(),
        b" leading-space".to_vec(),
        vec![b'x'; MAX_TOKEN + 1],
        vec![255],
    ] {
        assert!(matches!(
            restored_token(Zeroizing::new(invalid)),
            Err(StorageError::Corrupt)
        ));
    }
    let cancel = Cancellation::new();
    cancel.cancel();
    assert_eq!(cancel.check(), Err(StorageError::Cancelled));
}

#[test]
fn backend_errors_never_retain_service_payloads_or_secrets() {
    for (name, expected) in [
        (
            "org.freedesktop.Secret.Error.IsLocked",
            StorageError::Locked,
        ),
        (
            "org.freedesktop.DBus.Error.AccessDenied",
            StorageError::Permission,
        ),
        (
            "org.freedesktop.DBus.Error.ServiceUnknown",
            StorageError::Unavailable,
        ),
        ("org.freedesktop.DBus.Error.NoReply", StorageError::Timeout),
        ("org.example.UnknownError", StorageError::Unconfirmed),
    ] {
        let error = map_error(gio::DBusError::new_for_dbus_error(
            name,
            "synthetic-private-token-and-personal-payload",
        ));
        assert_eq!(error, expected);
        assert!(
            !format!("{error:?} {error} {}", error.user_message()).contains("synthetic-private")
        );
    }
}

fn private_test_directory(label: &str) -> PathBuf {
    use ring::rand::{SecureRandom, SystemRandom};
    use std::os::unix::fs::DirBuilderExt;
    let mut random = [0u8; 16];
    SystemRandom::new().fill(&mut random).unwrap();
    let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let directory = std::env::temp_dir().join(format!("karin-{label}-fixture-{suffix}"));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    directory
}

#[test]
fn handshake_timeout_keeps_current_intent_and_reports_a_retryable_failure() {
    use crate::session_lifecycle::{PersistenceState, SessionPersistence};
    use ring::rand::{SecureRandom, SystemRandom};
    use std::{
        os::unix::{fs::DirBuilderExt, net::UnixListener},
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    struct StalledBus {
        address: String,
    }
    impl CredentialBackend for StalledBus {
        fn probe(&self, cancel: &Cancellation) -> Result<BackendKind, StorageError> {
            with_context(|_| {
                connect_address(&self.address, cancel, Duration::from_millis(150))
                    .map(|_| BackendKind::SecretService)
            })
        }
        fn load(&self, cancel: &Cancellation) -> Result<Option<AccessToken>, StorageError> {
            self.probe(cancel).map(|_| None)
        }
        fn save_validated(&self, _: &AccessToken, _: &Cancellation) -> Result<(), StorageError> {
            Err(StorageError::Unavailable)
        }
        fn delete(&self, _: &Cancellation) -> Result<(), StorageError> {
            Err(StorageError::Unavailable)
        }
    }

    let mut random = [0u8; 16];
    SystemRandom::new().fill(&mut random).unwrap();
    let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let directory = std::env::temp_dir().join(format!("karin-stalled-bus-{suffix}"));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let socket = directory.join("bus");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let (stop, stopped) = mpsc::channel();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        let _peer = loop {
            match listener.accept() {
                Ok((peer, _)) => break peer,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "synthetic Unix peer did not connect"
                    );
                    thread::sleep(Duration::from_millis(1));
                }
                Err(_) => panic!("synthetic Unix listener failed"),
            }
        };
        // Accept the Unix connection, but never answer D-Bus authentication.
        // This exercises the actual transport timeout, not an invented error.
        let _ = stopped.recv_timeout(Duration::from_secs(2));
    });
    let manager = SessionPersistence::new(CredentialStore::with_backend(StalledBus {
        address: format!("unix:path={}", socket.display()),
    }));
    let ticket = manager.begin();
    assert_eq!(manager.probe(&ticket), Err(StorageError::Timeout));
    assert!(manager.is_current(&ticket));
    assert!(!ticket.cancellation().is_cancelled());
    assert_eq!(
        manager.state(),
        PersistenceState::Failed(StorageError::Timeout)
    );
    stop.send(()).unwrap();
    server.join().unwrap();
    let retry = manager.begin();
    assert!(manager.is_current(&retry));
    assert!(!manager.is_current(&ticket));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn portal_credential_files_reject_fifo_links_and_nonprivate_modes_before_transport() {
    use rustix::fs::{Mode, OFlags};
    use std::{
        fs::File,
        os::unix::fs::{PermissionsExt, symlink},
        sync::mpsc,
        thread,
        time::Duration,
    };
    let directory = private_test_directory("invalid-envelope");
    let parent = File::open(&directory).unwrap();
    let file = directory.join("session.enc");
    let store = CredentialStore::for_environment(true, directory.clone());
    rustix::fs::mkfifoat(&parent, "session.enc", Mode::RUSR | Mode::WUSR).unwrap();
    let worker_store = store.clone();
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        sender
            .send(worker_store.load(&Cancellation::new()))
            .unwrap();
    });
    let completed = receiver.recv_timeout(Duration::from_secs(1));
    if completed.is_err() {
        let writer = rustix::fs::openat(
            &parent,
            "session.enc",
            OFlags::WRONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .expect("release a regressed blocking credential FIFO reader");
        drop(writer);
    }
    worker.join().unwrap();
    assert!(matches!(completed, Ok(Err(StorageError::Corrupt))));
    std::fs::remove_file(&file).unwrap();

    let unrelated = directory.join("unrelated");
    std::fs::write(&unrelated, b"synthetic unrelated material").unwrap();
    std::fs::set_permissions(&unrelated, std::fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&unrelated, &file).unwrap();
    assert!(matches!(
        store.load(&Cancellation::new()),
        Err(StorageError::Io)
    ));
    std::fs::remove_file(&file).unwrap();
    std::fs::hard_link(&unrelated, &file).unwrap();
    assert!(matches!(
        store.load(&Cancellation::new()),
        Err(StorageError::Corrupt)
    ));
    std::fs::remove_file(&file).unwrap();
    std::fs::write(&file, b"synthetic malformed envelope").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(
        store.load(&Cancellation::new()),
        Err(StorageError::Permission)
    ));
    assert_eq!(
        std::fs::read(&unrelated).unwrap(),
        b"synthetic unrelated material"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

/// The fixture must own these names on an explicitly isolated session bus. It
/// refuses to run against an existing keyring or portal and uses synthetic data.
#[test]
#[ignore = "requires KARIN_TEST_SECRET_BUS=1 and an isolated dbus-run-session; no real keyring or account"]
fn native_secure_storage_secret_service_and_portal_protocol() {
    use gio::prelude::UnixFDListExtManual;
    use glib::variant::{Handle, ObjectPath, ToVariant};
    use std::{
        cell::RefCell,
        collections::BTreeMap,
        fs::File,
        io::Write,
        rc::Rc,
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
    assert_eq!(std::env::var("KARIN_TEST_SECRET_BUS").as_deref(), Ok("1"));
    const SERVICE: &str = "org.freedesktop.secrets";
    const PORTAL: &str = "org.freedesktop.portal.Desktop";
    const ROOT: &str = "/org/freedesktop/secrets";
    const COLLECTION: &str = "/org/freedesktop/secrets/collection/fixture";
    const ITEM: &str = "/org/freedesktop/secrets/collection/fixture/item";
    const SESSION: &str = "/org/freedesktop/secrets/session/fixture";
    const PROMPT: &str = "/org/freedesktop/secrets/prompt/fixture";
    const PORTAL_ROOT: &str = "/org/freedesktop/portal/desktop";
    type Secret = (ObjectPath, Vec<u8>, Vec<u8>, String);
    fn object(value: &str) -> ObjectPath {
        ObjectPath::try_from(value).unwrap()
    }
    fn pump<T: Send + 'static>(
        context: &glib::MainContext,
        operation: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let _ = sender.send(operation());
        });
        let deadline = Instant::now() + Duration::from_secs(15);
        let result = loop {
            if let Ok(result) = receiver.try_recv() {
                break result;
            }
            assert!(
                Instant::now() < deadline,
                "synthetic secure-storage operation timed out"
            );
            while context.pending() {
                context.iteration(false);
            }
            thread::sleep(Duration::from_millis(1));
        };
        worker.join().unwrap();
        result
    }
    fn observe_for(context: &glib::MainContext, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            for _ in 0..16 {
                if !context.pending() {
                    break;
                }
                context.iteration(false);
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn wait_until(context: &glib::MainContext, predicate: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !predicate() {
            assert!(
                Instant::now() < deadline,
                "synthetic service phase did not arrive"
            );
            observe_for(context, Duration::from_millis(5));
        }
    }
    fn receive<T>(context: &glib::MainContext, receiver: &mpsc::Receiver<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match receiver.try_recv() {
                Ok(value) => return value,
                Err(mpsc::TryRecvError::Empty) => {
                    assert!(Instant::now() < deadline, "synthetic worker did not finish");
                    observe_for(context, Duration::from_millis(5));
                }
                Err(mpsc::TryRecvError::Disconnected) => panic!("synthetic worker disconnected"),
            }
        }
    }
    #[derive(Default)]
    struct State {
        token: Option<String>,
        locked: bool,
        dismiss_prompt: bool,
        fail_delete: bool,
        malformed: bool,
        unavailable: bool,
        portal_available: bool,
        portal_cancel: bool,
        portal_hold: bool,
        portal_calls: usize,
        locator: usize,
        portal_identifier: bool,
        portal_secret_len: usize,
        portal_closes: usize,
        held_fd: Option<File>,
        defer_portal_start: bool,
        pending_portal_start: Option<(gio::DBusMethodInvocation, ObjectPath)>,
        defer_prompt_start: bool,
        pending_prompt_start: Option<gio::DBusMethodInvocation>,
        prompt_dismisses: usize,
        defer_create_item: bool,
        create_rejection: Option<&'static str>,
        pending_create_item: Option<(gio::DBusMethodInvocation, String)>,
        registrations: Vec<gio::RegistrationId>,
    }
    let context = glib::MainContext::new();
    context.with_thread_default(|| {
        let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
        for name in [SERVICE, PORTAL] {
            let reply = bus.call_sync(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", Some(&(name,).to_variant()), None, gio::DBusCallFlags::NONE, 1000, gio::Cancellable::NONE).unwrap();
            assert_eq!(reply.get::<(bool,)>(), Some((false,)), "fixture refuses an existing real service");
            let reply = bus.call_sync(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", "org.freedesktop.DBus", "RequestName", Some(&(name, 4u32).to_variant()), None, gio::DBusCallFlags::NONE, 1000, gio::Cancellable::NONE).unwrap();
            assert_eq!(reply.get::<(u32,)>(), Some((1,)));
        }
        let state = Rc::new(RefCell::new(State { portal_available: true, portal_identifier: true, portal_secret_len: 32, ..State::default() }));
        let xml = r#"<node><interface name="org.freedesktop.Secret.Service">
          <method name="ReadAlias"><arg type="s" direction="in"/><arg type="o" direction="out"/></method>
          <method name="SearchItems"><arg type="a{ss}" direction="in"/><arg type="ao" direction="out"/><arg type="ao" direction="out"/></method>
          <method name="OpenSession"><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="v" direction="out"/><arg type="o" direction="out"/></method>
          <method name="Unlock"><arg type="ao" direction="in"/><arg type="ao" direction="out"/><arg type="o" direction="out"/></method>
        </interface></node>"#;
        let info = gio::DBusNodeInfo::for_xml(xml).unwrap().lookup_interface("org.freedesktop.Secret.Service").unwrap();
        let changed = state.clone();
        let registration = bus.register_object(ROOT, &info).method_call(move |_,_,_,_,method,parameters,invocation| {
            let s = changed.borrow();
            if s.unavailable { invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownInterface", "synthetic-private-payload"); return; }
            match method {
                "ReadAlias" => invocation.return_value(Some(&(object(COLLECTION),).to_variant())),
                "SearchItems" => {
                    let (attributes,) = parameters.get::<(BTreeMap<String,String>,)>().unwrap();
                    assert_eq!(attributes.len(),3); assert_eq!(attributes["application"],crate::config::APP_ID); assert_eq!(attributes["karin-purpose"],"music-access-token-v1");
                    let list = if s.token.is_some() { vec![object(ITEM)] } else { Vec::new() };
                    let (unlocked,locked) = if s.locked { (Vec::<ObjectPath>::new(),list) } else { (list,Vec::new()) };
                    invocation.return_value(Some(&(unlocked,locked).to_variant()));
                }
                "OpenSession" => {
                    let (algorithm,_) = parameters.get::<(String,glib::Variant)>().unwrap(); assert_eq!(algorithm,"plain");
                    invocation.return_value(Some(&("".to_variant(),object(SESSION)).to_variant()));
                }
                "Unlock" => {
                    let (paths,) = parameters.get::<(Vec<ObjectPath>,)>().unwrap();
                    if s.locked { invocation.return_value(Some(&(Vec::<ObjectPath>::new(),object(PROMPT)).to_variant())); }
                    else { invocation.return_value(Some(&(paths,object("/")).to_variant())); }
                }
                _ => invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod","unsupported fixture method"),
            }
        }).build().unwrap(); state.borrow_mut().registrations.push(registration);
        let xml = r#"<node><interface name="org.freedesktop.Secret.Collection"><method name="CreateItem"><arg type="a{sv}" direction="in"/><arg type="(oayays)" direction="in"/><arg type="b" direction="in"/><arg type="o" direction="out"/><arg type="o" direction="out"/></method></interface></node>"#;
        let info = gio::DBusNodeInfo::for_xml(xml).unwrap().lookup_interface("org.freedesktop.Secret.Collection").unwrap(); let changed=state.clone();
        let registration=bus.register_object(COLLECTION,&info).method_call(move |_,_,_,_,_,parameters,invocation| {
            let mut s=changed.borrow_mut();
            if let Some(error)=s.create_rejection {invocation.return_dbus_error(error,"synthetic create rejection");return;}
            if s.locked { invocation.return_dbus_error("org.freedesktop.Secret.Error.IsLocked","synthetic-private-payload"); return; }
            let (properties,secret,replace)=parameters.get::<(BTreeMap<String,glib::Variant>,Secret,bool)>().unwrap(); assert!(replace); assert_eq!(secret.0,object(SESSION)); assert!(secret.1.is_empty()); assert!(properties.contains_key("org.freedesktop.Secret.Item.Attributes"));
            let value=String::from_utf8(secret.2).unwrap();
            if s.defer_create_item { s.pending_create_item=Some((invocation,value));return; }
            s.token=Some(value); invocation.return_value(Some(&(object(ITEM),object("/")).to_variant()));
        }).build().unwrap(); state.borrow_mut().registrations.push(registration);
        let xml=r#"<node><interface name="org.freedesktop.Secret.Item"><method name="GetSecret"><arg type="o" direction="in"/><arg type="(oayays)" direction="out"/></method><method name="Delete"><arg type="o" direction="out"/></method></interface></node>"#;
        let info=gio::DBusNodeInfo::for_xml(xml).unwrap().lookup_interface("org.freedesktop.Secret.Item").unwrap(); let changed=state.clone();
        let registration=bus.register_object(ITEM,&info).method_call(move |_,_,_,_,method,_,invocation| {
            let mut s=changed.borrow_mut(); match method {
                "GetSecret" => { let value=if s.malformed { "invalid\nsynthetic" } else { s.token.as_deref().unwrap_or("") }; let secret:Secret=(object(SESSION),Vec::new(),value.as_bytes().to_vec(),"text/plain; charset=utf-8".into()); invocation.return_value(Some(&(secret,).to_variant())); }
                "Delete" => { if !s.fail_delete { s.token=None; } invocation.return_value(Some(&(object("/"),).to_variant())); }
                _ => invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod","unsupported fixture method"),
            }
        }).build().unwrap(); state.borrow_mut().registrations.push(registration);
        let xml=r#"<node><interface name="org.freedesktop.Secret.Session"><method name="Close"/></interface></node>"#;
        let info=gio::DBusNodeInfo::for_xml(xml).unwrap().lookup_interface("org.freedesktop.Secret.Session").unwrap();
        let registration=bus.register_object(SESSION,&info).method_call(move |_,_,_,_,_,_,invocation| invocation.return_value(None)).build().unwrap(); state.borrow_mut().registrations.push(registration);
        let xml=r#"<node><interface name="org.freedesktop.Secret.Prompt"><method name="Prompt"><arg type="s" direction="in"/></method><method name="Dismiss"/><signal name="Completed"><arg type="b"/><arg type="v"/></signal></interface></node>"#;
        let info=gio::DBusNodeInfo::for_xml(xml).unwrap().lookup_interface("org.freedesktop.Secret.Prompt").unwrap(); let changed=state.clone();
        let registration=bus.register_object(PROMPT,&info).method_call(move |connection,sender,_,_,method,_,invocation| {
            let mut s=changed.borrow_mut();
            if method=="Prompt" && s.defer_prompt_start {s.pending_prompt_start=Some(invocation);return;}
            if method=="Dismiss" {s.prompt_dismisses+=1;}
            let dismissed=method=="Dismiss" || s.dismiss_prompt; if !dismissed { s.locked=false; } invocation.return_value(None);
            connection.emit_signal(sender,PROMPT,"org.freedesktop.Secret.Prompt","Completed",Some(&(dismissed,vec![object(ITEM)].to_variant()).to_variant())).unwrap();
        }).build().unwrap(); state.borrow_mut().registrations.push(registration);
        let xml=r#"<node><interface name="org.freedesktop.portal.Secret"><method name="RetrieveSecret"><arg type="h" direction="in"/><arg type="a{sv}" direction="in"/><arg type="o" direction="out"/></method><property name="version" type="u" access="read"/></interface></node>"#;
        let info=gio::DBusNodeInfo::for_xml(xml).unwrap().lookup_interface("org.freedesktop.portal.Secret").unwrap(); let changed=state.clone(); let properties=state.clone();
        let registration=bus.register_object(PORTAL_ROOT,&info).property(move |_,_,_,_,_| if properties.borrow().portal_available {1u32} else {0u32}.to_variant()).method_call(move |connection,sender,_,_,_,parameters,invocation| {
            let mut s=changed.borrow_mut();s.portal_calls+=1;let (handle,options)=parameters.get::<(Handle,BTreeMap<String,glib::Variant>)>().unwrap();
            if !s.portal_identifier {assert!(!options.contains_key("token"));}
            else if s.locator>0 { assert_eq!(options.get("token").and_then(|v|v.str()),Some(format!("synthetic-portal-{}",s.locator).as_str())); }
            let name=sender.unwrap().trim_start_matches(':').replace('.',"_"); let token=options["handle_token"].str().unwrap(); let path=format!("/org/freedesktop/portal/desktop/request/{name}/{token}");
            let fd=invocation.message().unix_fd_list().unwrap().get(handle.0).unwrap(); let mut output=File::from(fd);
            if s.portal_hold || s.defer_portal_start {
                let defer=s.defer_portal_start;
                s.held_fd=Some(output); drop(s);
                let request_xml=r#"<node><interface name="org.freedesktop.portal.Request"><method name="Close"/></interface></node>"#;
                let request_info=gio::DBusNodeInfo::for_xml(request_xml).unwrap().lookup_interface("org.freedesktop.portal.Request").unwrap(); let closing=changed.clone();
                let id=connection.register_object(&path,&request_info).method_call(move |_,_,_,_,_,_,invocation| { let mut state=closing.borrow_mut();state.held_fd=None;state.portal_closes+=1;invocation.return_value(None); }).build().unwrap();changed.borrow_mut().registrations.push(id);
                if defer {changed.borrow_mut().pending_portal_start=Some((invocation,object(&path)));}else{invocation.return_value(Some(&(object(&path),).to_variant()));}return;
            }
            let status=if s.portal_cancel {1u32} else {0u32};if status==0 {output.write_all(&vec![42u8;s.portal_secret_len]).unwrap();if s.portal_identifier {s.locator+=1;}}drop(output);
            let results=if status==0 && s.portal_identifier {BTreeMap::from([("token".to_owned(),format!("synthetic-portal-{}",s.locator).to_variant())])}else{BTreeMap::new()};
            invocation.return_value(Some(&(object(&path),).to_variant()));connection.emit_signal(sender,&path,"org.freedesktop.portal.Request","Response",Some(&(status,results).to_variant())).unwrap();
        }).build().unwrap();state.borrow_mut().registrations.push(registration);

        let token=AccessToken::new(Zeroizing::new("synthetic-private-access-token".into()),None);
        let native_directory=private_test_directory("native-secret");
        let native=CredentialStore::for_environment(false,native_directory.clone());
        let store=native.clone();assert_eq!(pump(&context,move||store.probe(&Cancellation::new())),Ok(BackendKind::SecretService));
        let store=native.clone();assert!(pump(&context,move||store.load(&Cancellation::new())).unwrap().is_none());
        let store=native.clone();let t=token.clone();assert_eq!(pump(&context,move||store.save_validated(&t,&Cancellation::new())),Ok(()));
        state.borrow_mut().locked=true;let store=native.clone();let loaded=pump(&context,move||store.load(&Cancellation::new())).unwrap().unwrap();assert_eq!(loaded.expose(),token.expose());assert!(!state.borrow().locked);
        state.borrow_mut().locked=true;state.borrow_mut().dismiss_prompt=true;let store=native.clone();assert!(matches!(pump(&context,move||store.load(&Cancellation::new())),Err(StorageError::Cancelled)));
        state.borrow_mut().locked=false;state.borrow_mut().dismiss_prompt=false;state.borrow_mut().malformed=true;let store=native.clone();assert!(matches!(pump(&context,move||store.load(&Cancellation::new())),Err(StorageError::Corrupt)));state.borrow_mut().malformed=false;
        state.borrow_mut().fail_delete=true;let store=native.clone();assert_eq!(pump(&context,move||store.delete(&Cancellation::new())),Err(StorageError::Unconfirmed));state.borrow_mut().fail_delete=false;let store=native.clone();assert_eq!(pump(&context,move||store.delete(&Cancellation::new())),Ok(()));
        state.borrow_mut().unavailable=true;let store=native.clone();assert_eq!(pump(&context,move||store.probe(&Cancellation::new())),Err(StorageError::Unavailable));state.borrow_mut().unavailable=false;

        // These two exact remote errors conclusively reject the write. They
        // must not leave an unresolved fence that prevents ordinary retry.
        for (rejection,expected) in [("org.freedesktop.DBus.Error.AccessDenied",StorageError::Permission),("org.freedesktop.Secret.Error.IsLocked",StorageError::Locked)] {
            state.borrow_mut().create_rejection=Some(rejection);
            let store=native.clone();let value=token.clone();
            assert_eq!(pump(&context,move||store.save_validated(&value,&Cancellation::new())),Err(expected));
            assert!(!native_directory.join("native-write.pending").exists());assert!(state.borrow().token.is_none());
            let store=native.clone();assert!(pump(&context,move||store.load(&Cancellation::new())).unwrap().is_none());
            state.borrow_mut().create_rejection=None;
            let store=native.clone();let value=token.clone();assert_eq!(pump(&context,move||store.save_validated(&value,&Cancellation::new())),Ok(()));
            let store=native.clone();assert_eq!(pump(&context,move||store.delete(&Cancellation::new())),Ok(()));
        }
        // A different remote error is not proof that no commit occurred.
        state.borrow_mut().create_rejection=Some("org.example.CreateItemFailure");
        let store=native.clone();let value=token.clone();assert_eq!(pump(&context,move||store.save_validated(&value,&Cancellation::new())),Err(StorageError::Unconfirmed));
        assert!(native_directory.join("native-write.pending").is_file());
        let store=native.clone();assert!(matches!(pump(&context,move||store.load(&Cancellation::new())),Err(StorageError::Unconfirmed)));
        let store=native.clone();assert_eq!(pump(&context,move||store.delete(&Cancellation::new())),Err(StorageError::Unconfirmed));
        state.borrow_mut().create_rejection=None;state.borrow_mut().token=Some(token.expose().to_owned());
        let store=native.clone();assert_eq!(pump(&context,move||store.delete(&Cancellation::new())),Ok(()));
        assert!(!native_directory.join("native-write.pending").exists());

        // Cancel while Prompt() itself has not returned. A client-side RPC
        // cancellation must not leave the already-created wallet prompt alive.
        state.borrow_mut().token=Some(token.expose().to_owned());
        state.borrow_mut().locked=true;
        state.borrow_mut().defer_prompt_start=true;
        let dismisses=state.borrow().prompt_dismisses;
        let cancel=Cancellation::new();let worker_cancel=cancel.clone();let store=native.clone();
        let (completed,reply)=mpsc::channel();
        let worker=thread::spawn(move||completed.send(store.load(&worker_cancel)).unwrap());
        wait_until(&context,||state.borrow().pending_prompt_start.is_some());
        cancel.cancel();observe_for(&context,Duration::from_millis(500));
        let invocation=state.borrow_mut().pending_prompt_start.take().unwrap();
        invocation.return_value(None);
        assert!(matches!(receive(&context,&reply),Err(StorageError::Cancelled)));
        worker.join().unwrap();
        assert!(state.borrow().prompt_dismisses>dismisses,"cancelled Prompt start must dismiss the wallet request");
        state.borrow_mut().defer_prompt_start=false;state.borrow_mut().locked=false;state.borrow_mut().token=None;

        // Commit CreateItem only after cancellation/new logout intent. The
        // older save must await its bounded mutation reply and compensate the
        // actual committed value before the newer deletion can be confirmed.
        state.borrow_mut().defer_create_item=true;
        let persistence=crate::session_lifecycle::SessionPersistence::new(native.clone());
        let saving=persistence.begin();let manager=persistence.clone();let value=token.clone();
        let (saved,save_reply)=mpsc::channel();
        let save_worker=thread::spawn(move||saved.send(manager.save_validated(&saving,&value)).unwrap());
        wait_until(&context,||state.borrow().pending_create_item.is_some());
        let forgetting=persistence.begin();let manager=persistence.clone();
        let (deleted,delete_reply)=mpsc::channel();
        let delete_worker=thread::spawn(move||deleted.send(manager.delete(&forgetting)).unwrap());
        observe_for(&context,Duration::from_millis(500));
        let (invocation,value)=state.borrow_mut().pending_create_item.take().unwrap();
        state.borrow_mut().token=Some(value);state.borrow_mut().defer_create_item=false;
        invocation.return_value(Some(&(object(ITEM),object("/")).to_variant()));
        assert_eq!(receive(&context,&save_reply),Err(StorageError::Cancelled));
        assert_eq!(receive(&context,&delete_reply),Ok(()));
        save_worker.join().unwrap();delete_worker.join().unwrap();
        assert!(state.borrow().token.is_none(),"a late server-side commit must not resurrect a saved login");
        assert_eq!(persistence.state(),crate::session_lifecycle::PersistenceState::MemoryOnly);

        // An unknown/timed-out CreateItem may still commit after an empty
        // search. Its durable non-secret fence survives a backend restart and
        // must not be cleared merely because no item is visible yet.
        let marker=super::portal::Directory::native_marker(&native_directory,true).unwrap().unwrap();
        marker.write(b"Karin native CreateItem pending v1\n",&Cancellation::new()).unwrap();
        let restarted=CredentialStore::for_environment(false,native_directory.clone());
        let store=restarted.clone();assert!(matches!(pump(&context,move||store.load(&Cancellation::new())),Err(StorageError::Unconfirmed)));
        let store=restarted.clone();assert_eq!(pump(&context,move||store.delete(&Cancellation::new())),Err(StorageError::Unconfirmed));
        assert!(marker.read().unwrap().is_some(),"an unresolved write fence must remain durable");
        // Simulate observing the late committed item. Only verified removal
        // of that item can now settle the earlier unknown mutation.
        state.borrow_mut().token=Some(token.expose().to_owned());
        let store=restarted.clone();assert_eq!(pump(&context,move||store.delete(&Cancellation::new())),Ok(()));
        assert!(marker.read().unwrap().is_none());assert!(state.borrow().token.is_none());
        let store=restarted.clone();assert!(pump(&context,move||store.load(&Cancellation::new())).unwrap().is_none());

        let directory=private_test_directory("portal-secret");
        let portal=CredentialStore::for_environment(true,directory.clone());let store=portal.clone();assert!(pump(&context,move||store.load(&Cancellation::new())).unwrap().is_none());assert_eq!(state.borrow().portal_calls,0,"missing envelope must not prompt");
        let store=portal.clone();assert_eq!(pump(&context,move||store.probe(&Cancellation::new())),Ok(BackendKind::SecretPortal));assert_eq!(state.borrow().portal_calls,0,"capability probe must not retrieve a key");
        let store=portal.clone();let t=token.clone();assert_eq!(pump(&context,move||store.save_validated(&t,&Cancellation::new())),Ok(()));let bytes=std::fs::read(directory.join("session.enc")).unwrap();assert!(!bytes.windows(token.expose().len()).any(|s|s==token.expose().as_bytes()));
        use std::os::unix::fs::PermissionsExt;assert_eq!(std::fs::metadata(&directory).unwrap().permissions().mode()&0o777,0o700);assert_eq!(std::fs::metadata(directory.join("session.enc")).unwrap().permissions().mode()&0o777,0o600);
        for _ in 0..2 {let store=portal.clone();let loaded=pump(&context,move||store.load(&Cancellation::new())).unwrap().unwrap();assert_eq!(loaded.expose(),token.expose());}
        // KWallet's published backend returns an opaque64-byte master and no
        // tokenidentifier. Repeated retrieval must work without inventing one.
        let store=portal.clone();assert_eq!(pump(&context,move||store.delete(&Cancellation::new())),Ok(()));
        state.borrow_mut().portal_identifier=false;state.borrow_mut().portal_secret_len=64;state.borrow_mut().locator=0;
        let store=portal.clone();let value=token.clone();assert_eq!(pump(&context,move||store.save_validated(&value,&Cancellation::new())),Ok(()));
        for _ in 0..2 {let store=portal.clone();let loaded=pump(&context,move||store.load(&Cancellation::new())).unwrap().unwrap();assert_eq!(loaded.expose(),token.expose());}
        state.borrow_mut().portal_available=false;let before=state.borrow().portal_calls;let store=portal.clone();assert_eq!(pump(&context,move||store.delete(&Cancellation::new())),Ok(()));assert_eq!(state.borrow().portal_calls,before,"forget must not need the portal master key");assert!(!directory.join("session.enc").exists());
        let store=portal.clone();assert_eq!(pump(&context,move||store.probe(&Cancellation::new())),Err(StorageError::Unavailable));state.borrow_mut().portal_available=true;state.borrow_mut().locator=0;state.borrow_mut().portal_identifier=true;state.borrow_mut().portal_secret_len=32;
        state.borrow_mut().portal_cancel=true;let store=portal.clone();let t=token.clone();assert_eq!(pump(&context,move||store.save_validated(&t,&Cancellation::new())),Err(StorageError::Cancelled));assert!(!directory.join("session.enc").exists());state.borrow_mut().portal_cancel=false;
        state.borrow_mut().portal_hold=true;let store=portal.clone();let t=token.clone();let cancel=Cancellation::new();let worker_cancel=cancel.clone();let (done,rx)=mpsc::sync_channel(1);let worker=thread::spawn(move||{let result=store.save_validated(&t,&worker_cancel);done.send(result).unwrap();});
        let deadline=Instant::now()+Duration::from_secs(5);while state.borrow().held_fd.is_none(){assert!(Instant::now()<deadline);while context.pending(){context.iteration(false);}thread::sleep(Duration::from_millis(1));}cancel.cancel();
        let deadline=Instant::now()+Duration::from_secs(5);let result=loop{if let Ok(result)=rx.try_recv(){break result;}assert!(Instant::now()<deadline);while context.pending(){context.iteration(false);}thread::sleep(Duration::from_millis(1));};worker.join().unwrap();assert_eq!(result,Err(StorageError::Cancelled));assert_eq!(state.borrow().portal_closes,1);assert!(!directory.join("session.enc").exists());

        // The portal request exists before RetrieveSecret returns its handle.
        // Cancel in that interval and require Close, not just a cancelled RPC.
        state.borrow_mut().portal_hold=false;state.borrow_mut().defer_portal_start=true;
        let closes=state.borrow().portal_closes;
        let cancel=Cancellation::new();let worker_cancel=cancel.clone();let store=portal.clone();let value=token.clone();
        let (completed,reply)=mpsc::channel();
        let worker=thread::spawn(move||completed.send(store.save_validated(&value,&worker_cancel)).unwrap());
        wait_until(&context,||state.borrow().pending_portal_start.is_some());
        cancel.cancel();observe_for(&context,Duration::from_millis(500));
        let (invocation,path)=state.borrow_mut().pending_portal_start.take().unwrap();
        invocation.return_value(Some(&(path,).to_variant()));
        assert_eq!(receive(&context,&reply),Err(StorageError::Cancelled));
        worker.join().unwrap();
        assert!(state.borrow().portal_closes>closes,"cancelled RetrieveSecret start must close its request");
        assert!(state.borrow().held_fd.is_none());assert!(!directory.join("session.enc").exists());
        state.borrow_mut().defer_portal_start=false;
        std::fs::remove_dir_all(&directory).unwrap();std::fs::remove_dir_all(&native_directory).unwrap();let registrations=std::mem::take(&mut state.borrow_mut().registrations);for id in registrations{bus.unregister_object(id).unwrap();}
        for name in [SERVICE,PORTAL]{bus.call_sync(Some("org.freedesktop.DBus"),"/org/freedesktop/DBus","org.freedesktop.DBus","ReleaseName",Some(&(name,).to_variant()),None,gio::DBusCallFlags::NONE,1000,gio::Cancellable::NONE).unwrap();}
    }).unwrap();
}
