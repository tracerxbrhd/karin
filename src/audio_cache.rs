// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic-only executable cache research, compiled exclusively by `cargo test`.
//!
//! This is NOT a service storage grant, production AudioCache, offline feature or
//! source fallback. Inputs can only be generated original PCM WAV. No URL, token,
//! network client, service Track or RemotePlaybackSource enters this component.
//! A future adapter needs independently established storage/entitlement rules.

use ring::{
    digest::{SHA256, digest},
    rand::{SecureRandom, SystemRandom},
};
use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, UNIX_EPOCH},
};

const MAX_AUDIO: usize = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 64;
const MAX_BUDGET: u64 = 32 * 1024 * 1024;
const MAX_REQUESTS: usize = 4;
const HEADER: usize = 112;
const MAGIC: &[u8; 8] = b"KARNSYN1";
type Key = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
enum CacheError {
    #[error("Synthetic cache input is invalid")]
    InvalidInput,
    #[error("Synthetic cache storage is unavailable")]
    Unavailable,
    #[error("Synthetic cache storage is busy")]
    Busy,
    #[error("Synthetic cache operation was cancelled")]
    Cancelled,
    #[error("Synthetic cache request is already in flight")]
    Duplicate,
    #[error("Synthetic cache limit was reached")]
    Limit,
    #[error("No synthetic storage permission was supplied")]
    NoGrant,
    #[error("Synthetic permission has expired")]
    Expired,
    #[error("Synthetic permission was revoked")]
    Revoked,
    #[error("Synthetic permission requires an online check")]
    NeedsOnline,
}
type Result<T> = std::result::Result<T, CacheError>;

/// The only input constructor synthesizes original audio; it cannot fetch or
/// import a service file. This small PCM WAV is independently decodable by Gst.
struct SyntheticAudio(Vec<u8>);
impl SyntheticAudio {
    fn tone(samples: u32) -> Result<Self> {
        let data_len = samples.checked_mul(2).ok_or(CacheError::Limit)?;
        if data_len == 0 || data_len as usize + 44 > MAX_AUDIO {
            return Err(CacheError::Limit);
        }
        let mut data = Vec::with_capacity(data_len as usize + 44);
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&(data_len + 36).to_le_bytes());
        data.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        data.extend_from_slice(&8_000u32.to_le_bytes());
        data.extend_from_slice(&16_000u32.to_le_bytes());
        data.extend_from_slice(b"\x02\0\x10\0data");
        data.extend_from_slice(&data_len.to_le_bytes());
        for sample in 0..samples {
            let amplitude: i16 = if sample % 20 < 10 { 512 } else { -512 };
            data.extend_from_slice(&amplitude.to_le_bytes());
        }
        Ok(Self(data))
    }
}

/// These deliberately synthetic identities never come from a Music account.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    account: u64,
    device: u64,
    track: u64,
}
impl Identity {
    fn key(self) -> Key {
        let mut bytes = b"Karin synthetic audio research v1\0".to_vec();
        for value in [self.account, self.device, self.track] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        hash(&bytes)
    }
    fn scope(self) -> Key {
        hash(&self.account.to_be_bytes())
    }
}
#[derive(Clone, Copy)]
struct SyntheticRights {
    revision: u64,
    expires_at: u64,
    offline_allowed: bool,
    online_check_required: bool,
    revoked: bool,
}
#[derive(Default)]
struct SyntheticAuthority {
    rights: Mutex<BTreeMap<Key, SyntheticRights>>,
    now: AtomicU64,
    online: AtomicBool,
}
#[derive(Clone, Copy)]
struct Permit {
    identity: Identity,
    revision: u64,
}
impl SyntheticAuthority {
    fn grant(&self, identity: Identity, expires_at: u64, offline_allowed: bool) {
        let mut rights = self.rights.lock().unwrap();
        let revision = rights.get(&identity.key()).map_or(1, |r| r.revision + 1);
        rights.insert(
            identity.key(),
            SyntheticRights {
                revision,
                expires_at,
                offline_allowed,
                online_check_required: false,
                revoked: false,
            },
        );
    }
    fn revoke(&self, identity: Identity) {
        if let Some(rights) = self.rights.lock().unwrap().get_mut(&identity.key()) {
            rights.revoked = true;
            rights.revision += 1;
        }
    }
    fn authorize(&self, identity: Identity) -> Result<Permit> {
        let all = self.rights.lock().map_err(|_| CacheError::Unavailable)?;
        let rights = all.get(&identity.key()).ok_or(CacheError::NoGrant)?;
        if rights.revoked {
            return Err(CacheError::Revoked);
        }
        if self.now.load(Ordering::Acquire) >= rights.expires_at {
            return Err(CacheError::Expired);
        }
        if (rights.online_check_required || !rights.offline_allowed)
            && !self.online.load(Ordering::Acquire)
        {
            return Err(CacheError::NeedsOnline);
        }
        Ok(Permit {
            identity,
            revision: rights.revision,
        })
    }
    fn check(&self, permit: Permit) -> Result<()> {
        let current = self.authorize(permit.identity)?;
        if current.revision != permit.revision {
            return Err(CacheError::Cancelled);
        }
        Ok(())
    }
}

