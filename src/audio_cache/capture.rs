// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded, nonblocking copy of the existing player's compressed HTTP bytes.
//!
//! This is transport only. Independently licensed content and ordinary private
//! HTTP caching have separate admission policies. Neither changes the playback
//! data or creates another request, GStreamer engine or temporary spool. The
//! writer can drain during preroll; EOF/publication remains gated on actual
//! Playing. Neither producer callbacks nor GTK wait for that gate.

use std::{
    io::{self, Read},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    time::Duration,
};

use gstreamer::{self as gst, prelude::*};

use super::{
    Cancellation,
    http_policy::{Reason, ResponsePolicy},
};

const WAITING: u8 = 0;
const RECEIVING: u8 = 1;
const COMPLETE: u8 = 2;
const ABORTED: u8 = 3;
const CHUNK: usize = 16 * 1024;
const MAX_AUDIO: u64 = 64 * 1024 * 1024;
// A finite supported response must fit while the writer is being scheduled or
// doing disk initialization. Coalescing below makes this a byte bound even when
// souphttpsrc emits tiny buffers. Slots do not preallocate the audio bodies.
const SLOTS: usize = MAX_AUDIO as usize / CHUNK;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptureFailure {
    Cancelled,
    AdmissionRetired,
    SourceAttachment,
    Headers,
    HeadersMissing,
    RepeatedHeaders,
    BufferOverflow,
    ReaderGone,
    ReaderDropped,
    OffsetUnknown,
    OffsetMismatch,
    LengthMismatch,
    Discontinuity,
    BufferMapping,
    ConcurrentInput,
    IncompleteEos,
    Seek,
    Flush,
    Segment,
    PipelineDropped,
    #[cfg(test)]
    Interrupted,
}
impl CaptureFailure {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Cancelled => "capture-cancelled",
            Self::AdmissionRetired => "capture-admission-retired",
            Self::SourceAttachment => "capture-source-unsupported",
            Self::Headers => "capture-headers-unsupported",
            Self::HeadersMissing => "capture-headers-missing",
            Self::RepeatedHeaders => "capture-headers-repeated",
            Self::BufferOverflow => "capture-buffer-overflow",
            Self::ReaderGone => "capture-reader-gone",
            Self::ReaderDropped => "capture-reader-dropped",
            Self::OffsetUnknown => "capture-offset-unknown",
            Self::OffsetMismatch => "capture-offset-mismatch",
            Self::LengthMismatch => "capture-length-mismatch",
            Self::Discontinuity => "capture-discontinuity",
            Self::BufferMapping => "capture-buffer-map-failed",
            Self::ConcurrentInput => "capture-concurrent-input",
            Self::IncompleteEos => "capture-incomplete-eos",
            Self::Seek => "capture-seek",
            Self::Flush => "capture-flush",
            Self::Segment => "capture-segment-discontinuity",
            Self::PipelineDropped => "capture-pipeline-dropped",
            #[cfg(test)]
            Self::Interrupted => "capture-interrupted",
        }
    }
}

struct Producer {
    pending: Vec<u8>,
    segment: Option<gst::Segment>,
}

struct Shared {
    status: AtomicU8,
    expected: AtomicU64,
    received: AtomicU64,
    attached: AtomicBool,
    producer: Mutex<Producer>,
    sender: SyncSender<Vec<u8>>,
    cancellation: Cancellation,
    private_http: bool,
    http_policy: OnceLock<ResponsePolicy>,
    http_rejection: OnceLock<Reason>,
    failure: OnceLock<CaptureFailure>,
    playing_required: AtomicBool,
    played: AtomicBool,
}

#[derive(Clone)]
pub(crate) struct Capture(Arc<Shared>);

pub(crate) struct CaptureReader {
    capture: Capture,
    receiver: Receiver<Vec<u8>>,
    current: Vec<u8>,
    position: usize,
}

pub(crate) fn channel(cancellation: Cancellation) -> (Capture, CaptureReader) {
    create_channel(cancellation, false)
}

