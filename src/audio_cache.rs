// SPDX-License-Identifier: GPL-3.0-or-later
//! Account-isolated, policy-admitted audio storage used by the ordinary app.
//!
//! All filesystem operations run on workers. Binding/fencing are memory-only.
//! Current playback access, HTTP response policy, representation revalidation
//! and independent content constraints are separate decisions. Private HTTP
//! candidates are not provider entitlements. No protected formats are handled.

use ring::{
    digest::{SHA256, digest},
    rand::{SecureRandom, SystemRandom},
};
use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, Weak,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

#[cfg(test)]
use std::os::unix::fs::PermissionsExt;

const MAX_AUDIO: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 1024;
const MAX_BUDGET: u64 = 4 * 1024 * 1024 * 1024;
const MAX_REQUESTS: usize = 4;
const HEADER: usize = 112;
// The bounded HTTP sidecar is included in every reservation and measured quota.
const MAX_HTTP_METADATA: u64 = 8 * 1024;
const MAGIC: &[u8; 8] = b"KARNADC2";
type Key = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CacheError {
    #[error("Audio cache input is invalid")]
    InvalidInput,
    #[error("Audio cache storage is unavailable")]
    Unavailable,
    #[error("Audio cache storage is busy")]
    Busy,
    #[error("Audio cache operation was cancelled")]
    Cancelled,
    #[error("Audio cache request is already in flight")]
    Duplicate,
    #[error("Audio cache limit was reached")]
    Limit,
    #[error("No reviewed service storage/reuse contract is available")]
    NoGrant,
    #[error("Storage permission has expired")]
    Expired,
    #[error("Storage permission was revoked")]
    Revoked,
    #[error("Storage permission requires an online check")]
    NeedsOnline,
    #[error("Private HTTP caching requires validated response policy")]
    HttpPolicyRequired,
}
pub type Result<T> = std::result::Result<T, CacheError>;

/// Local storage identity. Only hashes are retained, never a token or URL.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ResourceKey(Key);
impl ResourceKey {
    pub fn track(value: &str) -> Result<Self> {
        crate::music::validate_id(value).map_err(|_| CacheError::InvalidInput)?;
        let mut bytes = b"Karin reusable full MP3 resource v2\0".to_vec();
        bytes.extend_from_slice(value.as_bytes());
        Ok(Self(hash(&bytes)))
    }
}
impl From<&ResourceKey> for ResourceKey {
    fn from(value: &ResourceKey) -> Self {
        *value
    }
}
impl fmt::Debug for ResourceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AudioResource([opaque])")
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ResourceIdentity {
    account: Key,
    // Local disk namespace discriminator; not a provider device identity.
    namespace: u64,
    track: ResourceKey,
    // Process/lease/budget fence; deliberately excluded from the disk key.
    admission: Key,
}
impl ResourceIdentity {
    fn key(self) -> Key {
        let mut bytes = b"Karin audio cache resource v2\0".to_vec();
        bytes.extend_from_slice(&self.account);
        bytes.extend_from_slice(&self.namespace.to_be_bytes());
        bytes.extend_from_slice(&self.track.0);
        hash(&bytes)
    }
    fn scope(self) -> Key {
        self.account
    }
}
impl fmt::Debug for ResourceIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AudioIdentity([opaque])")
    }
}
/// The local reason for considering a resource for the cache. HTTP candidacy
/// still requires the actual response and current representation to be checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionBasis {
    IndependentLicense,
    // A local storage candidate, never a claim of provider entitlement. An
    // actual cacheable response and revalidation still gate write and reuse.
    PrivateHttpCandidate,
}
#[derive(Clone, Copy)]
/// Opaque local admission revision. This is neither a provider entitlement nor
/// an offline lease; its basis determines which additional checks are required.
pub struct StorageGrant {
    identity: ResourceIdentity,
    revision: u64,
    // An independent exact-content license can admit only these media bytes.
    content: Option<ContentClaim>,
    basis: AdmissionBasis,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContentClaim {
    sha256: Key,
    bytes: u64,
}
impl ContentClaim {
    pub(crate) fn length(self) -> u64 {
        self.bytes
    }
    fn matches(self, bytes: u64, sha256: &[u8]) -> bool {
        self.bytes == bytes && self.sha256.as_slice() == sha256
    }
}
impl StorageGrant {
    /// Authority-owned opaque generation, not a retention deadline or license.
    pub fn revision(&self) -> u64 {
        self.revision
    }
}
impl fmt::Debug for StorageGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AudioStorageGrant([opaque])")
    }
}
/// Preparation can perform policy work on a worker. check only examines an
/// established live admission/revocation lease, never does I/O on GTK. Private
/// HTTP callers separately validate response storage and current representation;
/// stored bytes cannot supply the current user's playback authorization.
pub trait StorageAuthority: Send + Sync {
    fn authorize(&self, identity: &ResourceIdentity) -> Result<StorageGrant>;
    fn check(&self, grant: &StorageGrant) -> Result<()>;
    /// A bounded, request-owned write operation, separate from admission of a
    /// new playback. Only a complete, actually played capture can be sealed.
    #[doc(hidden)]
    fn retain_write(&self, grant: &StorageGrant) -> Result<()> {
        self.check(grant)
    }
    #[doc(hidden)]
    fn seal_write(&self, grant: &StorageGrant) -> Result<()> {
        self.check(grant)
    }
    #[doc(hidden)]
    fn check_write(&self, grant: &StorageGrant) -> Result<()> {
        self.check(grant)
    }
    #[doc(hidden)]
    fn release_write(&self, _: &StorageGrant) {}
    #[doc(hidden)]
    fn revoke_writes(&self, _: &ResourceIdentity, _: u64) {}
    /// Whether callers must validate actual HTTP response/reuse policy. This
    /// flag does not grant a provider content license or permission to play.
    fn private_http(&self) -> bool {
        false
    }
    /// Memory-only start: retire earlier online admission before network I/O.
    #[doc(hidden)]
    fn begin_online(&self, identity: ResourceIdentity) -> Result<OnlineCheck> {
        Ok(OnlineCheck {
            identity,
            sequence: 0,
        })
    }
    /// Permission lookup/revalidation may do I/O; called only on a worker.
    #[doc(hidden)]
    fn prepare_online(&self, check: OnlineCheck) -> Result<VerifiedOnline> {
        let permission = self.authorize(&check.identity)?;
        if permission.identity != check.identity {
            return Err(CacheError::Cancelled);
        }
        self.check(&permission)?;
        Ok(VerifiedOnline { check, permission })
    }
    /// Memory-only commit, serialized with account/clear fences.
    #[doc(hidden)]
    fn commit_online(&self, prepared: VerifiedOnline) -> Result<()> {
        self.check(&prepared.permission)
    }
    /// Memory-only validation of the independently prepared storage permit;
    /// this is not yet permission to play a cached file without online admission.
    #[doc(hidden)]
    fn check_storage_prepared(&self, prepared: &VerifiedOnline) -> Result<()> {
        self.check(&prepared.permission)
    }
}
/// Opaque one-use online attempt; not persisted or a storage permission.
#[doc(hidden)]
pub struct OnlineCheck {
    identity: ResourceIdentity,
    sequence: u64,
}
#[doc(hidden)]
pub struct VerifiedOnline {
    check: OnlineCheck,
    permission: StorageGrant,
}
/// The current service does not establish a Karin-applicable storage contract.
/// There is no user flag or metadata/subscription shortcut around this gate.
pub struct MissingStorageContract;
impl StorageAuthority for MissingStorageContract {
    fn authorize(&self, _: &ResourceIdentity) -> Result<StorageGrant> {
        Err(CacheError::NoGrant)
    }
    fn check(&self, _: &StorageGrant) -> Result<()> {
        Err(CacheError::NoGrant)
    }
    fn begin_online(&self, _: ResourceIdentity) -> Result<OnlineCheck> {
        Err(CacheError::NoGrant)
    }
}

