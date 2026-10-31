// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded cover loading with an optional account-scoped raster disk cache.
//! This client never receives credentials or persistent media resources.

use std::{
    collections::VecDeque,
    fmt,
    io::Read,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

use gtk::gdk_pixbuf::{PixbufLoader, prelude::*};
use reqwest::{Url, blocking::Client, redirect::Policy};
use ring::digest::{Context, SHA256};

use crate::cache::{Cache, CacheKey as PersistentKey, CacheLease, EntryKind, Freshness};

const MAX_ENCODED_BYTES: usize = 2 * 1024 * 1024;
const MAX_SOURCE_EDGE: u32 = 2048;
const MAX_CACHE_BYTES: usize = 24 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 64;
const MAX_ACTIVE: usize = 4;
const MAX_WAITING: usize = 64;

/// A validated public image template, never an arbitrary network address.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CoverArt(String);

impl CoverArt {
    /// Music returns hostless templates. Accept only the exact public image
    /// origin and a final size placeholder; URLs with parameters are rejected.
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.len() > 1024 || !raw.is_ascii() {
            return None;
        }
        let path = raw
            .strip_prefix("https://avatars.yandex.net/")
            .or_else(|| raw.strip_prefix("avatars.yandex.net/"))?;
        let resource = path.strip_suffix("/%%")?;
        let segments: Vec<_> = resource.split('/').collect();
        if !(2..=12).contains(&segments.len())
            || segments.iter().any(|segment| {
                segment.is_empty()
                    || segment.len() > 256
                    || *segment == "."
                    || *segment == ".."
                    || !segment
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            })
        {
            return None;
        }
        Some(Self(resource.to_owned()))
    }

    fn url(&self, pixels: u16) -> Result<Url, ArtworkError> {
        Url::parse(&format!(
            "https://avatars.yandex.net/{}/{pixels}x{pixels}",
            self.0
        ))
        .map_err(|_| ArtworkError::InvalidImage)
    }

    /// Canonical already-validated public artwork template for typed metadata
    /// records. It cannot contain credentials, parameters or media addresses.
    pub(crate) fn cache_template(&self) -> String {
        format!("https://avatars.yandex.net/{}/%%", self.0)
    }
}

impl fmt::Debug for CoverArt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CoverArt([redacted])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ArtworkError {
    #[error("Cover loader is unavailable")]
    Unavailable,
    #[error("Cover request cancelled")]
    Cancelled,
    #[error("Cover request queue is full")]
    Busy,
    #[error("Cover could not be reached")]
    Network,
    #[error("Cover request was rejected")]
    Rejected,
    #[error("Cover exceeds supported limits")]
    TooLarge,
    #[error("Unsupported cover image")]
    InvalidImage,
}

/// Pixels cross to GTK only after bounded decoding on the worker. No provider
/// URL, response headers or raw errors are retained in this value.
pub struct CoverPixels {
    pub width: i32,
    pub height: i32,
    pub rowstride: usize,
    pub alpha: bool,
    pub bytes: Vec<u8>,
}

impl fmt::Debug for CoverPixels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CoverPixels([redacted])")
    }
}

#[derive(Clone, PartialEq, Eq)]
struct CacheKey(CoverArt, u16);
struct Cached {
    key: CacheKey,
    image: Arc<CoverPixels>,
    expires: Option<Instant>,
}
#[derive(Default)]
struct State {
    epoch: u64,
    active: usize,
    waiting: usize,
    bytes: usize,
    cache: VecDeque<Cached>,
    persistent_generation: Option<u64>,
}
struct Shared {
    client: Client,
    state: Mutex<State>,
    available: Condvar,
    jobs: SyncSender<Job>,
    persistent: Mutex<Option<Cache>>,
}

struct PersistentAccess {
    cache: Cache,
    lease: Option<CacheLease>,
    generation: u64,
    key: PersistentKey,
}

