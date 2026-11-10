// SPDX-License-Identifier: GPL-3.0-or-later
//! One bounded complete HTTP response for an independent cache download.
//!
//! Uses the resolver's existing no-redirect/no-retry client and its timeout. No
//! Authorization, cookies, prefix/range requests or native playback state enter
//! this request; only the resolver's sealed short-lived resource is accepted.

use reqwest::{
    blocking::{Client, Response},
    header::{ACCEPT_ENCODING, CACHE_CONTROL, USER_AGENT},
};

use super::DeliveryCancellation;
use crate::{
    audio_cache::http_policy::{Reason, ResponsePolicy},
    playback::RemotePlaybackSource,
    session::AccessToken,
};

/// Deliberately contains no transport error, URL or response header/body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DownloadError {
    pub(crate) reason: Reason,
    pub(crate) status: Option<u16>,
}

pub(super) fn response(
    client: &Client,
    remote: &RemotePlaybackSource,
    token: &AccessToken,
    cancellation: &DeliveryCancellation,
) -> Result<(Response, ResponsePolicy), DownloadError> {
    current(remote, token, cancellation).map_err(|reason| DownloadError {
        reason,
        status: None,
    })?;
    let url = remote.http_cache_probe_url().map_err(|_| DownloadError {
        reason: Reason::UnsupportedStatus,
        status: None,
    })?;
    let sent = client
        .get(url)
        .header(
            USER_AGENT,
            concat!("Karin/", env!("CARGO_PKG_VERSION"), " playback"),
        )
        .header(ACCEPT_ENCODING, "identity")
        .header(CACHE_CONTROL, "no-cache")
        .send();
    let status = sent
        .as_ref()
        .ok()
        .map(|response| response.status().as_u16());
    current(remote, token, cancellation).map_err(|reason| DownloadError { reason, status })?;
    let response = sent.map_err(|error| DownloadError {
        reason: if error.is_timeout() {
            Reason::Timeout
        } else {
            Reason::Network
        },
        status: None,
    })?;
    let status = response.status().as_u16();
    let fail = |reason| DownloadError {
        reason,
        status: Some(status),
    };
    let headers = super::revalidation::selected_headers(&response).map_err(fail)?;
    let policy = ResponsePolicy::parse(status, headers).map_err(fail)?;
    // Only an entire selected representation is eligible. Never append a tail,
    // reinterpret a manifest, decode transfer content or accept gzip here.
    policy.complete_mp3().map_err(fail)?;
    Ok((response, policy))
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

#[cfg(test)]
mod tests;