pub(crate) mod diagnostic;
mod http_admission;
pub(crate) mod http_policy;
pub mod licensed;
pub use http_admission::PrivateHttpCachePolicy;
mod online;
pub use online::OnlineStreamAuthority;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachePolicy {
    pub max_bytes: u64,
}
impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            max_bytes: 256 * 1024 * 1024,
        }
    }
}
impl CachePolicy {
    fn validate(self) -> Result<()> {
        if self.max_bytes == 0 || self.max_bytes > MAX_BUDGET {
            Err(CacheError::InvalidInput)
        } else {
            Ok(())
        }
    }
}

/// Private namespace. The app supplies its scoped XDG audio-cache directory.
struct Sandbox {
    path: PathBuf,
    remove_on_drop: bool,
}
impl Sandbox {
    fn persistent(path: PathBuf) -> Result<Arc<Self>> {
        if !path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(CacheError::InvalidInput);
        }
        create_private_tree(&path)?;
        Ok(Arc::new(Self {
            path,
            remove_on_drop: false,
        }))
    }
    #[cfg(test)]
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
    #[cfg(test)]
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
    account: Option<Key>,
    generation: u64,
    clearing: bool,
    pending: BTreeMap<Key, Arc<()>>,
    sequence: u64,
    active_stage: Option<PathBuf>,
    retirement: diagnostic::Cause,
}
struct Inner {
    sandbox: Arc<Sandbox>,
    authority: Arc<dyn StorageAuthority>,
    namespace: u64,
    budget: AtomicU64,
    state: Mutex<State>,
    io: Mutex<()>,
    writer: Mutex<()>,
    // Held for the complete namespace-owner lifetime. Separate instances must
    // not retain independent generations for the same directory.
    _lock_file: File,
    owner_pid: u32,
    admission_nonce: Key,
    lookup_running: Arc<AtomicBool>,
    shared: Option<Arc<crate::cache_budget::CacheBudget>>,
    #[cfg(test)]
    commit_gate: Mutex<Option<(ResourceKey, Arc<CommitGate>)>>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        // flock belongs to an open file description, not to this File value.
        // A concurrently forked child can retain that description until exec,
        // even with CLOEXEC. End ownership explicitly when the final cache Arc
        // drops, rather than waiting for every inherited descriptor to close.
        // Live clones/workers keep Inner alive; Weak Hit/Lease/Request do not.
        if self.owner_pid == std::process::id() {
            let _ = rustix::fs::flock(&self._lock_file, rustix::fs::FlockOperation::Unlock);
        }
    }
}
#[derive(Clone)]
pub struct AudioCache(Arc<Inner>);
#[derive(Clone)]
pub struct Lease {
    owner: Weak<Inner>,
    account: Key,
    generation: u64,
    budget_generation: u64,
}
impl Lease {
    pub fn matches_scope(&self, scope: &crate::cache::AccountScope) -> bool {
        self.account == scope.audio_cache_key()
    }
}
pub struct Request {
    lease: Lease,
    permit: StorageGrant,
    nonce: Arc<()>,
    cancel: Arc<AtomicBool>,
    write: Option<Arc<WriteOperation>>,
}
struct WriteOperation {
    capture: capture::Capture,
    sealed: AtomicBool,
    phase: AtomicU8,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
}
/// Deterministic test-only ordering at the completed-body/publication boundary.
#[cfg(test)]
pub(crate) struct CommitGate {
    entered: AtomicBool,
    released: Mutex<bool>,
    changed: std::sync::Condvar,
}
#[cfg(test)]
impl CommitGate {
    pub(crate) fn entered(&self) -> bool {
        self.entered.load(Ordering::Acquire)
    }
    pub(crate) fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }
    fn wait(&self) -> Result<()> {
        self.entered.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(25);
        let mut released = self.released.lock().map_err(|_| CacheError::Unavailable)?;
        while !*released {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(CacheError::Busy)?;
            let (next, timeout) = self
                .changed
                .wait_timeout(released, remaining)
                .map_err(|_| CacheError::Unavailable)?;
            released = next;
            if timeout.timed_out() && !*released {
                return Err(CacheError::Busy);
            }
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
impl Request {
    pub fn cancellation(&self) -> Cancellation {
        Cancellation(self.cancel.clone())
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        let Some(inner) = self.lease.owner.upgrade() else {
            return;
        };
        if self.write.is_some() {
            inner.authority.release_write(&self.permit);
        }
        if let Ok(mut state) = inner.state.lock()
            && state
                .pending
                .get(&self.permit.identity.key())
                .is_some_and(|nonce| Arc::ptr_eq(nonce, &self.nonce))
        {
            state.pending.remove(&self.permit.identity.key());
        }
    }
}
#[derive(Clone)]
pub struct Hit {
    owner: Weak<Inner>,
    lease: Lease,
    permit: StorageGrant,
    file: Arc<File>,
}
impl Hit {
    /// Return the same read-only, integrity-checked inode, not a reopened path.
    /// An already admitted FD survives Clear; old unadmitted handles are fenced.
    pub fn admit_file(&self) -> Result<Arc<File>> {
        let cache = AudioCache(self.owner.upgrade().ok_or(CacheError::Cancelled)?);
        cache.check(&self.lease, self.permit)?;
        Ok(self.file.clone())
    }
    #[cfg(test)]
    fn admit(&self) -> Result<Arc<[u8]>> {
        use std::os::unix::fs::FileExt;
        let file = self.admit_file()?;
        let length = file.metadata().map_err(|_| CacheError::Unavailable)?.len();
        let mut bytes = vec![0; length as usize];
        file.read_exact_at(&mut bytes, 0)
            .map_err(|_| CacheError::Unavailable)?;
        Ok(bytes.into())
    }
}
impl fmt::Debug for Hit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AudioCacheHit([opaque])")
    }
}
struct DiskEntry {
    path: PathBuf,
    key: Key,
    bytes: u64,
    access: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub bytes: u64,
    pub entries: usize,
}
struct IoGuard<'a> {
    _local: MutexGuard<'a, ()>,
}
struct PartialRecord {
    path: PathBuf,
    owner: Weak<Inner>,
}
impl Drop for PartialRecord {
    fn drop(&mut self) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        if owner.owner_pid != std::process::id() {
            return;
        }
        // Serialize path removal with Clear/scans, but never make teardown wait
        // for another disk operation. A contended leftover becomes an orphan
        // for the next scan; the shared budget still counts its actual bytes.
        if let Ok(_io) = owner.io.try_lock() {
            let _ = remove_record(&self.path);
        }
        if let Ok(mut state) = owner.state.lock()
            && state.active_stage.as_ref() == Some(&self.path)
        {
            state.active_stage = None;
        }
    }
}

