// SPDX-License-Identifier: GPL-3.0-or-later
//! Private, account-scoped metadata/raster cache. All filesystem operations run
//! on bounded application workers; this module owns no thread, GTK object or
//! network client. Cached values never establish authentication or entitlement.

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, TryLockError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use ring::{
    digest::{SHA256, digest},
    rand::{SecureRandom, SystemRandom},
};

const MAGIC: &[u8; 8] = b"KARNC3A\0";
const HEADER_BYTES: usize = 144;
const MAX_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_SCOPES: usize = 64;
const MAX_BUDGET: u64 = 4 * 1024 * 1024 * 1024;
const MAX_TTL_SECONDS: u64 = 30 * 24 * 60 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachePolicy {
    pub enabled: bool,
    pub max_bytes: u64,
    pub metadata_ttl: Duration,
    pub artwork_ttl: Duration,
}
impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            max_bytes: 256 * 1024 * 1024,
            metadata_ttl: Duration::from_secs(15 * 60),
            artwork_ttl: Duration::from_secs(7 * 24 * 60 * 60),
        }
    }
}
impl CachePolicy {
    fn validate(self) -> Result<Self, CacheError> {
        if self.max_bytes == 0
            || self.max_bytes > MAX_BUDGET
            || self.metadata_ttl.as_secs() > MAX_TTL_SECONDS
            || self.artwork_ttl.as_secs() > MAX_TTL_SECONDS
        {
            return Err(CacheError::InvalidInput);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CacheError {
    #[error("Persistent cache is disabled")]
    Disabled,
    #[error("Cache operation belongs to an obsolete session or policy")]
    Cancelled,
    #[error("Cache storage is busy")]
    Busy,
    #[error("Private cache storage is unavailable")]
    Unavailable,
    #[error("Invalid cache request")]
    InvalidInput,
    #[error("Unsupported cache record")]
    InvalidData,
    #[error("Cache record exceeds the supported limit")]
    TooLarge,
}

/// Derived only after Music account validation, never from a display name or
/// a token. A hash avoids user identifiers in filenames; it is not encryption.
#[derive(Clone, PartialEq, Eq)]
pub struct AccountScope([u8; 32]);
impl AccountScope {
    pub fn from_account_id(value: &str) -> Result<Self, CacheError> {
        if value.is_empty()
            || value.len() > 128
            || !value.bytes().all(|b| b.is_ascii_digit())
            || !value.parse::<u64>().is_ok_and(|n| n > 0)
        {
            return Err(CacheError::InvalidInput);
        }
        let mut context = b"Karin persistent metadata account v1\0".to_vec();
        let canonical = value
            .parse::<u64>()
            .map_err(|_| CacheError::InvalidInput)?
            .to_string();
        context.extend_from_slice(canonical.as_bytes());
        Ok(Self(hash(&context)))
    }
}
impl fmt::Debug for AccountScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccountScope([redacted])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EntryKind {
    LikedTracks = 1,
    Playlists = 2,
    PlaylistTracks = 3,
    ArtistTracks = 4,
    Album = 5,
    Track = 6,
    Artwork = 7,
    LikedPlaylists = 8,
}
impl EntryKind {
    fn from_byte(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::LikedTracks),
            2 => Some(Self::Playlists),
            3 => Some(Self::PlaylistTracks),
            4 => Some(Self::ArtistTracks),
            5 => Some(Self::Album),
            6 => Some(Self::Track),
            7 => Some(Self::Artwork),
            8 => Some(Self::LikedPlaylists),
            _ => None,
        }
    }
    fn ttl(self, policy: CachePolicy) -> Duration {
        if self == Self::Artwork {
            policy.artwork_ttl
        } else {
            policy.metadata_ttl
        }
    }
}

/// Selector values are never written to the filesystem. Only the strict,
/// bounded digest below is used in the private entry filename and header.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    kind: EntryKind,
    selector: String,
    page: u32,
    hash: [u8; 32],
}
impl CacheKey {
    pub fn new(kind: EntryKind, selector: &str, page: u32) -> Result<Self, CacheError> {
        if selector.is_empty()
            || selector.len() > 512
            || !selector
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
            || page > 10_000
            || (matches!(
                kind,
                EntryKind::Album | EntryKind::Track | EntryKind::Artwork
            ) && page != 0)
        {
            return Err(CacheError::InvalidInput);
        }
        let mut key = b"Karin persistent entry v1\0".to_vec();
        key.push(kind as u8);
        key.extend_from_slice(&page.to_be_bytes());
        key.extend_from_slice(selector.as_bytes());
        Ok(Self {
            kind,
            selector: selector.to_owned(),
            page,
            hash: hash(&key),
        })
    }
    pub fn kind(&self) -> EntryKind {
        self.kind
    }
}
impl fmt::Debug for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheKey")
            .field("kind", &self.kind)
            .field("selector", &"[redacted]")
            .field("page", &self.page)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freshness {
    Fresh,
    Stale,
}
pub struct CacheEntry {
    pub bytes: Vec<u8>,
    pub freshness: Freshness,
    pub age: Duration,
}
impl fmt::Debug for CacheEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheEntry")
            .field("byte_count", &self.bytes.len())
            .field("freshness", &self.freshness)
            .field("age", &self.age)
            .finish()
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Actual regular-entry file lengths, including their integrity headers.
    pub bytes: u64,
    pub entries: usize,
}