impl PersistentAccess {
    fn current(&self) -> bool {
        self.cache.generation() == self.generation
    }

    fn policy_ttl(&self) -> Option<Duration> {
        self.cache.policy().ok().map(|policy| policy.artwork_ttl)
    }
}

struct FetchedCover {
    pixels: CoverPixels,
    caching: CacheHint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CacheHint {
    allowed: bool,
    ttl: Option<Duration>,
}

struct Job {
    cover: CoverArt,
    pixels: u16,
    epoch: u64,
    url: Url,
    cancelled: Arc<AtomicBool>,
    result: SyncSender<Result<Arc<CoverPixels>, ArtworkError>>,
}

/// Poll from the GTK main loop without occupying its shared blocking pool.
/// Dropping a pending request prevents it from starting network work.
pub struct ArtworkRequest {
    result: Receiver<Result<Arc<CoverPixels>, ArtworkError>>,
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct ArtworkCancellation(Arc<AtomicBool>);

impl ArtworkCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}

impl ArtworkRequest {
    pub fn cancellation(&self) -> ArtworkCancellation {
        ArtworkCancellation(Arc::clone(&self.cancelled))
    }

    pub fn try_result(&self) -> Option<Result<Arc<CoverPixels>, ArtworkError>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(ArtworkError::Unavailable)),
        }
    }
}

impl Drop for ArtworkRequest {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Clone one loader per window/session; clear it on logout and close. Network
/// and decoding must be called from a worker, never GTK's main thread.
#[derive(Clone)]
pub struct ArtworkLoader(Arc<Shared>);

impl fmt::Debug for ArtworkLoader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ArtworkLoader([redacted])")
    }
}

