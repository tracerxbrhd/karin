// SPDX-License-Identifier: GPL-3.0-or-later
//! Secret Portal v1 supplies an opaque per-app master secret. Only an AEAD
//! envelope of the OAuth token is written; the portal key never reaches disk.
use super::*;
use gio::prelude::UnixFDListExtManual;
use glib::variant::{Handle, ObjectPath, ToVariant};
use ring::{
    aead, hkdf,
    rand::{SecureRandom, SystemRandom},
};
use rustix::fs::{AtFlags, Mode, OFlags};
use std::{
    collections::BTreeMap,
    fs::{File, Permissions},
    io::{Read, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::UnixStream,
    },
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};

const SERVICE: &str = "org.freedesktop.portal.Desktop";
const ROOT: &str = "/org/freedesktop/portal/desktop";
const API: &str = "org.freedesktop.portal.Secret";
const REQUEST: &str = "org.freedesktop.portal.Request";
const FILE: &str = "session.enc";
const MAGIC: &[u8] = b"KARIN-SECRET\x00\x01";
const MAX_LOCATOR: usize = 4096;
const MAX_MASTER: usize = 4096;
const MAX_ENVELOPE: usize = 16 * 1024;
const NONCE_LEN: usize = 12;

pub(super) struct PortalBackend {
    directory: PathBuf,
}
impl PortalBackend {
    pub(super) fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

struct PortalKey {
    secret: Zeroizing<Vec<u8>>,
    locator: Option<Zeroizing<String>>,
}
struct RequestGuard<'a> {
    connection: &'a gio::DBusConnection,
    path: String,
    completed: bool,
}
impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        if !self.completed {
            let _ = self.connection.call_sync(
                Some(SERVICE),
                &self.path,
                REQUEST,
                "Close",
                None,
                None,
                gio::DBusCallFlags::NONE,
                1000,
                gio::Cancellable::NONE,
            );
        }
    }
}
struct Envelope {
    nonce: [u8; NONCE_LEN],
    locator: Option<Zeroizing<String>>,
    header: Vec<u8>,
    ciphertext: Zeroizing<Vec<u8>>,
}

impl Envelope {
    fn parse(bytes: &[u8]) -> Result<Self, StorageError> {
        let fixed = MAGIC.len() + NONCE_LEN + 2;
        if bytes.len() < fixed + aead::AES_256_GCM.tag_len() + 1
            || bytes.len() > MAX_ENVELOPE
            || !bytes.starts_with(MAGIC)
        {
            return Err(StorageError::Corrupt);
        }
        let nonce = bytes[MAGIC.len()..MAGIC.len() + NONCE_LEN]
            .try_into()
            .map_err(|_| StorageError::Corrupt)?;
        let length = u16::from_be_bytes(
            bytes[fixed - 2..fixed]
                .try_into()
                .map_err(|_| StorageError::Corrupt)?,
        ) as usize;
        if length > MAX_LOCATOR || fixed + length + aead::AES_256_GCM.tag_len() >= bytes.len() {
            return Err(StorageError::Corrupt);
        }
        let locator = if length == 0 {
            None
        } else {
            let value = std::str::from_utf8(&bytes[fixed..fixed + length])
                .map_err(|_| StorageError::Corrupt)?;
            if value.contains('\0') {
                return Err(StorageError::Corrupt);
            }
            Some(Zeroizing::new(value.to_owned()))
        };
        Ok(Self {
            nonce,
            locator,
            header: bytes[..fixed + length].to_vec(),
            ciphertext: Zeroizing::new(bytes[fixed + length..].to_vec()),
        })
    }
    fn open(mut self, secret: &[u8]) -> Result<Zeroizing<Vec<u8>>, StorageError> {
        let key = key(secret)?;
        let plaintext = key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(self.nonce),
                aead::Aad::from(self.header.as_slice()),
                &mut self.ciphertext,
            )
            .map_err(|_| StorageError::Corrupt)?;
        let length = plaintext.len();
        self.ciphertext.truncate(length);
        validate_token(&self.ciphertext)?;
        Ok(self.ciphertext)
    }
}

