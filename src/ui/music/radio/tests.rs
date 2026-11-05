// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn lifecycle_starts_idle_and_cancel_retires_pending_begin() {
    let mut radio = RadioLifecycle::default();
    assert!(radio.begin.is_none());
    assert!(!radio.begin_pending);
    assert!(radio.running.is_none());
    assert!(radio.active.is_none());
    assert!(radio.retiring.is_none());
    let cancel = WaveCancellation::default();
    radio.begin = Some(cancel.clone());
    radio.begin_pending = true;
    radio.running = Some(7);
    radio.cancel();
    assert!(cancel.is_cancelled());
    assert!(!radio.begin_pending);
    assert!(radio.begin.is_none());
    assert_eq!(
        radio.running,
        Some(7),
        "admitted work retains its bounded lane"
    );
}
#[test]
fn errors_have_finite_categories_without_provider_values() {
    assert_eq!(
        classify(WaveError::WriteOutcomeUnknown),
        RadioFailure::OutcomeUnknown
    );
    assert_eq!(classify(WaveError::Terminated), RadioFailure::Unavailable);
    assert_eq!(
        classify(WaveError::Music(MusicError::Forbidden)),
        RadioFailure::Forbidden
    );
    assert_eq!(stage_name(RadioStage::Feedback), "feedback");
}

#[test]
fn outbox_admission_is_finite_and_overflow_does_not_wrap() {
    assert!(event_capacity(0, 2));
    assert!(event_capacity(MAX_EVENTS - 2, 2));
    assert!(!event_capacity(MAX_EVENTS - 1, 2));
    assert!(!event_capacity(MAX_EVENTS, 1));
    assert!(!event_capacity(usize::MAX, 1));
}

#[test]
fn provider_rejection_diagnostic_retains_every_fixed_event_and_local_origin() {
    use crate::music::wave::WaveEventKind;
    for event in [
        WaveEventKind::RadioStarted,
        WaveEventKind::TrackStarted,
        WaveEventKind::TrackFinished,
        WaveEventKind::Skip,
    ] {
        let error = WaveError::Music(MusicError::Http(400));
        let diagnostic = OperationDiagnostic {
            stage: RadioStage::Feedback,
            category: error.category(),
            http: error.http_status(),
            event: Some(event),
            phase: DiagnosticPhase::Retiring,
            origin: Some(FeedbackOrigin::Retirement),
            finish: Some(FinishReason::SourceChange),
        }
        .to_string();
        assert_eq!(
            diagnostic,
            format!(
                "Karin wave: stage=feedback category=http-rejected http=400 event={} phase=retiring origin=retirement finish=source-change",
                event.as_str()
            )
        );
        for forbidden in [
            "http://",
            "https://",
            "radioSessionId",
            "batchId",
            "trackId",
            "accountId",
            "token",
            "cookie",
            "totalPlayedSeconds",
            "timestamp",
            "{{",
            "}}",
        ] {
            assert!(
                !diagnostic.contains(forbidden),
                "diagnostics expose fixed categories only"
            );
        }
        assert!(diagnostic.len() <= 200);
    }
}

#[test]
fn uncertain_write_diagnostic_never_fabricates_http_or_event_values() {
    let error = WaveError::WriteOutcomeUnknown;
    let diagnostic = OperationDiagnostic {
        stage: RadioStage::Feedback,
        category: error.category(),
        http: error.http_status(),
        event: Some(crate::music::wave::WaveEventKind::Skip),
        phase: DiagnosticPhase::Active,
        origin: Some(FeedbackOrigin::Selection),
        finish: None,
    }
    .to_string();
    assert_eq!(
        diagnostic,
        "Karin wave: stage=feedback category=write-outcome-unknown http=none event=skip phase=active origin=selection finish=none"
    );
}
