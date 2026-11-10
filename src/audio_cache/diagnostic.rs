// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in finite cache results; never identifiers, URLs or network payloads.

#[derive(Clone, Copy)]
pub(crate) enum Event {
    Miss,
    WriteConfirmed,
    Hit,
    Stale,
    NotCacheable,
    Queued,
    Downloading,
    Downloaded,
    Checking,
}

#[derive(Clone, Copy)]
pub(crate) enum Cause {
    Empty,
    Complete,
    Validated,
    Busy,
    Limit,
    WriteFailed,
    AccessUnavailable,
    Decode,
    DownloadComplete,
    FullResponse,
    PlayRequested,
    SourceOriginUnverified,
    SignedUrlChanged,
    CdnHostChanged,
    DescriptorPathChanged,
    EncodingChanged,
    LogicalObjectChanged,
    BindingUpgrade,
    RecordRetained,
    UserStop,
    AccountChanged,
    CacheCleared,
    CacheDisabled,
    PolicyChanged,
    AdmissionRevoked,
    FinalizationCancelled,
    AtomicCommitFailed,
    PlaybackAccessExpired,
    Policy(super::http_policy::Reason),
}
impl Cause {
    fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "record-absent",
            Self::Complete => "complete-response",
            Self::Validated => "representation-validated",
            Self::Busy => "cache-unavailable",
            Self::Limit => "cache-limit",
            Self::WriteFailed => "cache-write-failed",
            Self::AccessUnavailable => "playback-access-unavailable",
            Self::Decode => "cache-decode-failed",
            Self::DownloadComplete => "download-complete",
            Self::FullResponse => "full-response",
            Self::PlayRequested => "play-requested",
            Self::SourceOriginUnverified => "media-origin-unverified",
            Self::SignedUrlChanged => "signed-url-changed",
            Self::CdnHostChanged => "cdn-host-changed",
            Self::DescriptorPathChanged => "descriptor-path-changed",
            Self::EncodingChanged => "encoding-changed",
            Self::LogicalObjectChanged => "logical-object-changed",
            Self::BindingUpgrade => "binding-upgrade",
            Self::RecordRetained => "record-retained",
            Self::UserStop => "user-stop",
            Self::AccountChanged => "account-changed",
            Self::CacheCleared => "cache-cleared",
            Self::CacheDisabled => "cache-disabled",
            Self::PolicyChanged => "cache-policy-changed",
            Self::AdmissionRevoked => "admission-revoked",
            Self::FinalizationCancelled => "finalization-cancelled",
            Self::AtomicCommitFailed => "atomic-commit-failed",
            Self::PlaybackAccessExpired => "playback-access-expired",
            Self::Policy(reason) => reason.as_str(),
        }
    }
}

pub(crate) fn report(event: Event, reason: Cause) {
    if std::env::var_os("KARIN_AUDIO_CACHE_DIAGNOSTICS").as_deref()
        != Some(std::ffi::OsStr::new("1"))
    {
        return;
    }
    let cache = match event {
        Event::Miss => "miss",
        Event::WriteConfirmed => "write-confirmed",
        Event::Hit => "hit",
        Event::Stale => "stale",
        Event::NotCacheable => "not-cacheable",
        Event::Queued => "queued",
        Event::Downloading => "downloading",
        Event::Downloaded => "downloaded",
        Event::Checking => "checking",
    };
    let source = if matches!(event, Event::Hit) {
        "local"
    } else {
        "remote"
    };
    eprintln!("cache={cache} source={source} reason={}", reason.as_str());
}

/// Classify only opaque equality comparisons; never format the underlying
/// descriptor, URI, validators, account or track. Several dimensions may change.
pub(crate) fn identity(
    stored: &super::http_policy::Metadata,
    remote: &crate::playback::RemotePlaybackSource,
) {
    if let Some((before, now)) = stored.binding.zip(remote.http_cache_binding()) {
        if before
            .logical
            .zip(now.logical)
            .is_some_and(|(before, now)| before != now)
        {
            report(Event::Checking, Cause::LogicalObjectChanged);
        }
        if before.host != now.host {
            report(Event::Checking, Cause::CdnHostChanged);
        }
        if before.path != now.path {
            report(Event::Checking, Cause::DescriptorPathChanged);
        }
        if before.variant != now.variant {
            report(Event::Checking, Cause::EncodingChanged);
        }
    }
    if stored.uri_hash != remote.http_cache_uri_hash()
        && stored.resource_hash == remote.http_cache_resource_hash()
        && signing_binding_only(stored.binding, remote.http_cache_binding())
    {
        report(Event::Checking, Cause::SignedUrlChanged);
    }
}

fn signing_binding_only(
    before: Option<super::http_policy::ResourceBinding>,
    now: Option<super::http_policy::ResourceBinding>,
) -> bool {
    match (before, now) {
        (Some(before), Some(now)) => before == now,
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_cache::http_policy::ResourceBinding;

    #[test]
    fn signing_only_never_masks_a_logical_object_or_variant_change() {
        let binding = ResourceBinding {
            logical: Some([1; 32]),
            host: [2; 32],
            path: [3; 32],
            variant: [4; 32],
        };
        assert!(signing_binding_only(Some(binding), Some(binding)));
        assert!(!signing_binding_only(
            Some(binding),
            Some(ResourceBinding {
                logical: Some([9; 32]),
                ..binding
            })
        ));
        assert!(!signing_binding_only(
            Some(binding),
            Some(ResourceBinding {
                variant: [9; 32],
                ..binding
            })
        ));
        assert!(!signing_binding_only(
            Some(binding),
            Some(ResourceBinding {
                host: [9; 32],
                ..binding
            })
        ));
        assert!(!signing_binding_only(Some(binding), None));
        assert!(signing_binding_only(None, None));
    }
}