/// A disposable, private namespace. No production XDG directory is constructed
/// by this research module. Tests can put it under a synthetic Flatpak XDG root.
struct Sandbox {
    path: PathBuf,
    remove_on_drop: bool,
}
impl Sandbox {
    fn create(parent: &Path) -> Result<Arc<Self>> {
        let path = parent.join(format!("karin-synthetic-audio-{}", random_name()?));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|_| CacheError::Unavailable)?;
        Ok(Arc::new(Self {
            path,
            remove_on_drop: true,
        }))
    }
    fn reopen(path: PathBuf) -> Result<Arc<Self>> {
        private_dir(&path)?;
        if !path.file_name().is_some_and(|name| {
            name.as_encoded_bytes()
                .starts_with(b"karin-synthetic-audio-")
        }) {
            return Err(CacheError::InvalidInput);
        }
        Ok(Arc::new(Self {
            path,
            remove_on_drop: false,
        }))
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        if self.remove_on_drop {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
struct State {
    account: Option<u64>,
    generation: u64,
    clearing: bool,
    pending: BTreeMap<Key, Arc<()>>,
    sequence: u64,
}
struct Inner {
    sandbox: Arc<Sandbox>,
    authority: Arc<SyntheticAuthority>,
    device: u64,
    budget: u64,
    state: Mutex<State>,
    io: Mutex<()>,
    // Held for the complete namespace-owner lifetime. Separate instances must
    // not retain independent generations for the same directory.
    _lock_file: File,
}
#[derive(Clone)]
struct AudioCache(Arc<Inner>);
#[derive(Clone)]
struct Lease {
    owner: Weak<Inner>,
    account: u64,
    generation: u64,
}
struct Request {
    lease: Lease,
    permit: Permit,
    nonce: Arc<()>,
    cancel: Arc<AtomicBool>,
}
impl Request {
    fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        if let Some(inner) = self.lease.owner.upgrade()
            && let Ok(mut state) = inner.state.lock()
            && state
                .pending
                .get(&self.permit.identity.key())
                .is_some_and(|nonce| Arc::ptr_eq(nonce, &self.nonce))
        {
            state.pending.remove(&self.permit.identity.key());
        }
    }
}
struct Hit {
    owner: Weak<Inner>,
    lease: Lease,
    permit: Permit,
    bytes: Arc<[u8]>,
}
impl Hit {
    /// Recheck immediately at admission; an old returned value cannot establish
    /// rights after Clear, logout, account replacement, renewal or revocation.
    fn admit(&self) -> Result<Arc<[u8]>> {
        let cache = AudioCache(self.owner.upgrade().ok_or(CacheError::Cancelled)?);
        cache.check(&self.lease, self.permit)?;
        Ok(self.bytes.clone())
    }
}
impl fmt::Debug for Hit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SyntheticAudioHit([opaque])")
    }
}
struct DiskEntry {
    path: PathBuf,
    key: Key,
    bytes: u64,
    access: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Stats {
    bytes: u64,
    entries: usize,
}
struct IoGuard<'a> {
    _local: MutexGuard<'a, ()>,
}
struct PartialRecord(PathBuf);
impl Drop for PartialRecord {
    fn drop(&mut self) {
        let _ = remove_record(&self.0);
    }
}

