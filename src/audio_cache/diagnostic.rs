// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in finite cache results; never identifiers, URLs or network payloads.

#[derive(Clone, Copy)]
pub(crate) enum Event {
    Miss,
    WriteConfirmed,
    Hit,
    Stale,
    NotCacheable,
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
    };
    let source = if matches!(event, Event::Hit) {
        "local"
    } else {
        "remote"
    };
    eprintln!("cache={cache} source={source} reason={}", reason.as_str());
}
