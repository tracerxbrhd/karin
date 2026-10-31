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

pub mod diagnostics;
use diagnostics::{Event, emit};

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
    /// origin and a final size placeholder. Only the documented numeric cache
    /// revision on custom-playlist covers is accepted after the placeholder.
    pub fn parse(raw: &str) -> Option<Self> {
        let parsed = Self::parse_validated(raw);
        if parsed.is_none() {
            emit(Event::UriRejected);
        }
        parsed
    }

    fn parse_validated(raw: &str) -> Option<Self> {
        if raw.len() > 1024 || !raw.is_ascii() {
            return None;
        }
        let path = raw
            .strip_prefix("https://avatars.yandex.net/")
            .or_else(|| raw.strip_prefix("//avatars.yandex.net/"))
            .or_else(|| raw.strip_prefix("avatars.yandex.net/"))?;
        let (template, revision) = path
            .split_once('?')
            .map_or((path, None), |(template, revision)| {
                (template, Some(revision))
            });
        let resource = template.strip_suffix("/%%")?;
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
        if let Some(revision) = revision
            && (segments.first().copied() != Some("get-music-user-playlist")
                || revision.is_empty()
                || revision.len() > 20
                || !revision.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return None;
        }
        Some(Self(revision.map_or_else(
            || resource.to_owned(),
            |revision| format!("{resource}?{revision}"),
        )))
    }

    fn url(&self, pixels: u16) -> Result<Url, ArtworkError> {
        let (resource, revision) = self.parts();
        // CDN rendition names are a service contract, not the memory/cache
        // bucket. The published square renditions are 200, 400 and 1000;
        // locally decoded/cache pixels remain bounded to 128, 256 and 512.
        let rendition = wire_pixels(pixels);
        let mut url = Url::parse(&format!(
            "https://avatars.yandex.net/{resource}/{rendition}x{rendition}"
        ))
        .map_err(|_| ArtworkError::InvalidImage)?;
        url.set_query(revision);
        Ok(url)
    }

    fn parts(&self) -> (&str, Option<&str>) {
        self.0
            .split_once('?')
            .map_or((self.0.as_str(), None), |(resource, revision)| {
                (resource, Some(revision))
            })
    }

    /// Canonical already-validated public artwork template for typed metadata
    /// records. It cannot contain credentials, arbitrary parameters or media
    /// addresses; only the validated numeric playlist revision is retained.
    pub(crate) fn cache_template(&self) -> String {
        let (resource, revision) = self.parts();
        revision.map_or_else(
            || format!("https://avatars.yandex.net/{resource}/%%"),
            |revision| format!("https://avatars.yandex.net/{resource}/%%?{revision}"),
        )
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
    #[cfg(test)]
    test_transport: Mutex<Option<TestTransport>>,
}

#[cfg(test)]
#[derive(Clone)]
struct TestTransport {
    origin: Url,
    client: Client,
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
    allow_stale: bool,
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
            #[cfg(test)]
            test_transport: Mutex::new(None),
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

    /// Test-only origin routing still uses the ordinary validated template,
    /// rendition path and public request method before the fetch boundary.
    #[cfg(test)]
    pub(crate) fn for_test_origin(origin: &str) -> Result<Self, ArtworkError> {
        let loader = Self::new()?;
        loader.set_test_origin(origin)?;
        Ok(loader)
    }