impl AudioCache {
    fn open(
        sandbox: Arc<Sandbox>,
        authority: Arc<SyntheticAuthority>,
        device: u64,
        budget: u64,
    ) -> Result<Self> {
        if device == 0 || budget == 0 || budget > MAX_BUDGET {
            return Err(CacheError::InvalidInput);
        }
        private_dir(&sandbox.path)?;
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(nofollow())
            .open(sandbox.path.join(".lock"))
            .map_err(|_| CacheError::Unavailable)?;
        private_file(&lock_file)?;
        rustix::fs::flock(
            &lock_file,
            rustix::fs::FlockOperation::NonBlockingLockExclusive,
        )
        .map_err(|_| CacheError::Busy)?;
        let cache = Self(Arc::new(Inner {
            sandbox,
            authority,
            device,
            budget,
            state: Mutex::new(State {
                account: None,
                generation: 1,
                clearing: false,
                pending: BTreeMap::new(),
                sequence: 0,
            }),
            io: Mutex::new(()),
            _lock_file: lock_file,
        }));
        {
            let _io = cache.io()?;
            let entries = cache.scan()?;
            cache.0.state.lock().unwrap().sequence =
                entries.iter().map(|e| e.access).max().unwrap_or(0);
            cache.evict(entries, 0, None)?;
        }
        Ok(cache)
    }
    fn bind(&self, account: Option<u64>) -> Result<Option<Lease>> {
        if account == Some(0) {
            return Err(CacheError::InvalidInput);
        }
        let mut state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
        state.generation = state.generation.checked_add(1).ok_or(CacheError::Limit)?;
        state.account = account;
        state.pending.clear();
        Ok(account.map(|account| Lease {
            owner: Arc::downgrade(&self.0),
            account,
            generation: state.generation,
        }))
    }
    fn lease_current(&self, lease: &Lease) -> Result<()> {
        let state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
        if !lease.owner.ptr_eq(&Arc::downgrade(&self.0))
            || state.clearing
            || state.generation != lease.generation
            || state.account != Some(lease.account)
        {
            return Err(CacheError::Cancelled);
        }
        Ok(())
    }
    fn check(&self, lease: &Lease, permit: Permit) -> Result<()> {
        self.lease_current(lease)?;
        if permit.identity.account != lease.account || permit.identity.device != self.0.device {
            return Err(CacheError::Cancelled);
        }
        self.0.authority.check(permit)
    }
    fn identity(&self, lease: &Lease, track: u64) -> Result<Identity> {
        if track == 0 {
            return Err(CacheError::InvalidInput);
        }
        Ok(Identity {
            account: lease.account,
            device: self.0.device,
            track,
        })
    }
    fn begin(&self, lease: &Lease, track: u64) -> Result<Request> {
        self.lease_current(lease)?;
        let permit = self.0.authority.authorize(self.identity(lease, track)?)?;
        self.check(lease, permit)?;
        let mut state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
        if state.clearing
            || state.generation != lease.generation
            || state.account != Some(lease.account)
        {
            return Err(CacheError::Cancelled);
        }
        if state.pending.contains_key(&permit.identity.key()) {
            return Err(CacheError::Duplicate);
        }
        if state.pending.len() >= MAX_REQUESTS {
            return Err(CacheError::Limit);
        }
        let nonce = Arc::new(());
        state.pending.insert(permit.identity.key(), nonce.clone());
        Ok(Request {
            lease: lease.clone(),
            permit,
            nonce,
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }
    fn request_current(&self, request: &Request) -> Result<()> {
        if request.cancel.load(Ordering::Acquire) {
            return Err(CacheError::Cancelled);
        }
        self.check(&request.lease, request.permit)
    }
    fn put(&self, request: Request, audio: &SyntheticAudio) -> Result<()> {
        self.put_before_commit(request, audio, || Ok(()))
    }
    fn put_before_commit(
        &self,
        request: Request,
        audio: &SyntheticAudio,
        before_commit: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        self.request_current(&request)?;
        let size = (HEADER + audio.0.len()) as u64;
        if size > self.0.budget {
            return Err(CacheError::Limit);
        }
        let _io = self.io()?;
        self.request_current(&request)?;
        let key = request.permit.identity.key();
        self.evict(self.scan()?, size, Some(key))?;
        let temporary = self
            .0
            .sandbox
            .path
            .join(format!(".part-{}", random_name()?));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&temporary)
            .map_err(|_| CacheError::Unavailable)?;
        let guard = PartialRecord(temporary.clone());
        let header = encode(request.permit.identity, &audio.0);
        write_private(&temporary.join("header"), &header)?;
        {
            let mut output = new_file(&temporary.join("audio.wav"))?;
            for chunk in audio.0.chunks(4096) {
                self.request_current(&request)?;
                output
                    .write_all(chunk)
                    .map_err(|_| CacheError::Unavailable)?;
            }
            output.sync_all().map_err(|_| CacheError::Unavailable)?;
        }
        sync_dir(&temporary)?;
        before_commit()?;
        self.request_current(&request)?;
        let path = self.path(key);
        if path.exists() {
            remove_record(&path)?;
        }
        fs::rename(&temporary, &path).map_err(|_| CacheError::Unavailable)?;
        sync_dir(&self.0.sandbox.path)?;
        if let Err(error) = self.request_current(&request) {
            // Serialized I/O prevents deleting a newer writer's replacement.
            if remove_record(&path).is_err() {
                self.0
                    .state
                    .lock()
                    .map_err(|_| CacheError::Unavailable)?
                    .clearing = true;
                return Err(CacheError::Unavailable);
            }
            return Err(error);
        }
        self.touch(&path)?;
        drop(guard);
        Ok(())
    }
    fn get(&self, lease: &Lease, track: u64) -> Result<Option<Hit>> {
        self.lease_current(lease)?;
        let identity = self.identity(lease, track)?;
        let _io = self.io()?;
        self.lease_current(lease)?;
        let permit = match self.0.authority.authorize(identity) {
            Ok(permit) => permit,
            Err(error) => {
                if matches!(error, CacheError::Expired | CacheError::Revoked) {
                    let path = self.path(identity.key());
                    if path.exists() {
                        remove_record(&path)?;
                    }
                }
                return Err(error);
            }
        };
        self.check(lease, permit)?;
        let path = self.path(identity.key());
        let Some(bytes) = read_record(&path, identity.key(), Some(identity.scope()))? else {
            return Ok(None);
        };
        self.check(lease, permit)?;
        self.touch(&path)?;
        Ok(Some(Hit {
            owner: Arc::downgrade(&self.0),
            lease: lease.clone(),
            permit,
            bytes: bytes.into(),
        }))
    }
    /// Invalidates immediately; Busy/failure leaves reads fenced until an
    /// explicit worker retry completes cleanup. Never reports an unmeasured zero.
    fn clear(&self) -> Result<Stats> {
        {
            let mut state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
            state.generation = state.generation.checked_add(1).ok_or(CacheError::Limit)?;
            state.clearing = true;
            state.pending.clear();
        }
        let _io = self.io()?;
        for entry in self.scan()? {
            remove_record(&entry.path)?;
        }
        sync_dir(&self.0.sandbox.path)?;
        self.0
            .state
            .lock()
            .map_err(|_| CacheError::Unavailable)?
            .clearing = false;
        Ok(Stats::default())
    }
    fn logout(&self) -> Result<Stats> {
        self.bind(None)?;
        self.clear()
    }
    fn stats(&self) -> Result<Stats> {
        let _io = self.io()?;
        let entries = self.scan()?;
        Ok(Stats {
            bytes: entries.iter().map(|e| e.bytes).sum(),
            entries: entries.len(),
        })
    }
    fn path(&self, key: Key) -> PathBuf {
        self.0.sandbox.path.join(hex(&key))
    }
    fn io(&self) -> Result<IoGuard<'_>> {
        let local = self.0.io.try_lock().map_err(|_| CacheError::Busy)?;
        let guard = IoGuard { _local: local };
        private_dir(&self.0.sandbox.path)?;
        Ok(guard)
    }
    fn touch(&self, path: &Path) -> Result<()> {
        let sequence = {
            let mut state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
            state.sequence = state.sequence.checked_add(1).ok_or(CacheError::Limit)?;
            state.sequence
        };
        File::open(path)
            .and_then(|dir| {
                dir.set_times(
                    fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(sequence)),
                )
            })
            .map_err(|_| CacheError::Unavailable)
    }
    fn scan(&self) -> Result<Vec<DiskEntry>> {
        let mut entries = Vec::new();
        for (number, item) in fs::read_dir(&self.0.sandbox.path)
            .map_err(|_| CacheError::Unavailable)?
            .enumerate()
        {
            if number > MAX_ENTRIES * 2 + MAX_REQUESTS {
                return Err(CacheError::Limit);
            }
            let item = item.map_err(|_| CacheError::Unavailable)?;
            let name = item.file_name();
            let name = name.to_str().ok_or(CacheError::Unavailable)?;
            if name == ".lock" {
                continue;
            }
            if name.strip_prefix(".part-").is_some_and(valid_hex) {
                // With the owner/I/O locks held there is no concurrent writer:
                // any visible staging record is orphaned. It must be removed
                // or fail the operation, never disappear from quota accounting.
                remove_record(&item.path())?;
                continue;
            }
            if !valid_hex(name) || name.len() != 64 {
                return Err(CacheError::Unavailable);
            }
            let key = decode_key(name)?;
            let Some(bytes) = record_size(&item.path(), key)? else {
                continue;
            };
            let access = fs::symlink_metadata(item.path())
                .map_err(|_| CacheError::Unavailable)?
                .modified()
                .map_err(|_| CacheError::Unavailable)?
                .duration_since(UNIX_EPOCH)
                .map_err(|_| CacheError::Unavailable)?
                .as_secs();
            entries.push(DiskEntry {
                path: item.path(),
                key,
                bytes,
                access,
            });
        }
        if entries.len() > MAX_ENTRIES {
            return Err(CacheError::Limit);
        }
        Ok(entries)
    }
    fn evict(
        &self,
        mut entries: Vec<DiskEntry>,
        incoming: u64,
        replacement: Option<Key>,
    ) -> Result<()> {
        if let Some(key) = replacement {
            entries.retain(|entry| entry.key != key);
            // Remove an older version before allocating its replacement's bytes.
            let old = self.path(key);
            if old.exists() {
                remove_record(&old)?;
            }
        }
        entries.sort_by_key(|entry| entry.access);
        let mut bytes: u64 = entries.iter().map(|entry| entry.bytes).sum();
        let keep = MAX_ENTRIES - usize::from(incoming > 0);
        let mut count = entries.len();
        for entry in &entries {
            if bytes.saturating_add(incoming) <= self.0.budget && count <= keep {
                break;
            }
            remove_record(&entry.path)?;
            bytes = bytes.saturating_sub(entry.bytes);
            count -= 1;
        }
        Ok(())
    }
}

