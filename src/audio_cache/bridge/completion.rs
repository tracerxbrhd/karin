// SPDX-License-Identifier: GPL-3.0-or-later
//! One bounded full-response retry for a genuinely ended, played capture.
//! The first staging transaction has already unwound before this starts: no
//! captured prefix is ever joined with bytes from the independent response.

use std::{
    io::{self, Read},
    thread,
    time::{Duration, Instant},
};

use super::*;
use crate::audio_cache::http_policy::{Metadata, Reason};

pub(super) struct Source {
    pub resolver: StreamResolver,
    pub service: MusicService,
    pub remote: RemotePlaybackSource,
    pub track: String,
}

pub(super) fn retry(
    source: &Source,
    cache: &AudioCache,
    request: &Request,
    token: &AccessToken,
    cancellation: &DeliveryCancellation,
    control: &FillControl,
    rejection: &mut Option<Reason>,
) -> Option<Result<(), CacheError>> {
    let permit = control.completion.as_ref()?;
    // This is the same finite active-write lifetime used while the original
    // capture reader waits for audio. An aborted pad capture can precede native
    // decoder EOS (e.g. parser-generated ranges); keep only this bounded request,
    // never a writer lock, until the true playback outcome is known.
    let deadline = Instant::now() + Duration::from_secs(30 * 60);
    loop {
        // A complete input needs no second network request. In particular, an
        // unrelated filesystem failure must not turn into an EOS wait/retry.
        if control.capture.is_complete() {
            return None;
        }
        if control.capture.can_complete_in_background() {
            break;
        }
        if !control.capture.can_wait_for_completion()
            || permit.current().is_err()
            || cancellation.cancelled(token)
            || Instant::now() >= deadline
        {
            return None;
        }
        thread::sleep(Duration::from_millis(10));
    }
    control.observe_capture();
    if !control.allow_completion() {
        return Some(Err(CacheError::Revoked));
    }
    let fetch = (|| {
        permit.current().map_err(|_| Reason::Cancelled)?;
        let scope = source
            .service
            .cache_scope(token)
            .map_err(|_| Reason::AccessDenied)?;
        if !request.lease.matches_scope(&scope) {
            return Err(Reason::AccessDenied);
        }
        // Refresh the account-bound delivery descriptor even if the old signed
        // source has expired. Do not mutate MusicService's shared account epoch.
        let remote = source
            .resolver
            .resolve(&source.service, token, &source.track, cancellation)
            .map_err(delivery_reason)?;
        permit.current().map_err(|_| Reason::Cancelled)?;
        if remote.http_cache_resource_hash() != source.remote.http_cache_resource_hash() {
            return Err(Reason::ResourceChanged);
        }
        let observed = control.capture.response_http_policy();
        let (response, policy) = source.resolver.cache_completion_response(
            &remote,
            observed.as_ref(),
            token,
            cancellation,
        )?;
        permit.current().map_err(|_| Reason::Cancelled)?;
        let length = policy.complete_mp3()?;
        Ok((response, policy, remote, length, observed))
    })();
    let (response, policy, remote, expected, observed) = match fetch {
        Ok(value) => value,
        Err(reason) => {
            *rejection = Some(reason);
            return Some(Err(CacheError::HttpPolicyRequired));
        }
    };
    let uri = remote.http_cache_uri_hash();
    let resource = remote.http_cache_resource_hash();
    let read_failure = std::sync::Arc::new(std::sync::Mutex::new(None));
    let reader = GuardedResponse {
        response,
        permit: permit.clone(),
        token: token.clone(),
        cancellation: cancellation.clone(),
        expected,
        received: 0,
        failure: read_failure.clone(),
    };
    let result = cache.put_reader_http_attempt(request, reader, |sha, bytes| {
        if observed
            .as_ref()
            .and_then(|policy| policy.repr_sha256)
            .is_some_and(|old| old != sha)
        {
            *rejection = Some(Reason::RepresentationChanged);
            return Err(CacheError::HttpPolicyRequired);
        }
        record(policy, uri, resource, sha, bytes).map_err(|reason| {
            *rejection = Some(reason);
            CacheError::HttpPolicyRequired
        })
    });
    if result.is_err() && rejection.is_none() {
        *rejection = read_failure.lock().ok().and_then(|failure| *failure);
    }
    Some(result)
}

fn record(
    policy: super::super::http_policy::ResponsePolicy,
    uri: [u8; 32],
    resource: [u8; 32],
    sha: [u8; 32],
    bytes: u64,
) -> Result<Metadata, Reason> {
    policy.record(uri, resource, sha, bytes)
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

struct GuardedResponse {
    response: reqwest::blocking::Response,
    permit: super::super::CompletionPermit,
    token: AccessToken,
    cancellation: DeliveryCancellation,
    expected: u64,
    received: u64,
    failure: std::sync::Arc<std::sync::Mutex<Option<Reason>>>,
}
impl GuardedResponse {
    fn current(&self) -> io::Result<()> {
        if self.cancellation.cancelled(&self.token) || self.permit.current().is_err() {
            self.failed(Reason::Cancelled);
            Err(io::Error::from(io::ErrorKind::Interrupted))
        } else {
            Ok(())
        }
    }
    fn failed(&self, reason: Reason) {
        if let Ok(mut failure) = self.failure.lock()
            && failure.is_none()
        {
            *failure = Some(reason);
        }
    }
}
impl Read for GuardedResponse {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.current()?;
        let count = self.response.read(out);
        self.current()?;
        let count = count.inspect_err(|error| {
            self.failed(
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) {
                    Reason::Timeout
                } else {
                    Reason::Network
                },
            );
        })?;
        let received = self
            .received
            .checked_add(count as u64)
            .filter(|received| *received <= self.expected);
        let Some(received) = received else {
            self.failed(Reason::InvalidLength);
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        };
        self.received = received;
        if count == 0 && self.received != self.expected {
            self.failed(Reason::InvalidLength);
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        Ok(count)
    }
}
