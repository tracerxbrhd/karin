// SPDX-License-Identifier: GPL-3.0-or-later
//! Measure a staged URI change with the same GStreamer element used by Karin.
//! All inputs are original local signals. No provider or application state is used.

use std::{
    fmt,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use gst::prelude::*;
use gstreamer as gst;
use thiserror::Error;

use super::{DIAGNOSTIC_WAV_RATE, DiagnosticMedia, TRANSITION_TIMEOUT};

const MP3_RATE: u32 = 44_100;
const MP3_SOURCE_FRAMES: [u64; 2] = [12_000, 15_600];
const MP3_FIXTURES: [&[u8]; 2] = [
    include_bytes!("../../resources/playback-fixtures/transition-a.mp3"),
    include_bytes!("../../resources/playback-fixtures/transition-b.mp3"),
];

#[derive(Debug, Error)]
pub enum TransitionDiagnosticError {
    #[error("The original transition fixtures could not be prepared.")]
    Fixture,
    #[error("The GStreamer transition diagnostic could not initialize.")]
    Initialization,
    #[error("The GStreamer transition diagnostic could not decode its two fixtures.")]
    Playback,
    #[error("The GStreamer transition diagnostic did not reach the final end of stream.")]
    Timeout,
    #[error("The sink did not deliver two complete, timestamped PCM streams.")]
    Incomplete,
}

/// Local sink observations for comparison across the Native and Flatpak runtimes.
#[derive(Debug)]
pub struct TransitionReport {
    version: gst::glib::GString,
    cases: [Measurement; 3],
}

impl fmt::Display for TransitionReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "Local playbin/fakesink transition diagnostic ({})",
            self.version
        )?;
        for case in &self.cases {
            writeln!(
                formatter,
                "{} at {} Hz: decoded frames {:?}, source frames {:?}, second stream starts at frame {}, timestamp gaps {} frames, overlaps {} frames",
                case.label,
                case.rate,
                case.decoded,
                case.source,
                case.second_start,
                case.gaps,
                case.overlaps,
            )?;
        }
        formatter.write_str("Frame counts and sink timestamps describe these fixtures and this decoder. The diagnostic does not use Yandex Music or physical audio output.")
    }
}

#[derive(Debug)]
struct Measurement {
    label: &'static str,
    rate: u32,
    source: [u64; 2],
    decoded: [u64; 2],
    second_start: u64,
    gaps: u64,
    overlaps: u64,
}

struct TemporaryMp3(PathBuf);

impl TemporaryMp3 {
    fn create(bytes: &[u8]) -> Result<Self, TransitionDiagnosticError> {
        let (descriptor, path) = gst::glib::file_open_tmp(Some("karin-transition-XXXXXX"))
            .map_err(|_| TransitionDiagnosticError::Fixture)?;
        let file = Self(path);
        File::from(descriptor)
            .write_all(bytes)
            .map_err(|_| TransitionDiagnosticError::Fixture)?;
        Ok(file)
    }
}

impl Drop for TemporaryMp3 {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[derive(Default)]
struct SinkTimeline {
    segment: Option<gst::FormattedSegment<gst::ClockTime>>,
    streams: usize,
    decoded: [u64; 2],
    second_start: Option<u64>,
    end: u64,
    gaps: u64,
    overlaps: u64,
    invalid: bool,
}

impl SinkTimeline {
    fn event(&mut self, event: &gst::EventRef) {
        match event.view() {
            gst::EventView::StreamStart(_) => {
                self.streams += 1;
                self.segment = None;
            }
            gst::EventView::Segment(event) => {
                self.segment = event.segment().downcast_ref::<gst::ClockTime>().cloned();
            }
            _ => {}
        }
    }

