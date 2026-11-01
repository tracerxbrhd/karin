// SPDX-License-Identifier: GPL-3.0-or-later
//! Layout and exact-occurrence regressions using synthetic metadata only.

use super::*;
use crate::ui::music::tests::{capture_native, pump_until};
use std::time::Duration;

fn track(id: &str, title: &str) -> Track {
    Track {
        id: id.into(),
        title: title.into(),
        artists: vec!["Synthetic artist".into()],
        artist_links: Vec::new(),
        album_id: Some("7".into()),
        album_title: Some("Synthetic album".into()),
        duration_ms: Some(205_000),
        cover: None,
        explicit: false,
        available: true,
    }
}

#[test]
fn current_row_binds_the_exact_loaded_occurrence_and_preserves_manual_appends() {
    let tracks = vec![track("42", "One"), track("43", "Two"), track("42", "One")];
    let first = PlaybackContext::new(
        ContextKind::Playlist,
        Some("1234:7".into()),
        tracks.clone(),
        0,
        false,
    )
    .unwrap();
    let duplicate = first.with_current_index(2).unwrap();
    assert!(!same_context_occurrence(&first, &duplicate));
    assert!(same_context_occurrence(&duplicate, &duplicate));
    let mut queue = MusicQueue::new();
    queue.replace_context(duplicate.clone());
    queue.append(track("44", "Manual addition")).unwrap();
    assert!(same_context_occurrence(
        &duplicate,
        queue.context().unwrap()
    ));
    for (kind, source, records) in [
        (ContextKind::Album, Some("1234:7".into()), tracks.clone()),
        (ContextKind::Playlist, Some("1234:8".into()), tracks.clone()),
        (
            ContextKind::Playlist,
            Some("1234:7".into()),
            vec![track("43", "Two"), track("42", "One"), track("42", "One")],
        ),
    ] {
        let other = PlaybackContext::new(kind, source, records, 2, false).unwrap();
        assert!(!same_context_occurrence(&duplicate, &other));
    }
}

fn children(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut result = Vec::new();
    let mut child = widget.as_ref().first_child();
    while let Some(current) = child {
        result.push(current.clone());
        result.extend(children(&current));
        child = current.next_sibling();
    }
    result
}

