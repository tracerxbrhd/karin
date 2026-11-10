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

use super::http_policy::Reason;
use super::selection::Revalidation;
use std::{
    io::{self, Read},
    sync::{Arc, OnceLock},
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
                _ if report.category == crate::stream::DeliveryCategory::MediaOriginUnverified => {
                    Cause::SourceOriginUnverified
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
        // Speculative disk work is optional. A freshly authenticated source
        // still admits a bounded private HTTP write through memory-only state.
        let admitted = ResourceKey::track(track).ok().is_some_and(|key| {
            cache
                .admit_source_for_write(&lease, key, || {
                    !cancellation.cancelled(token) && !remote.is_expired()
                })
                .is_ok()
        });
        return Ok(if admitted {
            diagnostic::report(Event::Miss, Cause::Empty);
            fill_selection(
                cache,
                lease,
                token,
                cancellation,
                remote,
                track,
                resolver,
                service.clone(),
                true,
            )
        } else {
            online()
        });
    };
    let mut preserve_record = false;
    if prepared.has_file() {
        match prepared.revalidate_http(&resolver, &remote, token, cancellation) {
            Ok(Revalidation::Same) => {}
            Ok(Revalidation::Different(reason)) => {
                // Only positive current representation/version evidence may
                // replace an otherwise intact retained MP3.
                diagnostic::report(Event::Stale, Cause::Policy(reason));
                if prepared.retire_record().is_err() {
                    diagnostic::report(Event::NotCacheable, Cause::Busy);
                }
                prepared.discard_file();
            }
            Ok(Revalidation::Unproven(reason)) => {
                // Independent replica tags, missing proof and legacy binding
                // uncertainty are not evidence of changed or corrupt bytes.
                preserve_record = true;
                diagnostic::report(Event::Stale, Cause::Policy(reason));
                diagnostic::report(Event::Stale, Cause::RecordRetained);
                prepared.discard_file();
            }
            Err(reason) => {
                let refusal = matches!(
                    reason,
                    Reason::NoStore | Reason::ContentRestricted | Reason::AccessDenied
                );
                let corrupt = reason == Reason::Integrity;
                preserve_record = !corrupt;
                diagnostic::report(Event::Stale, Cause::Policy(reason));
                if (refusal || corrupt) && prepared.retire_record().is_err() {
                    diagnostic::report(Event::NotCacheable, Cause::Busy);
                }
                prepared.discard_file();
                if !refusal && !corrupt {
                    diagnostic::report(Event::Stale, Cause::RecordRetained);
                }
            }
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
        Ok(None) if preserve_record => Ok(online()),
        Ok(None) => Ok(fill_selection(
            cache,
            lease,
            token,
            cancellation,
            remote,
            track,
            resolver,
            service.clone(),
            false,
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

/// One independent full-response save for a selected track. Neither the
/// GStreamer source nor its EOS/seek/range lifecycle owns the cache transaction.
#[allow(clippy::too_many_arguments)]
fn fill_selection(
    cache: AudioCache,
    lease: Lease,
    token: &AccessToken,
    cancellation: &DeliveryCancellation,
    remote: RemotePlaybackSource,
    track: &str,
    resolver: StreamResolver,
    service: MusicService,
    speculative: bool,
) -> Selection {
    let online = || PlaybackSource::remote(remote.clone());
    if cancellation.cancelled(token) || remote.is_expired() {
        return Selection {
            source: online(),
            fill: None,
        };
    }
    let request = ResourceKey::track(track)
        .ok()
        .and_then(|key| cache.begin(&lease, key).ok());
    let Some(mut request) = request else {
        diagnostic::report(Event::NotCacheable, Cause::Busy);
        return Selection {
            source: online(),
            fill: None,
        };
    };
    // The resolution attempt belongs to the player; this selected write has
    // its own cancellation and finite lifetime. Ordinary Next cannot revoke it.
    let delivery = DeliveryCancellation::new();
    let active_delivery = delivery.clone();
    let active_token = token.clone();
    let active_service = service.clone();
    let active_lease = lease.clone();
    if cache
        .retain_write(&mut request, move || {
            !active_delivery.cancelled(&active_token)
                && active_service
                    .cache_scope(&active_token)
                    .is_ok_and(|scope| active_lease.matches_scope(&scope))
        })
        .is_err()
    {
        diagnostic::report(Event::NotCacheable, Cause::AdmissionRevoked);
        return Selection {
            source: online(),
            fill: None,
        };
    }
    let control = FillControl {
        cancellation: request.cancellation(),
        delivery: delivery.clone(),
        reason: Arc::new(OnceLock::new()),
    };
    Selection {
        source: online(),
        fill: Some(Fill {
            cache,
            request,
            token: token.clone(),
            cancellation: delivery,
            resolver,
            service,
            remote,
            track: track.to_owned(),
            speculative,
            control,
        }),
    }
}

pub(crate) struct Fill {
    cache: AudioCache,
    request: Request,
    token: AccessToken,
    cancellation: DeliveryCancellation,
    resolver: StreamResolver,
    service: MusicService,
    remote: RemotePlaybackSource,
    track: String,
    speculative: bool,
    control: FillControl,
}
#[derive(Clone)]
pub(crate) struct FillControl {
    cancellation: Cancellation,
    delivery: DeliveryCancellation,
    reason: Arc<OnceLock<Cause>>,
}
impl FillControl {
    #[cfg(test)]
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
    pub(crate) fn cancel(&self, reason: Cause) {
        let _ = self.reason.set(reason);
        self.cancellation.cancel();
        self.delivery.cancel();
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
        let mut rejection = None;
        let result = self.download(&mut rejection);
        match &result {
            Ok(true) => diagnostic::report(Event::WriteConfirmed, Cause::Complete),
            Ok(false) => diagnostic::report(Event::Stale, Cause::RecordRetained),
            Err(error) => {
                let phase = self.request.write.as_ref().map_or(0, |write| {
                    write.phase.load(std::sync::atomic::Ordering::Acquire)
                });
                let cause = self
                    .control
                    .reason
                    .get()
                    .copied()
                    .or_else(|| self.cache.retired_lease_cause(&self.request.lease))
                    .or_else(|| {
                        self.token
                            .is_expired()
                            .then_some(Cause::PlaybackAccessExpired)
                    })
                    .or_else(|| rejection.map(Cause::Policy))
                    .unwrap_or(match error {
                        CacheError::Revoked | CacheError::NoGrant | CacheError::Expired => {
                            Cause::AdmissionRevoked
                        }
                        CacheError::Cancelled if phase > 0 => Cause::FinalizationCancelled,
                        CacheError::Cancelled => Cause::Policy(Reason::Cancelled),
                        CacheError::Limit => Cause::Limit,
                        CacheError::Busy | CacheError::Duplicate => Cause::Busy,
                        _ if phase >= 2 => Cause::AtomicCommitFailed,
                        _ => Cause::WriteFailed,
                    });
                diagnostic::report(Event::NotCacheable, cause);
            }
        }
        result.map(|_| ())
    }

    /// A speculative lookup miss is not proof that the disk record is absent.
    /// Verify the candidate on this worker before issuing any media GET; modern
    /// complete records are retained until selection can revalidate them.
    fn retain_speculative_record(&self) -> Result<bool, CacheError> {
        if !self.speculative {
            return Ok(false);
        }
        let current = || {
            self.cache.lease_current(&self.request.lease)?;
            if self.request.cancellation().is_cancelled()
                || self.cancellation.cancelled(&self.token)
            {
                return Err(CacheError::Cancelled);
            }
            let scope = self
                .service
                .cache_scope(&self.token)
                .map_err(|_| CacheError::Cancelled)?;
            if !self.request.lease.matches_scope(&scope) {
                return Err(CacheError::Cancelled);
            }
            Ok(())
        };
        current()?;
        let identity = self.request.permit.identity;
        let path = self.cache.path(identity.key());
        // Non-mutating and outside the I/O/state gate: a slow integrity read
        // cannot block GTK or remove a record concurrently replaced by Clear.
        let complete = match super::inspect_record(
            &path,
            identity.key(),
            Some(identity.scope()),
            self.request.permit.content,
        ) {
            Ok(file) => file.is_some(),
            Err(CacheError::InvalidInput) => false,
            Err(error) => return Err(error),
        };
        let metadata = if complete {
            match super::read_http_record(&path) {
                Ok(metadata) => metadata,
                Err(CacheError::InvalidInput | CacheError::Limit) => None,
                Err(error) => return Err(error),
            }
        } else {
            None
        };
        current()?;
        // Missing optional binding metadata is not permission to download the
        // same whole audio again. Legacy sidecars migrate only after positive
        // equivalence, on the existing read/LRU worker without another GET.
        Ok(metadata.is_some())
    }

    fn download(&self, rejection: &mut Option<Reason>) -> Result<bool, CacheError> {
        if self.retain_speculative_record()? {
            return Ok(false);
        }
        self.cache.start_write(&self.request)?;
        diagnostic::report(Event::Downloading, Cause::FullResponse);
        let fetched = self.fetch();
        let (response, policy, remote) = fetched.map_err(|reason| {
            *rejection = Some(reason);
            CacheError::HttpPolicyRequired
        })?;
        let private_http = self.cache.0.authority.private_http();
        if private_http && !policy.has_validator() {
            *rejection = Some(Reason::ValidatorsMissing);
            return Err(CacheError::HttpPolicyRequired);
        }
        let expected = policy.complete_mp3().map_err(|reason| {
            *rejection = Some(reason);
            CacheError::HttpPolicyRequired
        })?;
        self.cache.check_request(&self.request)?;
        let failure = std::cell::Cell::new(None);
        let reader = WholeResponse {
            response,
            expected,
            received: 0,
            failure: &failure,
        };
        let result = if private_http {
            self.cache
                .put_reader_http_attempt(&self.request, reader, |sha, bytes| {
                    policy.record_source(&remote, sha, bytes).map_err(|reason| {
                        *rejection = Some(reason);
                        CacheError::HttpPolicyRequired
                    })
                })
        } else {
            self.cache.put_reader_attempt(&self.request, reader)
        };
        if result.is_err() && rejection.is_none() {
            *rejection = failure.get();
        }
        result.map(|_| true)
    }

    fn fetch(
        &self,
    ) -> Result<
        (
            reqwest::blocking::Response,
            super::http_policy::ResponsePolicy,
            RemotePlaybackSource,
        ),
        Reason,
    > {
        self.current()?;
        let mut remote = self.remote.clone();
        let mut refreshed = false;
        if remote.is_expired() {
            remote = self.refresh()?;
            refreshed = true;
        }
        loop {
            self.current()?;
            match self
                .resolver
                .download_response(&remote, &self.token, &self.cancellation)
            {
                Ok((response, policy)) => {
                    self.current()?;
                    return Ok((response, policy, remote));
                }
                Err(error) if !refreshed && matches!(error.status, Some(403 | 410)) => {
                    let renewed = self.refresh()?;
                    if renewed.http_cache_uri_hash() == remote.http_cache_uri_hash() {
                        return Err(error.reason);
                    }
                    remote = renewed;
                    refreshed = true;
                }
                Err(error) => return Err(error.reason),
            }
        }
    }
    fn refresh(&self) -> Result<RemotePlaybackSource, Reason> {
        self.current()?;
        let remote = self
            .resolver
            .resolve(&self.service, &self.token, &self.track, &self.cancellation)
            .map_err(delivery_reason)?;
        self.current()?;
        Ok(remote)
    }
    fn current(&self) -> Result<(), Reason> {
        if self.cancellation.cancelled(&self.token)
            || self.cache.check_request(&self.request).is_err()
        {
            return Err(Reason::Cancelled);
        }
        let scope = self
            .service
            .cache_scope(&self.token)
            .map_err(|_| Reason::AccessDenied)?;
        if !self.request.lease.matches_scope(&scope) {
            return Err(Reason::AccessDenied);
        }
        Ok(())
    }
}
fn delivery_reason(report: DeliveryReport) -> Reason {
    use crate::stream::DeliveryCategory;
    match report.http_status {
        Some(401 | 403) => Reason::AccessDenied,
        Some(451) => Reason::ContentRestricted,
        _ => match report.category {
            DeliveryCategory::Cancelled => Reason::Cancelled,
            DeliveryCategory::Timeout => Reason::Timeout,
            DeliveryCategory::SourceExpired => Reason::SourceExpired,
            DeliveryCategory::SessionRequired | DeliveryCategory::AccessDenied => {
                Reason::AccessDenied
            }
            DeliveryCategory::Network => Reason::Network,
            _ => Reason::UnsupportedStatus,
        },
    }
}
struct WholeResponse<'a> {
    response: reqwest::blocking::Response,
    expected: u64,
    received: u64,
    failure: &'a std::cell::Cell<Option<Reason>>,
}
impl Read for WholeResponse<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let count = self.response.read(out).inspect_err(|error| {
            self.failure.set(Some(
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) {
                    Reason::Timeout
                } else {
                    Reason::Network
                },
            ));
        })?;
        let Some(received) = self
            .received
            .checked_add(count as u64)
            .filter(|n| *n <= self.expected)
        else {
            self.failure.set(Some(Reason::InvalidLength));
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        };
        self.received = received;
        if count == 0 && received != self.expected {
            self.failure.set(Some(Reason::InvalidLength));
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests;
