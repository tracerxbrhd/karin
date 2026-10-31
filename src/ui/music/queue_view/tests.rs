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
    assert_eq!(queue_prefix(0, Some(2), Some(3), true), "");
    assert_eq!(queue_prefix(1, Some(2), Some(3), false), "Недоступен · ");
    assert_eq!(queue_prefix(2, Some(2), Some(3), true), "Текущий · ");
    assert_eq!(
        queue_prefix(2, Some(2), Some(3), false),
        "Текущий · Недоступен · "
    );
    assert_eq!(queue_prefix(3, Some(2), Some(3), true), "Далее · ");
    assert_eq!(queue_prefix(4, Some(2), Some(3), true), "");
}
