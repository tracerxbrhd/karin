// SPDX-License-Identifier: GPL-3.0-or-later
//! Secret Service protocol, scoped to Karin's own credential attributes.
use super::*;
use glib::variant::{ObjectPath, ToVariant};
use std::{
    collections::BTreeMap,
    sync::mpsc,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;

const SERVICE: &str = "org.freedesktop.secrets";
const ROOT: &str = "/org/freedesktop/secrets";
const API: &str = "org.freedesktop.Secret.Service";
const ITEM: &str = "org.freedesktop.Secret.Item";
const PROMPT: &str = "org.freedesktop.Secret.Prompt";
const NO_PROMPT: &str = "/";
type Secret = (ObjectPath, Vec<u8>, Vec<u8>, String);
struct PromptGuard<'a> {
    connection: &'a gio::DBusConnection,
    path: &'a ObjectPath,
    completed: bool,
}
impl Drop for PromptGuard<'_> {
    fn drop(&mut self) {
        if !self.completed {
            let _ = self.connection.call_sync(
                Some(SERVICE),
                self.path.as_str(),
                PROMPT,
                "Dismiss",
                None,
                None,
                gio::DBusCallFlags::NONE,
                1000,
                gio::Cancellable::NONE,
            );
        }
    }
}

pub(super) struct SecretServiceBackend {
    directory: PathBuf,
}
impl SecretServiceBackend {
    pub(super) fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

fn attributes() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("application".into(), crate::config::APP_ID.into()),
        ("karin-purpose".into(), "music-access-token-v1".into()),
        (
            "xdg:schema".into(),
            "io.github.tracerxbrhd.Karin.MusicSession".into(),
        ),
    ])
}
fn safe_path(path: &ObjectPath) -> Result<(), StorageError> {
    if path.as_str().len() <= 1024 && path.as_str().starts_with("/org/freedesktop/secrets/") {
        Ok(())
    } else {
        Err(StorageError::Corrupt)
    }
}