struct KeyLength;
impl hkdf::KeyType for KeyLength {
    fn len(&self) -> usize {
        aead::AES_256_GCM.key_len()
    }
}
fn key(secret: &[u8]) -> Result<aead::LessSafeKey, StorageError> {
    if secret.is_empty() || secret.len() > MAX_MASTER {
        return Err(StorageError::Corrupt);
    }
    let salt = hkdf::Salt::new(
        hkdf::HKDF_SHA256,
        b"io.github.tracerxbrhd.Karin/SecretPortal/v1",
    );
    let prk = salt.extract(secret);
    let info = [b"music-access-token/AES-256-GCM".as_slice()];
    let material = prk
        .expand(&info, KeyLength)
        .map_err(|_| StorageError::Corrupt)?;
    let mut bytes = Zeroizing::new([0u8; 32]);
    material
        .fill(bytes.as_mut())
        .map_err(|_| StorageError::Corrupt)?;
    aead::UnboundKey::new(&aead::AES_256_GCM, bytes.as_ref())
        .map(aead::LessSafeKey::new)
        .map_err(|_| StorageError::Corrupt)
}
fn seal(value: &[u8], master: &PortalKey) -> Result<Zeroizing<Vec<u8>>, StorageError> {
    validate_token(value)?;
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| StorageError::Unavailable)?;
    let locator = master.locator.as_deref().map_or("", |v| v.as_str());
    if locator.len() > MAX_LOCATOR {
        return Err(StorageError::Corrupt);
    }
    let mut header = MAGIC.to_vec();
    header.extend_from_slice(&nonce);
    header.extend_from_slice(&(locator.len() as u16).to_be_bytes());
    header.extend_from_slice(locator.as_bytes());
    let mut ciphertext = Zeroizing::new(value.to_vec());
    key(&master.secret)?
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(header.as_slice()),
            &mut *ciphertext,
        )
        .map_err(|_| StorageError::Unconfirmed)?;
    let mut encoded = Zeroizing::new(header);
    encoded.extend_from_slice(&ciphertext);
    Ok(encoded)
}

/// Directory-relative NOFOLLOW operations protect the fixed credential name;
/// no URI, token, account name or portal locator is used in a pathname.
pub(super) struct Directory {
    file: File,
    name: &'static str,
}
impl Directory {
    pub(super) fn open(path: &Path, create: bool) -> Result<Option<Self>, StorageError> {
        if !path.is_absolute() {
            return Err(StorageError::Permission);
        }
        if create {
            std::fs::create_dir_all(path).map_err(|_| StorageError::Io)?;
        }
        let fd = match rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) if !create => return Ok(None),
            Err(_) => return Err(StorageError::Io),
        };
        let file = File::from(fd);
        let metadata = file.metadata().map_err(|_| StorageError::Io)?;
        if metadata.uid() != rustix::process::geteuid().as_raw() {
            return Err(StorageError::Permission);
        }
        file.set_permissions(Permissions::from_mode(0o700))
            .map_err(|_| StorageError::Permission)?;
        Ok(Some(Self { file, name: FILE }))
    }
    pub(super) fn native_marker(path: &Path, create: bool) -> Result<Option<Self>, StorageError> {
        Ok(Self::open(path, create)?.map(|mut directory| {
            directory.name = "native-write.pending";
            directory
        }))
    }
    pub(super) fn read(&self) -> Result<Option<Zeroizing<Vec<u8>>>, StorageError> {
        let fd = match rustix::fs::openat(
            &self.file,
            self.name,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(_) => return Err(StorageError::Io),
        };
        let file = File::from(fd);
        let info = file.metadata().map_err(|_| StorageError::Io)?;
        if info.uid() != rustix::process::geteuid().as_raw()
            || info.permissions().mode() & 0o7077 != 0
        {
            return Err(StorageError::Permission);
        }
        if !info.is_file() || info.nlink() != 1 || info.len() > MAX_ENVELOPE as u64 {
            return Err(StorageError::Corrupt);
        }
        let mut bytes = Zeroizing::new(Vec::new());
        file.take(MAX_ENVELOPE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| StorageError::Io)?;
        if bytes.len() > MAX_ENVELOPE {
            return Err(StorageError::Corrupt);
        }
        Ok(Some(bytes))
    }
    pub(super) fn write(&self, bytes: &[u8], cancel: &Cancellation) -> Result<(), StorageError> {
        cancel.check()?;
        let mut random = [0; 16];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| StorageError::Unavailable)?;
        let name = format!(
            ".session-{}.tmp",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let temporary = Temporary {
            directory: &self.file,
            name,
        };
        let fd = rustix::fs::openat(
            &self.file,
            &temporary.name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|_| StorageError::Io)?;
        let mut file = File::from(fd);
        file.write_all(bytes).map_err(|_| StorageError::Io)?;
        file.sync_all().map_err(|_| StorageError::Io)?;
        cancel.check()?;
        rustix::fs::renameat(&self.file, &temporary.name, &self.file, self.name)
            .map_err(|_| StorageError::Io)?;
        self.file
            .sync_all()
            .map_err(|_| StorageError::Unconfirmed)?;
        cancel.check()?;
        if self.read()?.as_deref().map(|v| v.as_slice()) != Some(bytes) {
            return Err(StorageError::Unconfirmed);
        }
        Ok(())
    }
    pub(super) fn delete(&self) -> Result<(), StorageError> {
        match rustix::fs::unlinkat(&self.file, self.name, AtFlags::empty()) {
            Ok(()) | Err(rustix::io::Errno::NOENT) => {}
            Err(_) => return Err(StorageError::Unconfirmed),
        }
        self.file
            .sync_all()
            .map_err(|_| StorageError::Unconfirmed)?;
        if self.read()?.is_none() {
            Ok(())
        } else {
            Err(StorageError::Unconfirmed)
        }
    }
}
struct Temporary<'a> {
    directory: &'a File,
    name: String,
}
impl Drop for Temporary<'_> {
    fn drop(&mut self) {
        let _ = rustix::fs::unlinkat(self.directory, &self.name, AtFlags::empty());
    }
}

