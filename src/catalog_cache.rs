// SPDX-License-Identifier: GPL-3.0-or-later
//! Cached reads never replace MusicService's authorization/network authority.
use crate::{
    cache::{AccountScope, Cache, CacheError, CacheKey, CacheLease, EntryKind, Freshness},
    music::{
        Album, MusicError, MusicService, Page, Playlist, PlaylistId, Track,
        cache_codec::{self, MetadataRecord},
    },
    session::AccessToken,
};
use std::{
    fmt,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    Network,
    FreshCache,
    StaleCache,
}
#[derive(Clone)]
pub struct Cached<T> {
    pub value: T,
    pub provenance: Provenance,
    /// Recheck at the UI reply boundary; clear/policy/account changes retire it.
    pub lease: Option<CacheLease>,
    pub cache_error: Option<CacheError>,
}
impl<T> fmt::Debug for Cached<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cached")
            .field("provenance", &self.provenance)
            .field("cache_error", &self.cache_error)
            .finish_non_exhaustive()
    }
}
impl<T> Cached<T> {
    fn map<U>(
        self,
        convert: impl FnOnce(T) -> Result<U, MusicError>,
    ) -> Result<Cached<U>, MusicError> {
        Ok(Cached {
            value: convert(self.value)?,
            provenance: self.provenance,
            lease: self.lease,
            cache_error: self.cache_error,
        })
    }
}

#[derive(Default)]
struct Flights {
    active: Vec<(u64, CacheKey)>,
    waiting: usize,
}
#[derive(Default)]
struct FlightRegistry {
    state: Mutex<Flights>,
    changed: Condvar,
}
struct FlightPermit {
    registry: Arc<FlightRegistry>,
    key: (u64, CacheKey),
}
impl Drop for FlightPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.registry.state.lock() {
            state.active.retain(|key| key != &self.key);
            self.registry.changed.notify_all();
        }
    }
}

