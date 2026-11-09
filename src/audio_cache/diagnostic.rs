// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in finite cache results; never identifiers, URLs or network payloads.

#[derive(Clone, Copy)]
pub(crate) enum Event {
    Miss,
    WriteConfirmed,
    Hit,
    Stale,
    NotCacheable,
    Captured,
    Completing,
    Observed,
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
    CaptureComplete,
    Capture(super::capture::CaptureFailure),
    UserStop,
    TrackReplaced,
    NaturalEos,
    AutomaticNextIncomplete,
    WaveEosIncomplete,
    UserSkipIncomplete,
    WaveSkipIncomplete,
    WaveReplacementIncomplete,
    BindingChanged,
    SourceError,
    UserSeek,
    BackgroundCompletion,
    CaptureState(super::capture::CaptureObservation),
    Transition(super::bridge::Retirement),
    TrackFinalizing,
    EosFinalizing,
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
            Self::CaptureComplete => "capture-complete",
            Self::Capture(reason) => reason.as_str(),
            Self::UserStop => "user-stop",
            Self::TrackReplaced => "track-replaced-incomplete",
            Self::NaturalEos => "natural-eos-incomplete",
            Self::AutomaticNextIncomplete => "automatic-next-before-completion",
            Self::WaveEosIncomplete => "wave-eos-before-completion",
            Self::UserSkipIncomplete => "user-skip-before-completion",
            Self::WaveSkipIncomplete => "wave-user-skip-before-completion",
            Self::WaveReplacementIncomplete => "wave-replacement-before-completion",
            Self::BindingChanged => "playback-binding-changed",
            Self::SourceError => "playback-source-failed",
            Self::UserSeek => "user-seek",
            Self::BackgroundCompletion => "natural-eos-full-fetch",
            Self::CaptureState(state) => state.as_str(),
            Self::Transition(origin) => match origin {
                super::bridge::Retirement::Replaced => "transition-user-context",
                super::bridge::Retirement::Eos => "transition-native-eos",
                super::bridge::Retirement::AutomaticNext => "transition-automatic-next",
                super::bridge::Retirement::WaveEos => "transition-wave-eos",
                super::bridge::Retirement::UserSkip => "transition-user-skip",
                super::bridge::Retirement::WaveSkip => "transition-wave-user-skip",
                super::bridge::Retirement::WaveReplacement => "transition-wave-source",
                super::bridge::Retirement::BindingChanged => "transition-binding-changed",
                super::bridge::Retirement::SourceError => "transition-source-error",
                super::bridge::Retirement::Seek => "transition-user-seek",
                super::bridge::Retirement::Stop => "transition-user-stop",
                super::bridge::Retirement::Account => "transition-account",
            },
            Self::TrackFinalizing => "track-replaced-finalizing",
            Self::EosFinalizing => "natural-eos-finalizing",
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
        Event::Captured => "captured",
        Event::Completing => "completing",
        Event::Observed => "observed",
    };
    let source = if matches!(event, Event::Hit) {
        "local"
    } else {
        "remote"
    };
    eprintln!("cache={cache} source={source} reason={}", reason.as_str());
}
