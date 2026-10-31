// SPDX-License-Identifier: GPL-3.0-or-later
//! Versioned non-secret preferences in the application's private XDG directory.
//!
//! Filesystem operations are synchronous and belong on an application worker.
//! This module never accepts credentials, browser state or service responses.

use std::{
    fmt,
    fs::{DirBuilder, File},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Component, PathBuf},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use ring::rand::{SecureRandom, SystemRandom};
use rustix::fs::{AtFlags, Mode, OFlags};
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "preferences.json";
const FORMAT_VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 16 * 1024;
pub const MIN_CACHE_BYTES: u64 = 16 * 1024 * 1024;
pub const DEFAULT_CACHE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_CACHE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub appearance: Appearance,
    /// The preference is effective only when protected storage is available.
    pub remember_login: bool,
    /// Durable deletion intent, managed by session lifecycle rather than a UI
    /// toggle. Never restore a credential while this barrier remains set.
    pub forget_pending: bool,
    pub cache_enabled: bool,
    pub cache_limit_bytes: u64,
    pub track_info_open: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            remember_login: true,
            forget_pending: false,
            cache_enabled: true,
            cache_limit_bytes: DEFAULT_CACHE_BYTES,
            track_info_open: true,
        }
    }
}

impl Preferences {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if !(MIN_CACHE_BYTES..=MAX_CACHE_BYTES).contains(&self.cache_limit_bytes) {
            return Err(SettingsError::InvalidPreferences);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadStatus {
    Missing,
    Loaded,
    /// A bounded but malformed/unsupported file was ignored, using defaults.
    /// No untrusted fields or original file contents are retained or reported.
    RecoveredCorrupt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsLoad {
    pub preferences: Preferences,
    pub status: LoadStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("Settings storage is unavailable")]
    Unavailable,
    #[error("Settings storage is not a private application location")]
    UnsafeLocation,
    #[error("Settings values exceed supported limits")]
    InvalidPreferences,
    #[error("A newer settings change replaced this save")]
    Superseded,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    preferences: Preferences,
}

struct Shared {
    directory: PathBuf,
    operations: Mutex<()>,
    revision: AtomicU64,
}

/// A synchronous UI reservation, before a worker is scheduled. Cloning the
/// ticket is safe; it cannot be used with another SettingsStore instance.
#[derive(Clone)]
pub struct SaveTicket {
    revision: u64,
    owner: Weak<Shared>,
}

impl fmt::Debug for SaveTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SaveTicket")
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

/// Clone this store rather than constructing independent stores for concurrent
/// UI updates. `update` serializes a read/modify/write, preserving other fields.
#[derive(Clone)]
pub struct SettingsStore(Arc<Shared>);

impl fmt::Debug for SettingsStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SettingsStore([private application directory])")
    }
}

impl SettingsStore {
    /// XDG_CONFIG_HOME is already the app's private config root in Flatpak;
    /// a dedicated subdirectory also keeps native installations separate.
    /// Construction performs no filesystem I/O.
    pub fn for_application() -> Result<Self, SettingsError> {
        Self::at_directory(gtk::glib::user_config_dir().join(crate::config::APP_ID))
    }

