// SPDX-License-Identifier: GPL-3.0-or-later
//! Worker-side online playback selection and a separately admitted cache fill.
//!
//! A fresh stream resolution is required before every online-only cache hit or
//! fill. Cached bytes never bypass the current Music account/source validation.
//! Storage permission is independent: a resolved stream alone cannot admit writes.

use super::{AudioCache, CacheError, Cancellation, Lease, OnlineCheck, Request, ResourceKey};
use crate::{
    music::MusicService,
    playback::{PlaybackSource, RemotePlaybackSource},
    session::AccessToken,
    stream::{DeliveryCancellation, DeliveryReport, StreamResolver},
};

pub(crate) struct Selection {
    pub source: PlaybackSource,
    pub fill: Option<Fill>,
}

/// Resolve against the current Music session before considering a local hit.
/// Every cache failure falls back to this already validated online source.
/// Call on the existing bounded resolution worker, never on GTK.
pub(crate) fn resolve(
    resolver: StreamResolver,
    service: &MusicService,
    token: &AccessToken,
    track: &str,
    cancellation: &DeliveryCancellation,
    binding: Option<(AudioCache, Lease)>,
) -> Result<Selection, DeliveryReport> {
    // A positively different account must not retire that account's admission.
    // An expired/cleared session must retire the caller's still-current lease,
    // however, rather than leaving its previous successful admission usable.
    let (binding, authenticated) = match service.cache_scope(token) {
        Ok(scope) => (
            binding.filter(|(_, lease)| lease.matches_scope(&scope)),
            true,
        ),
        Err(_) => (binding, false),
    };
    // Permission lookup and integrity verification overlap network resolution.
    // A slow cache never holds the already admitted online playback hostage.
    let lookup = start_check(&binding, track).and_then(|check| {
        let (cache, lease) = binding.as_ref()?;
        cache.lookup(lease.clone(), check)
    });
    let remote = resolver.resolve(service, token, track, cancellation)?;
    let online = || Selection {
        source: PlaybackSource::remote(remote.clone()),
        fill: None,
    };
    let Some((cache, lease)) = binding.filter(|_| authenticated) else {
        return Ok(online());
    };
    let Some(prepared) = lookup.and_then(|lookup| lookup.ready()) else {
        return Ok(online());
    };
    match prepared.admit(|| !cancellation.cancelled(token) && !remote.is_expired()) {
        Ok(Some(hit)) => Ok(Selection {
            source: PlaybackSource::cached_file(hit, remote.clone())
                .unwrap_or_else(|_| PlaybackSource::remote(remote)),
            fill: None,
        }),
        Ok(None) => Ok(fill_selection(
            cache,
            lease,
            token,
            cancellation,
            remote,
            track,
        )),
        Err(_) => Ok(online()),
    }
}

fn start_check(binding: &Option<(AudioCache, Lease)>, track: &str) -> Option<OnlineCheck> {
    let (cache, lease) = binding.as_ref()?;
    cache
        .begin_online_check(lease, ResourceKey::track(track).ok()?)
        .ok()
}

#[cfg(test)]
fn select(
    resolver: StreamResolver,
    token: &AccessToken,
    cancellation: &DeliveryCancellation,
    remote: RemotePlaybackSource,
    track: &str,
    binding: Option<(AudioCache, Lease)>,
) -> Selection {
    let check = start_check(&binding, track);
    select_admitted(resolver, token, cancellation, remote, track, binding, check)
}

#[cfg(test)]
fn select_admitted(
    _resolver: StreamResolver,
    token: &AccessToken,
    cancellation: &DeliveryCancellation,
    remote: RemotePlaybackSource,
    track: &str,
    binding: Option<(AudioCache, Lease)>,
    check: Option<OnlineCheck>,
) -> Selection {
    let online = || PlaybackSource::remote(remote.clone());
    let (Some((cache, lease)), Some(check)) = (binding, check) else {
        return Selection {
            source: online(),
            fill: None,
        };
    };
    let Ok(key) = ResourceKey::track(track) else {
        return Selection {
            source: online(),
            fill: None,
        };
    };
    // Online admission is necessary, never sufficient for storage/reuse.
    if remote.is_expired()
        || cancellation.cancelled(token)
        || cache
            .observe_verified_online_if(&lease, check, || {
                !cancellation.cancelled(token) && !remote.is_expired()
            })
            .is_err()
    {
        return Selection {
            source: online(),
            fill: None,
        };
    }
    match cache.get(&lease, key) {
        Ok(Some(hit)) => {
            let source =
                PlaybackSource::cached_file(hit, remote.clone()).unwrap_or_else(|_| online());
            Selection { source, fill: None }
        }
        Ok(None) => fill_selection(cache, lease, token, cancellation, remote, track),
        Err(_) => Selection {
            source: online(),
            fill: None,
        },
    }
}

/// Capture only the existing guarded player's bytes. Cache failure is optional
/// bookkeeping and cannot trigger another media request or another engine.
fn fill_selection(
    cache: AudioCache,
    lease: Lease,
    token: &AccessToken,
    cancellation: &DeliveryCancellation,
    remote: RemotePlaybackSource,
    track: &str,
) -> Selection {
    let request = ResourceKey::track(track)
        .ok()
        .and_then(|key| cache.begin(&lease, key).ok());
    let Some(request) = request else {
        return Selection {
            source: PlaybackSource::remote(remote),
            fill: None,
        };
    };
    let (capture, reader) = super::capture::channel(request.cancellation());
    Selection {
        source: PlaybackSource::remote(remote).with_cache_capture(capture),
        fill: Some(Fill {
            cache,
            request,
            reader,
            token: token.clone(),
            cancellation: cancellation.clone(),
        }),
    }
}

/// One bounded background cache writer, admitted only after actual Playing.
/// Its reader is fed by the existing GStreamer HTTP source, never a second GET.
pub(crate) struct Fill {
    cache: AudioCache,
    request: Request,
    reader: super::capture::CaptureReader,
    token: AccessToken,
    cancellation: DeliveryCancellation,
}
impl Fill {
    pub(crate) fn cancellation(&self) -> Cancellation {
        self.request.cancellation()
    }

    pub(crate) fn run(self) -> Result<(), CacheError> {
        self.cache.check_request(&self.request)?;
        let reader = GuardedCapture {
            cache: self.cache.clone(),
            lease: self.request.lease.clone(),
            permit: self.request.permit,
            reader: self.reader,
            token: self.token,
            cancellation: self.cancellation,
        };
        self.cache.put_reader(self.request, reader)
    }
}
struct GuardedCapture {
    cache: AudioCache,
    lease: Lease,
    permit: super::StorageGrant,
    reader: super::capture::CaptureReader,
    token: AccessToken,
    cancellation: DeliveryCancellation,
}
impl std::io::Read for GuardedCapture {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read_checked(out, || {
            !self.cancellation.cancelled(&self.token)
                && self.cache.check(&self.lease, self.permit).is_ok()
        })
    }
}

#[cfg(test)]
mod tests;