/// The ordinary app's private HTTP-cache path observes the actual player's
/// response. This is not a provider entitlement or a content-license claim.
pub(crate) fn channel_http(cancellation: Cancellation) -> (Capture, CaptureReader) {
    create_channel(cancellation, true)
}

fn create_channel(cancellation: Cancellation, private_http: bool) -> (Capture, CaptureReader) {
    create_channel_with_slots(cancellation, private_http, SLOTS)
}

fn create_channel_with_slots(
    cancellation: Cancellation,
    private_http: bool,
    slots: usize,
) -> (Capture, CaptureReader) {
    let (sender, receiver) = mpsc::sync_channel(slots);
    let capture = Capture(Arc::new(Shared {
        status: AtomicU8::new(WAITING),
        expected: AtomicU64::new(0),
        received: AtomicU64::new(0),
        attached: AtomicBool::new(false),
        producer: Mutex::new(Producer {
            pending: Vec::with_capacity(CHUNK),
            segment: None,
        }),
        sender,
        cancellation,
        private_http,
        http_policy: OnceLock::new(),
        http_rejection: OnceLock::new(),
        failure: OnceLock::new(),
        playing_required: AtomicBool::new(private_http),
        played: AtomicBool::new(false),
    }));
    let reader = CaptureReader {
        capture: capture.clone(),
        receiver,
        current: Vec::new(),
        position: 0,
    };
    (capture, reader)
}