    fn buffer(&mut self, buffer: &gst::BufferRef, rate: u32) {
        let Some(running_time) = self
            .segment
            .as_ref()
            .and_then(|segment| segment.to_running_time(buffer.pts()))
        else {
            self.invalid = true;
            return;
        };
        if !buffer.size().is_multiple_of(2)
            || buffer
                .flags()
                .intersects(gst::BufferFlags::DECODE_ONLY | gst::BufferFlags::CORRUPTED)
        {
            self.invalid = true;
            return;
        }
        // The fixture format is S16LE mono. Convert truncated nanosecond
        // timestamps back to the nearest frame before comparing boundaries.
        let start =
            (u128::from(running_time.nseconds()) * u128::from(rate) + 500_000_000) / 1_000_000_000;
        let frames = (buffer.size() / 2) as u64;
        self.span(start as u64, frames);
        if buffer.flags().contains(gst::BufferFlags::GAP) {
            self.gaps += frames;
        }
    }

    fn span(&mut self, start: u64, frames: u64) {
        if !(1..=2).contains(&self.streams) {
            self.invalid = true;
            return;
        }
        if frames == 0 {
            return;
        }
        if self.streams == 2 && self.second_start.is_none() {
            self.second_start = Some(start);
        }
        self.decoded[self.streams - 1] += frames;
        self.gaps += start.saturating_sub(self.end);
        self.overlaps += self.end.saturating_sub(start);
        self.end = start + frames;
    }

