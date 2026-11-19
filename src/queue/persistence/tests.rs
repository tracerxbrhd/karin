// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn track(id: &str) -> Track {
    Track {
        id: id.into(),
        title: "Synthetic normalized fixture".into(),
        artists: vec![],
        artist_links: vec![],
        album_id: None,
        album_title: None,
        duration_ms: Some(180_000),
        cover: None,
        explicit: false,
        available: true,
    }
}

fn queue() -> MusicQueue {
    let context = PlaybackContext::with_source_positions(
        ContextKind::Playlist,
        Some("continuous:48:synthetic-view-binding".into()),
        vec![
            track("same:10"),
            track("other"),
            track("same:10"),
            track("last"),
        ],
        2,
        true,
        vec![5, 8, 11, 23],
    )
    .unwrap()
    .with_origin(PlaybackOrigin::Playlist {
        owner: "owner".into(),
        kind: "42".into(),
    });
    let mut queue = MusicQueue::with_seed(125);
    queue.replace_context(context);
    queue.append(track("extra")).unwrap();
    queue
}

fn fresh() -> Vec<Track> {
    ["same:10", "other", "last", "extra"]
        .into_iter()
        .map(track)
        .collect()
}

#[test]
fn restoration_keeps_exact_duplicate_current_and_original_loaded_and_manual_positions() {
    let original = queue();
    let snapshot = QueueSnapshot::capture(&original).unwrap().unwrap();
    assert_eq!(snapshot.ids(), ["same:10", "other", "last", "extra"]);
    let restored = snapshot.restore(&fresh()).unwrap();
    assert_eq!(restored.omitted, 0);
    assert!(!restored.current_changed);
    assert_eq!(restored.queue.current_index(), Some(2));
    assert_eq!(restored.queue.items(), original.items());
    let context = restored.queue.context().unwrap();
    assert_eq!(context.loaded_items_len(), 4);
    assert_eq!(context.manual_items_len(), 1);
    assert_eq!(
        (0..5)
            .map(|index| context.source_position(index))
            .collect::<Vec<_>>(),
        [Some(5), Some(8), Some(11), Some(23), None]
    );
    assert_eq!(context.origin(), original.context().unwrap().origin());
    assert!(context.is_partial());
    assert!(context.source_id().unwrap().starts_with("restored:"));
    assert_ne!(context.source_id(), original.context().unwrap().source_id());
    assert_ne!(
        restored.queue.occurrence_id(0),
        restored.queue.occurrence_id(2)
    );
    for index in 0..5 {
        assert_ne!(
            restored.queue.occurrence_id(index),
            original.occurrence_id(index)
        );
    }
    assert!(QueueSnapshot::capture(&restored.queue).unwrap().is_some());
}

#[test]
fn captured_shuffle_plan_forward_history_repeat_and_rng_survive_repeated_cycles() {
    for seed in 0..24 {
        let mut original = queue();
        original.order = OrderPolicy::with_seed(seed);
        original.reset_order();
        original.set_shuffle(true);
        original.set_repeat_mode(RepeatMode::Queue);
        for _ in 0..3 {
            original.next_track().unwrap();
        }
        original.previous_track().unwrap();
        let snapshot = QueueSnapshot::capture(&original).unwrap().unwrap();
        let mut restored = snapshot.restore(&fresh()).unwrap().queue;
        assert_eq!(restored.history_indices(), original.history_indices());
        assert_eq!(restored.upcoming_indices(), original.upcoming_indices());
        for _ in 0..20 {
            assert_eq!(restored.current_index(), original.current_index());
            assert_eq!(restored.upcoming_indices(), original.upcoming_indices());
            restored.next_track().unwrap();
            original.next_track().unwrap();
        }
        restored.previous_track().unwrap();
        original.previous_track().unwrap();
        assert_eq!(restored.current_index(), original.current_index());
        assert_eq!(restored.history_indices(), original.history_indices());
    }
}