    #[cfg(test)]
    pub(crate) fn set_test_origin(&self, raw: &str) -> Result<(), ArtworkError> {
        let origin = Url::parse(raw).map_err(|_| ArtworkError::InvalidImage)?;
        let loopback = matches!(origin.host_str(), Some("127.0.0.1" | "[::1]" | "::1"));
        if !loopback
            || !matches!(origin.scheme(), "http" | "https")
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
            || origin.port_or_known_default().is_none_or(|port| port == 0)
        {
            return Err(ArtworkError::InvalidImage);
        }
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(1))
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|_| ArtworkError::Unavailable)?;
        *self
            .0
            .test_transport
            .lock()
            .map_err(|_| ArtworkError::Unavailable)? = Some(TestTransport { origin, client });
        Ok(())
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
                TrySendError::Full(_) => {
                    emit(Event::QueueBusy);
                    ArtworkError::Busy
                }
                TrySendError::Disconnected(_) => {
                    emit(Event::RequestNotStarted);
                    ArtworkError::Unavailable
                }
            })?;
        emit(Event::Queued);
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
                emit(Event::MemoryHit);
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
            emit(Event::QueueBusy);
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
        if let Some(access) = &persistent {
            if let Some(lease) = &access.lease {
                match access.cache.get(lease, &access.key) {
                    Ok(Some(entry)) => {
                        if let Some(image) = decode_raster(&entry.bytes, pixels) {
                            if self.cancelled(epoch, cancellation) || !access.current() {
                                emit(Event::Cancelled);
                                return Err(ArtworkError::Cancelled);
                            }
                            let image = Arc::new(image);
                            if entry.freshness == Freshness::Fresh {
                                // Keep the source TTL rather than promoting a
                                // disk hit into unbounded RAM freshness.
                                emit(Event::DiskHit);
                                return Ok(image);
                            }
                            if raster_allows_stale(&entry.bytes) {
                                stale = Some(image);
                            }
                        } else {
                            emit(Event::CacheReadFailure);
                        }
                    }
                    Ok(None) => emit(Event::CacheMiss),
                    Err(
                        crate::cache::CacheError::Cancelled | crate::cache::CacheError::Disabled,
                    ) => {
                        emit(Event::Cancelled);
                    }
                    Err(_) => emit(Event::CacheReadFailure),
                }
            } else {
                emit(Event::CacheBypassed);
            }
        } else {
            emit(Event::CacheMiss);
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
                    emit(Event::DiskHit);
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
                if let Some(bytes) = encode_raster_with_policy(&image, fetched.caching.allow_stale)
                    && access
                        .cache
                        .put_with_ttl(
                            lease,
                            &access.key,
                            &bytes,
                            ttl.unwrap_or(Duration::from_secs(7 * 24 * 60 * 60)),
                        )
                        .is_err()
                {
                    emit(Event::CacheWriteFailure);
                }
            } else {
                // A successful no-store/no-cache response supersedes any old
                // stale copy; never silently retain the earlier disk record.
                emit(Event::CacheBypassed);
                if access.cache.remove(lease, &access.key).is_err() {
                    emit(Event::CacheWriteFailure);
                }
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
            emit(Event::Cancelled);
            return Err(ArtworkError::Cancelled);
        }
        #[cfg(test)]
        let (url, client) = {
            let transport = self
                .0
                .test_transport
                .lock()
                .map_err(|_| ArtworkError::Unavailable)?
                .clone();
            if let Some(transport) = transport {
                let mut url = url;
                url.set_scheme(transport.origin.scheme())
                    .map_err(|_| ArtworkError::InvalidImage)?;
                url.set_host(transport.origin.host_str())
                    .map_err(|_| ArtworkError::InvalidImage)?;
                url.set_port(transport.origin.port())
                    .map_err(|_| ArtworkError::InvalidImage)?;
                (url, transport.client)
            } else {
                (url, self.0.client.clone())
            }
        };
        #[cfg(not(test))]
        let client = &self.0.client;
        let mut response = client
            .get(url)
            .header(reqwest::header::ACCEPT, "image/jpeg, image/png")
            .send()
            .map_err(|error| {
                emit(network_event(&error));
                ArtworkError::Network
            })?;
        emit(Event::HttpStatus(response.status().as_u16()));
        if self.cancelled(epoch, cancellation) {
            emit(Event::Cancelled);
            return Err(ArtworkError::Cancelled);
        }
        if response.status() != reqwest::StatusCode::OK {
            emit(Event::HttpRefused);
            return Err(ArtworkError::Rejected);
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_ENCODED_BYTES as u64)
        {
            emit(Event::TooLarge);
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
            _ => {
                emit(Event::UnsupportedContentType);
                return Err(ArtworkError::InvalidImage);
            }
        };
        let caching = cache_hint(response.headers());
        let mut bytes = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            if self.cancelled(epoch, cancellation) {
                return Err(ArtworkError::Cancelled);
            }
            let read = response.read(&mut chunk).map_err(|error| {
                emit(if error.kind() == std::io::ErrorKind::TimedOut {
                    Event::NetworkTimeout
                } else {
                    Event::NetworkOther
                });
                ArtworkError::Network
            })?;
            if read == 0 {
                break;
            }
            if bytes.len() + read > MAX_ENCODED_BYTES {
                emit(Event::TooLarge);
                return Err(ArtworkError::TooLarge);
            }
            bytes.extend_from_slice(&chunk[..read]);
        }
        if self.cancelled(epoch, cancellation) {
            return Err(ArtworkError::Cancelled);
        }
        let pixels = decode(&bytes, format, pixels).inspect_err(|error| {
            emit(if *error == ArtworkError::TooLarge {
                Event::TooLarge
            } else {
                Event::DecodeFailure
            });
        })?;
        emit(Event::PixelsReady);
        Ok(FetchedCover { pixels, caching })
    }
}