#[test]
#[ignore = "isolated Xvfb native allocation fixture; synthetic data, no account or service"]
fn native_catalog_native_music_layout() {
    gtk::init().unwrap();
    adw::init().unwrap();
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.NativeMusicCatalogFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let ui = crate::ui::build_with_music_pane(
        &app,
        crate::application::ConnectionService::new(crate::api::YandexApi::new().unwrap()),
        |app, window| {
            MusicPane::with_provider(
                app,
                window,
                None,
                Arc::new(OfflineProvider),
                launch_system_browser,
            )
        },
    );
    let this = &ui.music.controller;
    let window = &ui.window;
    window.set_title(Some("Karin synthetic HTTP fixture"));
    let tracks: Vec<_> = (0..12)
        .map(|index| {
            track(
                &format!("{}", index + 40),
                &format!("Synthetic song {}", index + 1),
            )
        })
        .collect();
    let playlist = Playlist {
        id: crate::music::PlaylistId {
            owner: "1234".into(),
            kind: "3".into(),
        },
        title: "Synthetic saved playlist".into(),
        track_count: 12,
        cover: None,
    };
    {
        let mut model = this.model.borrow_mut();
        model.token = Some(AccessToken::for_test(
            "synthetic-native-music-catalog",
            Duration::from_secs(300),
        ));
        model.account_valid = true;
        model.liked = Some(Ok(Page {
            items: tracks.clone(),
            has_more: false,
        }));
        model.playlists = Some(Ok(Page {
            items: vec![playlist],
            has_more: false,
        }));
    }
    this.render_controls();
    this.set_info_open(false);
    this.render_home_preview();
    assert!(
        children(&this.widgets.home_signedin)
            .iter()
            .any(|child| child.widget_name() == "music-wave-placeholder")
    );
    assert!(
        children(&this.widgets.home_preview).is_empty(),
        "Wave never loads detached personal-track previews"
    );
    // A real collection action is inert until there is a playable loaded
    // item. A later append failure must not disable the usable loaded prefix.
    let availability = this.new_track_feed(
        continuous_loading::FeedSource::Liked,
        ContextKind::LikedTracks,
        None,
        &this.widgets.library_rows,
        &this.widgets.library_feedback,
    );
    let availability_play = this.feed_play_button(&availability);
    assert!(
        !availability_play.is_sensitive(),
        "loading/empty collection has no enabled no-op Play"
    );
    let mut unavailable = tracks[0].clone();
    unavailable.available = false;
    this.append_track_feed(
        &availability,
        0,
        Page {
            items: vec![unavailable],
            has_more: true,
        },
    );
    assert!(
        !availability_play.is_sensitive(),
        "all-unavailable loaded rows cannot start playback"
    );
    this.append_track_feed(
        &availability,
        1,
        Page {
            items: vec![tracks[1].clone()],
            has_more: false,
        },
    );
    assert!(availability_play.is_sensitive());
    availability.data.borrow_mut().error = Some(MusicError::Network);
    this.refresh_feed_status(&availability);
    assert!(
        availability_play.is_sensitive(),
        "a failed append leaves actual loaded audio actions usable"
    );
    window.set_default_size(1440, 900);
    window.present();
    this.render_liked(
        Ok(Page {
            items: tracks.clone(),
            has_more: false,
        }),
        0,
    );
    this.widgets.stack.set_visible_child_name("library");
    super::super::test_input::wait_for_layout(window, &this.widgets.library_rows);
    let cache_feed = this
        .track_feeds
        .borrow()
        .iter()
        .find(|feed| feed.rows == this.widgets.library_rows)
        .unwrap()
        .clone();
    cache_feed
        .origins
        .borrow_mut()
        .record(0, crate::catalog_cache::Provenance::Network, false);
    cache_feed
        .origins
        .borrow_mut()
        .record(1, crate::catalog_cache::Provenance::StaleCache, true);
    cache_feed
        .origins
        .borrow_mut()
        .record(2, crate::catalog_cache::Provenance::Network, false);
    this.refresh_catalog_notice();
    assert!(
        this.widgets.cache_notice.is_revealed(),
        "laternetworkappenddoesnoteraseearlierstalerows"
    );
    this.render_liked(
        Ok(Page {
            items: tracks.clone(),
            has_more: false,
        }),
        0,
    );
    super::super::test_input::wait_for_layout(window, &this.widgets.library_rows);
    this.refresh_catalog_notice();
    assert!(
        !this.widgets.cache_notice.is_revealed(),
        "a new loaded epoch cannot inherit the old source warning"
    );
    let play_liked = children(&this.widgets.library_hero)
        .into_iter()
        .find(|child| child.widget_name() == "music-collection-play")
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    play_liked.emit_clicked();
    assert_eq!(
        this.model.borrow().queue.context().unwrap().source_kind(),
        ContextKind::LikedTracks
    );
    assert_eq!(this.model.borrow().queue.items().len(), 12);
    capture_native("native-liked-real-collection-first");
    this.widgets.stack.set_visible_child_name("search");
    this.clear_search_feedback();
    this.tracks_from(
        &this.widgets.search_rows,
        tracks.clone(),
        ContextKind::Search,
        None,
        false,
        0..usize::MAX,
    );
    let context =
        PlaybackContext::new(ContextKind::Search, None, tracks.clone(), 2, false).unwrap();
    this.model.borrow_mut().queue.replace_context(context);
    this.render_music_player();
    this.sync_catalog_rows();
    window.present();
    let rows: Vec<_> = children(&this.widgets.search_rows)
        .into_iter()
        .filter_map(|child| child.downcast::<gtk::ListBoxRow>().ok())
        .collect();
    assert_eq!(rows.len(), 12);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row.has_css_class("music-current"), index == 2);
    }
    for (theme, scheme) in [
        ("light", adw::ColorScheme::ForceLight),
        ("dark", adw::ColorScheme::ForceDark),
    ] {
        adw::StyleManager::default().set_color_scheme(scheme);
        for (shape, width, height) in [
            ("compact", 420, 720),
            ("medium", 900, 650),
            ("wide", 1440, 900),
        ] {
            // The shared capture helper visits both schemes and ends in dark.
            // Restore this iteration's scheme before measuring native geometry.
            adw::StyleManager::default().set_color_scheme(scheme);
            window.set_default_size(width, height);
            pump_until(|| {
                window
                    .surface()
                    .is_some_and(|surface| surface.width() == width && surface.height() == height)
                    && rows[0].height() > 0
            });
            super::super::test_input::wait_for_layout(window, &rows[0]);
            for row in rows.iter().take(6) {
                assert!(
                    (60..=66).contains(&row.height()),
                    "compact song-row allocation must remain 60–66 logical pixels"
                );
                let art = children(row)
                    .into_iter()
                    .find_map(|child| child.downcast::<gtk::Overlay>().ok())
                    .unwrap();
                assert_eq!((art.width(), art.height()), (48, 48));
            }
            if shape == "medium" {
                let footer = ui.music.player_bar.compute_bounds(window).unwrap();
                let bottom = if this.widgets.player_revealer.reveals_child() {
                    footer.y()
                } else {
                    window.height() as f32
                };
                let visible = rows
                    .iter()
                    .filter(|row| {
                        row.compute_bounds(window).is_some_and(|bounds| {
                            bounds.y() >= 0.0 && bounds.y() + bounds.height() <= bottom
                        })
                    })
                    .count();
                assert!(
                    visible >= 6,
                    "medium search should expose six complete songs; got {visible}"
                );
            }
            capture_native(&format!("native-music-search-{theme}-{shape}"));
            if shape == "medium" {
                adw::StyleManager::default().set_color_scheme(scheme);
                this.render_liked(
                    Ok(Page {
                        items: tracks.clone(),
                        has_more: false,
                    }),
                    0,
                );
                this.widgets.stack.set_visible_child_name("library");
                super::super::test_input::wait_for_layout(window, &this.widgets.library_rows);
                let footer = ui.music.player_bar.compute_bounds(window).unwrap();
                let bottom = if this.widgets.player_revealer.reveals_child() {
                    footer.y()
                } else {
                    window.height() as f32
                };
                let library_rows = children(&this.widgets.library_rows)
                    .into_iter()
                    .filter_map(|child| child.downcast::<gtk::ListBoxRow>().ok())
                    .collect::<Vec<_>>();
                // This collection now has the required native 144px artwork
                // and its own reusable search row. Preserve useful tracks and
                // readable actions rather than imposing the retired 44px text
                // summary/six-row composition. Global search above keeps its
                // independent six-whole-row density gate.
                let hero_widgets = children(&this.widgets.library_hero);
                let hero = hero_widgets
                    .iter()
                    .find(|widget| widget.widget_name() == "music-liked-hero")
                    .unwrap();
                let art = hero_widgets
                    .iter()
                    .find(|widget| widget.widget_name() == "music-liked-cover")
                    .unwrap();
                super::super::test_input::wait_for_layout(window, art);
                let hero_bounds = hero.compute_bounds(window).unwrap();
                let cover = art.compute_bounds(window).unwrap();
                assert!(
                    cover.width() >= 80.0 && (cover.width() - cover.height()).abs() <= 1.0,
                    "the real collection identity remains a readable native square: {cover:?}"
                );
                assert!(
                    hero_bounds.height() <= window.height() as f32 * 0.32,
                    "the expressive hero must leave useful music visible: {hero_bounds:?}"
                );
                for widget in &hero_widgets {
                    if [
                        "music-liked-cover",
                        "music-liked-title",
                        "music-liked-facts",
                        "music-collection-play",
                    ]
                    .contains(&widget.widget_name().as_str())
                    {
                        let bounds = widget.compute_bounds(window).unwrap();
                        assert!(
                            widget.is_mapped() && bounds.width() > 0.0 && bounds.height() > 0.0
                        );
                        assert!(
                            bounds.x() >= hero_bounds.x() - 1.0
                                && bounds.y() >= hero_bounds.y() - 1.0
                                && bounds.x() + bounds.width()
                                    <= hero_bounds.x() + hero_bounds.width() + 1.0
                                && bounds.y() + bounds.height()
                                    <= hero_bounds.y() + hero_bounds.height() + 1.0,
                            "native collection content fits its whole hero: {bounds:?}/{hero_bounds:?}"
                        );
                    }
                }
                let title = hero_widgets
                    .iter()
                    .find(|widget| widget.widget_name() == "music-liked-title")
                    .unwrap()
                    .downcast_ref::<gtk::Label>()
                    .unwrap();
                assert!(
                    !title.layout().is_ellipsized(),
                    "the collection title must stay readable"
                );
                let feed = this
                    .track_feeds
                    .borrow()
                    .iter()
                    .find(|feed| feed.rows == this.widgets.library_rows)
                    .unwrap()
                    .clone();
                let filter = &feed.filter_ui.as_ref().unwrap().search;
                assert!(filter.is_mapped() && filter.is_sensitive());
                let search_bounds = filter.compute_bounds(window).unwrap();
                assert!(
                    search_bounds.width() >= 120.0 && search_bounds.height() <= 56.0,
                    "the real collection search remains usable without dominating the list: {search_bounds:?}"
                );
                let play = hero_widgets
                    .iter()
                    .find(|widget| widget.widget_name() == "music-collection-play")
                    .unwrap()
                    .downcast_ref::<gtk::Button>()
                    .unwrap();
                for action in [
                    play.upcast_ref::<gtk::Widget>(),
                    this.widgets.library_refresh.upcast_ref(),
                    filter.upcast_ref(),
                ] {
                    let bounds = action.compute_bounds(window).unwrap();
                    assert!(
                        action.is_mapped()
                            && action.is_sensitive()
                            && bounds.x() >= 0.0
                            && bounds.y() >= 0.0
                            && bounds.x() + bounds.width() <= window.width() as f32
                            && bounds.y() + bounds.height() <= bottom,
                        "collection actions must remain exposed above the footer: {bounds:?}"
                    );
                    let picked = window.pick(
                        f64::from(bounds.x() + bounds.width() / 2.0),
                        f64::from(bounds.y() + bounds.height() / 2.0),
                        gtk::PickFlags::DEFAULT,
                    );
                    assert!(
                        picked
                            .as_ref()
                            .is_some_and(|picked| picked == action || picked.is_ancestor(action)),
                        "the native collection target must not be covered"
                    );
                }
                let viewport = this
                    .content_scroll
                    .borrow()
                    .upgrade()
                    .unwrap()
                    .compute_bounds(window)
                    .unwrap();
                let visible = library_rows
                    .iter()
                    .filter(|row| {
                        row.compute_bounds(window).is_some_and(|bounds| {
                            bounds.y() >= viewport.y()
                                && bounds.y() + bounds.height()
                                    <= bottom.min(viewport.y() + viewport.height())
                        })
                    })
                    .count();
                assert!(
                    visible >= 3,
                    "medium library hero/search must expose three complete songs; got {visible}"
                );
                capture_native(&format!("native-music-library-{theme}-medium"));
                this.widgets.stack.set_visible_child_name("search");
                super::super::test_input::wait_for_layout(window, &rows[0]);
            }
        }
    }
    this.detail_cache
        .set(Some((crate::catalog_cache::Provenance::StaleCache, true)));
    this.render_album(
        Rc::new(Album {
            id: "7".into(),
            title: "Synthetic album".into(),
            artists: vec!["Synthetic artist".into()],
            tracks: (0..63)
                .map(|index| tracks[index % tracks.len()].clone())
                .collect(),
            cover: None,
        }),
        0,
    );
    super::super::test_input::wait_for_layout(window, &this.widgets.detail);
    this.refresh_catalog_notice();
    assert!(
        this.widgets.cache_notice.is_revealed(),
        "local Album first chunk inherits the accepted cached metadata origin"
    );
    let album_feed = this
        .track_feeds
        .borrow()
        .iter()
        .find(|feed| matches!(feed.source, continuous_loading::FeedSource::Album(_)))
        .unwrap()
        .clone();
    assert_eq!(album_feed.data.borrow().items.len(), 50);
    this.request_track_feed(&album_feed, false);
    this.refresh_catalog_notice();
    assert_eq!(album_feed.data.borrow().items.len(), 63);
    assert_eq!(
        album_feed.origins.borrow().aggregate(),
        Some((crate::catalog_cache::Provenance::StaleCache, true))
    );
    assert!(
        this.widgets.cache_notice.is_revealed(),
        "later local chunks cannot erase the Album cache/storage warning"
    );
    let hero = children(&this.widgets.detail)
        .into_iter()
        .find(|child| child.widget_name() == "music-detail-hero")
        .unwrap();
    assert!(
        children(&hero)
            .iter()
            .any(|child| child.widget_name() == "music-detail-cover")
    );
    let primary = children(&this.widgets.detail)
        .into_iter()
        .filter(|child| child.has_css_class("music-collection-actions"))
        .collect::<Vec<_>>();
    assert_eq!(primary.len(), 1);
    assert_eq!(
        children(&primary[0])
            .iter()
            .filter(|child| child.widget_name() == "music-collection-play")
            .count(),
        1
    );
    this.model.borrow_mut().generation += 1;
    this.widgets.stack.set_visible_child_name("home");
    play_liked.emit_clicked();
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("home"),
        "retired-account collection action cannot navigate"
    );
    this.render_home_preview();
    assert!(children(&this.widgets.home_collections).is_empty());
    ui.music.close();
    window.close();
}
