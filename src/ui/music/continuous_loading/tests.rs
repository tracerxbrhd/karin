// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded-view windows and exact occurrences; no HTTP, account or GTK display.
use super::*;

fn track() -> Track {
    Track {
        id: "42".into(),
        title: "Synthetic repeated occurrence".into(),
        artists: Vec::new(),
        artist_links: Vec::new(),
        album_id: None,
        album_title: None,
        duration_ms: None,
        available: true,
        cover: None,
        explicit: false,
    }
}
fn page(count: usize, has_more: bool) -> Page<Track> {
    Page {
        items: vec![track(); count],
        has_more,
    }
}

#[test]
fn continuous_source_windows_keep_exact_499_500_1999_positions_and_bound_queue() {
    let mut view = LoadedTracks::new(ContextKind::LikedTracks, None, 7);
    for page_index in 0..40 {
        view.append(page_index, page(50, true));
    }
    assert_eq!(view.items.len(), VIEW_LIMIT);
    assert!(view.at_limit());
    assert!(!view.can_request());
    for (index, local) in [(0, 0), (499, 499), (500, 0), (999, 499), (1999, 499)] {
        let context = view.context(index).unwrap();
        assert_eq!(context.items().len(), 500);
        assert_eq!(context.current_index(), local);
        assert_eq!(context.source_position(local), Some(local));
        assert!(
            context.is_partial(),
            "a source window is not the complete remote collection"
        );
        let mut queue = MusicQueue::new();
        queue.replace_context(context);
        assert_eq!(queue.items().len(), 500);
        assert!(
            queue.append(track()).is_err(),
            "the visible 2000-item cap never expands the live queue cap"
        );
    }
    assert_ne!(
        view.context(499).unwrap().source_id(),
        view.context(1999).unwrap().source_id()
    );
    assert!(view.context(2000).is_none());
}

#[test]
fn appending_continuous_pages_preserves_live_queue_and_duplicate_origin_marker() {
    let mut view = LoadedTracks::new(ContextKind::Playlist, Some("synthetic-owner:7".into()), 3);
    view.append(0, page(50, true));
    let mut queue = MusicQueue::new();
    queue.replace_context(view.context(49).unwrap());
    let original = queue.context().unwrap().clone();
    view.append(1, page(50, true));
    assert_eq!(
        queue.context(),
        Some(&original),
        "scroll loading cannot replace or extend active playback"
    );
    assert_eq!(queue.items().len(), 50);
    assert!(same_loaded_occurrence(
        &view.context(49).unwrap(),
        queue.context().unwrap()
    ));
    assert!(!same_loaded_occurrence(
        &view.context(48).unwrap(),
        queue.context().unwrap()
    ));
    queue.remove(0);
    assert_eq!(queue.current_index(), Some(48));
    assert_eq!(queue.context().unwrap().source_position(48), Some(49));
    assert!(same_loaded_occurrence(
        &view.context(49).unwrap(),
        queue.context().unwrap()
    ));
    assert!(
        !same_loaded_occurrence(&view.context(48).unwrap(), queue.context().unwrap()),
        "a reindexed equal-ID queue entry is still the original 50th source occurrence"
    );
    queue.append(track()).unwrap();
    let manual = queue.items().len() - 1;
    queue.select(manual);
    assert_eq!(queue.context().unwrap().source_position(manual), None);
    assert!(
        (0..view.items.len()).all(|index| !same_loaded_occurrence(
            &view.context(index).unwrap(),
            queue.context().unwrap()
        )),
        "manual equal-ID additions have no original loaded-row occurrence"
    );
}

#[test]
fn repeated_or_wrong_page_never_duplicates_or_clears_active_loading_error() {
    let mut view = LoadedTracks::new(ContextKind::Search, None, 1);
    assert_eq!(view.append(0, page(2, true)), 0..2);
    view.loading = true;
    view.error = Some(MusicError::Network);
    assert_eq!(view.append(0, page(2, true)), 2..2);
    assert_eq!(view.append(2, page(2, true)), 2..2);
    assert_eq!(
        view.items.len(),
        2,
        "two equal IDs are two positions, page replay is not"
    );
    assert!(view.loading);
    assert_eq!(view.error, Some(MusicError::Network));
    assert_eq!(view.next_page, 1);
    assert_eq!(view.append(1, page(2, false)), 2..4);
    assert!(!view.loading);
    assert!(view.error.is_none());
    assert!(!view.has_more);
    assert!(!view.can_request());
    assert_eq!(view.items.len(), 4);
    assert!(!view.context(3).unwrap().is_partial());
}

#[test]
fn loaded_feed_epoch_retires_stale_markers_and_all_caps_remain_honest() {
    let mut old = LoadedTracks::new(ContextKind::Search, None, 1);
    old.append(0, page(2, false));
    let old_context = old.context(1).unwrap();
    let mut new = LoadedTracks::new(ContextKind::Search, None, 2);
    new.append(0, page(2, false));
    assert!(!same_loaded_occurrence(
        &new.context(1).unwrap(),
        &old_context
    ));
    new.incomplete = true;
    assert!(
        new.context(1).unwrap().is_partial(),
        "filtered unavailable data is not a complete context"
    );
    let mut bounded = LoadedTracks::new(ContextKind::LikedTracks, None, 3);
    assert_eq!(
        bounded.append(0, page(VIEW_LIMIT + 1, false)),
        0..VIEW_LIMIT
    );
    assert!(bounded.has_more);
    assert!(bounded.context(VIEW_LIMIT - 1).unwrap().is_partial());
    assert!(!bounded.can_request());
    let mut empty = LoadedTracks::new(ContextKind::Search, None, 4);
    for page_index in 0..PAGE_LIMIT {
        assert!(empty.can_request());
        empty.append(page_index, page(0, true));
    }
    assert!(empty.at_limit());
    assert!(
        !empty.can_request(),
        "even inconsistent empty/more pages have a finite request budget"
    );
    assert!(
        empty.has_more,
        "a local request cap must not claim remote end-of-list"
    );
    assert!(empty.context(0).is_none());
}

#[test]
fn repeated_marker_queries_share_at_most_four_immutable_track_windows() {
    let mut view = LoadedTracks::new(ContextKind::LikedTracks, None, 20);
    for page_index in 0..40 {
        view.append(page_index, page(50, true));
    }
    let windows = [0, 500, 1000, 1500].map(|index| view.context(index).unwrap());
    for _ in 0..3 {
        for index in 0..VIEW_LIMIT {
            let selected = view.context(index).unwrap();
            assert_eq!(
                selected.items().as_ptr(),
                windows[index / 500].items().as_ptr(),
                "an 80ms marker pass must share the window instead of cloning 500 Tracks per row"
            );
            assert_eq!(selected.current_index(), index % 500);
        }
    }
    assert_eq!(view.contexts.borrow().len(), 4);
    let preserved = windows[0].clone();
    // A duplicate page must not churn the cached windows or live queue data.
    view.append(0, page(50, true));
    assert_eq!(
        view.context(0).unwrap().items().as_ptr(),
        preserved.items().as_ptr()
    );
    assert_eq!(preserved.items().len(), 500);
    let mut growing = LoadedTracks::new(ContextKind::LikedTracks, None, 21);
    growing.append(0, page(50, true));
    let active = growing.context(49).unwrap();
    growing.append(1, page(50, true));
    let expanded = growing.context(49).unwrap();
    assert_ne!(active.items().as_ptr(), expanded.items().as_ptr());
    assert_eq!(
        active.items().len(),
        50,
        "accepted append cannot mutate a playing window snapshot"
    );
    assert_eq!(expanded.items().len(), 100);
}
