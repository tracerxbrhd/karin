// SPDX-License-Identifier: GPL-3.0-or-later
//! Private playback intent, never credentials, audio, URLs or radio sessions.
//!
//! Reservation/invalidation is memory-only. Every filesystem method belongs on
//! an application worker; callers admit restored state only after current Music
//! account validation and fresh bounded track normalization.

use crate::{
    cache::AccountScope,
    music::Track,
    queue::{MusicQueue, PlaybackOrigin, QueueSnapshot},
};
use ring::{
    digest::{SHA256, digest},
    rand::{SecureRandom, SystemRandom},
};
use rustix::fs::{AtFlags, Mode, OFlags};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
    fs::{DirBuilder, File},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Component, PathBuf},
    sync::{
        Arc, Condvar, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const MAGIC: &[u8; 8] = b"KARPB1\0\0";
const VERSION: u32 = 1;
const MAX_FILE: u64 = 256 * 1024;
const MAX_RECORDS: usize = 256;
const MAX_POSITION_MS: u64 = 24 * 60 * 60 * 1000;
const PENDING_TIMEOUT: Duration = Duration::from_secs(5);
type Key = [u8; 32];

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlaybackSnapshot {
    queue: QueueSnapshot,
    position_ms: u64,
}

pub(crate) struct RestoreOutcome {
    pub queue: MusicQueue,
    pub position: Duration,
    pub omitted: usize,
    pub current_changed: bool,
}

impl PlaybackSnapshot {
    pub(crate) fn capture(
        queue: &MusicQueue,
        position: Duration,
    ) -> Result<Option<Self>, SnapshotError> {
        let Some(queue) =
            QueueSnapshot::capture(queue).map_err(|_| SnapshotError::InvalidSnapshot)?
        else {
            return Ok(None);
        };
        let position_ms =
            u64::try_from(position.as_millis()).map_err(|_| SnapshotError::InvalidSnapshot)?;
        let snapshot = Self { queue, position_ms };
        snapshot.validate()?;
        Ok(Some(snapshot))
    }
    pub(crate) fn position(&self) -> Duration {
        Duration::from_millis(self.position_ms)
    }
    pub(crate) fn ids(&self) -> Vec<String> {
        self.queue.ids()
    }
    pub(crate) fn origin(&self) -> &PlaybackOrigin {
        self.queue.origin()
    }
    pub(crate) fn restore(&self, normalized: &[Track]) -> Result<RestoreOutcome, SnapshotError> {
        self.validate()?;
        let result = self
            .queue
            .restore(normalized)
            .map_err(|_| SnapshotError::InvalidSnapshot)?;
        let mut position = if result.current_changed {
            Duration::ZERO
        } else {
            self.position()
        };
        if let Some(duration) = result.queue.current().and_then(|track| track.duration_ms) {
            position = position.min(Duration::from_millis(duration.saturating_sub(250)));
        }
        Ok(RestoreOutcome {
            queue: result.queue,
            position,
            omitted: result.omitted,
            current_changed: result.current_changed,
        })
    }
    fn validate(&self) -> Result<(), SnapshotError> {
        if self.position_ms > MAX_POSITION_MS {
            return Err(SnapshotError::InvalidSnapshot);
        }
        self.queue
            .validate()
            .map_err(|_| SnapshotError::InvalidSnapshot)
    }
}
impl fmt::Debug for PlaybackSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlaybackSnapshot([private bounded intent])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoadStatus {
    Missing,
    Loaded,
    Cleared,
    RecoveredCorrupt,
}
pub(crate) struct SessionLoad {
    pub snapshot: Option<PlaybackSnapshot>,
    pub status: LoadStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum SnapshotError {
    #[error("Playback state storage is unavailable")]
    Unavailable,
    #[error("Playback state storage is not private")]
    UnsafeLocation,
    #[error("Playback state exceeds its supported format")]
    InvalidSnapshot,
    #[error("A newer playback state replaced this operation")]
    Superseded,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    account: Key,
    snapshot: Option<PlaybackSnapshot>,
}

struct Shared {
    directory: PathBuf,
    operations: Mutex<()>,
    revision: AtomicU64,
    pending: Mutex<Option<u64>>,
    changed: Condvar,
}

struct PendingTicket {
    owner: Weak<Shared>,
    revision: u64,
}
impl Drop for PendingTicket {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade() {
            complete(&owner, self.revision);
        }
    }
}
/// The last dropped clone retires an abandoned pending reservation. Worker
/// completion releases it earlier, even while GTK retains a ticket clone.
#[derive(Clone)]
pub(crate) struct SaveTicket {
    account: Key,
    pending: Arc<PendingTicket>,
}
impl fmt::Debug for SaveTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlaybackSaveTicket([private generation])")
    }
}