impl AudioCache {
    #[cfg(test)]
    pub(crate) fn set_commit_gate_for_test(&self, key: ResourceKey) -> Arc<CommitGate> {
        let gate = Arc::new(CommitGate {
            entered: AtomicBool::new(false),
            released: Mutex::new(false),
            changed: std::sync::Condvar::new(),
        });
        *self.0.commit_gate.lock().unwrap() = Some((key, gate.clone()));
        gate
    }
    #[cfg(test)]
    fn wait_commit_gate_for_test(&self, key: ResourceKey) -> Result<()> {
        let gate = {
            let mut slot = self
                .0
                .commit_gate
                .lock()
                .map_err(|_| CacheError::Unavailable)?;
            if slot.as_ref().is_some_and(|(target, _)| *target == key) {
                slot.take().map(|(_, gate)| gate)
            } else {
                None
            }
        };
        if let Some(gate) = gate {
            gate.wait()?;
        }
        Ok(())
    }
    pub fn open(
        path: PathBuf,
        authority: Arc<dyn StorageAuthority>,
        policy: CachePolicy,
    ) -> Result<Self> {
        policy.validate()?;
        Self::open_namespace(Sandbox::persistent(path)?, authority, 1, policy.max_bytes)
    }
    /// Use the one application-wide quota rather than an independent audio cap.
    pub fn open_with_budget(
        path: PathBuf,
        authority: Arc<dyn StorageAuthority>,
        policy: CachePolicy,
        budget: Arc<crate::cache_budget::CacheBudget>,
    ) -> Result<Self> {
        policy.validate()?;
        Self::open_namespace_with_budget(
            Sandbox::persistent(path)?,
            authority,
            1,
            policy.max_bytes,
            Some(budget),
        )
    }
    fn open_namespace(
        sandbox: Arc<Sandbox>,
        authority: Arc<dyn StorageAuthority>,
        namespace: u64,
        budget: u64,
    ) -> Result<Self> {
        Self::open_namespace_with_budget(sandbox, authority, namespace, budget, None)
    }
    fn open_namespace_with_budget(
        sandbox: Arc<Sandbox>,
        authority: Arc<dyn StorageAuthority>,
        namespace: u64,
        budget: u64,
        shared: Option<Arc<crate::cache_budget::CacheBudget>>,
    ) -> Result<Self> {
        if namespace == 0 || budget == 0 || budget > MAX_BUDGET {
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
        let mut admission_nonce = [0; 32];
        SystemRandom::new()
            .fill(&mut admission_nonce)
            .map_err(|_| CacheError::Unavailable)?;
        let cache = Self(Arc::new(Inner {
            sandbox,
            authority,
            namespace,
            budget: AtomicU64::new(budget),
            state: Mutex::new(State {
                account: None,
                generation: 1,
                clearing: false,
                pending: BTreeMap::new(),
                sequence: 0,
                active_stage: None,
                retirement: diagnostic::Cause::PolicyChanged,
            }),
            io: Mutex::new(()),
            writer: Mutex::new(()),
            _lock_file: lock_file,
            owner_pid: std::process::id(),
            admission_nonce,
            lookup_running: Arc::new(AtomicBool::new(false)),
            shared,
            #[cfg(test)]
            commit_gate: Mutex::new(None),
        }));
        {
            let _io = cache.io()?;
            let entries = cache.scan()?;
            cache.0.state.lock().unwrap().sequence =
                entries.iter().map(|e| e.access).max().unwrap_or(0);
            cache.evict(entries, 0, None)?;
        }
        if let Some(budget) = &cache.0.shared {
            budget.trim().map_err(budget_error)?;
        }
        Ok(cache)
    }
    pub fn bind(&self, account: Option<crate::cache::AccountScope>) -> Result<Option<Lease>> {
        let account = account.map(|scope| scope.audio_cache_key());
        let mut state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
        state.generation = state.generation.checked_add(1).ok_or(CacheError::Limit)?;
        state.account = account;
        state.retirement = diagnostic::Cause::AccountChanged;
        state.pending.clear();
        Ok(account.map(|account| Lease {
            owner: Arc::downgrade(&self.0),
            account,
            generation: state.generation,
            budget_generation: self
                .0
                .shared
                .as_ref()
                .map_or(0, |budget| budget.generation()),
        }))
    }
    pub fn lease(&self) -> Option<Lease> {
        let state = self.0.state.lock().ok()?;
        if state.clearing {
            return None;
        }
        Some(Lease {
            owner: Arc::downgrade(&self.0),
            account: state.account?,
            generation: state.generation,
            budget_generation: self
                .0
                .shared
                .as_ref()
                .map_or(0, |budget| budget.generation()),
        })
    }
    /// Start before resolving: failed, denied and cancelled resolutions cannot
    /// leave a previous online admission usable. No network or disk I/O here.
    pub(crate) fn begin_online_check(
        &self,
        lease: &Lease,
        track: ResourceKey,
    ) -> Result<OnlineCheck> {
        let state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
        self.lease_current_locked(lease, &state)?;
        self.0.authority.begin_online(self.identity(lease, track)?)
    }
    /// Independently revalidate storage rights on a worker, then commit only if
    /// the exact lease and attempt remain current. Never hold GTK's state lock
    /// across permission lookup/network I/O.
    #[cfg(test)]
    pub(crate) fn observe_verified_online(&self, lease: &Lease, check: OnlineCheck) -> Result<()> {
        self.observe_verified_online_if(lease, check, || true)
    }
    #[cfg(test)]
    pub(crate) fn observe_verified_online_if(
        &self,
        lease: &Lease,
        check: OnlineCheck,
        current: impl Fn() -> bool,
    ) -> Result<()> {
        self.lease_current(lease)?;
        if !current() {
            return Err(CacheError::Cancelled);
        }
        let prepared = self.0.authority.prepare_online(check)?;
        let state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
        self.lease_current_locked(lease, &state)?;
        // Cancellation/token expiry during permission I/O must not commit even
        // an unused cache admission. The predicate reads atomics/time only.
        if !current() {
            return Err(CacheError::Cancelled);
        }
        self.0.authority.commit_online(prepared)
    }

    pub fn fence(&self) -> Result<()> {
        let mut state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
        state.generation = state.generation.checked_add(1).ok_or(CacheError::Limit)?;
        state.clearing = true;
        state.retirement = diagnostic::Cause::CacheCleared;
        state.pending.clear();
        Ok(())
    }
    pub fn configure(&self, policy: CachePolicy) -> Result<()> {
        policy.validate()?;
        let mut state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
        if self.0.budget.load(Ordering::Acquire) == policy.max_bytes {
            return Ok(());
        }
        state.generation = state.generation.checked_add(1).ok_or(CacheError::Limit)?;
        state.pending.clear();
        state.retirement = diagnostic::Cause::PolicyChanged;
        self.0.budget.store(policy.max_bytes, Ordering::Release);
        // A budget change cannot clear a failed/in-flight cleanup fence.
        Ok(())
    }
    pub fn trim(&self) -> Result<Stats> {
        let _io = self.io()?;
        // Recover retired staging before the shared budget chooses victims;
        // counted orphans must not evict otherwise useful completed media.
        let entries = self.scan()?;
        if let Some(budget) = &self.0.shared {
            budget.trim().map_err(budget_error)?;
        } else {
            self.evict(entries, 0, None)?;
        }
        let entries = self.scan()?;
        Ok(Stats {
            bytes: entries.iter().map(|e| e.bytes).sum(),
            entries: entries.len(),
        })
    }
    fn lease_current(&self, lease: &Lease) -> Result<()> {
        let state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
        self.lease_current_locked(lease, &state)
    }
    fn lease_current_locked(&self, lease: &Lease, state: &State) -> Result<()> {
        if self.0.owner_pid != std::process::id()
            || self.0.shared.as_ref().is_some_and(|budget| {
                !budget.enabled() || budget.generation() != lease.budget_generation
            })
            || !lease.owner.ptr_eq(&Arc::downgrade(&self.0))
            || state.clearing
            || state.generation != lease.generation
            || state.account != Some(lease.account)
        {
            return Err(CacheError::Cancelled);
        }
        Ok(())
    }
    fn check(&self, lease: &Lease, permit: StorageGrant) -> Result<()> {
        self.lease_current(lease)?;
        if permit.identity != self.identity(lease, permit.identity.track)? {
            return Err(CacheError::Cancelled);
        }
        self.0.authority.check(&permit)
    }
    fn identity(&self, lease: &Lease, track: ResourceKey) -> Result<ResourceIdentity> {
        let mut context = self.0.admission_nonce.to_vec();
        context.extend_from_slice(&lease.generation.to_be_bytes());
        context.extend_from_slice(&lease.budget_generation.to_be_bytes());
        Ok(ResourceIdentity {
            account: lease.account,
            namespace: self.0.namespace,
            track,
            admission: hash(&context),
        })
    }
    pub fn begin(&self, lease: &Lease, track: impl Into<ResourceKey>) -> Result<Request> {
        self.lease_current(lease)?;
        let identity = self.identity(lease, track.into())?;
        let permit = self.0.authority.authorize(&identity)?;
        if permit.identity != identity {
            return Err(CacheError::Cancelled);
        }
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
            write: None,
        })
    }
    pub(crate) fn check_request(&self, request: &Request) -> Result<()> {
        self.request_current(request)
    }
    fn request_current(&self, request: &Request) -> Result<()> {
        if request.cancel.load(Ordering::Acquire) {
            return Err(CacheError::Cancelled);
        }
        self.check_write_operation(&request.lease, request.permit, request.write.as_deref())
    }
    fn retain_capture(
        &self,
        request: &mut Request,
        capture: capture::Capture,
        current: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<()> {
        self.request_current(request)?;
        self.0.authority.retain_write(&request.permit)?;
        request.write = Some(Arc::new(WriteOperation {
            capture,
            sealed: AtomicBool::new(false),
            phase: AtomicU8::new(0),
            current: Arc::new(current),
        }));
        Ok(())
    }
    fn check_write_operation(
        &self,
        lease: &Lease,
        permit: StorageGrant,
        write: Option<&WriteOperation>,
    ) -> Result<()> {
        self.lease_current(lease)?;
        if permit.identity != self.identity(lease, permit.identity.track)? {
            return Err(CacheError::Cancelled);
        }
        let Some(write) = write else {
            return self.0.authority.check(&permit);
        };
        if !(write.current)() {
            return Err(CacheError::Cancelled);
        }
        if !write.sealed.load(Ordering::Acquire)
            && write.capture.is_complete()
            && write.capture.has_played()
        {
            self.0.authority.seal_write(&permit)?;
            write.sealed.store(true, Ordering::Release);
        }
        self.0.authority.check_write(&permit)
    }
    fn revoke_resource_writes(&self, lease: &Lease, identity: ResourceIdentity, sequence: u64) {
        if self.lease_current(lease).is_ok()
            && self.identity(lease, identity.track).ok() == Some(identity)
        {
            self.0.authority.revoke_writes(&identity, sequence);
        }
    }
    fn retired_lease_cause(&self, lease: &Lease) -> Option<diagnostic::Cause> {
        let state = self.0.state.lock().ok()?;
        if state.account != Some(lease.account) {
            return Some(diagnostic::Cause::AccountChanged);
        }
        if self
            .0
            .shared
            .as_ref()
            .is_some_and(|budget| !budget.enabled())
        {
            return Some(diagnostic::Cause::CacheDisabled);
        }
        if state.clearing {
            return Some(diagnostic::Cause::CacheCleared);
        }
        if state.generation != lease.generation {
            return Some(state.retirement);
        }
        if self
            .0
            .shared
            .as_ref()
            .is_some_and(|budget| budget.generation() != lease.budget_generation)
        {
            return Some(diagnostic::Cause::PolicyChanged);
        }
        None
    }
    pub fn put_reader(&self, request: Request, reader: impl Read) -> Result<()> {
        self.put_reader_before_commit(request, reader, || Ok(()))
    }
    fn put_reader_before_commit(
        &self,
        request: Request,
        reader: impl Read,
        before_commit: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        if request.permit.basis == AdmissionBasis::PrivateHttpCandidate
            || self.0.authority.private_http()
        {
            return Err(CacheError::HttpPolicyRequired);
        }
        self.put_reader_complete(request, reader, before_commit, |_, _| Ok(None), 0)
    }
    /// Publish the media and response-policy metadata in the same transaction.
    /// The callback consumes only the completed native response policy; it must
    /// reject a failed/partial capture and bind the measured SHA and length.
    pub(crate) fn put_reader_http(
        &self,
        request: Request,
        reader: impl Read,
        http: impl FnOnce([u8; 32], u64) -> Result<http_policy::Metadata>,
    ) -> Result<()> {
        self.put_reader_complete(
            request,
            reader,
            || Ok(()),
            |sha256, bytes| http(sha256, bytes).map(Some),
            MAX_HTTP_METADATA,
        )
    }
    fn put_reader_complete(
        &self,
        request: Request,
        mut reader: impl Read,
        before_commit: impl FnOnce() -> Result<()>,
        http: impl FnOnce([u8; 32], u64) -> Result<Option<http_policy::Metadata>>,
        metadata_reserve: u64,
    ) -> Result<()> {
        self.request_current(&request)?;
        // One bounded staging writer, without occupying the read/Clear gate
        // while the captured source is paused or waiting for network bytes.
        let _writer = match self.0.writer.try_lock() {
            Ok(writer) => writer,
            Err(std::sync::TryLockError::WouldBlock) => return Err(CacheError::Busy),
            Err(std::sync::TryLockError::Poisoned(_)) => return Err(CacheError::Unavailable),
        };
        let key = request.permit.identity.key();
        let (temporary, reservation, output) = {
            // Stats/LRU/Clear use this short gate too. A transient observer
            // must not discard a valid capture; retry only on this worker,
            // with the same bounded deadline and cancellation as publication.
            let _io = self.commit_io(&request)?;
            self.request_current(&request)?;
            let reservation = self
                .0
                .shared
                .as_ref()
                .map(|budget| {
                    budget
                        .reserve(
                            crate::cache_budget::Category::Audio,
                            &self.path(key),
                            HEADER as u64
                                + metadata_reserve
                                + request
                                    .permit
                                    .content
                                    .map_or(MAX_AUDIO as u64, ContentClaim::length),
                        )
                        .map_err(budget_error)
                })
                .transpose()?;
            let temporary = self
                .0
                .sandbox
                .path
                .join(format!(".part-{}", random_name()?));
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&temporary)
                .map_err(|_| CacheError::Unavailable)?;
            let output = match new_file(&temporary.join("media.bin")) {
                Ok(output) => output,
                Err(error) => {
                    let _ = remove_record(&temporary);
                    return Err(error);
                }
            };
            let registered = self
                .0
                .state
                .lock()
                .map(|mut state| state.active_stage = Some(temporary.clone()))
                .is_ok();
            if !registered {
                let _ = remove_record(&temporary);
                return Err(CacheError::Unavailable);
            }
            (temporary, reservation, output)
        };
        let guard = PartialRecord {
            path: temporary.clone(),
            owner: Arc::downgrade(&self.0),
        };
        self.request_current(&request)?;
        let mut length = 0usize;
        let mut content = ring::digest::Context::new(&SHA256);
        {
            let mut output = output;
            let mut chunk = [0; 16 * 1024];
            loop {
                self.request_current(&request)?;
                let read = reader.read(&mut chunk);
                // A retiring capture reader may return an I/O error before
                // any new bytes. Preserve the actual lease/cancellation reason.
                self.request_current(&request)?;
                let read = read.map_err(|_| CacheError::Unavailable)?;
                if read == 0 {
                    break;
                }
                length = length.checked_add(read).ok_or(CacheError::Limit)?;
                if request
                    .permit
                    .content
                    .is_some_and(|expected| length as u64 > expected.length())
                {
                    return Err(CacheError::NoGrant);
                }
                if length > MAX_AUDIO
                    || (self.0.shared.is_none()
                        && (HEADER + length) as u64 + metadata_reserve
                            > self.0.budget.load(Ordering::Acquire))
                {
                    return Err(CacheError::Limit);
                }
                output
                    .write_all(&chunk[..read])
                    .map_err(|_| CacheError::Unavailable)?;
                content.update(&chunk[..read]);
            }
            if length == 0 {
                return Err(CacheError::InvalidInput);
            }
            if let Some(write) = &request.write {
                write.phase.store(1, Ordering::Release);
                diagnostic::report(
                    diagnostic::Event::Captured,
                    diagnostic::Cause::CaptureComplete,
                );
            }
            output.sync_all().map_err(|_| CacheError::Unavailable)?;
        }
        #[cfg(test)]
        self.wait_commit_gate_for_test(request.permit.identity.track)?;
        let _io = self.commit_io(&request)?;
        let measured_digest = content.finish();
        if request
            .permit
            .content
            .is_some_and(|claim| !claim.matches(length as u64, measured_digest.as_ref()))
        {
            return Err(CacheError::NoGrant);
        }
        let measured_sha: [u8; 32] = measured_digest
            .as_ref()
            .try_into()
            .map_err(|_| CacheError::Unavailable)?;
        let http = http(measured_sha, length as u64)?;
        if request.permit.basis == AdmissionBasis::PrivateHttpCandidate && http.is_none() {
            return Err(CacheError::HttpPolicyRequired);
        }
        let http_bytes = http
            .map(|metadata| {
                if metadata.sha256 != measured_sha || metadata.bytes != length as u64 {
                    return Err(CacheError::InvalidInput);
                }
                let encoded = metadata.encode().map_err(|_| CacheError::InvalidInput)?;
                if encoded.is_empty() || encoded.len() as u64 > MAX_HTTP_METADATA {
                    return Err(CacheError::Limit);
                }
                Ok(encoded)
            })
            .transpose()?;
        let record_bytes = HEADER as u64
            + length as u64
            + http_bytes.as_ref().map_or(0, |bytes| bytes.len() as u64);
        if self.0.shared.is_none() && record_bytes > self.0.budget.load(Ordering::Acquire) {
            return Err(CacheError::Limit);
        }
        let header = encode_digest(
            request.permit.identity,
            length as u64,
            measured_digest.as_ref(),
        );
        write_private(&temporary.join("header"), &header)?;
        if let Some(http_bytes) = http_bytes {
            write_private(&temporary.join("http"), &http_bytes)?;
        }
        sync_dir(&temporary)?;
        before_commit()?;
        self.request_current(&request)?;
        // Preserve useful old data on malformed/oversized/failed readers. At
        // most one bounded staging file exists under the writer gate. Completed
        // records obey the budget; peak temporary overhead is MAX_AUDIO.
        let entries = self.scan_except(Some(&temporary))?;
        self.evict(entries, record_bytes, Some(key))?;
        let path = self.path(key);
        if let Some(write) = &request.write {
            write.phase.store(2, Ordering::Release);
        }
        fs::rename(&temporary, &path).map_err(|_| CacheError::Unavailable)?;
        sync_dir(&self.0.sandbox.path)?;
        if let Err(error) = self.request_current(&request) {
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
        if let Some(reservation) = reservation
            && let Err(error) = reservation.finish(record_bytes)
        {
            remove_record(&path)?;
            return Err(budget_error(error));
        }
        self.touch(&path)?;
        drop(guard);
        Ok(())
    }
    #[cfg(test)]
    fn put(&self, request: Request, audio: &SyntheticAudio) -> Result<()> {
        self.put_reader(request, audio.0.as_slice())
    }
    #[cfg(test)]
    fn put_before_commit(
        &self,
        request: Request,
        audio: &SyntheticAudio,
        before_commit: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        self.put_reader_before_commit(request, audio.0.as_slice(), before_commit)
    }
    pub fn get(&self, lease: &Lease, track: impl Into<ResourceKey>) -> Result<Option<Hit>> {
        self.lease_current(lease)?;
        let identity = self.identity(lease, track.into())?;
        let permit = match self.0.authority.authorize(&identity) {
            Ok(permit) => permit,
            Err(error) => {
                if matches!(error, CacheError::Expired | CacheError::Revoked) {
                    let _io = self.io()?;
                    let path = self.path(identity.key());
                    if path.exists() {
                        remove_record(&path)?;
                    }
                }
                return Err(error);
            }
        };
        if permit.identity != identity {
            return Err(CacheError::Cancelled);
        }
        // Only the online-selection path can validate HTTP metadata and its
        // current representation. A direct get must not bypass that step.
        if permit.basis == AdmissionBasis::PrivateHttpCandidate || self.0.authority.private_http() {
            return Err(CacheError::NeedsOnline);
        }
        self.check(lease, permit)?;
        let _io = self.io()?;
        self.lease_current(lease)?;
        let path = self.path(identity.key());
        let Some(file) = read_record(
            &path,
            identity.key(),
            Some(identity.scope()),
            permit.content,
        )?
        else {
            return Ok(None);
        };
        self.check(lease, permit)?;
        // A verified, still-admitted inode must not become an online refetch
        // merely because LRU bookkeeping is busy or unavailable. In particular,
        // do not wait behind a global metadata/artwork scan on this hot path.
        if let Some(budget) = &self.0.shared {
            let _ = budget.try_touch(&path);
        } else {
            let _ = self.touch(&path);
        }
        Ok(Some(Hit {
            owner: Arc::downgrade(&self.0),
            lease: lease.clone(),
            permit,
            file: Arc::new(file),
        }))
    }
    /// Invalidates immediately; Busy/failure leaves reads fenced until an
    /// explicit worker retry completes cleanup. Never reports an unmeasured zero.
    pub fn clear(&self) -> Result<Stats> {
        self.clear_before_complete(|| {})
    }
    fn clear_before_complete(&self, before_complete: impl FnOnce()) -> Result<Stats> {
        self.clear_observing_contention(before_complete, || {})
    }
    fn clear_observing_contention(
        &self,
        before_complete: impl FnOnce(),
        mut busy: impl FnMut(),
    ) -> Result<Stats> {
        let (generation, active) = {
            let mut state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
            state.generation = state.generation.checked_add(1).ok_or(CacheError::Limit)?;
            state.clearing = true;
            state.retirement = diagnostic::Cause::CacheCleared;
            state.pending.clear();
            (state.generation, state.active_stage.clone())
        };
        // The fence above is immediate. Only this filesystem worker retries a
        // short stats/LRU/retiring-writer collision; persistent contention still
        // returns Busy and leaves the existing explicit Retry fence intact.
        let deadline = Instant::now() + Duration::from_millis(250);
        let _io = loop {
            {
                let state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
                if state.generation != generation || !state.clearing {
                    return Err(CacheError::Cancelled);
                }
            }
            match self.io() {
                Ok(guard) => break guard,
                Err(CacheError::Busy) if Instant::now() < deadline => {
                    busy();
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(error),
            }
        };
        {
            let state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
            if state.generation != generation || !state.clearing {
                return Err(CacheError::Cancelled);
            }
        }
        // Ordinary scans preserve a live writer. Clear is different: its
        // generation fence already retired that writer, so unlink the staging
        // path even if the source is paused and no further bytes arrive.
        if let Some(path) = active {
            remove_record(&path)?;
        }
        for entry in self.scan()? {
            remove_record(&entry.path)?;
        }
        sync_dir(&self.0.sandbox.path)?;
        before_complete();
        let mut state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
        if state.generation == generation {
            state.clearing = false;
        }
        Ok(Stats::default())
    }
    pub fn logout(&self) -> Result<Stats> {
        self.bind(None)?;
        self.clear()
    }
    pub fn stats(&self) -> Result<Stats> {
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
        if self.0.owner_pid != std::process::id() {
            return Err(CacheError::Cancelled);
        }
        let local = self.0.io.try_lock().map_err(|_| CacheError::Busy)?;
        let guard = IoGuard { _local: local };
        private_dir(&self.0.sandbox.path)?;
        Ok(guard)
    }
    fn commit_io(&self, request: &Request) -> Result<IoGuard<'_>> {
        let deadline = Instant::now() + Duration::from_millis(250);
        loop {
            self.request_current(request)?;
            match self.io() {
                Ok(guard) => return Ok(guard),
                Err(CacheError::Busy) if Instant::now() < deadline => {
                    // Only the cache worker waits; no player, GTK or streaming
                    // callback is joined, and teardown is checked every turn.
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(error),
            }
        }
    }
    fn touch(&self, path: &Path) -> Result<()> {
        if let Some(budget) = &self.0.shared {
            return budget.touch(path).map_err(budget_error);
        }
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
        self.scan_except(None)
    }
    fn scan_except(&self, active: Option<&Path>) -> Result<Vec<DiskEntry>> {
        let live = self
            .0
            .state
            .lock()
            .map_err(|_| CacheError::Unavailable)?
            .active_stage
            .clone();
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
            if active.is_some_and(|active| active == item.path())
                || live.as_ref().is_some_and(|active| *active == item.path())
            {
                continue;
            }
            if name.strip_prefix(".part-").is_some_and(valid_hex) {
                // The sole live stage is registered separately above. Every
                // other stage is an orphan, including a crash/reopen leftover.
                // Remove it or fail; do not hide it from quota accounting.
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
            if (self.0.shared.is_some()
                || bytes.saturating_add(incoming) <= self.0.budget.load(Ordering::Acquire))
                && count <= keep
            {
                break;
            }
            remove_record(&entry.path)?;
            bytes = bytes.saturating_sub(entry.bytes);
            count -= 1;
        }
        Ok(())
    }
}

fn encode_digest(identity: ResourceIdentity, length: u64, content_hash: &[u8]) -> [u8; HEADER] {
    let mut header = [0u8; HEADER];
    header[..8].copy_from_slice(MAGIC);
    header[8..40].copy_from_slice(&identity.key());
    header[40..72].copy_from_slice(&identity.scope());
    header[72..80].copy_from_slice(&length.to_be_bytes());
    let mut data = header[..80].to_vec();
    data.extend_from_slice(content_hash);
    header[80..112].copy_from_slice(&hash(&data));
    header
}
// Quota/recovery inspection uses metadata only, never unauthorised audio
// payload. Full content integrity is checked by get() after current admission.
fn budget_error(error: crate::cache_budget::BudgetError) -> CacheError {
    use crate::cache_budget::BudgetError;
    match error {
        BudgetError::Disabled | BudgetError::Cancelled => CacheError::Cancelled,
        BudgetError::Limit => CacheError::Limit,
        BudgetError::InvalidInput => CacheError::InvalidInput,
        BudgetError::Unavailable => CacheError::Unavailable,
    }
}

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
        let audio = open_private(&path.join("media.bin"))?;
        if length == 0
            || length > MAX_AUDIO as u64
            || audio.metadata().map_err(|_| CacheError::Unavailable)?.len() != length
        {
            return Ok(None);
        }
        let http_size = match read_http_record(path) {
            Ok(Some(metadata)) => {
                let mut data = header[..80].to_vec();
                data.extend_from_slice(&metadata.sha256);
                if metadata.bytes != length || hash(&data) != header[80..112] {
                    return Ok(None);
                }
                open_private(&path.join("http"))?
                    .metadata()
                    .map_err(|_| CacheError::Unavailable)?
                    .len()
            }
            Ok(None) => 0,
            Err(CacheError::InvalidInput | CacheError::Limit) => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(Some(HEADER as u64 + length + http_size))
    })();
    match record {
        Ok(None) => {
            remove_record(path)?;
            Ok(None)
        }
        other => other,
    }
}

