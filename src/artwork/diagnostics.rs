// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in, finite artwork outcomes. Inputs deliberately cannot carry a URL,
//! provider body, account field, image identifier, credential or error string.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    MissingMetadata,
    UriRejected,
    VisibilityDeferred,
    RequestNotStarted,
    Queued,
    QueueBusy,
    MemoryHit,
    DiskHit,
    CacheMiss,
    CacheReadFailure,
    CacheWriteFailure,
    CacheBypassed,
    NetworkDns,
    NetworkTls,
    NetworkTimeout,
    NetworkConnection,
    NetworkOther,
    HttpStatus(u16),
    HttpRefused,
    UnsupportedContentType,
    DecoderUnavailable,
    DecodeFailure,
    TooLarge,
    Cancelled,
    PixelsReady,
    Displayed,
    WidgetUnavailable,
}

impl Event {
    fn category(self) -> (&'static str, u32) {
        match self {
            Self::MissingMetadata => ("metadata-missing", 0),
            Self::UriRejected => ("uri-rejected", 1),
            Self::VisibilityDeferred => ("visibility-deferred", 2),
            Self::RequestNotStarted => ("request-not-started", 3),
            Self::Queued => ("request-queued", 4),
            Self::QueueBusy => ("queue-busy", 5),
            Self::MemoryHit => ("memory-cache-hit", 6),
            Self::DiskHit => ("disk-cache-hit", 7),
            Self::CacheMiss => ("cache-miss", 8),
            Self::CacheReadFailure => ("cache-read-failed", 9),
            Self::CacheWriteFailure => ("cache-write-failed", 10),
            Self::CacheBypassed => ("cache-bypassed", 11),
            Self::NetworkDns => ("dns-failure", 12),
            Self::NetworkTls => ("tls-failure", 13),
            Self::NetworkTimeout => ("network-timeout", 14),
            Self::NetworkConnection => ("connection-failure", 15),
            Self::NetworkOther => ("network-failure", 16),
            Self::HttpRefused => ("http-refused", 17),
            Self::UnsupportedContentType => ("content-type-unsupported", 18),
            Self::DecoderUnavailable => ("decoder-unavailable", 19),
            Self::DecodeFailure => ("decode-failed", 20),
            Self::TooLarge => ("image-too-large", 21),
            Self::Cancelled => ("cancelled", 22),
            Self::PixelsReady => ("pixels-ready", 23),
            Self::Displayed => ("gtk-displayed", 24),
            Self::WidgetUnavailable => ("gtk-lifecycle-unavailable", 25),
            Self::HttpStatus(status) => (
                "http-response",
                match status {
                    200 => 32,
                    301 => 33,
                    302 => 34,
                    303 => 35,
                    307 => 36,
                    308 => 37,
                    400 => 38,
                    401 => 39,
                    403 => 40,
                    404 => 41,
                    408 => 42,
                    413 => 43,
                    415 => 44,
                    429 => 45,
                    500 => 46,
                    502 => 47,
                    503 => 48,
                    504 => 49,
                    _ => 50,
                },
            ),
        }
    }
}

struct Recorder {
    enabled: AtomicBool,
    seen: AtomicU64,
}

impl Recorder {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            seen: AtomicU64::new(0),
        }
    }

    fn line(&self, event: Event) -> Option<String> {
        if !self.enabled.load(Ordering::Relaxed) {
            return None;
        }
        let (category, bit) = event.category();
        let mask = 1u64 << bit;
        if self.seen.fetch_or(mask, Ordering::Relaxed) & mask != 0 {
            return None;
        }
        Some(match event {
            Event::HttpStatus(status) => {
                format!("Karin artwork: event={category} http={status}")
            }
            _ => format!("Karin artwork: event={category}"),
        })
    }
}

static RECORDER: Recorder = Recorder::new();

/// Enable at the explicit ordinary application's diagnostic entry point.
/// Categories are emitted once per run (at most 45 fixed lines), independently
/// of the number of tracks, images, requests or unexpected HTTP status values.
pub fn enable() {
    RECORDER.enabled.store(true, Ordering::Relaxed);
}

pub fn emit(event: Event) {
    if let Some(line) = RECORDER.line(event) {
        eprintln!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_in_events_are_silent_by_default_bounded_and_contain_only_fixed_fields() {
        let recorder = Recorder::new();
        assert!(recorder.line(Event::UriRejected).is_none());
        recorder.enabled.store(true, Ordering::Relaxed);
        assert_eq!(
            recorder.line(Event::UriRejected).as_deref(),
            Some("Karin artwork: event=uri-rejected")
        );
        assert!(recorder.line(Event::UriRejected).is_none());
        assert_eq!(
            recorder.line(Event::HttpStatus(404)).as_deref(),
            Some("Karin artwork: event=http-response http=404")
        );
        assert_eq!(
            recorder.line(Event::HttpStatus(403)).as_deref(),
            Some("Karin artwork: event=http-response http=403")
        );
        let mut lines = Vec::new();
        for status in 0..=u16::MAX {
            if let Some(line) = recorder.line(Event::HttpStatus(status)) {
                lines.push(line);
            }
        }
        assert!(lines.len() < 20);
        assert!(lines.iter().all(|line| {
            line.starts_with("Karin artwork: event=http-response http=")
                && !line.contains("://")
                && !line.contains('?')
                && !line.contains("synthetic-private")
        }));
        assert_eq!(
            recorder.line(Event::Displayed).as_deref(),
            Some("Karin artwork: event=gtk-displayed")
        );
    }
}