fn encode(identity: Identity, audio: &[u8]) -> [u8; HEADER] {
    let mut header = [0u8; HEADER];
    header[..8].copy_from_slice(MAGIC);
    header[8..40].copy_from_slice(&identity.key());
    header[40..72].copy_from_slice(&identity.scope());
    header[72..80].copy_from_slice(&(audio.len() as u64).to_be_bytes());
    let mut data = header[..80].to_vec();
    data.extend_from_slice(audio);
    header[80..112].copy_from_slice(&hash(&data));
    header
}
// Quota/recovery inspection uses metadata only, never unauthorised audio
// payload. Full content integrity is checked by get() after current admission.
fn record_size(path: &Path, key: Key) -> Result<Option<u64>> {
    private_dir(path)?;
    let record = (|| {
        let mut file = open_private(&path.join("header"))?;
        if file.metadata().map_err(|_| CacheError::Unavailable)?.len() != HEADER as u64 {
            return Ok(None);
        }
        let mut header = [0; HEADER];
        file.read_exact(&mut header)
            .map_err(|_| CacheError::Unavailable)?;
        if &header[..8] != MAGIC || header[8..40] != key {
            return Ok(None);
        }
        let length = u64::from_be_bytes(header[72..80].try_into().unwrap());
        let audio = open_private(&path.join("audio.wav"))?;
        if length == 0
            || length > MAX_AUDIO as u64
            || audio.metadata().map_err(|_| CacheError::Unavailable)?.len() != length
        {
            return Ok(None);
        }
        Ok(Some(HEADER as u64 + length))
    })();
    match record {
        Ok(None) => {
            remove_record(path)?;
            Ok(None)
        }
        other => other,
    }
}