trait Clock: Send + Sync {
    fn seconds(&self) -> u64;
}
struct WallClock;
impl Clock for WallClock {
    fn seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}
struct State {
    policy: CachePolicy,
    scope: Option<AccountScope>,
    generation: u64,
    clearing: bool,
}
impl State {
    fn advance(&mut self) {
        self.generation = self.generation.wrapping_add(1).max(1);
    }
}
struct Inner {
    root: PathBuf,
    state: Mutex<State>,
    io: Mutex<()>,
    clock: Arc<dyn Clock>,
}
#[derive(Clone)]
pub struct Cache(Arc<Inner>);
impl fmt::Debug for Cache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Cache([private application storage])")
    }
}

#[derive(Clone)]
pub struct CacheLease {
    cache: Arc<Inner>,
    scope: AccountScope,
    generation: u64,
}
impl CacheLease {
    pub fn matches_scope(&self, scope: &AccountScope) -> bool {
        &self.scope == scope && self.is_current()
    }
    pub fn is_current(&self) -> bool {
        self.cache
            .state
            .lock()
            .is_ok_and(|state| current(&state, self))
    }
}
impl PartialEq for CacheLease {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.cache, &other.cache)
            && self.scope == other.scope
            && self.generation == other.generation
    }
}
impl Eq for CacheLease {}
impl fmt::Debug for CacheLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CacheLease([redacted])")
    }
}
fn current(state: &State, lease: &CacheLease) -> bool {
    state.policy.enabled
        && !state.clearing
        && state.generation == lease.generation
        && state.scope.as_ref() == Some(&lease.scope)
}