impl ArtworkLoader {
    pub fn new() -> Result<Self, ArtworkError> {
        let client = Client::builder()
            .user_agent(concat!("Karin/", env!("CARGO_PKG_VERSION"), " artwork"))
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(10))
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .map_err(|_| ArtworkError::Unavailable)?;
        Self::with_client(client)
    }

    fn with_client(client: Client) -> Result<Self, ArtworkError> {
        let (jobs, receiver) = mpsc::sync_channel::<Job>(MAX_WAITING);
        let shared = Arc::new(Shared {
            client,
            state: Mutex::new(State::default()),
            available: Condvar::new(),
            jobs,
            persistent: Mutex::new(None),
        });
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = 0;
        for _ in 0..MAX_ACTIVE {
            let shared = Arc::downgrade(&shared);
            let receiver = Arc::clone(&receiver);
            if thread::Builder::new()
                .name("karin-artwork".into())
                .spawn(move || {
                    loop {
                        let job = receiver
                            .lock()
                            .ok()
                            .and_then(|receiver| receiver.recv().ok());
                        let (Some(job), Some(shared)) = (job, shared.upgrade()) else {
                            break;
                        };
                        if job.cancelled.load(Ordering::Acquire) {
                            let _ = job.result.try_send(Err(ArtworkError::Cancelled));
                            continue;
                        }
                        let result = Self(shared).load_cancellable(
                            &job.cover,
                            job.pixels,
                            job.epoch,
                            job.url,
                            Some(&job.cancelled),
                        );
                        let result = if job.cancelled.load(Ordering::Acquire) {
                            Err(ArtworkError::Cancelled)
                        } else {
                            result
                        };
                        let _ = job.result.try_send(result);
                    }
                })
                .is_ok()
            {
                workers += 1;
            }
        }
        if workers == 0 {
            return Err(ArtworkError::Unavailable);
        }
        Ok(Self(shared))
    }

    /// A dedicated four-worker pool keeps cover requests from occupying the
    /// application-service worker pool used by sign-in, search and playlists.
    pub fn request(
        &self,
        cover: &CoverArt,
        pixels: i32,
        epoch: u64,
    ) -> Result<ArtworkRequest, ArtworkError> {
        let pixels = pixel_bucket(pixels);
        self.request_from(cover, pixels, epoch, cover.url(pixels)?)
    }

    fn request_from(
        &self,
        cover: &CoverArt,
        pixels: u16,
        epoch: u64,
        url: Url,
    ) -> Result<ArtworkRequest, ArtworkError> {
        if self.epoch() != epoch {
            return Err(ArtworkError::Cancelled);
        }
        let (result, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        self.0
            .jobs
            .try_send(Job {
                cover: cover.clone(),
                pixels,
                epoch,
                url,
                cancelled: Arc::clone(&cancelled),
                result,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => ArtworkError::Busy,
                TrySendError::Disconnected(_) => ArtworkError::Unavailable,
            })?;
        Ok(ArtworkRequest {
            result: receiver,
            cancelled,
        })
    }

    pub fn epoch(&self) -> u64 {
        self.0.state.lock().map(|s| s.epoch).unwrap_or(u64::MAX)
    }

    /// Bind the shared application cache; this fast operation does not access
    /// disk. Disk reads/writes still run only on the cover workers. The shared
    /// cache must be bound to an accepted account before it can supply leases.
    /// Rebinding invalidates existing RAM/queued work without touching images
    /// which GTK or the active player already owns.
    pub fn set_persistent_cache(&self, cache: Option<Cache>) {
        if let Ok(mut persistent) = self.0.persistent.lock() {
            *persistent = cache;
        }
        self.clear();
    }

    /// In-flight HTTP has a fixed deadline. Its response cannot repopulate the
    /// cache after this point, and queued work is woken without starting HTTP.
    pub fn clear(&self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.epoch = state.epoch.wrapping_add(1);
            state.cache.clear();
            state.bytes = 0;
            state.persistent_generation = None;
            self.0.available.notify_all();
        }
    }

    pub fn load(
        &self,
        cover: &CoverArt,
        requested_pixels: i32,
        epoch: u64,
    ) -> Result<Arc<CoverPixels>, ArtworkError> {
        let pixels = pixel_bucket(requested_pixels);
        self.load_from(cover, pixels, epoch, cover.url(pixels)?)
    }

    fn load_from(
        &self,
        cover: &CoverArt,
        pixels: u16,
        epoch: u64,
        url: Url,
    ) -> Result<Arc<CoverPixels>, ArtworkError> {
        self.load_cancellable(cover, pixels, epoch, url, None)
    }

    fn cancelled(&self, epoch: u64, cancellation: Option<&AtomicBool>) -> bool {
        self.epoch() != epoch || cancellation.is_some_and(|flag| flag.load(Ordering::Acquire))
    }

    fn persistent_access(&self, cover: &CoverArt, pixels: u16) -> Option<PersistentAccess> {
        let cache = self.0.persistent.lock().ok()?.clone()?;
        let generation = cache.generation();
        let mut digest = Context::new(&SHA256);
        digest.update(b"Karin public raster cover v1\0");
        digest.update(cover.0.as_bytes());
        digest.update(&pixels.to_be_bytes());
        let selector = digest
            .finish()
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let key = PersistentKey::new(EntryKind::Artwork, &selector, 0).ok()?;
        let lease = cache.lease();
        Some(PersistentAccess {
            cache,
            lease,
            generation,
            key,
        })
    }

    fn load_cancellable(
        &self,
        cover: &CoverArt,
        pixels: u16,
        epoch: u64,
        url: Url,
        cancellation: Option<&AtomicBool>,
    ) -> Result<Arc<CoverPixels>, ArtworkError> {
        let key = CacheKey(cover.clone(), pixels);
        if self.cancelled(epoch, cancellation) {
            return Err(ArtworkError::Cancelled);
        }
        let persistent = self.persistent_access(cover, pixels);
        let generation = persistent.as_ref().map(|access| access.generation);
        let mut state = self.0.state.lock().map_err(|_| ArtworkError::Unavailable)?;
        if state.epoch != epoch {
            return Err(ArtworkError::Cancelled);
        }
        if state.persistent_generation != generation {
            state.cache.clear();
            state.bytes = 0;
            state.persistent_generation = generation;
        }
        if let Some(position) = state.cache.iter().position(|entry| entry.key == key) {
            let entry = state.cache.remove(position).expect("known cache position");
            if entry
                .expires
                .is_none_or(|deadline| Instant::now() < deadline)
            {
                let image = Arc::clone(&entry.image);
                state.cache.push_back(entry);
                drop(state);
                return if self.cancelled(epoch, cancellation)
                    || persistent.as_ref().is_some_and(|access| !access.current())
                {
                    Err(ArtworkError::Cancelled)
                } else {
                    Ok(image)
                };
            }
            state.bytes -= entry.image.bytes.len();
        }
        if state.waiting >= MAX_WAITING {
            return Err(ArtworkError::Busy);
        }
        state.waiting += 1;
        while state.active >= MAX_ACTIVE
            && state.epoch == epoch
            && !cancellation.is_some_and(|flag| flag.load(Ordering::Acquire))
            && persistent.as_ref().is_none_or(PersistentAccess::current)
        {
            state = self
                .0
                .available
                .wait_timeout(state, Duration::from_millis(100))
                .map_err(|_| ArtworkError::Unavailable)?
                .0;
        }
        state.waiting -= 1;
        if state.epoch != epoch
            || cancellation.is_some_and(|flag| flag.load(Ordering::Acquire))
            || persistent.as_ref().is_some_and(|access| !access.current())
        {
            return Err(ArtworkError::Cancelled);
        }
        state.active += 1;
        drop(state);
        let _permit = Permit(Arc::clone(&self.0));
        let mut stale = None;
        if let Some(access) = &persistent
            && let Some(lease) = &access.lease
            && let Ok(Some(entry)) = access.cache.get(lease, &access.key)
            && let Some(image) = decode_raster(&entry.bytes, pixels)
        {
            if self.cancelled(epoch, cancellation) || !access.current() {
                return Err(ArtworkError::Cancelled);
            }
            let image = Arc::new(image);
            if entry.freshness == Freshness::Fresh {
                // GTK retains this decoded image. Do not turn a disk entry's
                // remaining server TTL into an unbounded in-memory cache hit.
                return Ok(image);
            }
            stale = Some(image);
        }
        let fetched = match self.fetch(url, pixels, epoch, cancellation) {
            Ok(fetched) => fetched,
            Err(ArtworkError::Network) => {
                if self.cancelled(epoch, cancellation)
                    || persistent.as_ref().is_some_and(|access| !access.current())
                {
                    return Err(ArtworkError::Cancelled);
                }
                if let Some(image) = stale {
                    return Ok(image);
                }
                return Err(ArtworkError::Network);
            }
            Err(error) => {
                if error == ArtworkError::Rejected
                    && let Some(access) = &persistent
                    && let Some(lease) = &access.lease
                {
                    let _ = access.cache.remove(lease, &access.key);
                }
                return Err(error);
            }
        };
        let image = Arc::new(fetched.pixels);
        if self.cancelled(epoch, cancellation)
            || persistent.as_ref().is_some_and(|access| !access.current())
        {
            return Err(ArtworkError::Cancelled);
        }
        let policy_ttl = persistent.as_ref().and_then(PersistentAccess::policy_ttl);
        let ttl = match (fetched.caching.ttl, policy_ttl) {
            (Some(server), Some(policy)) => Some(server.min(policy)),
            (server, policy) => server.or(policy),
        };
        let cacheable = fetched.caching.allowed && ttl.is_none_or(|ttl| !ttl.is_zero());
        if let Some(access) = &persistent
            && let Some(lease) = &access.lease
        {
            if cacheable {
                if let Some(bytes) = encode_raster(&image) {
                    let _ = access.cache.put_with_ttl(
                        lease,
                        &access.key,
                        &bytes,
                        ttl.unwrap_or(Duration::from_secs(7 * 24 * 60 * 60)),
                    );
                }
            } else {
                // A successful no-store/no-cache response supersedes any old
                // stale copy; never silently retain the earlier disk record.
                let _ = access.cache.remove(lease, &access.key);
            }
        }
        let mut state = self.0.state.lock().map_err(|_| ArtworkError::Unavailable)?;
        if state.epoch != epoch
            || cancellation.is_some_and(|flag| flag.load(Ordering::Acquire))
            || persistent.as_ref().is_some_and(|access| !access.current())
        {
            return Err(ArtworkError::Cancelled);
        }
        if !cacheable
            || persistent
                .as_ref()
                .is_some_and(|access| access.lease.is_none())
        {
            // A disabled/unbound persistent cache supplies no lease. Retain
            // only the caller's live image in that case, without memoization.
            return Ok(image);
        }
        if let Some(position) = state.cache.iter().position(|entry| entry.key == key) {
            let previous = state.cache.remove(position).expect("known cache position");
            state.bytes -= previous.image.bytes.len();
        }
        while state.bytes + image.bytes.len() > MAX_CACHE_BYTES
            || state.cache.len() >= MAX_CACHE_ENTRIES
        {
            let Some(previous) = state.cache.pop_front() else {
                break;
            };
            state.bytes -= previous.image.bytes.len();
        }
        state.bytes += image.bytes.len();
        state.cache.push_back(Cached {
            key,
            image: Arc::clone(&image),
            expires: ttl.and_then(|ttl| Instant::now().checked_add(ttl)),
        });
        Ok(image)
    }

    fn fetch(
        &self,
        url: Url,
        pixels: u16,
        epoch: u64,
        cancellation: Option<&AtomicBool>,
    ) -> Result<FetchedCover, ArtworkError> {
        if self.cancelled(epoch, cancellation) {
            return Err(ArtworkError::Cancelled);
        }
        let mut response = self
            .0
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, "image/jpeg, image/png")
            .send()
            .map_err(|_| ArtworkError::Network)?;
        if self.cancelled(epoch, cancellation) {
            return Err(ArtworkError::Cancelled);
        }
        if response.status() != reqwest::StatusCode::OK {
            return Err(ArtworkError::Rejected);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_ENCODED_BYTES as u64)
        {
            return Err(ArtworkError::TooLarge);
        }
        let mime = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim);
        let format = match mime {
            Some("image/png") => "png",
            Some("image/jpeg") => "jpeg",
            _ => return Err(ArtworkError::InvalidImage),
        };
        let caching = cache_hint(response.headers());
        let mut bytes = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            if self.cancelled(epoch, cancellation) {
                return Err(ArtworkError::Cancelled);
            }
            let read = response
                .read(&mut chunk)
                .map_err(|_| ArtworkError::Network)?;
            if read == 0 {
                break;
            }
            if bytes.len() + read > MAX_ENCODED_BYTES {
                return Err(ArtworkError::TooLarge);
            }
            bytes.extend_from_slice(&chunk[..read]);
        }
        if self.cancelled(epoch, cancellation) {
            return Err(ArtworkError::Cancelled);
        }
        Ok(FetchedCover {
            pixels: decode(&bytes, format, pixels)?,
            caching,
        })
    }
}

