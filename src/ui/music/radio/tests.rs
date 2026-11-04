// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn lifecycle_is_explicitly_disabled_and_cancel_retires_pending_begin() {
    let mut radio = RadioLifecycle::default();
    assert!(!radio.enabled);
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