#[derive(Clone)]
pub(crate) struct PlaybackSessionStore(Arc<Shared>);
impl fmt::Debug for PlaybackSessionStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlaybackSessionStore([private application state])")
    }
}

fn account_key(scope: &AccountScope) -> Key {
    let mut bytes = b"Karin playback account v1\0".to_vec();
    bytes.extend_from_slice(&scope.audio_cache_key());
    digest(&SHA256, &bytes)
        .as_ref()
        .try_into()
        .expect("SHA256 length")
}
fn name(account: Key) -> String {
    format!(
        "{}.json",
        account
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
fn complete(owner: &Shared, revision: u64) {
    if let Ok(mut pending) = owner.pending.lock()
        && *pending == Some(revision)
    {
        *pending = None;
        owner.changed.notify_all();
    }
}
struct Completion<'a>(&'a Shared, u64);
impl Drop for Completion<'_> {
    fn drop(&mut self) {
        complete(self.0, self.1);
    }
}

impl PlaybackSessionStore {
    pub(crate) fn for_application() -> Result<Self, SnapshotError> {
        Self::at_directory(
            gtk::glib::user_data_dir()
                .join(crate::config::APP_ID)
                .join("playback"),
        )
    }
    /// Construction performs no I/O. Process-local reuse also serializes a new
    /// window's load with an older window's finishing close write.
    pub(crate) fn at_directory(directory: impl Into<PathBuf>) -> Result<Self, SnapshotError> {
        static STORES: OnceLock<Mutex<BTreeMap<PathBuf, Weak<Shared>>>> = OnceLock::new();
        let directory = directory.into();
        if !directory.is_absolute()
            || directory
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(SnapshotError::UnsafeLocation);
        }
        let directory: PathBuf = directory.components().collect();
        let mut stores = STORES
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| SnapshotError::Unavailable)?;
        stores.retain(|_, store| store.strong_count() > 0);
        if let Some(shared) = stores.get(&directory).and_then(Weak::upgrade) {
            return Ok(Self(shared));
        }
        if stores.len() >= 64 {
            return Err(SnapshotError::Unavailable);
        }
        let shared = Arc::new(Shared {
            directory: directory.clone(),
            operations: Mutex::new(()),
            revision: AtomicU64::new(0),
            pending: Mutex::new(None),
            changed: Condvar::new(),
        });
        stores.insert(directory, Arc::downgrade(&shared));
        Ok(Self(shared))
    }

    pub(crate) fn reserve_save(&self, account: &AccountScope) -> Result<SaveTicket, SnapshotError> {
        let mut pending = self
            .0
            .pending
            .lock()
            .map_err(|_| SnapshotError::Unavailable)?;
        let revision = self
            .0
            .revision
            .load(Ordering::Acquire)
            .checked_add(1)
            .ok_or(SnapshotError::Unavailable)?;
        self.0.revision.store(revision, Ordering::Release);
        *pending = Some(revision);
        Ok(SaveTicket {
            account: account_key(account),
            pending: Arc::new(PendingTicket {
                owner: Arc::downgrade(&self.0),
                revision,
            }),
        })
    }

    pub(crate) fn invalidate(&self) -> Result<(), SnapshotError> {
        let mut pending = self
            .0
            .pending
            .lock()
            .map_err(|_| SnapshotError::Unavailable)?;
        let next = self
            .0
            .revision
            .load(Ordering::Acquire)
            .checked_add(1)
            .ok_or(SnapshotError::Unavailable)?;
        self.0.revision.store(next, Ordering::Release);
        *pending = None;
        self.0.changed.notify_all();
        Ok(())
    }

    fn current(&self, ticket: &SaveTicket) -> bool {
        Weak::ptr_eq(&ticket.pending.owner, &Arc::downgrade(&self.0))
            && self.0.revision.load(Ordering::Acquire) == ticket.pending.revision
    }

    pub(crate) fn save_if_current(
        &self,
        ticket: &SaveTicket,
        snapshot: Option<&PlaybackSnapshot>,
    ) -> Result<bool, SnapshotError> {
        let _completion = Weak::ptr_eq(&ticket.pending.owner, &Arc::downgrade(&self.0))
            .then(|| Completion(&self.0, ticket.pending.revision));
        let _operation = self
            .0
            .operations
            .lock()
            .map_err(|_| SnapshotError::Unavailable)?;
        if !self.current(ticket) {
            return Ok(false);
        }
        if let Some(snapshot) = snapshot {
            snapshot.validate()?;
        }
        let body = serde_json::to_vec(&Envelope {
            version: VERSION,
            account: ticket.account,
            snapshot: snapshot.cloned(),
        })
        .map_err(|_| SnapshotError::InvalidSnapshot)?;
        if body.len() as u64 + 40 > MAX_FILE {
            return Err(SnapshotError::InvalidSnapshot);
        }
        let directory = self.directory(true)?.ok_or(SnapshotError::Unavailable)?;
        let target = name(ticket.account);
        let _ = open_record(&directory, &target)?;
        let mut random = [0; 16];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| SnapshotError::Unavailable)?;
        let temporary = format!(
            ".playback-{}.tmp",
            random
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let fd = rustix::fs::openat(
            &directory,
            temporary.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|_| SnapshotError::Unavailable)?;
        let mut staged = PendingFile {
            directory: &directory,
            name: temporary,
            committed: false,
        };
        let mut file = File::from(fd);
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| SnapshotError::Unavailable)?;
        file.write_all(MAGIC)
            .and_then(|()| file.write_all(digest(&SHA256, &body).as_ref()))
            .and_then(|()| file.write_all(&body))
            .and_then(|()| file.sync_all())
            .map_err(|_| SnapshotError::Unavailable)?;
        if !self.current(ticket) {
            return Ok(false);
        }
        rustix::fs::renameat(
            &directory,
            staged.name.as_str(),
            &directory,
            target.as_str(),
        )
        .map_err(|_| SnapshotError::Unavailable)?;
        staged.committed = true;
        directory
            .sync_all()
            .map_err(|_| SnapshotError::Unavailable)?;
        if !self.current(ticket) {
            // Operations are serialized, so this is still our obsolete file,
            // not a newer writer's result. A pending newer write/load stays
            // fenced until it settles; old personal intent cannot reappear.
            rustix::fs::unlinkat(&directory, target.as_str(), AtFlags::empty())
                .map_err(|_| SnapshotError::Unavailable)?;
            directory
                .sync_all()
                .map_err(|_| SnapshotError::Unavailable)?;
            return Ok(false);
        }
        Ok(true)
    }

    pub(crate) fn load(&self, account: &AccountScope) -> Result<SessionLoad, SnapshotError> {
        self.wait_pending()?;
        let _operation = self
            .0
            .operations
            .lock()
            .map_err(|_| SnapshotError::Unavailable)?;
        let revision = self.0.revision.load(Ordering::Acquire);
        if self
            .0
            .pending
            .lock()
            .map_err(|_| SnapshotError::Unavailable)?
            .is_some()
        {
            return Err(SnapshotError::Superseded);
        }
        // Missing/corrupt results are subject to the same revision fence as a
        // successful read. They must not clear a newer pending continuation.
        let result = (|| {
            let Some(directory) = self.directory(false)? else {
                return Ok(load(None, LoadStatus::Missing));
            };
            let key = account_key(account);
            let Some(mut file) = open_record(&directory, &name(key))? else {
                return Ok(load(None, LoadStatus::Missing));
            };
            let length = file
                .metadata()
                .map_err(|_| SnapshotError::Unavailable)?
                .len();
            if !(40..=MAX_FILE).contains(&length) {
                return Ok(load(None, LoadStatus::RecoveredCorrupt));
            }
            let mut bytes = Vec::with_capacity(length as usize);
            Read::by_ref(&mut file)
                .take(MAX_FILE + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| SnapshotError::Unavailable)?;
            Ok(decode(&bytes, key))
        })();
        if revision != self.0.revision.load(Ordering::Acquire) {
            return Err(SnapshotError::Superseded);
        }
        result
    }

    /// Repair a durable forget barrier when a restarted process no longer knows
    /// which account was being forgotten. This belongs on its critical worker:
    /// callers keep login/restoration and new saves fenced until it completes.
    /// Only this store's bounded, private records and abandoned staging files
    /// are considered; an unexpected entry leaves the forget barrier pending.
    pub(crate) fn clear_all(&self) -> Result<(), SnapshotError> {
        self.invalidate()?;
        let _operation = self
            .0
            .operations
            .lock()
            .map_err(|_| SnapshotError::Unavailable)?;
        let revision = self.0.revision.load(Ordering::Acquire);
        let Some(directory) = self.directory(false)? else {
            return Ok(());
        };
        let mut entries =
            rustix::fs::Dir::read_from(&directory).map_err(|_| SnapshotError::Unavailable)?;
        let mut names = Vec::new();
        for entry in &mut entries {
            let entry = entry.map_err(|_| SnapshotError::Unavailable)?;
            let name = entry
                .file_name()
                .to_str()
                .map_err(|_| SnapshotError::UnsafeLocation)?;
            if matches!(name, "." | "..") {
                continue;
            }
            if names.len() >= MAX_RECORDS || !recognized_name(name) {
                return Err(SnapshotError::UnsafeLocation);
            }
            // Validate every candidate before deleting any. Descriptor-relative
            // no-follow opens reject links, directories, devices and FIFOs.
            let _record = open_record(&directory, name)?.ok_or(SnapshotError::Unavailable)?;
            names.push(name.to_owned());
        }
        for name in names {
            if revision != self.0.revision.load(Ordering::Acquire) {
                return Err(SnapshotError::Superseded);
            }
            rustix::fs::unlinkat(&directory, name.as_str(), AtFlags::empty())
                .map_err(|_| SnapshotError::Unavailable)?;
        }
        directory
            .sync_all()
            .map_err(|_| SnapshotError::Unavailable)?;
        if revision != self.0.revision.load(Ordering::Acquire) {
            return Err(SnapshotError::Superseded);
        }
        Ok(())
    }
    fn wait_pending(&self) -> Result<(), SnapshotError> {
        let deadline = Instant::now() + PENDING_TIMEOUT;
        let mut pending = self
            .0
            .pending
            .lock()
            .map_err(|_| SnapshotError::Unavailable)?;
        while pending.is_some() {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(SnapshotError::Superseded)?;
            let (state, _) = self
                .0
                .changed
                .wait_timeout(pending, remaining)
                .map_err(|_| SnapshotError::Unavailable)?;
            pending = state;
        }
        Ok(())
    }
    fn directory(&self, create: bool) -> Result<Option<File>, SnapshotError> {
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let fd = match rustix::fs::open(&self.0.directory, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) if create => {
                DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(&self.0.directory)
                    .map_err(|_| SnapshotError::Unavailable)?;
                rustix::fs::open(&self.0.directory, flags, Mode::empty())
                    .map_err(|_| SnapshotError::UnsafeLocation)?
            }
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(_) => return Err(SnapshotError::UnsafeLocation),
        };
        let file = File::from(fd);
        let metadata = file.metadata().map_err(|_| SnapshotError::Unavailable)?;
        if !metadata.is_dir()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(SnapshotError::UnsafeLocation);
        }
        Ok(Some(file))
    }
    #[cfg(test)]
    pub(crate) fn path_for_test(&self, account: &AccountScope) -> PathBuf {
        self.0.directory.join(name(account_key(account)))
    }
}