impl Cache {
    /// `root` must be a dedicated app-private directory, for example
    /// `$XDG_CACHE_HOME/karin/persistent`. Construction does no filesystem I/O.
    pub fn new(root: PathBuf, policy: CachePolicy) -> Result<Self, CacheError> {
        Self::with_clock(root, policy, Arc::new(WallClock))
    }
    fn with_clock(
        root: PathBuf,
        policy: CachePolicy,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, CacheError> {
        if !root.is_absolute()
            || root
                .components()
                .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
            || root.parent().is_none()
        {
            return Err(CacheError::InvalidInput);
        }
        Ok(Self(Arc::new(Inner {
            root,
            state: Mutex::new(State {
                policy: policy.validate()?,
                scope: None,
                generation: 1,
                clearing: false,
            }),
            io: Mutex::new(()),
            clock,
        })))
    }
    /// Binding, including binding the same account after a new session, retires
    /// all earlier leases. It does not delete other accounts' private records.
    pub fn bind(&self, scope: Option<AccountScope>) {
        if let Ok(mut state) = self.0.state.lock() {
            state.scope = scope;
            state.advance();
        }
    }
    pub fn generation(&self) -> u64 {
        self.0
            .state
            .lock()
            .map_or(u64::MAX, |state| state.generation)
    }
    pub fn lease(&self) -> Option<CacheLease> {
        let state = self.0.state.lock().ok()?;
        if !state.policy.enabled || state.clearing {
            return None;
        }
        Some(CacheLease {
            cache: Arc::clone(&self.0),
            scope: state.scope.clone()?,
            generation: state.generation,
        })
    }
    /// Fast policy update, with no disk access. Call `trim` on a worker after
    /// reducing the budget. Disabled policy performs no entry reads or writes.
    pub fn configure(&self, policy: CachePolicy) -> Result<(), CacheError> {
        let policy = policy.validate()?;
        let mut state = self.state()?;
        if state.policy != policy {
            state.policy = policy;
            state.advance();
        }
        Ok(())
    }
    pub fn policy(&self) -> Result<CachePolicy, CacheError> {
        Ok(self.state()?.policy)
    }
    fn state(&self) -> Result<MutexGuard<'_, State>, CacheError> {
        self.0.state.lock().map_err(|_| CacheError::Unavailable)
    }
    fn check(&self, lease: &CacheLease) -> Result<CachePolicy, CacheError> {
        if !Arc::ptr_eq(&self.0, &lease.cache) {
            return Err(CacheError::Cancelled);
        }
        let state = self.state()?;
        if !state.policy.enabled {
            return Err(CacheError::Disabled);
        }
        if !current(&state, lease) {
            return Err(CacheError::Cancelled);
        }
        Ok(state.policy)
    }
    fn admit(&self) -> Result<MutexGuard<'_, ()>, CacheError> {
        let deadline = Instant::now() + Duration::from_millis(250);
        loop {
            match self.0.io.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(TryLockError::WouldBlock) => return Err(CacheError::Busy),
                Err(TryLockError::Poisoned(_)) => return Err(CacheError::Unavailable),
            }
        }
    }
    fn base(&self, create: bool) -> Result<Option<PathBuf>, CacheError> {
        if !private_directory(&self.0.root, create)? {
            return Ok(None);
        }
        let root = self.0.root.join("v1");
        Ok(private_directory(&root, create)?.then_some(root))
    }
    fn path(
        &self,
        lease: &CacheLease,
        key: &CacheKey,
        create: bool,
    ) -> Result<Option<PathBuf>, CacheError> {
        let Some(base) = self.base(create)? else {
            return Ok(None);
        };
        let scope = base.join(hex(&lease.scope.0));
        if !private_directory(&scope, create)? {
            return Ok(None);
        }
        Ok(Some(scope.join(format!("{}.entry", hex(&key.hash)))))
    }
    /// Worker-only. A stale hit remains explicitly stale; the caller decides
    /// whether a failed network read permits showing it as saved metadata.
    pub fn get(
        &self,
        lease: &CacheLease,
        key: &CacheKey,
    ) -> Result<Option<CacheEntry>, CacheError> {
        self.check(lease)?;
        let _io = self.admit()?;
        self.check(lease)?;
        let Some(path) = self.path(lease, key, false)? else {
            return Ok(None);
        };
        let file = match private_file(&path, false) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => {
                let _ = fs::remove_file(&path);
                return Ok(None);
            }
        };
        let size = file.metadata().map_err(|_| CacheError::Unavailable)?.len();
        if !(HEADER_BYTES as u64..=(HEADER_BYTES + MAX_PAYLOAD_BYTES) as u64).contains(&size) {
            let _ = fs::remove_file(&path);
            return Ok(None);
        }
        let mut record = Vec::with_capacity(size as usize);
        file.take((MAX_PAYLOAD_BYTES + HEADER_BYTES + 1) as u64)
            .read_to_end(&mut record)
            .map_err(|_| CacheError::Unavailable)?;
        let now = self.0.clock.seconds();
        let entry = decode_record(&record, lease, key, now, self.state()?.policy);
        let Ok(entry) = entry else {
            let _ = fs::remove_file(&path);
            return Ok(None);
        };
        // Commit the cache hit/touch only while this lease remains admitted.
        let state = self.state()?;
        if !current(&state, lease) {
            return Err(if state.policy.enabled {
                CacheError::Cancelled
            } else {
                CacheError::Disabled
            });
        }
        if let Ok(file) = private_file(&path, true) {
            let _ = file.set_times(
                fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(now)),
            );
        }
        Ok(Some(entry))
    }
    /// Worker-only. The positive record formats reject credentials, media URLs
    /// and arbitrary binary media before any persistent write is attempted.
    pub fn put(&self, lease: &CacheLease, key: &CacheKey, bytes: &[u8]) -> Result<(), CacheError> {
        let ttl = key.kind.ttl(self.check(lease)?);
        self.put_with_ttl(lease, key, bytes, ttl)
    }
    /// A caller may shorten validity using a server's positive cache policy;
    /// it can never extend the application's configured TTL.
    pub fn put_with_ttl(
        &self,
        lease: &CacheLease,
        key: &CacheKey,
        bytes: &[u8],
        ttl: Duration,
    ) -> Result<(), CacheError> {
        let policy = self.check(lease)?;
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(CacheError::TooLarge);
        }
        if !valid_payload(key, bytes) {
            return Err(CacheError::InvalidData);
        }
        let record_size = (HEADER_BYTES + bytes.len()) as u64;
        if record_size > policy.max_bytes {
            return Err(CacheError::TooLarge);
        }
        let _io = self.admit()?;
        let policy = self.check(lease)?;
        let path = self
            .path(lease, key, true)?
            .ok_or(CacheError::Unavailable)?;
        let parent = path.parent().ok_or(CacheError::Unavailable)?;
        let mut entries = self.scan(true)?;
        entries.retain(|entry| entry.path != path);
        evict(
            &mut entries,
            policy.max_bytes.saturating_sub(record_size),
            MAX_ENTRIES - 1,
        )?;
        let mut random = [0u8; 16];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| CacheError::Unavailable)?;
        let temp = parent.join(format!(".tmp-{}", hex(&random)));
        let guard = Temporary(temp.clone());
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(nofollow_flags())
            .open(&temp)
            .map_err(|_| CacheError::Unavailable)?;
        let now = self.0.clock.seconds();
        let header = encode_header(
            lease,
            key,
            now,
            ttl.min(key.kind.ttl(policy)).as_secs(),
            bytes,
        );
        file.write_all(&header)
            .and_then(|_| file.write_all(bytes))
            .and_then(|_| file.sync_all())
            .map_err(|_| CacheError::Unavailable)?;
        file.set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(now)))
            .map_err(|_| CacheError::Unavailable)?;
        let state = self.state()?;
        if !current(&state, lease) {
            return Err(if state.policy.enabled {
                CacheError::Cancelled
            } else {
                CacheError::Disabled
            });
        }
        fs::rename(&temp, &path).map_err(|_| CacheError::Unavailable)?;
        drop(state);
        drop(guard);
        sync_directory(parent)?;
        Ok(())
    }
    /// Forget one admitted record after a successful noncacheable response.
    pub fn remove(&self, lease: &CacheLease, key: &CacheKey) -> Result<(), CacheError> {
        self.check(lease)?;
        let _io = self.admit()?;
        let Some(path) = self.path(lease, key, false)? else {
            return Ok(());
        };
        let state = self.state()?;
        if !current(&state, lease) {
            return Err(CacheError::Cancelled);
        }
        match fs::remove_file(&path) {
            Ok(()) => sync_directory(path.parent().ok_or(CacheError::Unavailable)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(CacheError::Unavailable),
        }
    }
    /// Explicit maintenance remains available when caching is disabled. This
    /// reads only the app's own cache-file metadata, not cached personal values.
    pub fn usage(&self) -> Result<CacheStats, CacheError> {
        let _io = self.admit()?;
        stats(&self.scan(true)?)
    }
    /// Worker-only size-limit enforcement after a settings change.
    pub fn trim(&self) -> Result<CacheStats, CacheError> {
        let _io = self.admit()?;
        let policy = self.state()?.policy;
        let mut files = self.scan(true)?;
        evict(&mut files, policy.max_bytes, MAX_ENTRIES)?;
        stats(&files)
    }
    /// A clear blocks new leases and invalidates operations both before and
    /// after deletion. A completed old download can never repopulate the cache.
    /// It does not touch settings, credentials, current images or audio workers.
    pub fn clear(&self) -> Result<CacheStats, CacheError> {
        let _clearing = self.begin_clear()?;
        let _io = self.0.io.lock().map_err(|_| CacheError::Unavailable)?;
        let Some(base) = self.base(false)? else {
            return Ok(CacheStats::default());
        };
        fs::remove_dir_all(&base).map_err(|_| CacheError::Unavailable)?;
        sync_directory(&self.0.root)?;
        Ok(CacheStats::default())
    }
    /// Library writes may have an ambiguous server result. Invalidate every
    /// cached membership/content view after any outcome, preserving rasters.
    pub fn invalidate_metadata(&self) -> Result<CacheStats, CacheError> {
        let _clearing = self.begin_clear()?;
        let _io = self.0.io.lock().map_err(|_| CacheError::Unavailable)?;
        let files = self.scan(true)?;
        for entry in files {
            let mut header = [0u8; 9];
            let artwork = private_file(&entry.path, false)
                .and_then(|mut file| file.read_exact(&mut header))
                .is_ok()
                && &header[..8] == MAGIC
                && header[8] == EntryKind::Artwork as u8;
            if !artwork {
                fs::remove_file(entry.path).map_err(|_| CacheError::Unavailable)?;
            }
        }
        stats(&self.scan(false)?)
    }
    fn begin_clear(&self) -> Result<Clearing, CacheError> {
        let mut state = self.state()?;
        if state.clearing {
            return Err(CacheError::Busy);
        }
        state.clearing = true;
        state.advance();
        Ok(Clearing(Arc::clone(&self.0)))
    }
    fn scan(&self, repair: bool) -> Result<Vec<DiskEntry>, CacheError> {
        let Some(base) = self.base(false)? else {
            return Ok(Vec::new());
        };
        let mut entries = Vec::new();
        let mut scopes = 0usize;
        for child in fs::read_dir(&base).map_err(|_| CacheError::Unavailable)? {
            let child = child.map_err(|_| CacheError::Unavailable)?;
            let path = child.path();
            let name = child.file_name();
            let valid_name = name.to_str().is_some_and(is_digest);
            let metadata = fs::symlink_metadata(&path).map_err(|_| CacheError::Unavailable)?;
            if !valid_name || !private_dir_metadata(&metadata) {
                if repair {
                    remove_cache_path(&path)?;
                }
                continue;
            }
            scopes += 1;
            if scopes > MAX_SCOPES {
                return Err(CacheError::TooLarge);
            }
            for file in fs::read_dir(&path).map_err(|_| CacheError::Unavailable)? {
                let file = file.map_err(|_| CacheError::Unavailable)?;
                let path = file.path();
                let name = file.file_name();
                let valid_name = name
                    .to_str()
                    .and_then(|name| name.strip_suffix(".entry"))
                    .is_some_and(is_digest);
                let metadata = fs::symlink_metadata(&path).map_err(|_| CacheError::Unavailable)?;
                if !valid_name
                    || !private_file_metadata(&metadata)
                    || !(HEADER_BYTES as u64..=(HEADER_BYTES + MAX_PAYLOAD_BYTES) as u64)
                        .contains(&metadata.len())
                {
                    if repair {
                        remove_cache_path(&path)?;
                    }
                    continue;
                }
                entries.push(DiskEntry {
                    path,
                    bytes: metadata.len(),
                    touched: metadata
                        .modified()
                        .unwrap_or(UNIX_EPOCH)
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                });
                if entries.len() > MAX_ENTRIES * 2 {
                    return Err(CacheError::TooLarge);
                }
            }
        }
        Ok(entries)
    }
}

