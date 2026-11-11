// SPDX-License-Identifier: GPL-3.0-or-later
//! A bounded, worker-only projection of actual local audio records.
//!
//! Presence means bytes exist, never playback permission or offline access.
//! Nothing is persisted separately: verified immutable file fingerprints only
//! avoid repeatedly hashing the same completed record on every UI refresh.

use super::*;

const MAX_QUERY: usize = 2_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Presence {
    #[default]
    Missing,
    Downloading,
    Stored,
}

pub(super) struct Download {
    nonce: Weak<()>,
    cancel: Weak<AtomicBool>,
}

#[derive(Default)]
pub(super) struct Memo {
    records: BTreeMap<Key, Fingerprint>,
    #[cfg(test)]
    verifications: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileFingerprint {
    device: u64,
    inode: u64,
    bytes: u64,
    mode: u32,
    owner: u32,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl FileFingerprint {
    fn of(file: &File) -> Result<Self> {
        private_file(file)?;
        let meta = file.metadata().map_err(|_| CacheError::Unavailable)?;
        Ok(Self {
            device: meta.dev(),
            inode: meta.ino(),
            bytes: meta.len(),
            mode: meta.mode(),
            owner: meta.uid(),
            modified: (meta.mtime(), meta.mtime_nsec()),
            changed: (meta.ctime(), meta.ctime_nsec()),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    header: FileFingerprint,
    media: FileFingerprint,
    http: Option<FileFingerprint>,
}
impl Fingerprint {
    fn at(path: &Path, private_http: bool) -> Result<Option<Self>> {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(CacheError::Unavailable),
            Ok(_) => {}
        }
        private_dir(path)?;
        let header = FileFingerprint::of(&open_private(&path.join("header"))?)?;
        let media = FileFingerprint::of(&open_private(&path.join("media.bin"))?)?;
        let sidecar = path.join("http");
        let http = match fs::symlink_metadata(&sidecar) {
            Ok(_) => Some(FileFingerprint::of(&open_private(&sidecar)?)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !private_http => None,
            Err(_) => return Err(CacheError::Unavailable),
        };
        Ok(Some(Self {
            header,
            media,
            http,
        }))
    }
}

impl AudioCache {
    /// Memory-only lease check for rejecting a late worker result on GTK.
    pub(crate) fn presence_current(&self, lease: &Lease) -> bool {
        self.lease_current(lease).is_ok()
    }

    /// Worker-only, bounded batch in input order; no HTTP, authority grant or
    /// LRU touch. Hidden/virtualized rows need no query until actually visible.
    /// Busy means keep the previous projection while an atomic writer finishes.
    pub(crate) fn presence(&self, lease: &Lease, tracks: &[ResourceKey]) -> Result<Vec<Presence>> {
        self.lease_current(lease)?;
        if tracks.len() > MAX_QUERY {
            return Err(CacheError::Limit);
        }
        let mut measured = BTreeMap::new();
        let mut result = Vec::with_capacity(tracks.len());
        for track in tracks {
            self.lease_current(lease)?;
            let identity = self.identity(lease, *track)?;
            let key = identity.key();
            let state = match measured.get(&key) {
                Some(state) => *state,
                None => {
                    let state = self.record_presence(lease, identity)?;
                    measured.insert(key, state);
                    state
                }
            };
            result.push(state);
        }
        self.lease_current(lease)?;
        Ok(result)
    }

    pub(super) fn mark_downloading(&self, request: &Request) -> Result<()> {
        let mut state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
        self.lease_current_locked(&request.lease, &state)?;
        if request.cancel.load(Ordering::Acquire)
            || !state
                .pending
                .get(&request.permit.identity.key())
                .is_some_and(|nonce| Arc::ptr_eq(nonce, &request.nonce))
        {
            return Err(CacheError::Cancelled);
        }
        state.downloading.insert(
            request.permit.identity.key(),
            Download {
                nonce: Arc::downgrade(&request.nonce),
                cancel: Arc::downgrade(&request.cancel),
            },
        );
        Ok(())
    }

    fn downloading(&self, lease: &Lease, key: Key) -> Result<bool> {
        let state = self.0.state.lock().map_err(|_| CacheError::Unavailable)?;
        self.lease_current_locked(lease, &state)?;
        Ok(state.downloading.get(&key).is_some_and(|download| {
            download
                .cancel
                .upgrade()
                .is_some_and(|cancel| !cancel.load(Ordering::Acquire))
                && download.nonce.upgrade().is_some_and(|nonce| {
                    state
                        .pending
                        .get(&key)
                        .is_some_and(|pending| Arc::ptr_eq(pending, &nonce))
                })
        }))
    }

    fn record_presence(&self, lease: &Lease, identity: ResourceIdentity) -> Result<Presence> {
        let key = identity.key();
        let path = self.path(key);
        let private_http = self.0.authority.private_http();
        let fingerprint = match Fingerprint::at(&path, private_http) {
            Ok(Some(value)) => value,
            Ok(None) | Err(_) => {
                self.forget_presence(key)?;
                return Ok(if self.downloading(lease, key)? {
                    Presence::Downloading
                } else {
                    Presence::Missing
                });
            }
        };
        if self
            .0
            .presence
            .lock()
            .map_err(|_| CacheError::Unavailable)?
            .records
            .get(&key)
            == Some(&fingerprint)
        {
            return Ok(Presence::Stored);
        }
        // An atomic rename is not yet the publication acknowledgement. The
        // active worker retains its request until its final fences succeed.
        if self.downloading(lease, key)? {
            return Ok(Presence::Downloading);
        }
        #[cfg(test)]
        {
            self.0.presence.lock().unwrap().verifications += 1;
        }
        // Hash outside the I/O gate so a large cold status query cannot keep
        // normal playback selection from using the same store.
        let verified = inspect_record(&path, key, Some(identity.scope()), None)
            .ok()
            .flatten()
            .is_some_and(|file| {
                !private_http || mp3::validate(&file, fingerprint.media.bytes).is_ok()
            });
        // Only this short final check joins the writer/Clear gate. Published
        // records are immutable; a changed/disappeared inode is measured again
        // next refresh instead of making a mixed old/new snapshot authoritative.
        let _io = self.io()?;
        self.lease_current(lease)?;
        if Fingerprint::at(&path, private_http).ok().flatten() != Some(fingerprint) {
            self.forget_presence(key)?;
            return Err(CacheError::Busy);
        }
        if !verified {
            self.forget_presence(key)?;
            return Ok(Presence::Missing);
        }
        if self.downloading(lease, key)? {
            return Ok(Presence::Downloading);
        }
        let mut memo = self
            .0
            .presence
            .lock()
            .map_err(|_| CacheError::Unavailable)?;
        if memo.records.len() >= MAX_ENTRIES
            && !memo.records.contains_key(&key)
            && let Some(victim) = memo.records.keys().next().copied()
        {
            memo.records.remove(&victim);
        }
        memo.records.insert(key, fingerprint);
        Ok(Presence::Stored)
    }

    fn forget_presence(&self, key: Key) -> Result<()> {
        self.0
            .presence
            .lock()
            .map_err(|_| CacheError::Unavailable)?
            .records
            .remove(&key);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