impl Capture {
    /// Body completion is separate from playback and write publication. It is
    /// established as soon as exactly the complete declared response is queued,
    /// before a later EOS, Flush or pipeline destruction can retire the source.
    pub(crate) fn is_complete(&self) -> bool {
        if self.0.status.load(Ordering::Acquire) == RECEIVING {
            let expected = self.0.expected.load(Ordering::Acquire);
            if expected > 0 && self.0.received.load(Ordering::Acquire) == expected {
                // The count is released only after every final chunk has been
                // queued. A source-retirement observer can finish this marker
                // if it runs between that store and the producer's final CAS.
                // An already aborted capture is deliberately never revived.
                let _ = self.0.status.compare_exchange(
                    RECEIVING,
                    COMPLETE,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
            }
        }
        self.0.status.load(Ordering::Acquire) == COMPLETE
    }

    pub(crate) fn has_played(&self) -> bool {
        self.0.played.load(Ordering::Acquire)
    }

    /// Call before preparation for any bridge-owned capture. The independent
    /// store tests retain their ungated channel; ordinary HTTP is gated by
    /// default. This never waits for the native player or the writer.
    pub(crate) fn require_playing(&self) {
        self.0.playing_required.store(true, Ordering::Release);
    }

    pub(crate) fn confirm_playing(&self) {
        self.0.played.store(true, Ordering::Release);
    }

    fn publication_ready(&self) -> bool {
        !self.0.playing_required.load(Ordering::Acquire) || self.has_played()
    }

    pub(crate) fn failure(&self) -> Option<CaptureFailure> {
        (self.0.status.load(Ordering::Acquire) == ABORTED)
            .then(|| self.0.failure.get().copied())
            .flatten()
    }

    /// Only exact completed input can supply persistence metadata. The cache
    /// writer calls this after consuming EOF and verifying payload length/hash,
    /// before publishing its media/metadata transaction.
    pub(crate) fn completed_http_policy(&self) -> Option<ResponsePolicy> {
        if !self.is_complete() || !self.publication_ready() || self.0.cancellation.is_cancelled() {
            return None;
        }
        self.0.http_policy.get().cloned()
    }

    /// A finite classification only; native events and header values are never
    /// formatted or exposed in diagnostics.
    pub(crate) fn http_rejection(&self) -> Option<Reason> {
        self.0.http_rejection.get().copied()
    }

    /// Attach only after the existing source guard has validated souphttpsrc.
    /// Probe callbacks always pass the player's data/events unchanged. A slow
    /// cache writer abandons its own capture instead of delaying streaming.
    pub(crate) fn attach(&self, source: &gst::Element) {
        if self.0.attached.swap(true, Ordering::AcqRel)
            || !source
                .factory()
                .is_some_and(|factory| factory.name() == "souphttpsrc")
        {
            self.interrupt_with(CaptureFailure::SourceAttachment);
            return;
        }
        let Some(pad) = source.static_pad("src") else {
            self.interrupt_with(CaptureFailure::SourceAttachment);
            return;
        };
        let capture = self.clone();
        pad.add_probe(
            gst::PadProbeType::BUFFER
                | gst::PadProbeType::BUFFER_LIST
                | gst::PadProbeType::EVENT_DOWNSTREAM
                | gst::PadProbeType::EVENT_UPSTREAM
                | gst::PadProbeType::EVENT_FLUSH,
            move |_, info| {
                if let Some(buffer) = info.buffer() {
                    capture.buffer(buffer);
                } else if let Some(list) = info.buffer_list() {
                    for buffer in list.iter() {
                        capture.buffer(buffer);
                    }
                } else if let Some(event) = info.event() {
                    capture.event(event);
                }
                gst::PadProbeReturn::Ok
            },
        );
    }

    fn event(&self, event: &gst::EventRef) {
        match event.view() {
            gst::EventView::CustomDownstreamSticky(event)
                if event
                    .structure()
                    .is_some_and(|s| s.name() == "http-headers") =>
            {
                if let Some(headers) = event.structure() {
                    self.headers(headers);
                }
            }
            gst::EventView::Segment(event) => self.segment(event.segment()),
            gst::EventView::Seek(_) => self.interrupt_with(CaptureFailure::Seek),
            gst::EventView::FlushStart(_) | gst::EventView::FlushStop(_) => {
                self.interrupt_with(CaptureFailure::Flush)
            }
            gst::EventView::Eos(_) => self.eos(),
            _ => {}
        }
    }

    fn segment(&self, segment: &gst::Segment) {
        if self.is_complete() {
            return;
        }
        // A repeated source segment can update its known stop/duration without
        // changing the byte mapping. Preserve that benign update; actual seek,
        // flush, remapping and non-contiguous buffer offsets remain forbidden.
        let Ok(mut producer) = self.0.producer.try_lock() else {
            self.interrupt_with(CaptureFailure::ConcurrentInput);
            return;
        };
        if segment.format() != gst::Format::Bytes
            || producer.segment.as_ref().is_some_and(|first| {
                first.format() != segment.format()
                    || first.flags() != segment.flags()
                    || first.rate().to_bits() != segment.rate().to_bits()
                    || first.applied_rate().to_bits() != segment.applied_rate().to_bits()
                    || first.base() != segment.base()
                    || first.offset() != segment.offset()
                    || first.start() != segment.start()
                    || first.time() != segment.time()
            })
        {
            self.interrupt_with(CaptureFailure::Segment);
            return;
        }
        producer.segment = Some(segment.clone());
    }

    fn headers(&self, structure: &gst::StructureRef) {
        if self.0.status.load(Ordering::Acquire) != WAITING {
            self.interrupt_with(CaptureFailure::RepeatedHeaders);
            return;
        }
        let accepted = (|| {
            let status = structure.get::<u32>("http-status-code").ok()?;
            if !matches!(status, 200 | 206) {
                return None;
            }
            // Deliberately never read `uri`, `request-headers`, cookies, tokens
            // or any other field. The native event itself is never formatted.
            let headers = structure.get::<gst::Structure>("response-headers").ok()?;
            if headers.n_fields() > 64 {
                return None;
            }
            if self.0.private_http {
                return self.private_http_headers(status, &headers);
            }
            let mut content_type = None;
            let mut length = None;
            let mut range = None;
            for (name, value) in headers.iter() {
                if name.eq_ignore_ascii_case("content-encoding") {
                    return None;
                }
                if name.eq_ignore_ascii_case("content-range") {
                    let value = value.get::<String>().ok()?;
                    if range.is_some() || value.len() > 96 {
                        return None;
                    }
                    range = Some(value);
                } else if name.eq_ignore_ascii_case("content-type") {
                    let value = value.get::<String>().ok()?;
                    if content_type.is_some() || value.len() > 128 {
                        return None;
                    }
                    content_type = Some(value);
                } else if name.eq_ignore_ascii_case("content-length") {
                    let value = value.get::<String>().ok()?;
                    if length.is_some()
                        || value.len() > 20
                        || value.is_empty()
                        || !value.bytes().all(|byte| byte.is_ascii_digit())
                    {
                        return None;
                    }
                    length = Some(value.parse::<u64>().ok()?);
                } else if name.eq_ignore_ascii_case("cache-control") {
                    let value = value.get::<String>().ok()?;
                    if value.len() > 4096
                        || value.split(',').any(|directive| {
                            directive
                                .trim()
                                .split('=')
                                .next()
                                .is_some_and(|name| name.eq_ignore_ascii_case("no-store"))
                        })
                    {
                        return None;
                    }
                }
            }
            if content_type?.split(';').next()?.trim() != "audio/mpeg" {
                return None;
            }
            let length = length.filter(|length| *length > 0 && *length <= MAX_AUDIO)?;
            match (status, range) {
                (200, None) => Some(length),
                (206, Some(range)) if whole_range(&range, length) => Some(length),
                _ => None,
            }
        })();
        let Some(length) = accepted else {
            if self.0.private_http {
                let _ = self.0.http_rejection.set(Reason::HeadersInvalid);
            }
            self.interrupt_with(CaptureFailure::Headers);
            return;
        };
        self.0.expected.store(length, Ordering::Release);
        if self
            .0
            .status
            .compare_exchange(WAITING, RECEIVING, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.interrupt_with(CaptureFailure::RepeatedHeaders);
        }
    }

    fn private_http_headers(&self, status: u32, headers: &gst::StructureRef) -> Option<u64> {
        // A native response event also carries the signed URI and request
        // headers. Neither is read here. Even response fields are a small
        // allowlist, so Set-Cookie, Authorization and unrelated personal data
        // cannot enter the persistent metadata.
        const ALLOWED: &[&str] = &[
            "content-type",
            "content-length",
            "content-range",
            "content-encoding",
            "cache-control",
            "pragma",
            "expires",
            "date",
            "age",
            "vary",
            "etag",
            "last-modified",
            "repr-digest",
        ];
        let mut fields = Vec::new();
        let mut total = 0usize;
        for (name, value) in headers.iter() {
            if !ALLOWED
                .iter()
                .any(|allowed| name.eq_ignore_ascii_case(allowed))
            {
                continue;
            }
            // souphttpsrc's insert_http_header represents repeated response
            // fields as GST_TYPE_ARRAY. Accept its bounded string elements,
            // preserving each occurrence so the HTTP parser can merge Cache-
            // Control/Vary while rejecting duplicate representation fields.
            if let Ok(values) = value.get::<gst::ArrayRef<'_>>() {
                if values.is_empty() || values.len() > 64 {
                    let _ = self.0.http_rejection.set(Reason::HeadersInvalid);
                    return None;
                }
                for value in values.iter() {
                    self.push_private_header(&mut fields, &mut total, name, value)?;
                }
            } else if let Ok(values) = value.get::<gst::ListRef<'_>>() {
                if values.is_empty() || values.len() > 64 {
                    let _ = self.0.http_rejection.set(Reason::HeadersInvalid);
                    return None;
                }
                for value in values.iter() {
                    self.push_private_header(&mut fields, &mut total, name, value)?;
                }
            } else {
                self.push_private_header(&mut fields, &mut total, name, value)?;
            }
        }
        let parsed = u16::try_from(status).ok().and_then(|status| {
            match ResponsePolicy::parse(status, fields) {
                Ok(policy) => Some(policy),
                Err(reason) => {
                    let _ = self.0.http_rejection.set(reason);
                    None
                }
            }
        })?;
        let length = match parsed.complete_mp3() {
            Ok(length) => length,
            Err(reason) => {
                let _ = self.0.http_rejection.set(reason);
                return None;
            }
        };
        self.0.http_policy.set(parsed).ok()?;
        Some(length)
    }

    fn push_private_header(
        &self,
        fields: &mut Vec<(String, String)>,
        total: &mut usize,
        name: &str,
        value: &gst::glib::Value,
    ) -> Option<()> {
        // Containers are deliberately not recursively flattened. Every native
        // element must itself be a string, never another list or arbitrary
        // typed value that might be formatted into persistent metadata.
        let value = value.get::<&str>().ok()?;
        *total = total.checked_add(name.len())?.checked_add(value.len())?;
        if fields.len() >= 64 || value.len() > 4096 || *total > 8192 {
            let _ = self.0.http_rejection.set(Reason::ResponseTooLarge);
            return None;
        }
        fields.push((name.to_owned(), value.to_owned()));
        Some(())
    }

    fn buffer(&self, buffer: &gst::BufferRef) {
        if self.0.cancellation.is_cancelled() {
            self.interrupt_with(CaptureFailure::Cancelled);
            return;
        }
        let status = self.0.status.load(Ordering::Acquire);
        if status != RECEIVING {
            if status == WAITING {
                self.interrupt_with(CaptureFailure::HeadersMissing);
            }
            return;
        }
        // Source pads serialize data/events. Use only a nonwaiting guard so an
        // unexpected concurrent input cannot block GStreamer or corrupt the
        // chunk accumulator. Readers and GTK never acquire this guard.
        let Ok(mut producer) = self.0.producer.try_lock() else {
            self.interrupt_with(CaptureFailure::ConcurrentInput);
            return;
        };
        let received = self.0.received.load(Ordering::Acquire);
        let expected = self.0.expected.load(Ordering::Acquire);
        if buffer.offset() == u64::MAX {
            self.interrupt_with(CaptureFailure::OffsetUnknown);
            return;
        }
        if buffer.offset() != received {
            self.interrupt_with(CaptureFailure::OffsetMismatch);
            return;
        }
        if received > 0 && buffer.flags().contains(gst::BufferFlags::DISCONT) {
            self.interrupt_with(CaptureFailure::Discontinuity);
            return;
        }
        let Some(next) = received
            .checked_add(buffer.size() as u64)
            .filter(|next| *next <= expected)
        else {
            self.interrupt_with(CaptureFailure::LengthMismatch);
            return;
        };
        let Ok(bytes) = buffer.map_readable() else {
            self.interrupt_with(CaptureFailure::BufferMapping);
            return;
        };
        let mut bytes = bytes.as_slice();
        while !bytes.is_empty() {
            if self.0.cancellation.is_cancelled() {
                self.interrupt_with(CaptureFailure::Cancelled);
                return;
            }
            if self.0.status.load(Ordering::Acquire) != RECEIVING {
                return;
            }
            let count = bytes.len().min(CHUNK - producer.pending.len());
            producer.pending.extend_from_slice(&bytes[..count]);
            bytes = &bytes[count..];
            if producer.pending.len() == CHUNK {
                let chunk = std::mem::replace(&mut producer.pending, Vec::with_capacity(CHUNK));
                if !self.queue_chunk(chunk) {
                    return;
                }
            }
        }
        if next == expected && !producer.pending.is_empty() {
            let chunk = std::mem::replace(&mut producer.pending, Vec::with_capacity(CHUNK));
            if !self.queue_chunk(chunk) {
                return;
            }
        }
        self.0.received.store(next, Ordering::Release);
        if next == expected {
            self.complete();
        }
    }

    fn queue_chunk(&self, bytes: Vec<u8>) -> bool {
        match self.0.sender.try_send(bytes) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                self.interrupt_with(CaptureFailure::BufferOverflow);
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                self.interrupt_with(CaptureFailure::ReaderGone);
                false
            }
        }
    }

    fn complete(&self) {
        if self.0.cancellation.is_cancelled() {
            self.interrupt_with(CaptureFailure::Cancelled);
            return;
        }
        let _ = self.0.status.compare_exchange(
            RECEIVING,
            COMPLETE,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn eos(&self) {
        if self.is_complete() {
            return;
        }
        if self.0.status.load(Ordering::Acquire) == RECEIVING
            && self.0.received.load(Ordering::Acquire) == self.0.expected.load(Ordering::Acquire)
        {
            self.complete();
        } else {
            self.interrupt_with(CaptureFailure::IncompleteEos);
        }
    }

    /// Seeking, a failed source, shutdown or an incomplete pipeline can never
    /// publish a partial record. Once the complete resource has arrived, later
    /// player seeks need not invalidate those already received exact bytes.
    #[cfg(test)]
    pub(crate) fn interrupt(&self) {
        self.interrupt_with(CaptureFailure::Interrupted);
    }

    pub(crate) fn interrupt_with(&self, failure: CaptureFailure) {
        let mut status = self.0.status.load(Ordering::Acquire);
        while status != COMPLETE && status != ABORTED {
            if self.is_complete() {
                return;
            }
            let _ = self.0.failure.set(failure);
            match self.0.status.compare_exchange_weak(
                status,
                ABORTED,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(current) => status = current,
            }
        }
    }
}

// A request for `bytes=0-` can legitimately produce 206 for the WHOLE
// representation. Accept no multipart, unknown total, gap or partial response.
// Publication still requires contiguous offsets, exact EOF and the independent
// content claim; status 206 is never treated as a general range-assembly mode.
fn whole_range(value: &str, length: u64) -> bool {
    let Some(value) = value.trim().strip_prefix("bytes ") else {
        return false;
    };
    let Some((interval, total)) = value.split_once('/') else {
        return false;
    };
    let Some((start, end)) = interval.split_once('-') else {
        return false;
    };
    let decimal = |value: &str| {
        (!value.is_empty() && value.len() <= 20 && value.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| value.parse::<u64>().ok())
            .flatten()
    };
    matches!((decimal(start), decimal(end), decimal(total)), (Some(0), Some(end), Some(total))
        if total == length && end.checked_add(1) == Some(total))
}

impl CaptureReader {
    fn check(&self) -> io::Result<()> {
        if self.capture.0.cancellation.is_cancelled() {
            self.capture.interrupt_with(CaptureFailure::Cancelled);
            return Err(io::Error::other("Audio cache capture was retired"));
        }
        if self.capture.0.status.load(Ordering::Acquire) == ABORTED {
            Err(io::Error::other("Audio cache capture was retired"))
        } else {
            Ok(())
        }
    }
}

impl CaptureReader {
    /// The writer's memory-only lease/rights predicate is rechecked while no
    /// network bytes arrive (for example during pause), so Clear and account
    /// teardown can retire the existing I/O worker without waiting for data.
    pub(crate) fn read_checked(
        &mut self,
        output: &mut [u8],
        current: impl Fn() -> bool,
    ) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            self.check()?;
            if !current() {
                self.capture
                    .interrupt_with(CaptureFailure::AdmissionRetired);
                return Err(io::Error::other("Audio cache capture lease was retired"));
            }
            if self.position < self.current.len() {
                let count = output.len().min(self.current.len() - self.position);
                output[..count]
                    .copy_from_slice(&self.current[self.position..self.position + count]);
                self.position += count;
                return Ok(count);
            }
            self.current.clear();
            self.position = 0;
            match self.receiver.try_recv() {
                Ok(bytes) => {
                    self.current = bytes;
                    continue;
                }
                Err(TryRecvError::Empty)
                    if self.capture.0.status.load(Ordering::Acquire) == COMPLETE =>
                {
                    // The final byte may have raced the empty observation.
                    // Complete means the entire body is already queued; EOF
                    // additionally waits for the actual Playing gate.
                    match self.receiver.try_recv() {
                        Ok(bytes) => {
                            self.current = bytes;
                            continue;
                        }
                        Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                            if self.capture.publication_ready() {
                                return Ok(0);
                            }
                        }
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    return Err(io::Error::other("Audio cache capture ended early"));
                }
                Err(TryRecvError::Empty) => {}
            }
            match self.receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(bytes) => self.current = bytes,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("Audio cache capture ended early"));
                }
            }
        }
    }
}

impl Read for CaptureReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.read_checked(output, || true)
    }
}

impl Drop for CaptureReader {
    fn drop(&mut self) {
        self.capture.interrupt_with(CaptureFailure::ReaderDropped);
    }
}

#[cfg(test)]
mod tests;