    /// Explicit root for fixtures and callers already using an app-private XDG
    /// location. Parent symlinks (e.g. Fedora's /home -> /var/home) are allowed;
    /// the application directory and preference file themselves are not links.
    pub fn at_directory(directory: impl Into<PathBuf>) -> Result<Self, SettingsError> {
        let directory = directory.into();
        if !directory.is_absolute()
            || directory
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(SettingsError::UnsafeLocation);
        }
        Ok(Self(Arc::new(Shared {
            directory,
            operations: Mutex::new(()),
            revision: AtomicU64::new(0),
        })))
    }

    pub fn load(&self) -> Result<SettingsLoad, SettingsError> {
        let _operation = self
            .0
            .operations
            .lock()
            .map_err(|_| SettingsError::Unavailable)?;
        self.load_inner()
    }

    pub fn save(&self, preferences: &Preferences) -> Result<(), SettingsError> {
        let ticket = self.reserve_save()?;
        if self.save_if_current(&ticket, preferences)? {
            Ok(())
        } else {
            Err(SettingsError::Superseded)
        }
    }

    pub fn reserve_save(&self) -> Result<SaveTicket, SettingsError> {
        let mut previous = self.0.revision.load(Ordering::Acquire);
        let revision = loop {
            let next = previous.checked_add(1).ok_or(SettingsError::Unavailable)?;
            match self.0.revision.compare_exchange_weak(
                previous,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break next,
                Err(actual) => previous = actual,
            }
        };
        Ok(SaveTicket {
            revision,
            owner: Arc::downgrade(&self.0),
        })
    }

    /// Returns false without writing when a newer UI reservation superseded
    /// this operation. A durable deletion barrier is effective only after a
    /// successful current save; scheduling a worker alone is not persistence.
    pub fn save_if_current(
        &self,
        ticket: &SaveTicket,
        preferences: &Preferences,
    ) -> Result<bool, SettingsError> {
        preferences.validate()?;
        let _operation = self
            .0
            .operations
            .lock()
            .map_err(|_| SettingsError::Unavailable)?;
        if !self.ticket_current(ticket) {
            return Ok(false);
        }
        self.save_inner(preferences, ticket)
    }

    pub fn update(
        &self,
        change: impl FnOnce(&mut Preferences),
    ) -> Result<Preferences, SettingsError> {
        let _operation = self
            .0
            .operations
            .lock()
            .map_err(|_| SettingsError::Unavailable)?;
        let ticket = self.reserve_save()?;
        let mut preferences = self.load_inner()?.preferences;
        change(&mut preferences);
        preferences.validate()?;
        if self.save_inner(&preferences, &ticket)? {
            Ok(preferences)
        } else {
            Err(SettingsError::Superseded)
        }
    }

    fn load_inner(&self) -> Result<SettingsLoad, SettingsError> {
        let Some(directory) = self.directory(false)? else {
            return Ok(defaults(LoadStatus::Missing));
        };
        let Some(mut file) = open_preferences(&directory)? else {
            return Ok(defaults(LoadStatus::Missing));
        };
        let length = file
            .metadata()
            .map_err(|_| SettingsError::Unavailable)?
            .len();
        if length > MAX_FILE_BYTES {
            return Ok(defaults(LoadStatus::RecoveredCorrupt));
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| SettingsError::Unavailable)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Ok(defaults(LoadStatus::RecoveredCorrupt));
        }
        match serde_json::from_slice::<Envelope>(&bytes) {
            Ok(envelope)
                if envelope.version == FORMAT_VERSION
                    && envelope.preferences.validate().is_ok() =>
            {
                Ok(SettingsLoad {
                    preferences: envelope.preferences,
                    status: LoadStatus::Loaded,
                })
            }
            _ => Ok(defaults(LoadStatus::RecoveredCorrupt)),
        }
    }

    fn ticket_current(&self, ticket: &SaveTicket) -> bool {
        Weak::ptr_eq(&ticket.owner, &Arc::downgrade(&self.0))
            && self.0.revision.load(Ordering::Acquire) == ticket.revision
    }

    fn save_inner(
        &self,
        preferences: &Preferences,
        ticket: &SaveTicket,
    ) -> Result<bool, SettingsError> {
        let bytes = serde_json::to_vec_pretty(&Envelope {
            version: FORMAT_VERSION,
            preferences: preferences.clone(),
        })
        .map_err(|_| SettingsError::InvalidPreferences)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(SettingsError::InvalidPreferences);
        }
        let directory = self.directory(true)?.ok_or(SettingsError::Unavailable)?;
        // Refuse an unsafe existing target even though rename would replace a
        // symlink rather than follow it. Do not silently destroy user links.
        let _ = open_preferences(&directory)?;
        let mut random = [0; 16];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| SettingsError::Unavailable)?;
        let suffix = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let name = format!(".preferences-{suffix}.tmp");
        let fd = rustix::fs::openat(
            &directory,
            name.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|_| SettingsError::Unavailable)?;
        let mut pending = PendingFile {
            directory: &directory,
            name,
            committed: false,
        };
        let mut file = File::from(fd);
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| SettingsError::Unavailable)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| SettingsError::Unavailable)?;
        if !self.ticket_current(ticket) {
            return Ok(false);
        }
        rustix::fs::renameat(&directory, pending.name.as_str(), &directory, FILE_NAME)
            .map_err(|_| SettingsError::Unavailable)?;
        pending.committed = true;
        directory
            .sync_all()
            .map_err(|_| SettingsError::Unavailable)?;
        Ok(self.ticket_current(ticket))
    }

    fn directory(&self, create: bool) -> Result<Option<File>, SettingsError> {
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let fd = match rustix::fs::open(&self.0.directory, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) if create => {
                DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(&self.0.directory)
                    .map_err(|_| SettingsError::Unavailable)?;
                rustix::fs::open(&self.0.directory, flags, Mode::empty())
                    .map_err(|_| SettingsError::UnsafeLocation)?
            }
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(_) => return Err(SettingsError::UnsafeLocation),
        };
        let file = File::from(fd);
        let metadata = file.metadata().map_err(|_| SettingsError::Unavailable)?;
        if !metadata.is_dir()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(SettingsError::UnsafeLocation);
        }
        Ok(Some(file))
    }
}

fn defaults(status: LoadStatus) -> SettingsLoad {
    SettingsLoad {
        preferences: Preferences::default(),
        status,
    }
}

fn open_preferences(directory: &File) -> Result<Option<File>, SettingsError> {
    let fd = match rustix::fs::openat(
        directory,
        FILE_NAME,
        // A damaged named pipe must not block restoration or a logout barrier
        // before fstat can reject it as a nonregular preferences record.
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(_) => return Err(SettingsError::UnsafeLocation),
    };
    let file = File::from(fd);
    let metadata = file.metadata().map_err(|_| SettingsError::Unavailable)?;
    if !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.nlink() != 1
        || metadata.permissions().mode() & 0o7177 != 0
        || metadata.permissions().mode() & 0o600 != 0o600
    {
        return Err(SettingsError::UnsafeLocation);
    }
    Ok(Some(file))
}

struct PendingFile<'a> {
    directory: &'a File,
    name: String,
    committed: bool,
}

impl Drop for PendingFile<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = rustix::fs::unlinkat(self.directory, self.name.as_str(), AtFlags::empty());
        }
    }
}

#[cfg(test)]
mod tests;