#[test]
fn interleaved_manual_next_reorder_and_shuffle_history_survive_restart() {
    for shuffle in [false, true] {
        let mut original = queue();
        original.set_shuffle(shuffle);
        original.set_repeat_mode(RepeatMode::Queue);
        original.next_track();
        original.previous_track();
        original.insert_next(track("manual-next")).unwrap();
        let future = original.editable_upcoming();
        let source = original
            .occurrence_id(*future.last().unwrap())
            .unwrap()
            .clone();
        let target = original.occurrence_id(future[0]).unwrap().clone();
        assert!(original.move_upcoming(&source, &target, false));
        let snapshot = QueueSnapshot::capture(&original).unwrap().unwrap();
        let mut normalized = fresh();
        normalized.push(track("manual-next"));
        let mut restored = snapshot.restore(&normalized).unwrap().queue;
        assert_eq!(original.items(), restored.items());
        assert_eq!(
            original.context().unwrap().source_positions,
            restored.context().unwrap().source_positions
        );
        assert_eq!(original.context().unwrap().loaded_items_len(), 4);
        assert_eq!(restored.context().unwrap().manual_items_len(), 2);
        assert_eq!(original.history_indices(), restored.history_indices());
        for _ in 0..14 {
            assert_eq!(original.current_index(), restored.current_index());
            assert_eq!(original.upcoming_indices(), restored.upcoming_indices());
            original.next_track();
            restored.next_track();
        }
    }
}

#[test]
fn fresh_unavailability_prunes_exact_occurrences_and_preserves_survivor_provenance() {
    let snapshot = QueueSnapshot::capture(&queue()).unwrap().unwrap();
    let mut normalized = fresh();
    normalized[1].available = false;
    normalized[0].title = "Fresh current title".into();
    let restored = snapshot.restore(&normalized).unwrap();
    assert_eq!(restored.omitted, 1);
    assert!(!restored.current_changed);
    assert_eq!(restored.queue.current_index(), Some(1));
    assert_eq!(
        restored.queue.current().unwrap().title,
        "Fresh current title"
    );
    assert_eq!(
        restored.queue.context().unwrap().source_position(1),
        Some(11)
    );
    assert_eq!(restored.queue.context().unwrap().loaded_items_len(), 3);
    assert_eq!(restored.queue.context().unwrap().manual_items_len(), 1);

    let restored = snapshot.restore(&[track("last"), track("extra")]).unwrap();
    assert_eq!(restored.omitted, 3);
    assert!(restored.current_changed);
    assert_eq!(restored.queue.current().unwrap().id, "last");
    assert_eq!(
        restored.queue.context().unwrap().source_position(0),
        Some(23)
    );
    assert!(QueueSnapshot::capture(&restored.queue).unwrap().is_some());

    let restored = snapshot.restore(&[]).unwrap();
    assert_eq!(restored.omitted, 5);
    assert!(restored.current_changed);
    assert!(restored.queue.context().is_none());
}

#[test]
fn missing_shuffled_current_uses_saved_future_before_physical_order() {
    let mut original = queue();
    original.set_shuffle(true);
    let expected = original
        .upcoming_indices()
        .into_iter()
        .find(|index| *index != 0)
        .unwrap();
    let expected_id = original.items()[expected].id.clone();
    let snapshot = QueueSnapshot::capture(&original).unwrap().unwrap();
    let normalized: Vec<_> = fresh()
        .into_iter()
        .filter(|track| track.id != "same:10")
        .collect();
    let restored = snapshot.restore(&normalized).unwrap();
    assert!(restored.current_changed);
    assert_eq!(restored.queue.current().unwrap().id, expected_id);
    assert!(QueueSnapshot::capture(&restored.queue).unwrap().is_some());
}

#[test]
fn manual_edits_and_all_repeat_modes_remain_valid_after_capture() {
    for mode in [RepeatMode::Off, RepeatMode::Queue, RepeatMode::One] {
        let mut original = queue();
        original.set_repeat_mode(mode);
        original.remove(1);
        original.remove(2);
        original.append(track("tail")).unwrap();
        let normalized = ["same:10", "extra", "tail"]
            .into_iter()
            .map(track)
            .collect::<Vec<_>>();
        let mut restored = QueueSnapshot::capture(&original)
            .unwrap()
            .unwrap()
            .restore(&normalized)
            .unwrap()
            .queue;
        assert_eq!(restored.repeat_mode(), mode);
        assert_eq!(restored.items(), original.items());
        assert_eq!(restored.current_index(), original.current_index());
        assert_eq!(
            restored.advance_eos().map(|track| track.id.clone()),
            original.advance_eos().map(|track| track.id.clone())
        );
    }
}

