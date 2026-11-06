// SPDX-License-Identifier: GPL-3.0-or-later
//! One worker-side disk budget for the application's typed cache stores.
//! Configuration is memory-only. Filesystem scans/eviction/reservations never
//! run on GTK. No store callback is called while the budget gate is held.
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

const MIB: u64 = 1024 * 1024;
const MAX_SCAN: usize = 16_384;
const MAX_RECORD: u64 = 64 * MIB + 112;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Metadata,
    Artwork,
    Audio,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BudgetError {
    #[error("Caching is disabled")]
    Disabled,
    #[error("Cache budget or disk reserve is exhausted")]
    Limit,
    #[error("Cache storage is unavailable")]
    Unavailable,
    #[error("Cache budget input is invalid")]
    InvalidInput,
    #[error("Cache budget operation was retired")]
    Cancelled,
}
type Result<T> = std::result::Result<T, BudgetError>;
#[derive(Clone)]
struct Pending {
    path: PathBuf,
    bytes: u64,
    category: Category,
}
struct State {
    enabled: bool,
    limit: Option<u64>,
    generation: u64,
    serial: u64,
    pending: BTreeMap<u64, Pending>,
}
pub struct CacheBudget {
    metadata: PathBuf,
    audio: PathBuf,
    state: Mutex<State>,
    io: Mutex<()>,
    #[cfg(test)]
    available: Mutex<Option<(u64, u64)>>,
}
pub struct Reservation {
    budget: Arc<CacheBudget>,
    id: u64,
    generation: u64,
    upper: u64,
}
struct Record {
    path: PathBuf,
    bytes: u64,
    category: Category,
    modified: SystemTime,
}
struct Scan {
    records: Vec<Record>,
    staging: u64,
}
impl CacheBudget {
    /// Does not create directories or touch the filesystem.
    pub fn new(
        metadata: PathBuf,
        audio: PathBuf,
        enabled: bool,
        limit: Option<u64>,
    ) -> Result<Arc<Self>> {
        if !valid_root(&metadata)
            || !valid_root(&audio)
            || metadata.starts_with(&audio)
            || audio.starts_with(&metadata)
            || limit == Some(0)
        {
            return Err(BudgetError::InvalidInput);
        }
        Ok(Arc::new(Self {
            metadata,
            audio,
            state: Mutex::new(State {
                enabled,
                limit,
                generation: 1,
                serial: 0,
                pending: BTreeMap::new(),
            }),
            io: Mutex::new(()),
            #[cfg(test)]
            available: Mutex::new(None),
        }))
    }
    /// Immediate admission fence; maintenance is deliberately separate.
    pub fn configure(&self, enabled: bool, limit: Option<u64>) -> Result<()> {
        if limit == Some(0) {
            return Err(BudgetError::InvalidInput);
        }
        let mut s = self.state.lock().map_err(|_| BudgetError::Unavailable)?;
        if (s.enabled, s.limit) != (enabled, limit) {
            s.enabled = enabled;
            s.limit = limit;
            s.generation = s.generation.wrapping_add(1).max(1);
        }
        Ok(())
    }
    pub fn enabled(&self) -> bool {
        self.state.lock().is_ok_and(|s| s.enabled)
    }
    pub fn generation(&self) -> u64 {
        self.state.lock().map_or(u64::MAX, |s| s.generation)
    }
    /// Reserve the entire bounded staging record, including its header. Slow
    /// network copies retain this ticket, not the coordinator's I/O mutex.
    pub fn reserve(
        self: &Arc<Self>,
        category: Category,
        path: &Path,
        upper: u64,
    ) -> Result<Reservation> {
        if upper == 0 || upper > MAX_RECORD || !self.record_path(category, path) {
            return Err(BudgetError::InvalidInput);
        }
        let _io = self.io.lock().map_err(|_| BudgetError::Unavailable)?;
        let (generation, limit, pending) = {
            let s = self.state.lock().map_err(|_| BudgetError::Unavailable)?;
            if !s.enabled {
                return Err(BudgetError::Disabled);
            }
            (
                s.generation,
                s.limit,
                s.pending.values().cloned().collect::<Vec<_>>(),
            )
        };
        if pending.iter().any(|p| p.path == path) {
            return Err(BudgetError::Limit);
        }
        let reserved = pending
            .iter()
            .try_fold(upper, |n, p| n.checked_add(p.bytes))
            .ok_or(BudgetError::Limit)?;
        self.disk_room(reserved)?;
        let mut scan = self.scan()?;
        // Large in-flight audio must leave admission room for small metadata.
        // This is a protected floor inside the ONE quota, never a second quota.
        let floor = if category == Category::Audio
            || pending.iter().any(|p| p.category == Category::Audio)
        {
            limit.map_or(0, |n| (n / 8).min(8 * MIB))
        } else {
            0
        };
        let small = scan
            .records
            .iter()
            .filter(|r| r.category == Category::Metadata)
            .map(|r| r.bytes)
            .sum::<u64>()
            .saturating_add(
                pending
                    .iter()
                    .filter(|p| p.category == Category::Metadata)
                    .map(|p| p.bytes)
                    .sum::<u64>(),
            )
            .saturating_add(if category == Category::Metadata {
                upper
            } else {
                0
            });
        let held_floor = floor.saturating_sub(small);
        self.evict(
            &mut scan,
            limit,
            reserved.saturating_add(held_floor),
            &pending,
            Some(category),
        )?;
        let mut s = self.state.lock().map_err(|_| BudgetError::Unavailable)?;
        if !s.enabled || s.generation != generation {
            return Err(BudgetError::Cancelled);
        }
        s.serial = s.serial.checked_add(1).ok_or(BudgetError::Limit)?;
        let id = s.serial;
        s.pending.insert(
            id,
            Pending {
                path: path.to_owned(),
                bytes: upper,
                category,
            },
        );
        Ok(Reservation {
            budget: Arc::clone(self),
            id,
            generation,
            upper,
        })
    }
    /// Actual completed and recognized staging bytes; no invented Unlimited size.
    pub fn usage(&self) -> Result<crate::cache::CacheStats> {
        let _io = self.io.lock().map_err(|_| BudgetError::Unavailable)?;
        let scan = self.scan()?;
        Ok(crate::cache::CacheStats {
            bytes: total(&scan)?,
            entries: scan.records.len(),
        })
    }
    pub fn trim(&self) -> Result<crate::cache::CacheStats> {
        let _io = self.io.lock().map_err(|_| BudgetError::Unavailable)?;
        let (limit, pending) = {
            let s = self.state.lock().map_err(|_| BudgetError::Unavailable)?;
            (s.limit, s.pending.values().cloned().collect::<Vec<_>>())
        };
        let mut scan = self.scan()?;
        // A reduced quota cancels old reservations via generation. Their
        // bounded staging bytes are removed by owners, never by a second store.
        self.evict(&mut scan, limit, 0, &pending, None)?;
        Ok(crate::cache::CacheStats {
            bytes: total(&scan)?,
            entries: scan.records.len(),
        })
    }
    pub fn touch(&self, path: &Path) -> Result<()> {
        if !self.record_path(Category::Audio, path) && !self.record_path(Category::Metadata, path) {
            return Err(BudgetError::InvalidInput);
        }
        let _io = self.io.lock().map_err(|_| BudgetError::Unavailable)?;
        self.touch_locked(path)
    }
    /// Opportunistic worker-side LRU bookkeeping for playback selection. Do not
    /// queue behind a metadata/artwork scan or trim just to refresh a timestamp.
    /// A later uncontended access can refresh the same shared LRU entry.
    /// This only avoids waiting for the coordinator; filesystem work still
    /// belongs on a worker, never on GTK or a GStreamer streaming callback.
    pub fn try_touch(&self, path: &Path) -> Result<bool> {
        if !self.record_path(Category::Audio, path) && !self.record_path(Category::Metadata, path) {
            return Err(BudgetError::InvalidInput);
        }
        let _io = match self.io.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(false),
            Err(std::sync::TryLockError::Poisoned(_)) => return Err(BudgetError::Unavailable),
        };
        self.touch_locked(path)?;
        Ok(true)
    }
    fn touch_locked(&self, path: &Path) -> Result<()> {
        if !self.safe_record(path)? {
            return Ok(());
        }
        match OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(path)
        {
            Ok(file) => file
                .set_times(fs::FileTimes::new().set_modified(SystemTime::now()))
                .map_err(|_| BudgetError::Unavailable),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(BudgetError::Unavailable),
        }
    }
    fn record_path(&self, category: Category, path: &Path) -> bool {
        if category == Category::Audio {
            path.strip_prefix(&self.audio).is_ok_and(|p| {
                let v = p.components().collect::<Vec<_>>();
                v.len() == 1 && hex_name(v[0].as_os_str().as_encoded_bytes(), 64)
            })
        } else {
            path.strip_prefix(self.metadata.join("v1")).is_ok_and(|p| {
                let v = p.components().collect::<Vec<_>>();
                v.len() == 2
                    && hex_name(v[0].as_os_str().as_encoded_bytes(), 64)
                    && v[1]
                        .as_os_str()
                        .as_encoded_bytes()
                        .strip_suffix(b".entry")
                        .is_some_and(|n| hex_name(n, 64))
            })
        }
    }
    fn safe_record(&self, path: &Path) -> Result<bool> {
        let root = if path.starts_with(&self.audio) {
            &self.audio
        } else {
            &self.metadata
        };
        if !safe_tree(root)? {
            return Ok(false);
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| BudgetError::InvalidInput)?;
        let mut parent = root.clone();
        for component in relative.parent().unwrap_or(Path::new("")).components() {
            parent.push(component);
            if !private_dir(&parent)? {
                return Ok(false);
            }
        }
        Ok(metadata(path)?.is_some_and(|m| private(&m)))
    }
    fn scan(&self) -> Result<Scan> {
        let mut out = Scan {
            records: Vec::new(),
            staging: 0,
        };
        let mut visited = 0;
        if safe_tree(&self.metadata)? {
            let base = self.metadata.join("v1");
            if private_dir(&base)? {
                for scope in children(&base, &mut visited)? {
                    if !hex_name(scope.file_name().as_encoded_bytes(), 64)
                        || !private_dir(&scope.path())?
                    {
                        continue;
                    }
                    for entry in children(&scope.path(), &mut visited)? {
                        let path = entry.path();
                        let name = entry.file_name();
                        let Some(m) = metadata(&path)? else {
                            continue;
                        };
                        if !m.is_file() || !private(&m) || m.nlink() != 1 {
                            continue;
                        }
                        if name
                            .as_encoded_bytes()
                            .strip_prefix(b".tmp-")
                            .is_some_and(|n| hex_name(n, 32))
                        {
                            out.staging =
                                out.staging.checked_add(m.len()).ok_or(BudgetError::Limit)?;
                        } else if self.record_path(Category::Metadata, &path) {
                            let mut header = [0u8; 9];
                            let mut f = read_file(&path)?;
                            // Corrupt recognized cache records still occupy space;
                            // typed stores own integrity repair, budget owns size.
                            let _ = f.read_exact(&mut header);
                            out.records.push(Record {
                                path,
                                bytes: m.len(),
                                category: if header[8] == 7 {
                                    Category::Artwork
                                } else {
                                    Category::Metadata
                                },
                                modified: m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                            });
                        }
                    }
                }
            }
        }
        if safe_tree(&self.audio)? {
            for entry in children(&self.audio, &mut visited)? {
                let path = entry.path();
                let name = entry.file_name();
                let staging = name
                    .as_encoded_bytes()
                    .strip_prefix(b".part-")
                    .is_some_and(|n| hex_name(n, 32));
                if (!staging && !hex_name(name.as_encoded_bytes(), 64)) || !private_dir(&path)? {
                    continue;
                }
                let mut bytes = 0u64;
                let mut valid = true;
                let mut count = 0;
                for child in children(&path, &mut visited)? {
                    let name = child.file_name();
                    if name != "header" && name != "media.bin" {
                        valid = false;
                        break;
                    }
                    let Some(m) = metadata(&child.path())? else {
                        valid = false;
                        break;
                    };
                    if !m.is_file() || !private(&m) || m.nlink() != 1 {
                        valid = false;
                        break;
                    }
                    bytes = bytes.checked_add(m.len()).ok_or(BudgetError::Limit)?;
                    count += 1;
                }
                if !valid {
                    return Err(BudgetError::Unavailable);
                }
                if staging {
                    out.staging = out.staging.checked_add(bytes).ok_or(BudgetError::Limit)?;
                } else if count != 0 {
                    out.records.push(Record {
                        modified: metadata(&path)?
                            .and_then(|m| m.modified().ok())
                            .unwrap_or(SystemTime::UNIX_EPOCH),
                        path,
                        bytes,
                        category: Category::Audio,
                    });
                }
            }
        }
        Ok(out)
    }
    fn evict(
        &self,
        scan: &mut Scan,
        limit: Option<u64>,
        reserved: u64,
        pending: &[Pending],
        incoming: Option<Category>,
    ) -> Result<()> {
        let Some(limit) = limit else {
            return Ok(());
        };
        let reserved = reserved.saturating_add(scan.staging);
        // Audio cannot evict the last small metadata floor. Small metadata can
        // reclaim large audio first, so a streaming writer cannot starve it.
        scan.records.sort_by_key(|r| {
            (
                if incoming == Some(Category::Metadata) {
                    match r.category {
                        Category::Audio => 0,
                        Category::Artwork => 1,
                        Category::Metadata => 2,
                    }
                } else {
                    0
                },
                r.modified,
            )
        });
        let mut completed = scan
            .records
            .iter()
            .try_fold(0u64, |n, r| n.checked_add(r.bytes))
            .ok_or(BudgetError::Limit)?;
        let mut small = scan
            .records
            .iter()
            .filter(|r| r.category == Category::Metadata)
            .map(|r| r.bytes)
            .sum::<u64>();
        let protected = (limit / 8).min(8 * MIB);
        let mut removed = Vec::new();
        for (index, record) in scan.records.iter().enumerate() {
            if completed.saturating_add(reserved) <= limit {
                break;
            }
            if pending.iter().any(|p| p.path == record.path) {
                continue;
            }
            if incoming == Some(Category::Audio)
                && record.category == Category::Metadata
                && small.saturating_sub(record.bytes) < protected
            {
                continue;
            }
            if !self.safe_record(&record.path)? {
                return Err(BudgetError::Unavailable);
            }
            let result = if record.category == Category::Audio {
                // Never recurse: unexpected children are preserved and fail closed.
                (|| {
                    for name in ["header", "media.bin"] {
                        match fs::remove_file(record.path.join(name)) {
                            Ok(()) => {}
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(e) => return Err(e),
                        }
                    }
                    fs::remove_dir(&record.path)
                })()
            } else {
                fs::remove_file(&record.path)
            };
            match result {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(BudgetError::Unavailable),
            }
            if record.category == Category::Metadata {
                small = small.saturating_sub(record.bytes);
            }
            completed = completed.saturating_sub(record.bytes);
            removed.push(index);
        }
        for index in removed.into_iter().rev() {
            scan.records.remove(index);
        }
        if completed.saturating_add(reserved) > limit {
            Err(BudgetError::Limit)
        } else {
            Ok(())
        }
    }
    fn disk_room(&self, reserved: u64) -> Result<()> {
        #[cfg(test)]
        if let Some((available, capacity)) = *self
            .available
            .lock()
            .map_err(|_| BudgetError::Unavailable)?
        {
            return room(available, capacity, reserved);
        }
        for root in [&self.metadata, &self.audio] {
            safe_tree(root)?;
            let mut path = root.as_path();
            while !path.exists() {
                path = path.parent().ok_or(BudgetError::Unavailable)?;
            }
            let stat = rustix::fs::statvfs(path).map_err(|_| BudgetError::Unavailable)?;
            room(
                stat.f_bavail.saturating_mul(stat.f_frsize),
                stat.f_blocks.saturating_mul(stat.f_frsize),
                reserved,
            )?;
        }
        Ok(())
    }
}
impl Reservation {
    pub fn finish(self, actual: u64) -> Result<()> {
        if actual > self.upper || actual == 0 {
            return Err(BudgetError::InvalidInput);
        }
        let _io = self
            .budget
            .io
            .lock()
            .map_err(|_| BudgetError::Unavailable)?;
        let s = self
            .budget
            .state
            .lock()
            .map_err(|_| BudgetError::Unavailable)?;
        if !s.enabled || s.generation != self.generation || !s.pending.contains_key(&self.id) {
            return Err(BudgetError::Cancelled);
        }
        Ok(())
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if let Ok(mut s) = self.budget.state.lock() {
            s.pending.remove(&self.id);
        }
    }
}
fn room(available: u64, capacity: u64, reserved: u64) -> Result<()> {
    let margin = (capacity / 100).clamp(64 * MIB, 512 * MIB);
    if available < reserved.saturating_add(margin) {
        Err(BudgetError::Limit)
    } else {
        Ok(())
    }
}
fn valid_root(path: &Path) -> bool {
    path.is_absolute()
        && path.parent().is_some()
        && path
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
}
fn hex_name(name: &[u8], length: usize) -> bool {
    name.len() == length
        && name
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}
fn metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(m) => Ok(Some(m)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(BudgetError::Unavailable),
    }
}
fn private(m: &fs::Metadata) -> bool {
    m.uid() == rustix::process::geteuid().as_raw()
        && m.mode() & 0o077 == 0
        && !m.file_type().is_symlink()
}
fn private_dir(path: &Path) -> Result<bool> {
    Ok(metadata(path)?.is_some_and(|m| m.is_dir() && private(&m)))
}
fn safe_tree(path: &Path) -> Result<bool> {
    // Ancestors need not be private (/tmp), but must never be symbolic links.
    for ancestor in path.ancestors() {
        if let Some(m) = metadata(ancestor)?
            && (!m.is_dir() || m.file_type().is_symlink())
        {
            return Err(BudgetError::Unavailable);
        }
    }
    private_dir(path)
}
fn read_file(path: &Path) -> Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)
        .map_err(|_| BudgetError::Unavailable)
}
fn children(path: &Path, visited: &mut usize) -> Result<Vec<fs::DirEntry>> {
    let mut result = Vec::new();
    for entry in fs::read_dir(path).map_err(|_| BudgetError::Unavailable)? {
        *visited += 1;
        if *visited > MAX_SCAN {
            return Err(BudgetError::Limit);
        }
        result.push(entry.map_err(|_| BudgetError::Unavailable)?);
    }
    Ok(result)
}
fn total(scan: &Scan) -> Result<u64> {
    scan.records
        .iter()
        .try_fold(scan.staging, |n, r| n.checked_add(r.bytes))
        .ok_or(BudgetError::Limit)
}

#[cfg(test)]
mod tests;
