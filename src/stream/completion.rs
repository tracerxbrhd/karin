// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded whole-response completion after the one player's natural EOS.
//!
//! No partial bytes are joined, no cookies/authorization are sent and the
//! resolver's existing no-redirect/no-retry client is reused. A new full response
//! must satisfy the same private HTTP policy as the original source capture.

use reqwest::{
    blocking::{Client, Response},
    header::{ACCEPT_ENCODING, CACHE_CONTROL, IF_MATCH, IF_UNMODIFIED_SINCE, USER_AGENT},
};

use super::{DeliveryCancellation, Duration};
use crate::{
    audio_cache::http_policy::{Reason, ResponsePolicy},
    playback::RemotePlaybackSource,
    session::AccessToken,
};

const COMPLETION_HTTP_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) fn response(
    client: &Client,
    remote: &RemotePlaybackSource,
    observed: Option<&ResponsePolicy>,
    token: &AccessToken,
    cancellation: &DeliveryCancellation,
) -> Result<(Response, ResponsePolicy), Reason> {
    current(remote, token, cancellation)?;
    let url = remote
        .http_cache_probe_url()
        .map_err(|_| Reason::UnsupportedStatus)?;
    let mut request = client
        .get(url)
        .timeout(COMPLETION_HTTP_TIMEOUT)
        .header(
            USER_AGENT,
            concat!("Karin/", env!("CARGO_PKG_VERSION"), " playback"),
        )
        .header(ACCEPT_ENCODING, "identity")
        .header(CACHE_CONTROL, "no-cache");
    if let Some(observed) = observed {
        if let Some(etag) = &observed.etag {
            request = request.header(IF_MATCH, etag);
        } else if let Some(modified) = &observed.last_modified
            && observed
                .modified_unix
                .zip(observed.date_unix)
                .is_some_and(|(modified, date)| {
                    date.checked_sub(modified).is_some_and(|age| age >= 60)
                })
        {
            request = request.header(IF_UNMODIFIED_SINCE, modified);
        }
    }
    let response = request.send();
    current(remote, token, cancellation)?;
    let response = response.map_err(|error| {
        if error.is_timeout() {
            Reason::Timeout
        } else {
            Reason::Network
        }
    })?;
    if response.status().as_u16() == 412 {
        return Err(Reason::RepresentationChanged);
    }
    let policy = ResponsePolicy::parse(
        response.status().as_u16(),
        super::revalidation::selected_headers(&response)?,
    )?;
    // Full 200 or a 206 covering the entire representation only. A server
    // cannot make us append a tail, reinterpret a manifest or accept gzip.
    policy.complete_mp3()?;
    if let Some(observed) = observed {
        same_representation(observed, &policy)?;
    }
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

fn same_representation(before: &ResponsePolicy, now: &ResponsePolicy) -> Result<(), Reason> {
    if before
        .representation_length()
        .is_some_and(|length| now.length != Some(length))
        || before
            .etag
            .as_ref()
            .is_some_and(|etag| now.etag.as_ref() != Some(etag))
        || before
            .modified_unix
            .is_some_and(|modified| now.modified_unix != Some(modified))
        || before
            .repr_sha256
            .is_some_and(|sha| now.repr_sha256.is_some_and(|current| current != sha))
    {
        return Err(Reason::RepresentationChanged);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(etag: &str, length: &str) -> ResponsePolicy {
        ResponsePolicy::parse(
            200,
            [
                ("content-type".into(), "audio/mpeg".into()),
                ("content-length".into(), length.into()),
                ("etag".into(), etag.into()),
            ],
        )
        .unwrap()
    }

    #[test]
    fn fresh_whole_response_must_not_replace_known_representation_fields() {
        let original = policy("\"original\"", "123");
        assert!(same_representation(&original, &policy("\"original\"", "123")).is_ok());
        assert_eq!(
            same_representation(&original, &policy("\"changed\"", "123")),
            Err(Reason::RepresentationChanged)
        );
        assert_eq!(
            same_representation(&original, &policy("\"original\"", "124")),
            Err(Reason::RepresentationChanged)
        );
        let missing = ResponsePolicy::parse(
            200,
            [
                ("content-type".into(), "audio/mpeg".into()),
                ("content-length".into(), "123".into()),
            ],
        )
        .unwrap();
        assert_eq!(
            same_representation(&original, &missing),
            Err(Reason::RepresentationChanged)
        );
    }
}