#[test]
fn capture_excludes_wave_and_wire_rejects_unsupported_queue_states() {
    assert!(
        QueueSnapshot::capture(&MusicQueue::new())
            .unwrap()
            .is_none()
    );
    let mut wave = MusicQueue::new();
    wave.replace_context(
        PlaybackContext::new(
            ContextKind::Wave,
            Some("sensitive-radio-session".into()),
            vec![track("one")],
            0,
            false,
        )
        .unwrap(),
    );
    assert!(QueueSnapshot::capture(&wave).unwrap().is_none());
    let valid = QueueSnapshot::capture(&queue()).unwrap().unwrap();
    let value = serde_json::to_value(&valid).unwrap();
    for mutate in [
        |value: &mut serde_json::Value| value["source_kind"] = "wave".into(),
        |value: &mut serde_json::Value| value["current"] = 500.into(),
        |value: &mut serde_json::Value| value["order"]["cursor"] = 500.into(),
        |value: &mut serde_json::Value| value["order"]["history"] = serde_json::json!([99]),
        |value: &mut serde_json::Value| value["order"]["remaining"] = serde_json::json!([1, 1]),
        |value: &mut serde_json::Value| value["positions"][1] = 5.into(),
        |value: &mut serde_json::Value| value["positions"][4] = 30.into(),
        |value: &mut serde_json::Value| value["ids"][0] = "https://private.invalid/token".into(),
        |value: &mut serde_json::Value| {
            value["origin"] = serde_json::json!({"type":"album", "id":"album"})
        },
    ] {
        let mut invalid = value.clone();
        mutate(&mut invalid);
        assert!(
            serde_json::from_value::<QueueSnapshot>(invalid)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut injected = value;
    injected["access_token"] = "synthetic-secret-must-not-be-admitted".into();
    assert!(serde_json::from_value::<QueueSnapshot>(injected).is_err());
    assert!(
        valid
            .restore(&[track("same:10"), track("same:10")])
            .is_err()
    );
}

#[test]
fn snapshot_contains_selectors_but_no_private_display_or_session_material() {
    let mut original = queue();
    Arc::make_mut(&mut original.context.as_mut().unwrap().items)[0].title =
        "synthetic-private-title".into();
    let snapshot = QueueSnapshot::capture(&original).unwrap().unwrap();
    let json = serde_json::to_string(&snapshot).unwrap();
    for private in [
        "synthetic-private-title",
        "continuous:48",
        "synthetic-view-binding",
        "access_token",
        "cookie",
        "https:",
    ] {
        assert!(!json.contains(private));
        assert!(!format!("{snapshot:?}").contains(private));
    }
    assert!(json.contains("same:10"));
    assert!(json.contains("owner"));
}

#[test]
fn origin_types_keep_server_identity_separate_from_ephemeral_view_keys() {
    for (kind, source) in [
        (ContextKind::LikedTracks, None),
        (ContextKind::Search, Some("private-search-phrase")),
        (ContextKind::Playlist, Some("owner:32")),
        (ContextKind::Album, Some("album")),
        (ContextKind::Artist, Some("artist")),
        (ContextKind::SingleTrack, Some("track:album")),
    ] {
        let origin = PlaybackOrigin::from_source(kind, source);
        assert!(origin.valid_for(kind));
        assert_ne!(origin, PlaybackOrigin::Unknown);
        assert!(!format!("{origin:?}").contains(source.unwrap_or("private-search-phrase")));
    }
    assert!(matches!(
        PlaybackOrigin::from_source(ContextKind::Playlist, Some("a:b:c")),
        PlaybackOrigin::Unknown
    ));
}
