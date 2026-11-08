// SPDX-License-Identifier: GPL-3.0-or-later
//! Worker-side online playback selection and a separately admitted cache fill.
//!
//! A fresh stream resolution is required before every online-only cache hit or
//! fill. Cached bytes never bypass the current Music account/source validation.
//! Storage permission is independent: a resolved stream alone cannot admit writes.

use super::diagnostic::{self, Cause, Event};
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
    let check = start_check(&binding, track);
    // This projection can only retire the same still-current attempt. It
    // cannot authorize/commit a replay of the one-use online ticket.
    let refusal = check.as_ref().map(|check| (check.identity, check.sequence));
    let lookup = check.and_then(|check| {
        let (cache, lease) = binding.as_ref()?;
        cache.lookup(lease.clone(), check)
    });
    let remote = resolver
        .resolve(service, token, track, cancellation)
        .inspect_err(|report| {
            if matches!(report.http_status, Some(401 | 403 | 451))
                && let (Some((cache, lease)), Some((identity, sequence))) = (&binding, refusal)
            {
                cache.revoke_resource_writes(lease, identity, sequence);
            }
            let reason = match report.http_status {
                Some(401 | 403) => Cause::Policy(super::http_policy::Reason::AccessDenied),
                Some(451) => Cause::Policy(super::http_policy::Reason::ContentRestricted),
                _ if cancellation.cancelled(token) => {
                    Cause::Policy(super::http_policy::Reason::Cancelled)
                }
                _ => Cause::AccessUnavailable,
            };
            diagnostic::report(Event::NotCacheable, reason);
        })?;
    let online = || Selection {
        source: PlaybackSource::remote(remote.clone()),
        fill: None,
    };
    let Some((cache, lease)) = binding.filter(|_| authenticated) else {
        return Ok(online());
    };
    let Some(mut prepared) = lookup.and_then(|lookup| lookup.ready()) else {
        diagnostic::report(Event::NotCacheable, Cause::Busy);
        return Ok(online());
    };
    let mut storage_refused = false;
    if prepared.has_file() {
        if let Err(reason) = prepared.revalidate_http(&resolver, &remote, token, cancellation) {
            storage_refused = matches!(
                reason,
                super::http_policy::Reason::NoStore
                    | super::http_policy::Reason::ContentRestricted
                    | super::http_policy::Reason::AccessDenied
            );
            diagnostic::report(Event::Stale, Cause::Policy(reason));
            if matches!(
                reason,
                super::http_policy::Reason::NoStore
                    | super::http_policy::Reason::ContentRestricted
                    | super::http_policy::Reason::AccessDenied
                    | super::http_policy::Reason::RepresentationChanged
                    | super::http_policy::Reason::ResourceChanged
                    | super::http_policy::Reason::DigestMismatch
            ) && prepared.retire_record().is_err()
            {
                diagnostic::report(Event::NotCacheable, Cause::Busy);
            }
            prepared.discard_file();
        }
    } else {
        diagnostic::report(Event::Miss, Cause::Empty);
    }
    match prepared.admit(|| !cancellation.cancelled(token) && !remote.is_expired()) {
        Ok(Some(hit)) => Ok(Selection {
            source: PlaybackSource::cached_file(hit, remote.clone())
                .unwrap_or_else(|_| PlaybackSource::remote(remote)),
            fill: None,
        }),
        Ok(None) if storage_refused => Ok(online()),
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
    let Some(mut request) = request else {
        diagnostic::report(Event::NotCacheable, Cause::Busy);
        return Selection {
            source: PlaybackSource::remote(remote),
            fill: None,
        };
    };
    let private_http = cache.0.authority.private_http();
    let (capture, reader) = if private_http {
        super::capture::channel_http(request.cancellation())
    } else {
        super::capture::channel(request.cancellation())
    };
    capture.require_playing();
    let active_token = token.clone();
    let active_resolution = cancellation.clone();
    if cache
        .retain_capture(&mut request, capture.clone(), move || {
            !active_resolution.cancelled(&active_token)
        })
        .is_err()
    {
        diagnostic::report(Event::NotCacheable, Cause::AdmissionRevoked);
        return Selection {
            source: PlaybackSource::remote(remote),
            fill: None,
        };
    }
    let control = FillControl {
        cancellation: request.cancellation(),
        capture: capture.clone(),
        reason: std::sync::Arc::new(std::sync::OnceLock::new()),
        reported: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        finalizing_reported: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };
    let http = private_http.then(|| {
        (
            capture.clone(),
            remote.http_cache_uri_hash(),
            remote.http_cache_resource_hash(),
        )
    });
    Selection {
        source: PlaybackSource::remote(remote).with_cache_capture(capture),
        fill: Some(Fill {
            cache,
            request,
            reader,
            token: token.clone(),
            cancellation: cancellation.clone(),
            http,
            control,
        }),
    }
}