fn probe(connection: &gio::DBusConnection, cancel: &Cancellation) -> Result<(), StorageError> {
    let reply = call(
        connection,
        SERVICE,
        ROOT,
        "org.freedesktop.DBus.Properties",
        "Get",
        Some(&(API, "version").to_variant()),
        cancel,
    )?;
    let (value,) = reply
        .get::<(glib::Variant,)>()
        .ok_or(StorageError::Corrupt)?;
    if value.get::<u32>().is_some_and(|v| v >= 1) {
        Ok(())
    } else {
        Err(StorageError::Unavailable)
    }
}
fn retrieve(
    connection: &gio::DBusConnection,
    context: &glib::MainContext,
    locator: Option<&str>,
    cancel: &Cancellation,
) -> Result<PortalKey, StorageError> {
    probe(connection, cancel)?;
    let mut random = [0u8; 16];
    SystemRandom::new()
        .fill(&mut random)
        .map_err(|_| StorageError::Unavailable)?;
    let handle_token = format!(
        "karin_{}",
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let mut options = BTreeMap::from([("handle_token".to_owned(), handle_token.to_variant())]);
    let sender_name = connection.unique_name().ok_or(StorageError::Unavailable)?;
    let sender_name = sender_name.trim_start_matches(':').replace('.', "_");
    let mut guard = RequestGuard {
        connection,
        path: format!("{ROOT}/request/{sender_name}/{handle_token}"),
        completed: false,
    };
    if let Some(locator) = locator {
        if locator.len() > MAX_LOCATOR {
            return Err(StorageError::Corrupt);
        }
        options.insert("token".to_owned(), locator.to_variant());
    }
    let (sender, receiver) = mpsc::sync_channel(4);
    // A private D-Bus connection has no other application portal operations.
    // Subscribe before RetrieveSecret so an immediate Response cannot be lost.
    let _subscription = connection.subscribe_to_signal(
        Some(SERVICE),
        Some(REQUEST),
        Some("Response"),
        None,
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let _ = sender.try_send((signal.object_path.to_owned(), signal.parameters.clone()));
        },
    );
    let (mut reader, writer) = UnixStream::pair().map_err(|_| StorageError::Io)?;
    reader.set_nonblocking(true).map_err(|_| StorageError::Io)?;
    let descriptors = gio::UnixFDList::new();
    let index = descriptors.append(&writer).map_err(map_error)?;
    let (reply, _) = connection
        .call_with_unix_fd_list_sync(
            Some(SERVICE),
            ROOT,
            API,
            "RetrieveSecret",
            Some(&(Handle(index), options).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            Some(&descriptors),
            gio::Cancellable::NONE,
        )
        .map_err(map_error)?;
    drop(descriptors);
    drop(writer);
    let (handle,) = reply.get::<(ObjectPath,)>().ok_or(StorageError::Corrupt)?;
    if handle.as_str().len() > 512
        || !handle
            .as_str()
            .starts_with("/org/freedesktop/portal/desktop/request/")
    {
        return Err(StorageError::Corrupt);
    }
    guard.path = handle.as_str().to_owned();
    let deadline = Instant::now() + INTERACTION_TIMEOUT;
    let mut secret = Zeroizing::new(Vec::new());
    let mut result = None;
    let mut eof = false;
    loop {
        while let Ok((path, response)) = receiver.try_recv() {
            if path != handle.as_str() {
                continue;
            }
            let (status, results) = response
                .get::<(u32, BTreeMap<String, glib::Variant>)>()
                .ok_or(StorageError::Corrupt)?;
            match status {
                0 => {}
                1 => {
                    guard.completed = true;
                    return Err(StorageError::Cancelled);
                }
                _ => {
                    guard.completed = true;
                    return Err(StorageError::Unconfirmed);
                }
            }
            let identifier = match results.get("token") {
                None => locator.map(|s| Zeroizing::new(s.to_owned())),
                Some(value) => {
                    let value = value
                        .str()
                        .filter(|s| s.len() <= MAX_LOCATOR && !s.contains('\0'))
                        .ok_or(StorageError::Corrupt)?;
                    Some(Zeroizing::new(value.to_owned()))
                }
            };
            result = Some(identifier);
        }
        let mut chunk = Zeroizing::new([0u8; 512]);
        loop {
            match reader.read(chunk.as_mut()) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(count) => {
                    if secret.len() + count > MAX_MASTER {
                        return Err(StorageError::Corrupt);
                    }
                    secret.extend_from_slice(&chunk[..count]);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(StorageError::Io),
            }
        }
        if eof && let Some(locator) = result.take() {
            guard.completed = true;
            if secret.is_empty() {
                return Err(StorageError::Corrupt);
            }
            return Ok(PortalKey { secret, locator });
        }
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err(if cancel.is_cancelled() {
                StorageError::Cancelled
            } else {
                StorageError::Timeout
            });
        }
        for _ in 0..16 {
            if !context.pending() {
                break;
            }
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

impl CredentialBackend for PortalBackend {
    fn probe(&self, cancel: &Cancellation) -> Result<BackendKind, StorageError> {
        with_context(|_| {
            let bus = connection(cancel)?;
            probe(&bus, cancel)?;
            Ok(BackendKind::SecretPortal)
        })
    }
    fn load(&self, cancel: &Cancellation) -> Result<Option<AccessToken>, StorageError> {
        let Some(directory) = Directory::open(&self.directory, false)? else {
            return Ok(None);
        };
        let Some(bytes) = directory.read()? else {
            return Ok(None);
        };
        let envelope = Envelope::parse(&bytes)?;
        with_context(|context| {
            let connection = connection(cancel)?;
            let locator = envelope.locator.as_deref().map(|s| s.as_str());
            let master = retrieve(&connection, context, locator, cancel)?;
            let changed = master.locator.as_deref().map(|s| s.as_str()) != locator;
            let plaintext = envelope.open(&master.secret)?;
            if changed {
                directory.write(&seal(&plaintext, &master)?, cancel)?;
            }
            cancel.check()?;
            restored_token(plaintext).map(Some)
        })
    }
    fn save_validated(
        &self,
        token: &AccessToken,
        cancel: &Cancellation,
    ) -> Result<(), StorageError> {
        let directory = Directory::open(&self.directory, true)?.ok_or(StorageError::Io)?;
        // A newly validated login can replace a damaged old envelope. No old
        // ciphertext is treated as proof of authorization.
        let previous = match directory.read() {
            Ok(bytes) => bytes.and_then(|bytes| Envelope::parse(&bytes).ok()),
            Err(StorageError::Corrupt) => None,
            Err(error) => return Err(error),
        };
        with_context(|context| {
            let connection = connection(cancel)?;
            let locator = previous
                .as_ref()
                .and_then(|p| p.locator.as_deref().map(|s| s.as_str()));
            let master = retrieve(&connection, context, locator, cancel)?;
            if token.is_expired() {
                return Err(StorageError::Cancelled);
            }
            directory.write(&seal(token.expose().as_bytes(), &master)?, cancel)
        })
    }
    fn delete(&self, cancel: &Cancellation) -> Result<(), StorageError> {
        cancel.check()?;
        // Removing the encrypted OAuth envelope requires neither portal access
        // nor the master key. The per-app portal key and server grant remain.
        let Some(directory) = Directory::open(&self.directory, false)? else {
            return Ok(());
        };
        directory.delete()?;
        cancel.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portal_envelope_authenticates_header_secret_and_ciphertext_without_plaintext() {
        let master = PortalKey {
            secret: Zeroizing::new(vec![42; 32]),
            locator: Some(Zeroizing::new("synthetic-portal-identifier".into())),
        };
        let token = b"synthetic-private-access-token";
        let encoded = seal(token, &master).unwrap();
        assert!(!encoded.windows(token.len()).any(|s| s == token));
        let recovered = Envelope::parse(&encoded)
            .unwrap()
            .open(&master.secret)
            .unwrap();
        assert_eq!(recovered.as_slice(), token);
        assert!(Envelope::parse(&encoded).unwrap().open(&[43; 32]).is_err());
        for index in [MAGIC.len(), MAGIC.len() + NONCE_LEN + 2, encoded.len() - 1] {
            let mut damaged = encoded.to_vec();
            damaged[index] ^= 1;
            assert!(
                Envelope::parse(&damaged)
                    .and_then(|e| e.open(&master.secret))
                    .is_err()
            );
        }
        assert!(
            Envelope::parse(&encoded[..encoded.len() - 1])
                .and_then(|e| e.open(&master.secret))
                .is_err()
        );
    }
}