    fn finish(
        &self,
        label: &'static str,
        rate: u32,
        source: [u64; 2],
    ) -> Result<Measurement, TransitionDiagnosticError> {
        let second_start = self
            .second_start
            .ok_or(TransitionDiagnosticError::Incomplete)?;
        if self.invalid || self.streams != 2 || self.decoded.contains(&0) {
            return Err(TransitionDiagnosticError::Incomplete);
        }
        Ok(Measurement {
            label,
            rate,
            source,
            decoded: self.decoded,
            second_start,
            gaps: self.gaps,
            overlaps: self.overlaps,
        })
    }
}

fn measure(
    label: &'static str,
    rate: u32,
    source: [u64; 2],
    [first, next]: [&Path; 2],
) -> Result<Measurement, TransitionDiagnosticError> {
    use TransitionDiagnosticError as Error;
    let uri = |path: &Path| gst::glib::filename_to_uri(path, None).map_err(|_| Error::Fixture);
    let first = uri(first)?;
    let next = Mutex::new(Some(uri(next)?));
    let timeline = Arc::new(Mutex::new(SinkTimeline::default()));
    // No resampling or audiorate: GStreamer must neither drop nor generate
    // samples to make this probe's output look contiguous.
    let sink = gst::parse::bin_from_description(
        &format!("audioconvert ! audio/x-raw,format=S16LE,layout=interleaved,channels=1,rate={rate} ! fakesink name=pcm-sink sync=true"),
        true,
    )
    .map_err(|_| Error::Initialization)?;
    let pad = sink
        .by_name("pcm-sink")
        .and_then(|element| element.static_pad("sink"))
        .ok_or(Error::Initialization)?;
    let observed = Arc::clone(&timeline);
    pad.add_probe(
        gst::PadProbeType::EVENT_DOWNSTREAM
            | gst::PadProbeType::BUFFER
            | gst::PadProbeType::BUFFER_LIST,
        move |_, probe| {
            let mut timeline = observed.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(event) = probe.event() {
                timeline.event(event);
            } else if let Some(buffer) = probe.buffer() {
                timeline.buffer(buffer, rate);
            } else if let Some(buffers) = probe.buffer_list() {
                for buffer in buffers.iter() {
                    timeline.buffer(buffer, rate);
                }
            }
            gst::PadProbeReturn::Ok
        },
    )
    .ok_or(Error::Initialization)?;

    let player = gst::ElementFactory::make("playbin")
        .build()
        .map_err(|_| Error::Initialization)?;
    let bus = player.bus().ok_or(Error::Initialization)?;
    player.set_property_from_str("flags", "audio+soft-volume");
    player.set_property("audio-sink", &sink);
    player.set_property("uri", first.as_str());
    player.connect("about-to-finish", false, move |values| {
        let uri = next
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(uri) = uri
            && let Ok(player) = values[0].get::<gst::Element>()
        {
            player.set_property("uri", uri.as_str());
        }
        None
    });

    let result = player
        .set_state(gst::State::Playing)
        .map_err(|_| Error::Playback)
        .and_then(|_| {
            let message = bus
                .timed_pop_filtered(
                    gst::ClockTime::from_seconds(TRANSITION_TIMEOUT.as_secs()),
                    &[gst::MessageType::Eos, gst::MessageType::Error],
                )
                .ok_or(Error::Timeout)?;
            match message.view() {
                gst::MessageView::Eos(_) => Ok(()),
                _ => Err(Error::Playback),
            }
        });
    // Clear the native elements and callback before dropping either fixture.
    let cleanup = player
        .set_state(gst::State::Null)
        .map_err(|_| Error::Playback);
    result?;
    cleanup?;
    timeline
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .finish(label, rate, source)
}

fn without_trim_metadata(bytes: &[u8]) -> Result<Vec<u8>, TransitionDiagnosticError> {
    // Both generated files carry Info followed by a 36-byte LAME extension.
    // Clear only that extension, preserving all encoded audio and Xing data.
    let start = bytes
        .windows(4)
        .position(|field| field == b"LAME")
        .ok_or(TransitionDiagnosticError::Fixture)?;
    let mut untagged = bytes.to_vec();
    untagged
        .get_mut(start..start + 36)
        .ok_or(TransitionDiagnosticError::Fixture)?
        .fill(0);
    Ok(untagged)
}

/// Run the bounded local probe outside of GTK and without requesting a sound device.
pub fn run_transition_diagnostic() -> Result<TransitionReport, TransitionDiagnosticError> {
    use TransitionDiagnosticError as Error;
    gst::init().map_err(|_| Error::Initialization)?;
    let duration_ms = [280_u64, 340];
    let [first, next] = duration_ms
        .map(|ms| DiagnosticMedia::create(Duration::from_millis(ms)).map_err(|_| Error::Fixture));
    let (first, next) = (first?, next?);
    let wav = measure(
        "WAV",
        DIAGNOSTIC_WAV_RATE,
        duration_ms.map(|ms| ms * u64::from(DIAGNOSTIC_WAV_RATE) / 1_000),
        [&first.source.path, &next.source.path],
    )?;
    let [first, next] = MP3_FIXTURES;
    let tagged = {
        let (first, next) = (TemporaryMp3::create(first)?, TemporaryMp3::create(next)?);
        measure(
            "MP3 with LAME trim metadata",
            MP3_RATE,
            MP3_SOURCE_FRAMES,
            [&first.0, &next.0],
        )?
    };
    let untagged = {
        let first = TemporaryMp3::create(&without_trim_metadata(first)?)?;
        let next = TemporaryMp3::create(&without_trim_metadata(next)?)?;
        measure(
            "MP3 without LAME trim metadata",
            MP3_RATE,
            MP3_SOURCE_FRAMES,
            [&first.0, &next.0],
        )?
    };
    Ok(TransitionReport {
        version: gst::version_string(),
        cases: [wav, tagged, untagged],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_sample_gaps_and_overlaps_across_two_streams() {
        let mut timeline = SinkTimeline {
            streams: 1,
            ..SinkTimeline::default()
        };
        timeline.span(0, 100);
        timeline.streams = 2;
        timeline.span(101, 20);
        timeline.span(120, 10);
        let report = timeline.finish("test", 44_100, [100, 30]).unwrap();
        assert_eq!(report.second_start, 101);
        assert_eq!(report.decoded, [100, 30]);
        assert_eq!((report.gaps, report.overlaps), (1, 1));
    }

    #[test]
    fn local_probe_detects_untrimmed_mp3_separately_from_pcm_timestamps() {
        let TransitionReport {
            cases: [wav, tagged, untagged],
            ..
        } = run_transition_diagnostic().unwrap();
        assert_eq!(wav.decoded, wav.source);
        assert_eq!(wav.second_start, wav.source[0]);
        assert_eq!((wav.gaps, wav.overlaps), (0, 0));

        assert_eq!(tagged.source, untagged.source);
        assert!(
            untagged
                .decoded
                .iter()
                .zip(untagged.source)
                .all(|(decoded, source)| *decoded > source)
        );
        if tagged.decoded == tagged.source {
            assert_eq!(tagged.second_start, tagged.source[0]);
            assert_eq!((tagged.gaps, tagged.overlaps), (0, 0));
        }
    }
}
