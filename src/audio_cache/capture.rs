// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded, nonblocking copy of the existing player's compressed HTTP bytes.
//!
//! This is transport only. The caller must first acquire independent storage
//! permission. There is no extra request, GStreamer engine or temporary spool.
//! The existing cache writer consumes this reader only after actual Playing.

use std::{
    io::{self, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    time::Duration,
};

use gstreamer::{self as gst, prelude::*};

use super::Cancellation;

const WAITING: u8 = 0;
const RECEIVING: u8 = 1;
const COMPLETE: u8 = 2;
const ABORTED: u8 = 3;
const CHUNK: usize = 16 * 1024;
const SLOTS: usize = 128; // At most 2 MiB of queued preroll/copy data.
const MAX_AUDIO: u64 = 64 * 1024 * 1024;

struct Shared {
    status: AtomicU8,
    expected: AtomicU64,
    received: AtomicU64,
    attached: AtomicBool,
    segment_seen: AtomicBool,
    sender: SyncSender<Vec<u8>>,
    cancellation: Cancellation,
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
    let (sender, receiver) = mpsc::sync_channel(SLOTS);
    let capture = Capture(Arc::new(Shared {
        status: AtomicU8::new(WAITING),
        expected: AtomicU64::new(0),
        received: AtomicU64::new(0),
        attached: AtomicBool::new(false),
        segment_seen: AtomicBool::new(false),
        sender,
        cancellation,
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
    /// Attach only after the existing source guard has validated souphttpsrc.
    /// Probe callbacks always pass the player's data/events unchanged. A slow
    /// cache writer abandons its own capture instead of delaying streaming.
    pub(crate) fn attach(&self, source: &gst::Element) {
        if self.0.attached.swap(true, Ordering::AcqRel)
            || !source
                .factory()
                .is_some_and(|factory| factory.name() == "souphttpsrc")
        {
            self.interrupt();
            return;
        }
        let Some(pad) = source.static_pad("src") else {
            self.interrupt();
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
                    match event.view() {
                        gst::EventView::CustomDownstreamSticky(event)
                            if event
                                .structure()
                                .is_some_and(|s| s.name() == "http-headers") =>
                        {
                            if let Some(headers) = event.structure() {
                                capture.headers(headers);
                            }
                        }
                        gst::EventView::Segment(_) => {
                            if capture.0.segment_seen.swap(true, Ordering::AcqRel) {
                                capture.interrupt();
                            }
                        }
                        gst::EventView::Seek(_)
                        | gst::EventView::FlushStart(_)
                        | gst::EventView::FlushStop(_) => capture.interrupt(),
                        gst::EventView::Eos(_) => capture.eos(),
                        _ => {}
                    }
                }
                gst::PadProbeReturn::Ok
            },
        );
    }

    fn headers(&self, structure: &gst::StructureRef) {
        if self.0.status.load(Ordering::Acquire) != WAITING {
            self.interrupt();
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
            self.interrupt();
            return;
        };
        self.0.expected.store(length, Ordering::Release);
        if self
            .0
            .status
            .compare_exchange(WAITING, RECEIVING, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.interrupt();
        }
    }

    fn buffer(&self, buffer: &gst::BufferRef) {
        if self.0.cancellation.is_cancelled() {
            self.interrupt();
            return;
        }
        if self.0.status.load(Ordering::Acquire) != RECEIVING {
            return;
        }
        let received = self.0.received.load(Ordering::Acquire);
        let expected = self.0.expected.load(Ordering::Acquire);
        if buffer.offset() != received
            || (received > 0 && buffer.flags().contains(gst::BufferFlags::DISCONT))
            || received
                .checked_add(buffer.size() as u64)
                .is_none_or(|next| next > expected)
        {
            self.interrupt();
            return;
        }
        let Ok(bytes) = buffer.map_readable() else {
            self.interrupt();
            return;
        };
        for bytes in bytes.as_slice().chunks(CHUNK) {
            match self.0.sender.try_send(bytes.to_vec()) {
                Ok(()) => {}
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                    self.interrupt();
                    return;
                }
            }
        }
        self.0
            .received
            .store(received + buffer.size() as u64, Ordering::Release);
    }

    fn eos(&self) {
        if self.0.status.load(Ordering::Acquire) == RECEIVING
            && self.0.received.load(Ordering::Acquire) == self.0.expected.load(Ordering::Acquire)
            && !self.0.cancellation.is_cancelled()
        {
            let _ = self.0.status.compare_exchange(
                RECEIVING,
                COMPLETE,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        } else {
            self.interrupt();
        }
    }

    /// Seeking, a failed source, shutdown or an incomplete pipeline can never
    /// publish a partial record. Once the complete resource has arrived, later
    /// player seeks need not invalidate those already received exact bytes.
    pub(crate) fn interrupt(&self) {
        let mut status = self.0.status.load(Ordering::Acquire);
        while status != COMPLETE && status != ABORTED {
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
        if self.capture.0.cancellation.is_cancelled()
            || self.capture.0.status.load(Ordering::Acquire) == ABORTED
        {
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
                    // EOS may have raced the first empty observation. Once
                    // Complete is acquired, no producer can enqueue more data.
                    match self.receiver.try_recv() {
                        Ok(bytes) => {
                            self.current = bytes;
                            continue;
                        }
                        Err(TryRecvError::Empty | TryRecvError::Disconnected) => return Ok(0),
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
        self.capture.interrupt();
    }
}

#[cfg(test)]
mod tests;