fn cache_hint(headers: &reqwest::header::HeaderMap) -> CacheHint {
    let denied = CacheHint {
        allowed: false,
        ttl: None,
    };
    let mut max_age: Option<u64> = None;
    for value in headers.get_all(reqwest::header::CACHE_CONTROL) {
        let Ok(value) = value.to_str() else {
            return denied;
        };
        for directive in value.split(',') {
            let (name, value) = directive
                .split_once('=')
                .map_or((directive.trim(), None), |(name, value)| {
                    (name.trim(), Some(value.trim()))
                });
            match name.to_ascii_lowercase().as_str() {
                "no-store" | "no-cache" | "private" | "must-revalidate" => return denied,
                "max-age" => {
                    let Some(value) = value else {
                        return denied;
                    };
                    let value = value
                        .strip_prefix('"')
                        .and_then(|value| value.strip_suffix('"'))
                        .unwrap_or(value);
                    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                        return denied;
                    }
                    let Ok(seconds) = value.parse::<u64>() else {
                        return denied;
                    };
                    max_age = Some(max_age.map_or(seconds, |current| current.min(seconds)));
                }
                _ => {}
            }
        }
    }
    for value in headers.get_all(reqwest::header::PRAGMA) {
        if value.to_str().ok().is_none_or(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("no-cache"))
        }) {
            return denied;
        }
    }
    for value in headers.get_all(reqwest::header::VARY) {
        if value.to_str().ok().is_none_or(|value| {
            value.split(',').any(|part| {
                matches!(
                    part.trim().to_ascii_lowercase().as_str(),
                    "*" | "authorization" | "cookie"
                )
            })
        }) {
            return denied;
        }
    }
    if headers.contains_key(reqwest::header::SET_COOKIE)
        || (max_age.is_none() && headers.contains_key(reqwest::header::EXPIRES))
    {
        return denied;
    }
    let age = match headers.get(reqwest::header::AGE) {
        Some(value) => {
            let Some(seconds) = value
                .to_str()
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
            else {
                return denied;
            };
            if max_age.is_none() && seconds != 0 {
                return denied;
            }
            seconds
        }
        None => 0,
    };
    let ttl = max_age
        .map(|seconds| Duration::from_secs(seconds.saturating_sub(age).min(30 * 24 * 60 * 60)));
    CacheHint {
        allowed: ttl.is_none_or(|ttl| !ttl.is_zero()),
        ttl,
    }
}