struct Client<'a> {
    connection: BusConnection,
    context: &'a glib::MainContext,
    cancel: &'a Cancellation,
}
impl<'a> Client<'a> {
    fn new(context: &'a glib::MainContext, cancel: &'a Cancellation) -> Result<Self, StorageError> {
        Ok(Self {
            connection: connection(cancel)?,
            context,
            cancel,
        })
    }
    fn call(
        &self,
        path: &str,
        interface: &str,
        method: &str,
        args: Option<&glib::Variant>,
    ) -> Result<glib::Variant, StorageError> {
        call(
            &self.connection,
            SERVICE,
            path,
            interface,
            method,
            args,
            self.cancel,
        )
    }
    fn mutate(
        &self,
        path: &str,
        interface: &str,
        method: &str,
        args: Option<&glib::Variant>,
    ) -> Result<glib::Variant, StorageError> {
        self.cancel.check()?;
        // A cancelled client RPC does not cancel the server's mutation. Wait
        // for its bounded reply before compensation/newer IO may proceed.
        self.connection
            .call_sync(
                Some(SERVICE),
                path,
                interface,
                method,
                args,
                None,
                gio::DBusCallFlags::NONE,
                CALL_TIMEOUT_MS,
                gio::Cancellable::NONE,
            )
            .map_err(map_error)
    }
    fn search(&self) -> Result<(Vec<ObjectPath>, Vec<ObjectPath>), StorageError> {
        let result = self.call(
            ROOT,
            API,
            "SearchItems",
            Some(&(attributes(),).to_variant()),
        )?;
        let (unlocked, locked) = result
            .get::<(Vec<ObjectPath>, Vec<ObjectPath>)>()
            .ok_or(StorageError::Corrupt)?;
        if unlocked.len() + locked.len() > 16 {
            return Err(StorageError::Corrupt);
        }
        for path in unlocked.iter().chain(&locked) {
            safe_path(path)?;
        }
        Ok((unlocked, locked))
    }
    fn prompt(&self, path: &ObjectPath) -> Result<(), StorageError> {
        if path.as_str() == NO_PROMPT {
            return Ok(());
        }
        safe_path(path)?;
        let mut guard = PromptGuard {
            connection: &self.connection,
            path,
            completed: false,
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let _subscription = self.connection.subscribe_to_signal(
            Some(SERVICE),
            Some(PROMPT),
            Some("Completed"),
            Some(path.as_str()),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                let result = signal
                    .parameters
                    .get::<(bool, glib::Variant)>()
                    .map(|(dismissed, _)| {
                        if dismissed {
                            Err(StorageError::Cancelled)
                        } else {
                            Ok(())
                        }
                    })
                    .unwrap_or(Err(StorageError::Corrupt));
                let _ = sender.try_send(result);
            },
        );
        self.mutate(path.as_str(), PROMPT, "Prompt", Some(&("",).to_variant()))?;
        let deadline = Instant::now() + INTERACTION_TIMEOUT;
        loop {
            if let Ok(result) = receiver.try_recv() {
                guard.completed = true;
                return result;
            }
            if self.cancel.is_cancelled() || Instant::now() >= deadline {
                return Err(if self.cancel.is_cancelled() {
                    StorageError::Cancelled
                } else {
                    StorageError::Timeout
                });
            }
            for _ in 0..16 {
                if !self.context.pending() {
                    break;
                }
                self.context.iteration(false);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn unlock(&self, paths: Vec<ObjectPath>) -> Result<(), StorageError> {
        if paths.is_empty() {
            return Ok(());
        }
        let result = self.call(ROOT, API, "Unlock", Some(&(paths.clone(),).to_variant()))?;
        let (unlocked, prompt) = result
            .get::<(Vec<ObjectPath>, ObjectPath)>()
            .ok_or(StorageError::Corrupt)?;
        if prompt.as_str() == NO_PROMPT && !paths.iter().all(|p| unlocked.contains(p)) {
            return Err(StorageError::Locked);
        }
        self.prompt(&prompt)
    }
    fn collection(&self, create: bool) -> Result<Option<ObjectPath>, StorageError> {
        let result = self.call(ROOT, API, "ReadAlias", Some(&("default",).to_variant()))?;
        let (path,) = result.get::<(ObjectPath,)>().ok_or(StorageError::Corrupt)?;
        if path.as_str() != "/" {
            safe_path(&path)?;
            return Ok(Some(path));
        }
        if !create {
            return Ok(None);
        }
        let properties = BTreeMap::from([(
            "org.freedesktop.Secret.Collection.Label".to_owned(),
            "Karin".to_variant(),
        )]);
        let created = self.mutate(
            ROOT,
            API,
            "CreateCollection",
            Some(&(properties, "default").to_variant()),
        )?;
        let (_, prompt) = created
            .get::<(ObjectPath, ObjectPath)>()
            .ok_or(StorageError::Corrupt)?;
        self.prompt(&prompt)?;
        self.collection(false)?
            .map(Some)
            .ok_or(StorageError::Unconfirmed)
    }
    fn session(&self) -> Result<Session<'_>, StorageError> {
        // "plain" is the standard Secret Service *session transport* algorithm;
        // the only persistent copy is stored by the user's protected keyring.
        let result = self.call(
            ROOT,
            API,
            "OpenSession",
            Some(&("plain", "".to_variant()).to_variant()),
        )?;
        let (_, path) = result
            .get::<(glib::Variant, ObjectPath)>()
            .ok_or(StorageError::Corrupt)?;
        safe_path(&path)?;
        Ok(Session { client: self, path })
    }
    fn remove_items(&self) -> Result<bool, StorageError> {
        let (unlocked, locked) = self.search()?;
        let present = !unlocked.is_empty() || !locked.is_empty();
        for item in unlocked.into_iter().chain(locked) {
            let reply = self.mutate(item.as_str(), ITEM, "Delete", None)?;
            let (prompt,) = reply.get::<(ObjectPath,)>().ok_or(StorageError::Corrupt)?;
            self.prompt(&prompt)?;
        }
        let (unlocked, locked) = self.search()?;
        if unlocked.is_empty() && locked.is_empty() {
            Ok(present)
        } else {
            Err(StorageError::Unconfirmed)
        }
    }
    fn unlocked_items(&self) -> Result<Vec<ObjectPath>, StorageError> {
        let (unlocked, locked) = self.search()?;
        if locked.is_empty() {
            return Ok(unlocked);
        }
        self.unlock(locked)?;
        let (unlocked, locked) = self.search()?;
        if !locked.is_empty() {
            return Err(StorageError::Locked);
        }
        Ok(unlocked)
    }
}
struct Session<'a> {
    client: &'a Client<'a>,
    path: ObjectPath,
}
impl Session<'_> {
    fn read(&self, item: &ObjectPath) -> Result<Zeroizing<Vec<u8>>, StorageError> {
        let reply = self.client.call(
            item.as_str(),
            ITEM,
            "GetSecret",
            Some(&(self.path.clone(),).to_variant()),
        )?;
        if reply.type_().as_str() != "((oayays))" {
            return Err(StorageError::Corrupt);
        }
        let fields = reply.child_value(0);
        if fields.child_value(1).n_children() != 0
            || fields.child_value(2).n_children() > MAX_TOKEN
            || fields.child_value(3).str().is_none_or(|s| s.len() > 128)
        {
            return Err(StorageError::Corrupt);
        }
        let (secret,) = reply.get::<(Secret,)>().ok_or(StorageError::Corrupt)?;
        let value = Zeroizing::new(secret.2);
        if secret.0 != self.path
            || !secret.1.is_empty()
            || !matches!(
                secret.3.as_str(),
                "text/plain" | "text/plain; charset=utf-8" | "application/octet-stream"
            )
        {
            return Err(StorageError::Corrupt);
        }
        validate_token(&value)?;
        Ok(value)
    }
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        let _ = self.client.connection.call_sync(
            Some(SERVICE),
            self.path.as_str(),
            "org.freedesktop.Secret.Session",
            "Close",
            None,
            None,
            gio::DBusCallFlags::NONE,
            1000,
            gio::Cancellable::NONE,
        );
    }
}

