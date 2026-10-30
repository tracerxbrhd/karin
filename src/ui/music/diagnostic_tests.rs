// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::music::{SearchFailureKind, SearchParseStage};

#[test]
fn catalog_reports_distinguish_requested_detail_from_result() {
    let requested = catalog_line(
        Operation::Detail,
        CatalogPhase::Requested,
        "requested",
        None,
        None,
    );
    let result = catalog_line(
        Operation::Detail,
        CatalogPhase::Result,
        "success",
        None,
        None,
    );
    assert!(requested.ends_with("stage=none event=detail-requested"));
    assert!(result.ends_with("stage=none event=detail-result"));
    assert_ne!(requested, result);
    let queued = catalog_line(
        Operation::Detail,
        CatalogPhase::State,
        "queued_latest",
        None,
        None,
    );
    assert!(queued.ends_with("event=state"));
}

#[test]
fn catalog_reports_keep_parse_stage_and_observed_http_without_payload_fields() {
    let error = MusicError::SearchResponse {
        stage: SearchParseStage::TrackNormalization,
        kind: SearchFailureKind::Contract,
        http: 200,
    };
    assert_eq!(
        catalog_line(
            Operation::Search,
            CatalogPhase::Result,
            error.category(),
            error.http_status(),
            error.diagnostic_stage(),
        ),
        "Karin catalog: operation=search outcome=response-contract http=200 stage=track-normalization event=search-result"
    );
    assert_eq!(
        catalog_line(
            Operation::Search,
            CatalogPhase::Result,
            "success",
            Some(200),
            None,
        ),
        "Karin catalog: operation=search outcome=success http=200 stage=none event=search-result"
    );
}

#[test]
fn staged_search_errors_retain_their_native_error_presentation() {
    for (kind, ordinary) in [
        (SearchFailureKind::Contract, MusicError::InvalidResponse),
        (
            SearchFailureKind::ResponseTooLarge,
            MusicError::ResponseTooLarge,
        ),
        (SearchFailureKind::Network, MusicError::Network),
        (SearchFailureKind::Timeout, MusicError::Timeout),
    ] {
        let staged = MusicError::SearchResponse {
            stage: SearchParseStage::Envelope,
            kind,
            http: 200,
        };
        assert_eq!(music_error_message(staged), music_error_message(ordinary));
    }
}