/// Store only packed decoded RGB/RGBA. Raster rows contain no JPEG EXIF, PNG
/// ancillary metadata, provider URL, cookies, authorization or audio payload.
fn encode_raster(image: &CoverPixels) -> Option<Vec<u8>> {
    let width = u32::try_from(image.width).ok()?;
    let height = u32::try_from(image.height).ok()?;
    if !(1..=512).contains(&width) || !(1..=512).contains(&height) {
        return None;
    }
    let channels = if image.alpha { 4 } else { 3 };
    let rowstride = width.checked_mul(channels)?;
    let row_bytes = usize::try_from(rowstride).ok()?;
    let rows = usize::try_from(height).ok()?;
    let required = image
        .rowstride
        .checked_mul(rows - 1)?
        .checked_add(row_bytes)?;
    if image.rowstride < row_bytes || required > image.bytes.len() {
        return None;
    }
    let mut bytes = Vec::with_capacity(21 + row_bytes.checked_mul(rows)?);
    bytes.extend_from_slice(b"KARINRGB");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&rowstride.to_be_bytes());
    bytes.push(u8::from(image.alpha));
    for row in 0..rows {
        let start = row.checked_mul(image.rowstride)?;
        bytes.extend_from_slice(&image.bytes[start..start + row_bytes]);
    }
    Some(bytes)
}

