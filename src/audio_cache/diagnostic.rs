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
    };
    let source = if matches!(event, Event::Hit) {
        "local"
    } else {
        "remote"
    };
    eprintln!("cache={cache} source={source} reason={}", reason.as_str());
}