fn valid_payload(key: &CacheKey, bytes: &[u8]) -> bool {
    if key.kind == EntryKind::Artwork {
        valid_raster(bytes)
    } else {
        crate::music::cache_codec::validate_for_key(key.kind, &key.selector, bytes)
    }
}
fn valid_raster(bytes: &[u8]) -> bool {
    if bytes.len() < 21 || &bytes[..8] != b"KARINRGB" || bytes[20] > 1 {
        return false;
    }
    let width = u32::from_be_bytes(bytes[8..12].try_into().expect("fixed raster slice"));
    let height = u32::from_be_bytes(bytes[12..16].try_into().expect("fixed raster slice"));
    let stride = u32::from_be_bytes(bytes[16..20].try_into().expect("fixed raster slice"));
    if !(1..=512).contains(&width) || !(1..=512).contains(&height) {
        return false;
    }
    let channels = if bytes[20] == 1 { 4 } else { 3 };
    stride == width * channels && bytes.len() == 21 + (stride * height) as usize
}
fn hash(bytes: &[u8]) -> [u8; 32] {
    digest(&SHA256, bytes)
        .as_ref()
        .try_into()
        .expect("SHA256 length")
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn nofollow_flags() -> i32 {
    // Open first, then inspect that exact descriptor. A corrupt FIFO must not
    // block in open before the regular-file check can reject it. NONBLOCK has
    // no effect on the admitted ordinary cache files.
    (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC | rustix::fs::OFlags::NONBLOCK)
        .bits() as i32
}
fn owner() -> u32 {
    rustix::process::geteuid().as_raw()
}
fn private_dir_metadata(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_dir()
        && metadata.uid() == owner()
        && metadata.permissions().mode() & 0o777 == 0o700
}
fn private_file_metadata(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_file()
        && metadata.uid() == owner()
        && metadata.nlink() == 1
        && metadata.permissions().mode() & 0o777 == 0o600
}
fn private_directory(path: &Path, create: bool) -> Result<bool, CacheError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if private_dir_metadata(&metadata) {
                Ok(true)
            } else {
                Err(CacheError::Unavailable)
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !create => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(path)
                .map_err(|_| CacheError::Unavailable)?;
            if private_dir_metadata(
                &fs::symlink_metadata(path).map_err(|_| CacheError::Unavailable)?,
            ) {
                Ok(true)
            } else {
                Err(CacheError::Unavailable)
            }
        }
        Err(_) => Err(CacheError::Unavailable),
    }
}
fn private_file(path: &Path, writable: bool) -> std::io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(writable)
        .custom_flags(nofollow_flags())
        .open(path)?;
    if !private_file_metadata(&file.metadata()?) {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
    }
    Ok(file)
}
fn sync_directory(path: &Path) -> Result<(), CacheError> {
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(nofollow_flags() | rustix::fs::OFlags::DIRECTORY.bits() as i32)
        .open(path)
        .map_err(|_| CacheError::Unavailable)?;
    if !private_dir_metadata(&directory.metadata().map_err(|_| CacheError::Unavailable)?) {
        return Err(CacheError::Unavailable);
    }
    directory.sync_all().map_err(|_| CacheError::Unavailable)
}
fn remove_cache_path(path: &Path) -> Result<(), CacheError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| CacheError::Unavailable)?;
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
    .map_err(|_| CacheError::Unavailable)
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
struct Clearing(Arc<Inner>);
impl Drop for Clearing {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.clearing = false;
            state.advance();
        }
    }
}
struct DiskEntry {
    path: PathBuf,
    bytes: u64,
    touched: u64,
}
fn stats(entries: &[DiskEntry]) -> Result<CacheStats, CacheError> {
    let bytes = entries
        .iter()
        .try_fold(0u64, |total, entry| total.checked_add(entry.bytes))
        .ok_or(CacheError::TooLarge)?;
    Ok(CacheStats {
        bytes,
        entries: entries.len(),
    })
}
fn evict(entries: &mut Vec<DiskEntry>, limit: u64, count: usize) -> Result<(), CacheError> {
    entries.sort_by(|a, b| a.touched.cmp(&b.touched).then_with(|| a.path.cmp(&b.path)));
    let mut bytes = stats(entries)?.bytes;
    let mut removed = 0;
    while removed < entries.len() && (bytes > limit || entries.len() - removed > count) {
        fs::remove_file(&entries[removed].path).map_err(|_| CacheError::Unavailable)?;
        bytes = bytes.saturating_sub(entries[removed].bytes);
        removed += 1;
    }
    entries.drain(..removed);
    Ok(())
}
fn encode_header(
    lease: &CacheLease,
    key: &CacheKey,
    now: u64,
    ttl: u64,
    payload: &[u8],
) -> [u8; HEADER_BYTES] {
    let mut header = [0u8; HEADER_BYTES];
    header[..8].copy_from_slice(MAGIC);
    header[8] = key.kind as u8;
    header[16..24].copy_from_slice(&now.to_be_bytes());
    header[24..32].copy_from_slice(&now.to_be_bytes());
    header[32..40].copy_from_slice(&ttl.to_be_bytes());
    header[40..48].copy_from_slice(&(payload.len() as u64).to_be_bytes());
    header[80..112].copy_from_slice(&lease.scope.0);
    header[112..144].copy_from_slice(&key.hash);
    let checksum = record_hash(&header, payload);
    header[48..80].copy_from_slice(&checksum);
    header
}
fn decode_record(
    bytes: &[u8],
    lease: &CacheLease,
    key: &CacheKey,
    now: u64,
    policy: CachePolicy,
) -> Result<CacheEntry, CacheError> {
    if bytes.len() < HEADER_BYTES
        || &bytes[..8] != MAGIC
        || EntryKind::from_byte(bytes[8]) != Some(key.kind)
        || bytes[9..16].iter().any(|b| *b != 0)
        || bytes[80..112] != lease.scope.0
        || bytes[112..144] != key.hash
    {
        return Err(CacheError::InvalidData);
    }
    let number = |start: usize| {
        u64::from_be_bytes(bytes[start..start + 8].try_into().expect("bounded header"))
    };
    let written = number(16);
    let ttl = number(32);
    let length = number(40);
    let payload = &bytes[HEADER_BYTES..];
    if length != payload.len() as u64
        || ttl > MAX_TTL_SECONDS
        || record_hash(&bytes[..HEADER_BYTES], payload) != bytes[48..80]
        || !valid_payload(key, payload)
    {
        return Err(CacheError::InvalidData);
    }
    let age = now.saturating_sub(written);
    let ttl = ttl.min(key.kind.ttl(policy).as_secs());
    Ok(CacheEntry {
        bytes: payload.to_vec(),
        freshness: if now >= written && age < ttl {
            Freshness::Fresh
        } else {
            Freshness::Stale
        },
        age: Duration::from_secs(age),
    })
}
fn record_hash(header: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut context = ring::digest::Context::new(&SHA256);
    context.update(&header[..48]);
    context.update(&header[80..HEADER_BYTES]);
    context.update(payload);
    context.finish().as_ref().try_into().expect("SHA256 length")
}

#[cfg(test)]
mod tests;