fn decode_raster(bytes: &[u8], pixels: u16) -> Option<CoverPixels> {
    if bytes.len() < 21 || &bytes[..8] != b"KARINRGB" || bytes[20] > 1 {
        return None;
    }
    let width = u32::from_be_bytes(bytes[8..12].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[12..16].try_into().ok()?);
    let rowstride = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let alpha = bytes[20] == 1;
    let channels = if alpha { 4 } else { 3 };
    if width == 0
        || height == 0
        || width > u32::from(pixels)
        || height > u32::from(pixels)
        || rowstride != width.checked_mul(channels)?
    {
        return None;
    }
    let length = usize::try_from(rowstride.checked_mul(height)?).ok()?;
    if bytes.len() != 21 + length {
        return None;
    }
    Some(CoverPixels {
        width: i32::try_from(width).ok()?,
        height: i32::try_from(height).ok()?,
        rowstride: usize::try_from(rowstride).ok()?,
        alpha,
        bytes: bytes[21..].to_vec(),
    })
}

struct Permit(Arc<Shared>);
impl Drop for Permit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.active = state.active.saturating_sub(1);
            self.0.available.notify_one();
        }
    }
}

fn pixel_bucket(requested: i32) -> u16 {
    match requested {
        ..=128 => 128,
        129..=256 => 256,
        _ => 512,
    }
}

