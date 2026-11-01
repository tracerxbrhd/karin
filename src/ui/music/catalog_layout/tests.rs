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
    let home_rows: Vec<_> = children(&this.widgets.home_preview)
        .into_iter()
        .filter_map(|child| child.downcast::<gtk::ListBoxRow>().ok())
        .collect();
    assert_eq!(
        home_rows.len(),
        3,
        "Home previews loaded tracks instead of repeating navigation cards"
    );
    assert!(
        children(&this.widgets.home_hero)
            .iter()
            .any(|child| { child.widget_name() == "music-home-play-liked" }),
        "the real loaded liked collection has its own Play action"
    );
    assert!(
        !children(&this.widgets.home_signedin)
            .iter()
            .any(|child| { child.has_css_class("music-destination") }),
        "primary rail destinations must not become oversized Home cards"
    );
    window.set_default_size(1440, 900);
    window.present();
    super::super::test_input::wait_for_layout(window, &home_rows[0]);
    assert!(
        home_rows[0].compute_bounds(window).unwrap().y() < 270.0,
        "real Home music starts near the first useful screen band"
    );
    capture_native("redesign-home-real-library-first");
    let play_home = children(&this.widgets.home_hero)
        .into_iter()
        .find(|child| child.widget_name() == "music-home-play-liked")
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    play_home.emit_clicked();
    assert_eq!(
        this.model.borrow().queue.context().unwrap().source_kind(),
        ContextKind::LikedTracks
    );
    assert_eq!(
        this.model.borrow().queue.items().len(),
        12,
        "the short Home preview still starts the actual loaded context"
    );
    assert_eq!(
        children(&this.widgets.home_collections)
            .iter()
            .filter(|child| child.widget_name() == "music-home-playlist-tile")
            .count(),
        1
    );
    let old_home_tile = children(&this.widgets.home_collections)
        .into_iter()
        .find(|child| child.widget_name() == "music-home-playlist-tile")
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
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
                let visible = rows
                    .iter()
                    .filter(|row| {
                        row.compute_bounds(window).is_some_and(|bounds| {
                            bounds.y() >= 0.0 && bounds.y() + bounds.height() <= footer.y()
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
                let library_rows = children(&this.widgets.library_rows)
                    .into_iter()
                    .filter_map(|child| child.downcast::<gtk::ListBoxRow>().ok())
                    .collect::<Vec<_>>();
                let visible = library_rows
                    .iter()
                    .filter(|row| {
                        row.compute_bounds(window).is_some_and(|bounds| {
                            bounds.y() >= 0.0 && bounds.y() + bounds.height() <= footer.y()
                        })
                    })
                    .count();
                assert!(
                    this.widgets.library_hero.height() <= 44,
                    "library summary must stay compact"
                );
                assert!(
                    visible >= 6,
                    "medium library should expose six complete songs; got {visible}"
                );
                capture_native(&format!("native-music-library-{theme}-medium"));
                this.widgets.stack.set_visible_child_name("search");
                super::super::test_input::wait_for_layout(window, &rows[0]);
            }
        }
    }
    this.render_album(
        Rc::new(Album {
            id: "7".into(),
            title: "Synthetic album".into(),
            artists: vec!["Synthetic artist".into()],
            tracks,
            cover: None,
        }),
        0,
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
            .filter(|child| child.widget_name() == "music-track-play")
            .count(),
        1
    );
    this.model.borrow_mut().generation += 1;
    this.widgets.stack.set_visible_child_name("home");
    old_home_tile.emit_clicked();
    assert_eq!(
        this.widgets.stack.visible_child_name().as_deref(),
        Some("home"),
        "retired-account tile cannot navigate"
    );
    this.model.borrow_mut().playlists = None;
    this.render_home_preview();
    assert!(!this.widgets.home_collections.is_visible());
    ui.music.close();
    window.close();
}
