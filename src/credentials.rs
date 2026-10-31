// SPDX-License-Identifier: GPL-3.0-or-later
//! Scoped secure storage. Call the synchronous operations on a bounded worker,
//! never on GTK's main thread. No password, cookie or media URI belongs here.

use crate::session::AccessToken;
use gtk::{gio, glib};
use std::{fmt, path::PathBuf, sync::Arc};
use zeroize::Zeroizing;

mod portal;
mod secret_service;

pub(crate) const MAX_TOKEN: usize = 8192;
const CALL_TIMEOUT_MS: i32 = 10_000;
const INTERACTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    SecretService,
    SecretPortal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    #[error("Secure storage is unavailable.")]
    Unavailable,
    #[error("Secure storage is locked.")]
    Locked,
    #[error("The secure-storage operation was cancelled.")]
    Cancelled,
    #[error("The saved credential is malformed or cannot be decrypted.")]
    Corrupt,
    #[error("Secure-storage access was denied.")]
    Permission,
    #[error("Secure storage did not respond in time.")]
    Timeout,
    #[error("The private credential file could not be accessed.")]
    Io,
    #[error("Secure storage did not confirm the requested operation.")]
    Unconfirmed,
}

impl StorageError {
    pub const fn user_message(self) -> &'static str {
        match self {
            Self::Unavailable => {
                "Защищённое хранилище недоступно. Вход действует только в этом запуске."
            }
            Self::Locked => {
                "Хранилище заблокировано. Разблокируйте системный кошелёк и повторите попытку."
            }
            Self::Cancelled => "Действие с защищённым хранилищем отменено.",
            Self::Corrupt => {
                "Сохранённый вход повреждён или недоступен для расшифровки. Войдите снова."
            }
            Self::Permission => "Система не разрешила доступ к защищённому хранилищу.",
            Self::Timeout => "Защищённое хранилище не ответило вовремя. Повторите попытку.",
            Self::Io => "Не удалось прочитать или обновить защищённые данные Karin.",
            Self::Unconfirmed => {
                "Результат изменения сохранённого входа не подтверждён. Повторите попытку."
            }
        }
    }
}

/// Cancellation can be shared across UI and worker threads; its Debug contains
/// no D-Bus error, file path, provider response or credential.
#[derive(Clone, Default)]
pub struct Cancellation(gio::Cancellable);

impl Cancellation {
    pub fn new() -> Self {
        Self(gio::Cancellable::new())
    }
    pub fn cancel(&self) {
        use gio::prelude::CancellableExt;
        self.0.cancel();
    }
    pub fn is_cancelled(&self) -> bool {
        use gio::prelude::CancellableExt;
        self.0.is_cancelled()
    }
    pub(crate) fn check(&self) -> Result<(), StorageError> {
        if self.is_cancelled() {
            Err(StorageError::Cancelled)
        } else {
            Ok(())
        }
    }
}
impl fmt::Debug for Cancellation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cancellation")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

/// Injectable boundary for deterministic synthetic tests. Production selects
/// only an actual system backend, never an in-memory or plaintext fallback.
pub trait CredentialBackend: Send + Sync {
    fn probe(&self, cancel: &Cancellation) -> Result<BackendKind, StorageError>;
    fn load(&self, cancel: &Cancellation) -> Result<Option<AccessToken>, StorageError>;
    fn save_validated(
        &self,
        token: &AccessToken,
        cancel: &Cancellation,
    ) -> Result<(), StorageError>;
    fn delete(&self, cancel: &Cancellation) -> Result<(), StorageError>;
}

#[derive(Clone)]
pub struct CredentialStore {
    backend: Arc<dyn CredentialBackend>,
}

impl fmt::Debug for CredentialStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CredentialStore([system backend])")
    }
}

impl CredentialStore {
    pub fn system() -> Self {
        let flatpak = std::path::Path::new("/.flatpak-info").is_file();
        Self::for_environment(
            flatpak,
            glib::user_data_dir()
                .join(crate::config::APP_ID)
                .join("credentials"),
        )
    }

    /// Explicit paths/backends support isolated fixtures and packaging tests.
    /// `flatpak` chooses the portal, never a silent direct-session-bus grant.
    pub fn for_environment(flatpak: bool, private_directory: PathBuf) -> Self {
        if flatpak {
            Self::with_backend(portal::PortalBackend::new(private_directory))
        } else {
            Self::with_backend(secret_service::SecretServiceBackend::new(private_directory))
        }
    }

    pub fn with_backend(backend: impl CredentialBackend + 'static) -> Self {
        Self {
            backend: Arc::new(backend),
        }
    }
    pub fn probe(&self, cancel: &Cancellation) -> Result<BackendKind, StorageError> {
        cancel.check()?;
        self.backend.probe(cancel)
    }
    pub fn load(&self, cancel: &Cancellation) -> Result<Option<AccessToken>, StorageError> {
        cancel.check()?;
        self.backend.load(cancel)
    }
    /// Caller must have independently validated this token with MusicService.
    pub fn save_validated(
        &self,
        token: &AccessToken,
        cancel: &Cancellation,
    ) -> Result<(), StorageError> {
        cancel.check()?;
        if token.is_expired() {
            return Err(StorageError::Cancelled);
        }
        validate_token(token.expose().as_bytes())?;
        self.backend.save_validated(token, cancel)
    }
    /// Success means absence was checked, not just that deletion was requested.
    pub fn delete(&self, cancel: &Cancellation) -> Result<(), StorageError> {
        cancel.check()?;
        self.backend.delete(cancel)
    }
}