/// Bounded policy-only read, available to the child selection module while it
/// already owns the store I/O gate. A missing sidecar is a legacy record; it is
/// not evidence that the record is admissible for private HTTP reuse.
fn read_http_record(path: &Path) -> Result<Option<http_policy::Metadata>> {
    let sidecar = path.join("http");
    match fs::symlink_metadata(&sidecar) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(CacheError::Unavailable),
        Ok(_) => {}
    }
    let mut input = open_private(&sidecar)?;
    let length = input.metadata().map_err(|_| CacheError::Unavailable)?.len();
    if length == 0 || length > MAX_HTTP_METADATA {
        return Err(CacheError::Limit);
    }
    let mut bytes = vec![0; length as usize];
    input
        .read_exact(&mut bytes)
        .map_err(|_| CacheError::Unavailable)?;
    http_policy::Metadata::decode(&bytes)
        .map(Some)
        .map_err(|_| CacheError::InvalidInput)
}

fn read_record(
    path: &Path,
    key: Key,
    scope: Option<Key>,
    claim: Option<ContentClaim>,
) -> Result<Option<File>> {
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
        if length == 0
            || length > MAX_AUDIO as u64
            || claim.is_some_and(|expected| expected.length() != length)
        {
            return Ok(None);
        }
        let mut input = open_private(&path.join("media.bin"))?;
        if input.metadata().map_err(|_| CacheError::Unavailable)?.len() != length {
            return Ok(None);
        }
        let mut context = ring::digest::Context::new(&SHA256);
        let mut chunk = [0; 16 * 1024];
        let mut measured = 0u64;
        loop {
            let read = input
                .read(&mut chunk)
                .map_err(|_| CacheError::Unavailable)?;
            if read == 0 {
                break;
            }
            measured = measured.checked_add(read as u64).ok_or(CacheError::Limit)?;
            if measured > length {
                return Ok(None);
            }
            context.update(&chunk[..read]);
        }
        let measured_digest = context.finish();
        let mut data = header[..80].to_vec();
        data.extend_from_slice(measured_digest.as_ref());
        if measured != length
            || hash(&data) != header[80..112]
            || claim.is_some_and(|expected| !expected.matches(measured, measured_digest.as_ref()))
        {
            return Ok(None);
        }
        match read_http_record(path) {
            Ok(Some(metadata)) => {
                if metadata.bytes != measured
                    || metadata.sha256.as_slice() != measured_digest.as_ref()
                {
                    return Ok(None);
                }
            }
            Ok(None) => {}
            Err(CacheError::InvalidInput | CacheError::Limit) => return Ok(None),
            Err(error) => return Err(error),
        }
        input
            .seek(SeekFrom::Start(0))
            .map_err(|_| CacheError::Unavailable)?;
        Ok(Some(input))
    })();
    match record {
        Ok(None) => {
            remove_record(path)?;
            Ok(None)
        }
        other => other,
    }
}
fn create_private_tree(path: &Path) -> Result<()> {
    let mut ancestor = PathBuf::new();
    for component in path.components() {
        ancestor.push(component);
        match fs::symlink_metadata(&ancestor) {
            Ok(meta) if !meta.is_dir() => return Err(CacheError::Unavailable),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => return Err(CacheError::Unavailable),
            _ => {}
        }
    }
    if path.exists() {
        return private_dir(path);
    }
    let parent = path.parent().ok_or(CacheError::InvalidInput)?;
    if !parent.exists() {
        create_private_tree(parent)?;
    }
    let meta = fs::symlink_metadata(parent).map_err(|_| CacheError::Unavailable)?;
    if !meta.is_dir() || meta.uid() != rustix::process::geteuid().as_raw() {
        return Err(CacheError::Unavailable);
    }
    fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .map_err(|_| CacheError::Unavailable)?;
    private_dir(path)
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
        .take(4)
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|_| CacheError::Unavailable)?;
    if children.len() > 3
        || children.iter().any(|entry| {
            entry.file_name() != "header"
                && entry.file_name() != "media.bin"
                && entry.file_name() != "http"
        })
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

pub(crate) mod bridge;
pub(crate) mod capture;
mod selection;

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
use test_support::*;
#[cfg(test)]
mod tests;
