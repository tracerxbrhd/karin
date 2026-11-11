// SPDX-License-Identifier: GPL-3.0-or-later
//! Metadata-only validation of an already resolved, account-admitted MP3.
//!
//! Reuses the resolver's bounded HTTP client, never sends credentials, never
//! follows redirects and never reads a media response body. An unsupported
//! validator leaves ordinary playback online rather than inventing identity.

use reqwest::{
    blocking::{Client, RequestBuilder, Response},
    header::{
        ACCEPT_ENCODING, CACHE_CONTROL, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH, RANGE,
        USER_AGENT,
    },
};
use std::time::Instant;

use super::{DeliveryCancellation, Duration};
use crate::{
    audio_cache::http_policy::{Metadata, Reason, ResponsePolicy, Validation},
    playback::RemotePlaybackSource,
    session::AccessToken,
};

const PROBE_TIMEOUT: Duration = Duration::from_millis(900);
const MAX_HEADERS: usize = 64;
const MAX_HEADER_BYTES: usize = 8 * 1024;

pub(super) fn revalidate(
    client: &Client,
    remote: &RemotePlaybackSource,
    stored: &Metadata,
    token: &AccessToken,
    cancellation: &DeliveryCancellation,
) -> Result<Validation, Reason> {
    current(remote, token, cancellation)?;
    let url = remote
        .http_cache_probe_url()
        .map_err(|_| Reason::UnsupportedStatus)?;
    let request = conditionals(client.head(url.clone()), stored, remote)?;
    let (response, started) = send(request, remote, token, cancellation)?;
    let status = response.status().as_u16();
    let validation = if matches!(status, 405 | 501) {
        // A range probe is useful only when HEAD is unsupported. A conforming
        // server sends one byte; a server ignoring Range is immediately dropped
        // without consuming its body, and cannot establish a local hit.
        drop(response);
        current(remote, token, cancellation)?;
        let request = conditionals(client.get(url).header(RANGE, "bytes=0-0"), stored, remote)?;
        let (response, started) = send(request, remote, token, cancellation)?;
        let status = response.status().as_u16();
        if !matches!(status, 206 | 304) {
            return Err(if status == 451 {
                Reason::ContentRestricted
            } else if matches!(status, 401 | 403) {
                Reason::AccessDenied
            } else {
                Reason::RangeUnsupported
            });
        }
        validate_response(response, started, stored, remote)?
    } else {
        if status == 451 {
            return Err(Reason::ContentRestricted);
        }
        if matches!(status, 401 | 403) {
            return Err(Reason::AccessDenied);
        }
        if !matches!(status, 200 | 304) {
            return Err(Reason::UnsupportedStatus);
        }
        validate_response(response, started, stored, remote)?
    };
    current(remote, token, cancellation)?;
    Ok(validation)
}

fn current(
    remote: &RemotePlaybackSource,
    token: &AccessToken,
    cancellation: &DeliveryCancellation,
) -> Result<(), Reason> {
    if cancellation.cancelled(token) {
        Err(Reason::Cancelled)
    } else if remote.is_expired() {
        Err(Reason::SourceExpired)
    } else {
        Ok(())
    }
}

fn conditionals(
    request: RequestBuilder,
    stored: &Metadata,
    remote: &RemotePlaybackSource,
) -> Result<RequestBuilder, Reason> {
    let request = request
        .timeout(PROBE_TIMEOUT)
        .header(
            USER_AGENT,
            concat!("Karin/", env!("CARGO_PKG_VERSION"), " playback"),
        )
        .header(ACCEPT_ENCODING, "identity")
        // A supporting origin can supply full selected-representation proof on
        // this same metadata request. Absence never weakens the admission rule.
        .header("Want-Repr-Digest", "sha-256=1")
        // A successful fresh access check must not be paired with a silently
        // stale intermediary response. No cookie or Authorization is supplied.
        .header(CACHE_CONTROL, "no-cache");
    if !stored.validator_scope(remote) {
        // A logical object can be served by independent replicas. Never send
        // its old conditional to another physical validator scope; a complete
        // Repr-Digest may independently establish byte equivalence there.
        return Ok(request);
    }
    if let Some(etag) = stored
        .etag
        .as_deref()
        .filter(|value| value.starts_with('"') && value.ends_with('"'))
    {
        let value = HeaderValue::from_str(etag).map_err(|_| Reason::HeadersInvalid)?;
        Ok(request.header(IF_NONE_MATCH, value))
    } else if let Some(modified) = stored
        .last_modified
        .as_deref()
        .filter(|_| stored.strong_modified())
    {
        let value = HeaderValue::from_str(modified).map_err(|_| Reason::HeadersInvalid)?;
        Ok(request.header(IF_MODIFIED_SINCE, value))
    } else {
        Ok(request)
    }
}

fn send(
    request: RequestBuilder,
    remote: &RemotePlaybackSource,
    token: &AccessToken,
    cancellation: &DeliveryCancellation,
) -> Result<(Response, Instant), Reason> {
    current(remote, token, cancellation)?;
    let started = Instant::now();
    let result = request.send();
    current(remote, token, cancellation)?;
    result.map(|response| (response, started)).map_err(|error| {
        if error.is_timeout() {
            Reason::Timeout
        } else {
            Reason::Network
        }
    })
}

fn validate_response(
    response: Response,
    started: Instant,
    stored: &Metadata,
    remote: &RemotePlaybackSource,
) -> Result<Validation, Reason> {
    let headers = selected_headers(&response)?;
    let status = response.status().as_u16();
    // In particular, do not call bytes/text/read, even when a broken HEAD or
    // Range implementation has attached an entire media body to the response.
    drop(response);
    ResponsePolicy::parse(status, headers)?
        .observed_now(started.elapsed())
        .revalidate_source(stored, remote)
}

pub(super) fn selected_headers(response: &Response) -> Result<Vec<(String, String)>, Reason> {
    if response.headers().len() > MAX_HEADERS {
        return Err(Reason::ResponseTooLarge);
    }
    let mut result = Vec::new();
    let mut size = 0_usize;
    for (name, value) in response.headers() {
        if ![
            "cache-control",
            "pragma",
            "expires",
            "age",
            "vary",
            "content-type",
            "content-length",
            "content-range",
            "content-encoding",
            "etag",
            "last-modified",
            "date",
            "repr-digest",
        ]
        .contains(&name.as_str())
        {
            continue;
        }
        size = size
            .checked_add(name.as_str().len() + value.as_bytes().len())
            .filter(|size| *size <= MAX_HEADER_BYTES)
            .ok_or(Reason::ResponseTooLarge)?;
        let value = value.to_str().map_err(|_| Reason::HeadersInvalid)?;
        result.push((name.as_str().to_owned(), value.to_owned()));
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