fn validate_token(value: &[u8]) -> Result<(), StorageError> {
    if value.is_empty() || value.len() > MAX_TOKEN || !value.iter().all(|b| (33..=126).contains(b))
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn restored_token(value: Zeroizing<Vec<u8>>) -> Result<AccessToken, StorageError> {
    validate_token(&value)?;
    let token = std::str::from_utf8(&value).map_err(|_| StorageError::Corrupt)?;
    Ok(AccessToken::new(Zeroizing::new(token.to_owned()), None))
}

/// Private connection: unrelated application portal replies are not observed.
struct BusConnection(gio::DBusConnection);
impl std::ops::Deref for BusConnection {
    type Target = gio::DBusConnection;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Drop for BusConnection {
    fn drop(&mut self) {
        // This is a private local connection. Closing it also retires portal
        // requests bound to this caller, rather than leaving a shared bus alive.
        let _ = self.0.close_sync(gio::Cancellable::NONE);
    }
}
fn connection(cancel: &Cancellation) -> Result<BusConnection, StorageError> {
    cancel.check()?;
    let address =
        std::env::var("DBUS_SESSION_BUS_ADDRESS").map_err(|_| StorageError::Unavailable)?;
    // Linux desktops and Flatpak use a local Unix bus. Never auto-launch a bus
    // or forward a credential to a user-configured TCP fallback.
    if !address.starts_with("unix:") || address.len() > 4096 || address.contains(';') {
        return Err(StorageError::Unavailable);
    }
    connect_address(&address, cancel, std::time::Duration::from_secs(10))
}
fn connect_address(
    address: &str,
    cancel: &Cancellation,
    deadline: std::time::Duration,
) -> Result<BusConnection, StorageError> {
    use gio::prelude::CancellableExtManual;
    cancel.check()?;
    let local = Cancellation::new();
    let forward = local.clone();
    let link = cancel.0.connect_cancelled(move |_| forward.cancel());
    let (done, wait) = std::sync::mpsc::channel::<()>();
    let timed_out = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let elapsed = timed_out.clone();
    let cancellation = local.clone();
    let timer = std::thread::spawn(move || {
        if matches!(
            wait.recv_timeout(deadline),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ) {
            elapsed.store(true, std::sync::atomic::Ordering::Release);
            cancellation.cancel();
        }
    });
    let result = gio::DBusConnection::for_address_sync(
        address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None::<&gio::DBusAuthObserver>,
        Some(&local.0),
    )
    .map_err(map_error);
    drop(done);
    let _ = timer.join();
    if let Some(link) = link {
        cancel.0.disconnect_cancelled(link);
    }
    if cancel.is_cancelled() {
        return Err(StorageError::Cancelled);
    }
    if timed_out.load(std::sync::atomic::Ordering::Acquire) {
        Err(StorageError::Timeout)
    } else {
        result.map(|connection| {
            connection.set_exit_on_close(false);
            BusConnection(connection)
        })
    }
}

fn with_context<T>(
    body: impl FnOnce(&glib::MainContext) -> Result<T, StorageError>,
) -> Result<T, StorageError> {
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| body(&context))
        .map_err(|_| StorageError::Unavailable)?
}

fn call(
    connection: &gio::DBusConnection,
    destination: &str,
    path: &str,
    interface: &str,
    method: &str,
    parameters: Option<&glib::Variant>,
    cancel: &Cancellation,
) -> Result<glib::Variant, StorageError> {
    cancel.check()?;
    connection
        .call_sync(
            Some(destination),
            path,
            interface,
            method,
            parameters,
            None,
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            Some(&cancel.0),
        )
        .map_err(map_error)
}

fn map_error(error: glib::Error) -> StorageError {
    if error.matches(gio::IOErrorEnum::Cancelled) {
        return StorageError::Cancelled;
    }
    if error.matches(gio::IOErrorEnum::TimedOut)
        || error.matches(gio::DBusError::TimedOut)
        || error.matches(gio::DBusError::Timeout)
        || error.matches(gio::DBusError::NoReply)
    {
        return StorageError::Timeout;
    }
    if error.matches(gio::IOErrorEnum::PermissionDenied)
        || error.matches(gio::DBusError::AccessDenied)
        || error.matches(gio::DBusError::AuthFailed)
    {
        return StorageError::Permission;
    }
    if gio::DBusError::remote_error(&error).as_deref()
        == Some("org.freedesktop.Secret.Error.IsLocked")
    {
        return StorageError::Locked;
    }
    if error.matches(gio::DBusError::ServiceUnknown)
        || error.matches(gio::DBusError::NameHasNoOwner)
        || error.matches(gio::DBusError::UnknownInterface)
        || error.matches(gio::DBusError::UnknownMethod)
        || error.matches(gio::IOErrorEnum::NotConnected)
        || error.matches(gio::IOErrorEnum::Closed)
    {
        return StorageError::Unavailable;
    }
    StorageError::Unconfirmed
}

#[cfg(test)]
mod tests;