fn load(snapshot: Option<PlaybackSnapshot>, status: LoadStatus) -> SessionLoad {
    SessionLoad { snapshot, status }
}
fn recognized_name(name: &str) -> bool {
    let hex = |value: &str, length| {
        value.len() == length
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    name.strip_suffix(".json")
        .is_some_and(|value| hex(value, 64))
        || name
            .strip_prefix(".playback-")
            .and_then(|value| value.strip_suffix(".tmp"))
            .is_some_and(|value| hex(value, 32))
}
fn decode(bytes: &[u8], account: Key) -> SessionLoad {
    let corrupt = || load(None, LoadStatus::RecoveredCorrupt);
    if bytes.len() as u64 > MAX_FILE
        || bytes.len() <= 40
        || &bytes[..8] != MAGIC
        || digest(&SHA256, &bytes[40..]).as_ref() != &bytes[8..40]
    {
        return corrupt();
    }
    let Ok(envelope) = serde_json::from_slice::<Envelope>(&bytes[40..]) else {
        return corrupt();
    };
    if envelope.version != VERSION
        || envelope.account != account
        || envelope
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.validate().is_err())
    {
        return corrupt();
    }
    let status = if envelope.snapshot.is_some() {
        LoadStatus::Loaded
    } else {
        LoadStatus::Cleared
    };
    load(envelope.snapshot, status)
}
fn open_record(directory: &File, name: &str) -> Result<Option<File>, SnapshotError> {
    let fd = match rustix::fs::openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(_) => return Err(SnapshotError::UnsafeLocation),
    };
    let file = File::from(fd);
    let metadata = file.metadata().map_err(|_| SnapshotError::Unavailable)?;
    if !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.nlink() != 1
        || metadata.permissions().mode() & 0o7177 != 0
        || metadata.permissions().mode() & 0o600 != 0o600
    {
        return Err(SnapshotError::UnsafeLocation);
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