impl CredentialBackend for SecretServiceBackend {
    fn probe(&self, cancel: &Cancellation) -> Result<BackendKind, StorageError> {
        with_context(|context| {
            let client = Client::new(context, cancel)?;
            client.collection(false)?;
            Ok(BackendKind::SecretService)
        })
    }
    fn load(&self, cancel: &Cancellation) -> Result<Option<AccessToken>, StorageError> {
        if let Some(marker) = super::portal::Directory::native_marker(&self.directory, false)?
            && marker.read()?.is_some()
        {
            return Err(StorageError::Unconfirmed);
        }
        with_context(|context| {
            let client = Client::new(context, cancel)?;
            let items = client.unlocked_items()?;
            if items.is_empty() {
                return Ok(None);
            }
            if items.len() != 1 {
                return Err(StorageError::Corrupt);
            }
            let session = client.session()?;
            restored_token(session.read(&items[0])?).map(Some)
        })
    }
    fn save_validated(
        &self,
        token: &AccessToken,
        cancel: &Cancellation,
    ) -> Result<(), StorageError> {
        if let Some(marker) = super::portal::Directory::native_marker(&self.directory, false)?
            && marker.read()?.is_some()
        {
            return Err(StorageError::Unconfirmed);
        }
        with_context(|context| {
            let client = Client::new(context, cancel)?;
            let collection = client.collection(true)?.ok_or(StorageError::Unavailable)?;
            client.unlock(vec![collection.clone()])?;
            // No older scoped item may make a delayed write appear resolved.
            client.remove_items()?;
            let session = client.session()?;
            let properties = BTreeMap::from([
                (
                    "org.freedesktop.Secret.Item.Label".to_owned(),
                    "Karin — Яндекс Музыка".to_variant(),
                ),
                (
                    "org.freedesktop.Secret.Item.Attributes".to_owned(),
                    attributes().to_variant(),
                ),
            ]);
            cancel.check()?;
            if token.is_expired() {
                return Err(StorageError::Cancelled);
            }
            let marker = super::portal::Directory::native_marker(&self.directory, true)?
                .ok_or(StorageError::Io)?;
            if let Err(error) = marker.write(b"Karin native CreateItem pending v1\n", cancel) {
                let _ = marker.delete(); // no remote mutation has been sent
                return Err(error);
            }
            let value = Zeroizing::new(token.expose().as_bytes().to_vec());
            let secret = (
                session.path.clone(),
                &[] as &[u8],
                value.as_slice(),
                "text/plain; charset=utf-8",
            );
            // Once fenced, await the bounded mutation even if the UI cancels;
            // its manager compensates afterwards. An ambiguous timeout leaves
            // the non-secret tombstone across restarts and forbids auto restore.
            let result = match client.connection.call_sync(
                Some(SERVICE),
                collection.as_str(),
                "org.freedesktop.Secret.Collection",
                "CreateItem",
                Some(&(properties, secret, true).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                CALL_TIMEOUT_MS,
                gio::Cancellable::NONE,
            ) {
                Ok(reply) => reply,
                Err(error) => {
                    // Only these exact negative protocol replies establish
                    // that CreateItem was rejected without committing. A
                    // timeout, cancellation, transport loss or unknown remote
                    // failure must retain its uncertain-write tombstone.
                    let rejected = matches!(
                        gio::DBusError::remote_error(&error).as_deref(),
                        Some(
                            "org.freedesktop.DBus.Error.AccessDenied"
                                | "org.freedesktop.Secret.Error.IsLocked"
                        )
                    );
                    let category = map_error(error);
                    if rejected {
                        marker.delete()?;
                    }
                    return Err(category);
                }
            };
            let (_, prompt) = result
                .get::<(ObjectPath, ObjectPath)>()
                .ok_or(StorageError::Corrupt)?;
            client.prompt(&prompt)?;
            let items = client.unlocked_items()?;
            if items.len() != 1 {
                return Err(StorageError::Unconfirmed);
            }
            let saved = session.read(&items[0])?;
            if saved
                .as_slice()
                .ct_eq(token.expose().as_bytes())
                .unwrap_u8()
                != 1
            {
                return Err(StorageError::Unconfirmed);
            }
            cancel.check()?;
            marker.delete()
        })
    }
    fn delete(&self, cancel: &Cancellation) -> Result<(), StorageError> {
        with_context(|context| {
            let client = Client::new(context, cancel)?;
            let marker = super::portal::Directory::native_marker(&self.directory, false)?;
            let pending = marker
                .as_ref()
                .map(|m| m.read())
                .transpose()?
                .flatten()
                .is_some();
            let observed = client.remove_items()?;
            if pending && !observed {
                // Empty now cannot prove a timed-out CreateItem will not commit.
                return Err(StorageError::Unconfirmed);
            }
            if let Some(marker) = marker {
                marker.delete()?;
            }
            Ok(())
        })
    }
}
