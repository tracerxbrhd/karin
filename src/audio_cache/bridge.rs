// SPDX-License-Identifier: GPL-3.0-or-later
//! Worker-side online playback selection and a separately admitted cache fill.
//!
//! A fresh stream resolution is required before every online-only cache hit or
//! fill. Cached bytes never bypass the current Music account/source validation.
//! The online authority is an experimental transport gate, not a license grant.

use super::{AudioCache, CacheError, Cancellation, Lease, Request, ResourceKey};
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
    let remote = resolver.resolve(service, token, track, cancellation)?;
    let binding = binding.filter(|(_, lease)| {
        service
            .cache_scope(token)
            .is_ok_and(|scope| lease.matches_scope(&scope))
    });
    Ok(select(
        resolver,
        token,
        cancellation,
        remote,
        track,
        binding,
    ))
}

fn select(
    resolver: StreamResolver,
    token: &AccessToken,
    cancellation: &DeliveryCancellation,
    remote: RemotePlaybackSource,
    track: &str,
    binding: Option<(AudioCache, Lease)>,
) -> Selection {
    let online = || PlaybackSource::remote(remote.clone());
    let Some((cache, lease)) = binding else {
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
    // The caller has just resolved the actual track against the current
    // account. Only this worker-bound event can admit a cached media record.
    if remote.is_expired() || cache.observe_verified_online(&lease, key).is_err() {
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
        Ok(None) => {
            // The cache authority still validates the observed account/source
            // session; failures never create a second HTTP request or a write.
            let fill = cache.begin(&lease, key).ok().map(|request| Fill {
                cache,
                request,
                resolver,
                remote: remote.clone(),
                token: token.clone(),
                cancellation: cancellation.clone(),
            });
            Selection {
                source: online(),
                fill,
            }
        }
        Err(_) => Selection {
            source: online(),
            fill: None,
        },
    }
}

/// One bounded background copy, admitted only after actual Playing. It never
/// changes the player and contains no second audio or HTTP engine.
pub(crate) struct Fill {
    cache: AudioCache,
    request: Request,
    resolver: StreamResolver,
    remote: RemotePlaybackSource,
    token: AccessToken,
    cancellation: DeliveryCancellation,
}
impl Fill {
    pub(crate) fn cancellation(&self) -> Cancellation {
        self.request.cancellation()
    }

    pub(crate) fn run(self) -> Result<(), CacheError> {
        self.cache.check_request(&self.request)?;
        let body = self.resolver.cache_body(
            &self.remote,
            &self.token,
            &self.cancellation,
            self.request.cancellation(),
        )?;
        self.cache.put_reader(self.request, body)
    }
}

#[cfg(test)]
mod tests;
