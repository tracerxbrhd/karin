// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn track(available: bool) -> Track {
    Track {
        id: "42".into(),
        title: "Synthetic queue track".into(),
        artists: Vec::new(),
        artist_links: Vec::new(),
        album_id: None,
        album_title: None,
        duration_ms: None,
        cover: None,
        explicit: false,
        available,
    }
}

#[test]
fn queue_next_marker_follows_actual_available_occurrences() {
    let items = vec![track(true), track(false), track(true), track(true)];
    assert_eq!(next_available_index(&items, Some(0)), Some(2));
    assert_eq!(next_available_index(&items, Some(1)), Some(2));
    assert_eq!(next_available_index(&items, Some(2)), Some(3));
    assert_eq!(next_available_index(&items, Some(3)), None);
    assert_eq!(next_available_index(&items, None), None);
    assert_eq!(next_available_index(&items, Some(4)), None);
    assert_eq!(next_available_index(&items, Some(usize::MAX)), None);
    assert_eq!(next_available_index(&[], Some(0)), None);
}

#[test]
fn queue_labels_do_not_invent_listening_history() {
    // Only the selected and next available occurrence receive state badges.
    // An earlier position is not proof that the user has listened to it.
    assert_eq!(queue_badge(0, Some(2), Some(3)), None);
    assert_eq!(queue_badge(1, Some(2), Some(3)), None);
    assert_eq!(queue_badge(2, Some(2), Some(3)), Some("Сейчас"));
    assert_eq!(queue_badge(3, Some(2), Some(3)), Some("Далее"));
    assert_eq!(queue_badge(4, Some(2), Some(3)), None);
    assert_eq!(queue_badge(0, None, None), None);
}