fn cache_hint(headers: &reqwest::header::HeaderMap) -> CacheHint {
    let denied = CacheHint {
        allowed: false,
        ttl: None,
        allow_stale: false,
    };
    let mut max_age: Option<u64> = None;
    let mut must_revalidate = false;
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
                "no-store" | "no-cache" | "private" => return denied,
                "must-revalidate" => must_revalidate = true,
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
    if must_revalidate && max_age.is_none() {
        // No explicit freshness horizon: do not invent one for a response that
        // demands successful origin revalidation after becoming stale.
        return denied;
    }
    CacheHint {
        allowed: ttl.is_none_or(|ttl| !ttl.is_zero()),
        ttl,
        allow_stale: !must_revalidate,
    }
}

/// Store only packed decoded RGB/RGBA. Raster rows contain no JPEG EXIF, PNG
/// ancillary metadata, provider URL, cookies, authorization or audio payload.
#[cfg(test)]
fn encode_raster(image: &CoverPixels) -> Option<Vec<u8>> {
    encode_raster_with_policy(image, true)
}

fn encode_raster_with_policy(image: &CoverPixels, allow_stale: bool) -> Option<Vec<u8>> {
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
    // Backward-compatible v1 raster flags: bit0 alpha; bit1 forbids stale
    // fallback. This stores one cache-policy fact, never source headers.
    bytes.push(u8::from(image.alpha) | if allow_stale { 0 } else { 2 });
    for row in 0..rows {
        let start = row.checked_mul(image.rowstride)?;
        bytes.extend_from_slice(&image.bytes[start..start + row_bytes]);
    }
    Some(bytes)
}

fn decode_raster(bytes: &[u8], pixels: u16) -> Option<CoverPixels> {
    if bytes.len() < 21 || &bytes[..8] != b"KARINRGB" || bytes[20] & !3 != 0 {
        return None;
    }
    let width = u32::from_be_bytes(bytes[8..12].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[12..16].try_into().ok()?);
    let rowstride = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let alpha = bytes[20] & 1 != 0;
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

fn raster_allows_stale(bytes: &[u8]) -> bool {
    bytes.get(20).is_some_and(|flags| flags & 2 == 0)
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

fn wire_pixels(decoded_bucket: u16) -> u16 {
    match decoded_bucket {
        ..=128 => 200,
        129..=256 => 400,
        _ => 1000,
    }
}

fn known_network_detail(message: &str) -> Option<Event> {
    if message.starts_with("dns error")
        || message.starts_with("failed to lookup address information")
    {
        Some(Event::NetworkDns)
    } else if message.starts_with("invalid peer certificate:")
        || message.starts_with("received fatal alert:")
        || message.starts_with("peer sent no certificates")
        || message.starts_with("invalid TLS message:")
    {
        Some(Event::NetworkTls)
    } else {
        None
    }
}

fn network_event(error: &reqwest::Error) -> Event {
    if error.is_timeout() {
        return Event::NetworkTimeout;
    }
    // Classification is diagnostic only, never an authorization/security
    // decision. Inspect only known source prefixes and discard each temporary
    // message immediately. Never emit or retain provider/network error text.
    let mut source = std::error::Error::source(error);
    for _ in 0..8 {
        let Some(current) = source else {
            break;
        };
        if let Some(event) = known_network_detail(&current.to_string()) {
            return event;
        }
        source = current.source();
    }
    if error.is_connect() {
        Event::NetworkConnection
    } else {
        Event::NetworkOther
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
    let loader = PixbufLoader::with_type(format).map_err(|_| {
        emit(Event::DecoderUnavailable);
        ArtworkError::InvalidImage
    })?;
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
