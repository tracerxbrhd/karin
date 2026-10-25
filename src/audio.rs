// SPDX-License-Identifier: GPL-3.0-or-later
//! Independent, bounded diagnostics for the native audio stack.
//!
//! No service URLs, credentials or copyrighted audio enter this module. Run
//! [`run_test_tone`] on a worker thread: waiting for the bus must never block GTK.

use std::fmt;

use gst::prelude::*;
use gstreamer as gst;
use thiserror::Error;

const TEST_BUFFERS: u32 = 40;
const DIAGNOSTIC_TIMEOUT: gst::ClockTime = gst::ClockTime::from_seconds(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioOutput {
    /// Process synthetic audio without sending it to a physical audio device.
    Fakesink,
    /// Ask GStreamer to select a device. Audibility still requires manual testing.
    System,
}

impl fmt::Display for AudioOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Fakesink => "fakesink (silent)",
            Self::System => "system audio (verify audibility manually)",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioReport {
    pub output: AudioOutput,
    pub buffers: u32,
}

impl fmt::Display for AudioReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Synthetic test signal completed: {} buffers via {}. This is not Yandex Music playback.",
            self.buffers, self.output
        )
    }
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("GStreamer could not initialize: {0}")]
    Initialization(String),
    #[error("The diagnostic pipeline could not be created: {0}")]
    Pipeline(String),
    #[error("The diagnostic pipeline has no message bus")]
    MissingBus,
    #[error("The diagnostic pipeline could not enter the playing state")]
    Start,
    #[error("GStreamer reported a diagnostic playback error: {0}")]
    Playback(String),
    #[error("The diagnostic audio pipeline timed out")]
    Timeout,
}

/// Play a finite, quiet synthetic tone, or validate processing silently.
///
/// Every exit path resets the pipeline to `Null`, including start failures,
/// timeout and bus errors. A successful report means EOS was observed; it does
/// not prove that the user heard sound or that music streaming is supported.
pub fn run_test_tone(output: AudioOutput) -> Result<AudioReport, AudioError> {
    gst::init().map_err(|error| AudioError::Initialization(error.to_string()))?;
    let sink = match output {
        AudioOutput::Fakesink => "fakesink sync=false",
        AudioOutput::System => "autoaudiosink",
    };
    let description = format!(
        "audiotestsrc num-buffers={TEST_BUFFERS} samplesperbuffer=1024 wave=sine freq=440 volume=0.03 \
         ! audioconvert ! audioresample ! {sink}"
    );
    let pipeline = create_pipeline(&description)?;
    run_pipeline(&pipeline, DIAGNOSTIC_TIMEOUT)?;
    Ok(AudioReport {
        output,
        buffers: TEST_BUFFERS,
    })
}

fn create_pipeline(description: &str) -> Result<gst::Pipeline, AudioError> {
    gst::parse::launch(description)
        .map_err(|error| AudioError::Pipeline(error.to_string()))?
        .downcast::<gst::Pipeline>()
        .map_err(|_| AudioError::Pipeline("expected a GStreamer pipeline".to_owned()))
}

struct StopOnDrop<'a>(&'a gst::Pipeline);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        // All descriptions are local diagnostic elements. None can leave a
        // remote stream active when this bounded worker operation finishes.
        let _ = self.0.set_state(gst::State::Null);
    }
}

fn run_pipeline(pipeline: &gst::Pipeline, timeout: gst::ClockTime) -> Result<(), AudioError> {
    let _stop_on_drop = StopOnDrop(pipeline);
    let bus = pipeline.bus().ok_or(AudioError::MissingBus)?;
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|_| AudioError::Start)?;
    let message = bus
        .timed_pop_filtered(timeout, &[gst::MessageType::Eos, gst::MessageType::Error])
        .ok_or(AudioError::Timeout)?;
    match message.view() {
        gst::MessageView::Eos(..) => Ok(()),
        gst::MessageView::Error(error) => Err(AudioError::Playback(error.error().to_string())),
        // The bus filter restricts this branch to the two message kinds above.
        _ => unreachable!("unexpected filtered GStreamer message"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_signal_reaches_eos_without_an_audio_device() {
        let report = run_test_tone(AudioOutput::Fakesink).expect("synthetic audio reaches EOS");
        assert_eq!(report.output, AudioOutput::Fakesink);
        assert_eq!(report.buffers, TEST_BUFFERS);
        assert!(report.to_string().contains("not Yandex Music playback"));
    }

    #[test]
    fn bus_error_is_reported_and_pipeline_is_stopped() {
        gst::init().expect("GStreamer initializes");
        let pipeline =
            create_pipeline("audiotestsrc num-buffers=4 ! identity error-after=1 ! fakesink")
                .expect("test pipeline is valid");
        let result = run_pipeline(&pipeline, gst::ClockTime::from_seconds(2));
        assert!(matches!(result, Err(AudioError::Playback(_))));
        assert_eq!(pipeline.current_state(), gst::State::Null);
    }

    #[test]
    fn timeout_is_reported_and_pipeline_is_stopped() {
        gst::init().expect("GStreamer initializes");
        let pipeline = create_pipeline("audiotestsrc is-live=true ! fakesink")
            .expect("test pipeline is valid");
        let result = run_pipeline(&pipeline, gst::ClockTime::from_mseconds(30));
        assert!(matches!(result, Err(AudioError::Timeout)));
        assert_eq!(pipeline.current_state(), gst::State::Null);
    }

    #[test]
    fn malformed_pipeline_is_reported() {
        gst::init().expect("GStreamer initializes");
        assert!(matches!(
            create_pipeline("karin-element-that-does-not-exist ! fakesink"),
            Err(AudioError::Pipeline(_))
        ));
    }
}