fn read_record(path: &Path, key: Key, scope: Option<Key>) -> Result<Option<Vec<u8>>> {
    if !path.exists() {
        return Ok(None);
    }
    private_dir(path)?;
    let mut header = [0u8; HEADER];
    let record = (|| {
        let mut input = open_private(&path.join("header"))?;
        if input.metadata().map_err(|_| CacheError::Unavailable)?.len() != HEADER as u64 {
            return Ok(None);
        }
        input
            .read_exact(&mut header)
            .map_err(|_| CacheError::Unavailable)?;
        if &header[..8] != MAGIC
            || header[8..40] != key
            || scope.is_some_and(|s| header[40..72] != s)
        {
            return Ok(None);
        }
        let length = u64::from_be_bytes(header[72..80].try_into().unwrap());
        if length == 0 || length > MAX_AUDIO as u64 {
            return Ok(None);
        }
        let input = open_private(&path.join("audio.wav"))?;
        if input.metadata().map_err(|_| CacheError::Unavailable)?.len() != length {
            return Ok(None);
        }
        let mut audio = Vec::with_capacity(length as usize);
        input
            .take(length + 1)
            .read_to_end(&mut audio)
            .map_err(|_| CacheError::Unavailable)?;
        let mut data = header[..80].to_vec();
        data.extend_from_slice(&audio);
        if audio.len() != length as usize || hash(&data) != header[80..112] {
            return Ok(None);
        }
        Ok(Some(audio))
    })();
    match record {
        Ok(None) => {
            remove_record(path)?;
            Ok(None)
        }
        other => other,
    }
}
fn private_dir(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|_| CacheError::Unavailable)?;
    if !meta.is_dir()
        || meta.uid() != rustix::process::geteuid().as_raw()
        || meta.mode() & 0o777 != 0o700
    {
        return Err(CacheError::Unavailable);
    }
    Ok(())
}
fn private_file(file: &File) -> Result<()> {
    let meta = file.metadata().map_err(|_| CacheError::Unavailable)?;
    if !meta.is_file()
        || meta.nlink() != 1
        || meta.uid() != rustix::process::geteuid().as_raw()
        || meta.mode() & 0o777 != 0o600
    {
        return Err(CacheError::Unavailable);
    }
    Ok(())
}
fn open_private(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nofollow())
        .open(path)
        .map_err(|_| CacheError::Unavailable)?;
    private_file(&file)?;
    Ok(file)
}
fn new_file(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(nofollow())
        .open(path)
        .map_err(|_| CacheError::Unavailable)
}
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = new_file(path)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| CacheError::Unavailable)
}
fn remove_record(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    private_dir(path)?;
    let children = fs::read_dir(path)
        .map_err(|_| CacheError::Unavailable)?
        .take(3)
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|_| CacheError::Unavailable)?;
    if children.len() > 2
        || children
            .iter()
            .any(|entry| entry.file_name() != "header" && entry.file_name() != "audio.wav")
    {
        return Err(CacheError::Unavailable);
    }
    for child in children {
        if child
            .file_type()
            .map_err(|_| CacheError::Unavailable)?
            .is_dir()
        {
            return Err(CacheError::Unavailable);
        }
        fs::remove_file(child.path()).map_err(|_| CacheError::Unavailable)?;
    }
    fs::remove_dir(path).map_err(|_| CacheError::Unavailable)
}
fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|dir| dir.sync_all())
        .map_err(|_| CacheError::Unavailable)
}
fn nofollow() -> i32 {
    rustix::fs::OFlags::NOFOLLOW.bits() as i32
}
fn hash(bytes: &[u8]) -> Key {
    digest(&SHA256, bytes).as_ref().try_into().unwrap()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn valid_hex(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn decode_key(value: &str) -> Result<Key> {
    if value.len() != 64 || !valid_hex(value) {
        return Err(CacheError::Unavailable);
    }
    let mut result = [0; 32];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| CacheError::Unavailable)?;
    }
    Ok(result)
}
fn random_name() -> Result<String> {
    let mut bytes = [0; 16];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| CacheError::Unavailable)?;
    Ok(hex(&bytes))
}

mod tests;