/// One cloneable, bounded in-process facade; it owns no network/runtime thread.
/// Stable track/album reads reuse a fresh record. Membership and playlist
/// content always ask Music first; transport failure may show saved data only.
#[derive(Clone)]
pub struct CachedCatalog {
    cache: Cache,
    flights: Arc<FlightRegistry>,
}
impl fmt::Debug for CachedCatalog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CachedCatalog([redacted])")
    }
}
impl CachedCatalog {
    pub fn new(cache: Cache) -> Self {
        Self {
            cache,
            flights: Arc::new(FlightRegistry::default()),
        }
    }
    pub fn cache(&self) -> &Cache {
        &self.cache
    }
    pub fn liked_tracks(
        &self,
        service: &MusicService,
        token: &AccessToken,
        page: u32,
        force_refresh: bool,
    ) -> Result<Cached<Page<Track>>, MusicError> {
        self.read(
            service,
            token,
            CacheKey::new(EntryKind::LikedTracks, "liked", page),
            true,
            force_refresh,
            || {
                service
                    .liked_tracks(token, page)
                    .map(MetadataRecord::Tracks)
            },
        )?
        .map(tracks)
    }
    pub fn playlists(
        &self,
        service: &MusicService,
        token: &AccessToken,
        page: u32,
        force_refresh: bool,
    ) -> Result<Cached<Page<Playlist>>, MusicError> {
        self.read(
            service,
            token,
            CacheKey::new(EntryKind::Playlists, "owned", page),
            true,
            force_refresh,
            || {
                service
                    .playlists(token, page)
                    .map(MetadataRecord::Playlists)
            },
        )?
        .map(playlists)
    }
    pub fn liked_playlists(
        &self,
        service: &MusicService,
        token: &AccessToken,
        page: u32,
        force_refresh: bool,
    ) -> Result<Cached<Page<Playlist>>, MusicError> {
        self.read(
            service,
            token,
            CacheKey::new(EntryKind::LikedPlaylists, "liked", page),
            true,
            force_refresh,
            || {
                service
                    .liked_playlists(token, page)
                    .map(MetadataRecord::Playlists)
            },
        )?
        .map(playlists)
    }
    pub fn playlist_tracks(
        &self,
        service: &MusicService,
        token: &AccessToken,
        id: &PlaylistId,
        page: u32,
        force_refresh: bool,
    ) -> Result<Cached<Page<Track>>, MusicError> {
        let selector = format!("{}:{}", id.owner, id.kind);
        self.read(
            service,
            token,
            CacheKey::new(EntryKind::PlaylistTracks, &selector, page),
            true,
            force_refresh,
            || {
                service
                    .playlist_tracks(token, id, page)
                    .map(MetadataRecord::Tracks)
            },
        )?
        .map(tracks)
    }
    pub fn artist_tracks(
        &self,
        service: &MusicService,
        token: &AccessToken,
        id: &str,
        page: u32,
        force_refresh: bool,
    ) -> Result<Cached<Page<Track>>, MusicError> {
        self.read(
            service,
            token,
            CacheKey::new(EntryKind::ArtistTracks, id, page),
            true,
            force_refresh,
            || {
                service
                    .artist_tracks(token, id, page)
                    .map(MetadataRecord::Tracks)
            },
        )?
        .map(tracks)
    }
    pub fn album(
        &self,
        service: &MusicService,
        token: &AccessToken,
        id: &str,
        force_refresh: bool,
    ) -> Result<Cached<Album>, MusicError> {
        self.read(
            service,
            token,
            CacheKey::new(EntryKind::Album, id, 0),
            false,
            force_refresh,
            || service.album(token, id).map(MetadataRecord::Album),
        )?
        .map(|record| match record {
            MetadataRecord::Album(value) => Ok(value),
            _ => Err(MusicError::InvalidResponse),
        })
    }
    pub fn track(
        &self,
        service: &MusicService,
        token: &AccessToken,
        id: &str,
        force_refresh: bool,
    ) -> Result<Cached<Track>, MusicError> {
        self.read(
            service,
            token,
            CacheKey::new(EntryKind::Track, id, 0),
            false,
            force_refresh,
            || service.track(token, id).map(MetadataRecord::Track),
        )?
        .map(|record| match record {
            MetadataRecord::Track(value) => Ok(value),
            _ => Err(MusicError::InvalidResponse),
        })
    }
    fn read(
        &self,
        service: &MusicService,
        token: &AccessToken,
        key: Result<CacheKey, CacheError>,
        mutable: bool,
        force: bool,
        fetch: impl FnOnce() -> Result<MetadataRecord, MusicError>,
    ) -> Result<Cached<MetadataRecord>, MusicError> {
        let scope = service.cache_scope(token)?;
        let lease = self
            .cache
            .lease()
            .filter(|lease| lease.matches_scope(&scope));
        let (key, mut cache_error) = match key {
            Ok(key) => (Some(key), None),
            Err(error) => (None, Some(error)),
        };
        if let (Some(lease), Some(key)) = (&lease, &key) {
            match self.cache.get(lease, key) {
                Ok(Some(entry)) => {
                    if let Ok(value) = cache_codec::decode(key.kind(), &entry.bytes) {
                        if !mutable
                            && !force
                            && entry.freshness == Freshness::Fresh
                            && lease.is_current()
                        {
                            verify_scope(service, token, &scope)?;
                            return Ok(Cached {
                                value,
                                provenance: Provenance::FreshCache,
                                lease: Some(lease.clone()),
                                cache_error: None,
                            });
                        }
                    } else {
                        cache_error = Some(CacheError::InvalidData);
                    }
                }
                Ok(None) => {}
                Err(error) => cache_error = notice(error),
            }
        }
        // Waiters recheck the stable record after the active reader's result.
        // Mutable calls deliberately do not convert an old snapshot to truth.
        let _flight = if !mutable {
            match (&lease, &key) {
                (Some(lease), Some(key)) => match self.acquire(lease, key) {
                    Ok(permit) => Some(permit),
                    Err(error) => {
                        cache_error = notice(error);
                        None
                    }
                },
                _ => None,
            }
        } else {
            None
        };
        if !mutable
            && !force
            && let (Some(lease), Some(key)) = (&lease, &key)
            && let Ok(Some(entry)) = self.cache.get(lease, key)
            && entry.freshness == Freshness::Fresh
            && lease.is_current()
            && let Ok(value) = cache_codec::decode(key.kind(), &entry.bytes)
        {
            verify_scope(service, token, &scope)?;
            return Ok(Cached {
                value,
                provenance: Provenance::FreshCache,
                lease: Some(lease.clone()),
                cache_error,
            });
        }
        let result = fetch();
        verify_scope(service, token, &scope)?;
        match result {
            Ok(value) => {
                if let (Some(lease), Some(key)) = (&lease, &key) {
                    let persisted = cache_codec::encode(key.kind(), &value)
                        .and_then(|bytes| self.cache.put(lease, key, &bytes));
                    if let Err(error) = persisted {
                        cache_error = notice(error);
                    }
                }
                // A cache clear does not invalidate a fresh server reply. Session
                // tickets independently guard logout/account races in the UI.
                Ok(Cached {
                    value,
                    provenance: Provenance::Network,
                    lease: None,
                    cache_error,
                })
            }
            Err(error @ (MusicError::Network | MusicError::Timeout)) => {
                if let (Some(lease), Some(key)) = (lease, key)
                    && lease.matches_scope(&scope)
                    && let Ok(Some(entry)) = self.cache.get(&lease, &key)
                    && let Ok(value) = cache_codec::decode(key.kind(), &entry.bytes)
                    && lease.is_current()
                {
                    return Ok(Cached {
                        value,
                        provenance: Provenance::StaleCache,
                        lease: Some(lease),
                        cache_error,
                    });
                }
                Err(error)
            }
            Err(error) => {
                // Never let a known refusal/unsupported response leave a fresh
                // record masking that outcome on the next visit.
                if let (Some(lease), Some(key)) = (&lease, &key) {
                    let _ = self.cache.remove(lease, key);
                }
                Err(error)
            }
        }
    }
    fn acquire(&self, lease: &CacheLease, key: &CacheKey) -> Result<FlightPermit, CacheError> {
        const MAX_ACTIVE: usize = 8;
        const MAX_WAITING: usize = 8;
        let id = (self.cache.generation(), key.clone());
        let mut state = self
            .flights
            .state
            .lock()
            .map_err(|_| CacheError::Unavailable)?;
        let waiting = state.active.contains(&id);
        if waiting && state.waiting >= MAX_WAITING {
            return Err(CacheError::Busy);
        }
        if waiting {
            state.waiting += 1;
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let error = if !lease.is_current() {
                Some(CacheError::Cancelled)
            } else if Instant::now() >= deadline {
                Some(CacheError::Busy)
            } else {
                None
            };
            if let Some(error) = error {
                if waiting {
                    state.waiting -= 1;
                }
                return Err(error);
            }
            if !state.active.contains(&id) {
                if waiting {
                    state.waiting -= 1;
                }
                if state.active.len() >= MAX_ACTIVE {
                    return Err(CacheError::Busy);
                }
                state.active.push(id.clone());
                return Ok(FlightPermit {
                    registry: Arc::clone(&self.flights),
                    key: id,
                });
            }
            let (next, _) = self
                .flights
                .changed
                .wait_timeout(state, Duration::from_millis(50))
                .map_err(|_| CacheError::Unavailable)?;
            state = next;
        }
    }
}
fn verify_scope(
    service: &MusicService,
    token: &AccessToken,
    expected: &AccountScope,
) -> Result<(), MusicError> {
    if &service.cache_scope(token)? == expected {
        Ok(())
    } else {
        Err(MusicError::Unauthenticated)
    }
}
fn notice(error: CacheError) -> Option<CacheError> {
    match error {
        CacheError::Disabled | CacheError::Cancelled => None,
        error => Some(error),
    }
}
fn tracks(record: MetadataRecord) -> Result<Page<Track>, MusicError> {
    match record {
        MetadataRecord::Tracks(value) => Ok(value),
        _ => Err(MusicError::InvalidResponse),
    }
}
fn playlists(record: MetadataRecord) -> Result<Page<Playlist>, MusicError> {
    match record {
        MetadataRecord::Playlists(value) => Ok(value),
        _ => Err(MusicError::InvalidResponse),
    }
}
#[cfg(test)]
mod tests;
