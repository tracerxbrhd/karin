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
    // Retire prior admission before network I/O, including pre-auth failures.
    // Stale workers cannot retire a newer session: start_check checks the lease.
    let check = start_check(&binding, track);
    let remote = resolver.resolve(service, token, track, cancellation)?;
    let binding = binding.filter(|_| authenticated);
    Ok(select_admitted(
        resolver,
        token,
        cancellation,
        remote,
        track,
        binding,
        check,
    ))
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

fn select_admitted(
    resolver: StreamResolver,
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
        Ok(None) => {
            // Both the independent storage permit and current online selection
            // remain live; failures never create a copy request or a write.
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