fn decode(bytes: &[u8], format: &str, pixels: u16) -> Result<CoverPixels, ArtworkError> {
    let (width, height) = match format {
        "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 => {
            if &bytes[12..16] != b"IHDR" {
                return Err(ArtworkError::InvalidImage);
            }
            (
                u32::from_be_bytes(bytes[16..20].try_into().expect("four bytes")),
                u32::from_be_bytes(bytes[20..24].try_into().expect("four bytes")),
            )
        }
        "jpeg" => jpeg_dimensions(bytes).ok_or(ArtworkError::InvalidImage)?,
        _ => return Err(ArtworkError::InvalidImage),
    };
    if width == 0 || height == 0 || width > MAX_SOURCE_EDGE || height > MAX_SOURCE_EDGE {
        return Err(ArtworkError::TooLarge);
    }
    let loader = PixbufLoader::with_type(format).map_err(|_| ArtworkError::InvalidImage)?;
    // Select only these raster decoders, never SVG or external resources.
    let scale = (f64::from(pixels) / f64::from(width.max(height))).min(1.0);
    let target_width = (f64::from(width) * scale).round().max(1.0) as i32;
    let target_height = (f64::from(height) * scale).round().max(1.0) as i32;
    loader.set_size(target_width, target_height);
    let written = loader.write(bytes);
    let closed = loader.close();
    written
        .and(closed)
        .map_err(|_| ArtworkError::InvalidImage)?;
    let pixbuf = loader.pixbuf().ok_or(ArtworkError::InvalidImage)?;
    if pixbuf.width() <= 0
        || pixbuf.height() <= 0
        || pixbuf.width() > i32::from(pixels)
        || pixbuf.height() > i32::from(pixels)
        || pixbuf.bits_per_sample() != 8
        || !matches!(pixbuf.n_channels(), 3 | 4)
    {
        return Err(ArtworkError::InvalidImage);
    }
    Ok(CoverPixels {
        width: pixbuf.width(),
        height: pixbuf.height(),
        rowstride: usize::try_from(pixbuf.rowstride()).map_err(|_| ArtworkError::InvalidImage)?,
        alpha: pixbuf.has_alpha(),
        bytes: pixbuf.read_pixel_bytes().as_ref().to_vec(),
    })
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut index = 2;
    while index + 4 <= bytes.len() {
        if bytes[index] != 0xff {
            return None;
        }
        while bytes.get(index) == Some(&0xff) {
            index += 1;
        }
        let marker = *bytes.get(index)?;
        index += 1;
        if matches!(marker, 0xd9 | 0xda | 0x00) {
            return None;
        }
        if matches!(marker, 0xd0..=0xd8 | 0x01) {
            continue;
        }
        let length = usize::from(u16::from_be_bytes([
            *bytes.get(index)?,
            *bytes.get(index + 1)?,
        ]));
        if length < 2 || index + length > bytes.len() {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if length < 8 {
                return None;
            }
            return Some((
                u32::from(u16::from_be_bytes([bytes[index + 5], bytes[index + 6]])),
                u32::from(u16::from_be_bytes([bytes[index + 3], bytes[index + 4]])),
            ));
        }
        index += length;
    }
    None
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn cached_cover_fixture() -> (ArtworkLoader, CoverArt) {
    tests::cached_fixture()
}
