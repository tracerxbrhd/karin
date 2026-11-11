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
const FORMAT_VERSION: u32 = 2;
const MAX_FILE_BYTES: u64 = 16 * 1024;
const LEGACY_DEFAULT_BYTES: u64 = 256 * 1024 * 1024;
const LEGACY_MIN_BYTES: u64 = 16 * 1024 * 1024;
const LEGACY_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// A user-visible aggregate disk budget, independent of each store's safety bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheLimit {
    MiB500,
    GiB1,
    #[default]
    GiB2,
    GiB3,
    GiB4,
    Unlimited,
}

impl CacheLimit {
    pub const ALL: [Self; 6] = [
        Self::MiB500,
        Self::GiB1,
        Self::GiB2,
        Self::GiB3,
        Self::GiB4,
        Self::Unlimited,
    ];

    pub const fn bytes(self) -> Option<u64> {
        match self {
            Self::MiB500 => Some(500 * 1024 * 1024),
            Self::GiB1 => Some(1024 * 1024 * 1024),
            Self::GiB2 => Some(2 * 1024 * 1024 * 1024),
            Self::GiB3 => Some(3 * 1024 * 1024 * 1024),
            Self::GiB4 => Some(4 * 1024 * 1024 * 1024),
            Self::Unlimited => None,
        }
    }

    pub const fn index(self) -> u32 {
        match self {
            Self::MiB500 => 0,
            Self::GiB1 => 1,
            Self::GiB2 => 2,
            Self::GiB3 => 3,
            Self::GiB4 => 4,
            Self::Unlimited => 5,
        }
    }

    pub fn from_index(index: u32) -> Option<Self> {
        Self::ALL.get(index as usize).copied()
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::MiB500 => "500 МБ",
            Self::GiB1 => "1 ГБ",
            Self::GiB2 => "2 ГБ",
            Self::GiB3 => "3 ГБ",
            Self::GiB4 => "4 ГБ",
            Self::Unlimited => "Без ограничений",
        }
    }

    fn migrated(metadata: u64, audio: u64) -> Self {
        if metadata == LEGACY_DEFAULT_BYTES && audio == LEGACY_DEFAULT_BYTES {
            return Self::default();
        }
        // Preserve the old combined allowance: rounding up never silently
        // shrinks a custom budget. An aggregate above 4 GiB becomes Unlimited.
        let combined = metadata.saturating_add(audio);
        Self::ALL
            .into_iter()
            .find(|limit| limit.bytes().is_none_or(|bytes| bytes >= combined))
            .unwrap_or_default()
    }
}

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
    pub cache_limit: CacheLimit,
    pub track_info_open: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            remember_login: true,
            forget_pending: false,
            cache_enabled: true,
            cache_limit: CacheLimit::default(),
            track_info_open: true,
        }
    }
}

impl Preferences {
    /// Choices are valid by construction; retain the owner's validation boundary
    /// so preference admission stays explicit before applying or saving a change.
    pub fn validate(&self) -> Result<(), SettingsError> {
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

#[derive(Serialize)]
struct Envelope {
    version: u32,
    preferences: Preferences,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredEnvelope {
    version: u32,
    preferences: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LegacyPreferences {
    appearance: Appearance,
    remember_login: bool,
    forget_pending: bool,
    cache_enabled: serde_json::Value,
    cache_limit_bytes: serde_json::Value,
    audio_cache_limit_bytes: serde_json::Value,
    track_info_open: bool,
}
impl Default for LegacyPreferences {
    fn default() -> Self {
        let defaults = Preferences::default();
        Self {
            appearance: defaults.appearance,
            remember_login: defaults.remember_login,
            forget_pending: defaults.forget_pending,
            cache_enabled: serde_json::json!(defaults.cache_enabled),
            cache_limit_bytes: serde_json::json!(LEGACY_DEFAULT_BYTES),
            audio_cache_limit_bytes: serde_json::json!(LEGACY_DEFAULT_BYTES),
            track_info_open: defaults.track_info_open,
        }
    }
}

fn decode_preferences(bytes: &[u8]) -> Option<Preferences> {
    let envelope: StoredEnvelope = serde_json::from_slice(bytes).ok()?;
    match envelope.version {
        1 => {
            let legacy: LegacyPreferences = serde_json::from_value(envelope.preferences).ok()?;
            let legacy_bytes = |raw: &serde_json::Value| {
                raw.as_u64()
                    .filter(|value| (LEGACY_MIN_BYTES..=LEGACY_MAX_BYTES).contains(value))
            };
            let cache_limit = match (
                legacy_bytes(&legacy.cache_limit_bytes),
                legacy_bytes(&legacy.audio_cache_limit_bytes),
            ) {
                (Some(metadata), Some(audio)) => CacheLimit::migrated(metadata, audio),
                _ => CacheLimit::default(),
            };
            Some(Preferences {
                appearance: legacy.appearance,
                remember_login: legacy.remember_login,
                forget_pending: legacy.forget_pending,
                cache_enabled: legacy.cache_enabled.as_bool().unwrap_or(true),
                cache_limit,
                track_info_open: legacy.track_info_open,
            })
        }
        FORMAT_VERSION => {
            let mut object = envelope.preferences.as_object()?.clone();
            // A malformed quota alone must not reset valid session/barrier or
            // appearance preferences. Credentials remain in their separate store.
            let limit = object
                .remove("cache_limit")
                .and_then(|value| serde_json::from_value::<CacheLimit>(value).ok())
                .unwrap_or_default();
            object.insert("cache_limit".into(), serde_json::to_value(limit).ok()?);
            let enabled = object
                .remove("cache_enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            object.insert("cache_enabled".into(), serde_json::Value::Bool(enabled));
            serde_json::from_value(serde_json::Value::Object(object)).ok()
        }
        _ => None,
    }
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
        match decode_preferences(&bytes) {
            Some(preferences) => Ok(SettingsLoad {
                preferences,
                status: LoadStatus::Loaded,
            }),
            None => Ok(defaults(LoadStatus::RecoveredCorrupt)),
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