/// Drain from preparation; publication requires complete input and native Playing.
/// Its reader is fed by the existing GStreamer HTTP source, never a second GET.
pub(crate) struct Fill {
    cache: AudioCache,
    request: Request,
    reader: super::capture::CaptureReader,
    token: AccessToken,
    cancellation: DeliveryCancellation,
    http: Option<(super::capture::Capture, [u8; 32], [u8; 32])>,
    control: FillControl,
}
#[derive(Clone, Copy)]
pub(crate) enum Retirement {
    Replaced,
    Eos,
    Stop,
    Account,
}
#[derive(Clone)]
pub(crate) struct FillControl {
    cancellation: Cancellation,
    capture: super::capture::Capture,
    reason: std::sync::Arc<std::sync::OnceLock<Cause>>,
    reported: std::sync::Arc<std::sync::atomic::AtomicBool>,
    finalizing_reported: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl FillControl {
    #[cfg(test)]
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
    pub(crate) fn retire(&self, why: Retirement) -> bool {
        if matches!(why, Retirement::Replaced | Retirement::Eos)
            && self.capture.is_complete()
            && self.capture.has_played()
            && !self.cancellation.is_cancelled()
        {
            if !self
                .finalizing_reported
                .swap(true, std::sync::atomic::Ordering::AcqRel)
            {
                diagnostic::report(
                    Event::Captured,
                    if matches!(why, Retirement::Eos) {
                        Cause::EosFinalizing
                    } else {
                        Cause::TrackFinalizing
                    },
                );
            }
            return true;
        }
        let reason = self
            .capture
            .failure()
            .map(Cause::Capture)
            .unwrap_or(match why {
                Retirement::Replaced => Cause::TrackReplaced,
                Retirement::Eos => Cause::NaturalEos,
                Retirement::Stop => Cause::UserStop,
                Retirement::Account => Cause::AccountChanged,
            });
        let _ = self.reason.set(reason);
        self.cancellation.cancel();
        self.report_failure(reason);
        false
    }
    fn report_failure(&self, reason: Cause) {
        if !self
            .reported
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            diagnostic::report(Event::NotCacheable, reason);
        }
    }
}
impl Fill {
    pub(crate) fn control(&self) -> FillControl {
        self.control.clone()
    }
    #[cfg(test)]
    pub(crate) fn cancellation(&self) -> Cancellation {
        self.request.cancellation()
    }

    pub(crate) fn run(self) -> Result<(), CacheError> {
        let Self {
            cache,
            request,
            reader,
            token,
            cancellation,
            http,
            control,
        } = self;
        let lease = request.lease.clone();
        let permit = request.permit;
        let write = request.write.clone();
        let guard_token = token.clone();
        let mut rejection = None;
        let result = (|| {
            cache.check_request(&request)?;
            let reader = GuardedCapture {
                cache: cache.clone(),
                lease: lease.clone(),
                permit,
                write: write.clone(),
                reader,
                token,
                cancellation,
            };
            if let Some((capture, uri, resource)) = http {
                cache.put_reader_http(request, reader, |sha, bytes| {
                    capture
                        .completed_http_policy()
                        .ok_or(CacheError::HttpPolicyRequired)?
                        .record(uri, resource, sha, bytes)
                        .map_err(|reason| {
                            rejection = Some(reason);
                            CacheError::HttpPolicyRequired
                        })
                })
            } else {
                cache.put_reader(request, reader)
            }
        })();
        let http_rejection = rejection.or(control.capture.http_rejection());
        if matches!(
            http_rejection,
            Some(
                super::http_policy::Reason::NoStore | super::http_policy::Reason::ContentRestricted
            )
        ) {
            cache.revoke_resource_writes(&lease, permit.identity, permit.revision);
            // Only the same still-current revision may retire an older record.
            if let Ok(_io) = cache.io()
                && cache.check(&lease, permit).is_ok()
            {
                let _ = super::remove_record(&cache.path(permit.identity.key()));
            }
        }
        match &result {
            Ok(()) => {
                if !control
                    .reported
                    .swap(true, std::sync::atomic::Ordering::AcqRel)
                {
                    diagnostic::report(Event::WriteConfirmed, Cause::Complete);
                }
            }
            Err(error) => {
                let phase = write.as_ref().map_or(0, |write| {
                    write.phase.load(std::sync::atomic::Ordering::Acquire)
                });
                let cause = control
                    .reason
                    .get()
                    .copied()
                    .or_else(|| http_rejection.map(Cause::Policy))
                    .or_else(|| {
                        control
                            .capture
                            .failure()
                            .filter(|failure| {
                                !matches!(
                                    failure,
                                    super::capture::CaptureFailure::Cancelled
                                        | super::capture::CaptureFailure::AdmissionRetired
                                )
                            })
                            .map(Cause::Capture)
                    })
                    .or_else(|| cache.retired_lease_cause(&lease))
                    .or_else(|| {
                        guard_token
                            .is_expired()
                            .then_some(Cause::PlaybackAccessExpired)
                    })
                    .or_else(|| control.capture.failure().map(Cause::Capture))
                    .unwrap_or_else(|| {
                        if guard_token.is_expired() {
                            return Cause::PlaybackAccessExpired;
                        }
                        match error {
                            CacheError::Revoked | CacheError::NoGrant | CacheError::Expired => {
                                Cause::AdmissionRevoked
                            }
                            CacheError::Cancelled if phase > 0 => Cause::FinalizationCancelled,
                            CacheError::Cancelled => {
                                Cause::Policy(super::http_policy::Reason::Cancelled)
                            }
                            CacheError::Limit => Cause::Limit,
                            CacheError::Busy => Cause::Busy,
                            _ if phase >= 2 => Cause::AtomicCommitFailed,
                            _ => Cause::WriteFailed,
                        }
                    });
                control.report_failure(cause);
            }
        }
        result
    }
}
struct GuardedCapture {
    cache: AudioCache,
    lease: Lease,
    permit: super::StorageGrant,
    write: Option<std::sync::Arc<super::WriteOperation>>,
    reader: super::capture::CaptureReader,
    token: AccessToken,
    cancellation: DeliveryCancellation,
}
impl std::io::Read for GuardedCapture {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read_checked(out, || {
            !self.cancellation.cancelled(&self.token)
                && self
                    .cache
                    .check_write_operation(&self.lease, self.permit, self.write.as_deref())
                    .is_ok()
        })
    }
}

#[cfg(test)]
mod tests;
