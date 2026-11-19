// SPDX-License-Identifier: GPL-3.0-or-later
//! Inspect the ordinary mapped application's rendered text during real playback.

use super::*;
use gtk::{graphene, gsk};

#[path = "glyph_pixels.rs"]
mod glyph_pixels;
use glyph_pixels::verify_transition;

struct Frame {
    frame_time: i64,
    position: Duration,
    playing: bool,
    node: gsk::RenderNode,
    scroll: f64,
    active: Option<u32>,
    settled: bool,
    center_error: Option<f32>,
    rows: [graphene::Rect; 3],
    scales: [f64; 3],
}

pub(super) fn ordinary_lyrics_playback(app: &adw::Application) {
    let fixture = Fixture::with_karaoke_audio(true);
    let ui = window_for(app, &fixture);
    let window = &ui.window;
    let this = &ui.music.controller;
    let view = &this.track_pane.extras;
    let context = PlaybackContext::new(
        ContextKind::Queue,
        None,
        vec![track(46), track(46), track(44)],
        1,
        false,
    )
    .unwrap();
    this.play_music_context(context.clone());
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == PlaybackState::Playing)
    });
    this.pause_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == PlaybackState::Paused)
    });
    let output = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic HTTP fixture$",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();
    let id = output.lines().next().unwrap();
    input(&["windowfocus", id]);
    let open_from_info = |context| {
        this.open_loaded_track_info(track(46), Some(context));
        pump_until(|| this.track_pane.menu.is_mapped());
        click(window, id, &this.track_pane.menu, false);
        let popover = this.track_pane.menu.popover().unwrap();
        pump_until(|| {
            test_input::menu_action_widget(&popover, "track-pane.lyrics")
                .is_some_and(|item| item.is_mapped())
        });
        let action = test_input::menu_action_widget(&popover, "track-pane.lyrics").unwrap();
        action.grab_focus();
        input(&["key", "--clearmodifiers", "Return"]);
        pump_until(|| view.lyrics_widget.is_mapped() && !popover.is_mapped());
    };
    // Both rows carry the same service ID, yet only the actual occurrence
    // owns the confirmed playhead when its ordinary Info → Lyrics is opened.
    open_from_info(context.with_current_index(0).unwrap());
    assert!(!this.information_follows_current_playback());
    open_from_info(context);
    assert!(
        this.information_follows_current_playback(),
        "Info → Lyrics on the proven current occurrence must receive the real player clock"
    );
    pump_until(|| view.lyrics_widget.is_mapped() && view.inner.lyrics.borrow().is_some());
    click(window, id, &view.inner.external, false);
    let dialog = external_consent_ready(window, view);
    click(
        window,
        id,
        &find_button(dialog.upcast_ref(), "Искать").unwrap(),
        false,
    );
    external_consent_closed(view, &dialog);
    pump_until(|| view.inner.synchronized.get());
    assert_eq!(fixture.counts.lock().unwrap().external, 1);

    let settings = window.settings();
    assert!(
        settings.is_gtk_enable_animations(),
        "normal desktop test starts with motion enabled"
    );
    let style = adw::StyleManager::default();
    let original_scheme = style.color_scheme();
    let hc = style.is_high_contrast();
    assert_eq!(hc, window.has_css_class("karin-high-contrast"));
    // Keep both sampled pairs fully visible at the top of the text. The
    // third boundary scrolls to row 11 and is measured separately below.
    super::super::super::player::tests::resize_player_window(&ui, 900, 960);
    let surface = std::iter::successors(view.lyrics_widget.parent(), gtk::Widget::parent)
        .find(|widget| widget.has_css_class("music-information-view"))
        .expect("mapped native sidebar with its themed background");
    let host = surface.parent().unwrap();
    let sidebar = host
        .downcast_ref::<crate::ui::music::sliding_panel::SlidingPanel>()
        .unwrap();
    let panel = || surface.compute_bounds(&host).unwrap();
    for (dark, motion) in [(false, true), (true, true), (true, false), (false, false)] {
        settings.set_gtk_enable_animations(motion);
        style.set_color_scheme(if dark {
            adw::ColorScheme::ForceDark
        } else {
            adw::ColorScheme::ForceLight
        });
        pump_until(|| style.is_dark() == dark);
        if dark {
            // Reopen the same synchronized selection through the footer in a
            // compact window. No extra provider lookup or timer is involved.
            click(window, id, &this.track_pane.close, false);
            pump_until(|| !view.lyrics_widget.is_mapped());
            super::super::super::player::tests::resize_player_window(&ui, 420, 900);
            click(window, id, &this.widgets.lyrics, false);
            pump_until(|| view.lyrics_widget.is_mapped() && view.inner.synchronized.get());
            assert!(this.information_follows_current_playback());
        } else {
            super::super::super::player::tests::resize_player_window(&ui, 900, 960);
        }
        let theme = match (hc, dark) {
            (false, false) => "light",
            (false, true) => "dark",
            (true, false) => "hc-light",
            (true, true) => "hc-dark",
        };
        let serial = this.music_snapshot().unwrap().seek_serial;
        this.seek_music(Duration::from_millis(160));
        pump_until(|| {
            this.music_snapshot()
                .is_some_and(|s| s.seek_serial > serial)
                && view.inner.active.get() == Some(0)
        });
        let mapped_rows = || {
            let bound = view.inner.visible_lines.borrow();
            [0, 1, 5].map(|index| {
                bound
                    .get(&index)
                    .and_then(glib::WeakRef::upgrade)
                    .filter(|label| label.is_mapped() && label.height() > 0)
            })
        };
        // The snapshot and GtkListView binding/allocation run in different
        // callbacks on a resize or backwards seek. Wait for the actual rows.
        pump_until(|| {
            sidebar.is_child_revealed()
                && mapped_rows().iter().all(Option::is_some)
                && view.inner.lyrics_scroll.vadjustment().value() < 1.0
        });
        let labels = mapped_rows().map(Option::unwrap);
        let rows = labels.each_ref().map(|l| l.parent().unwrap());
        let viewport = view.inner.lyrics_scroll.compute_bounds(&host).unwrap();
        // The scroll widget includes its native scrollbar. Measure physical
        // ink only in its exposed content column, including the final pixel
        // next to the scrollbar in the compact footer-opened pane.
        let bar = view
            .inner
            .lyrics_scroll
            .vscrollbar()
            .compute_bounds(&host)
            .unwrap();
        let exposed_width = viewport.width().min(bar.x() - viewport.x());
        let viewport =
            graphene::Rect::new(viewport.x(), viewport.y(), exposed_width, viewport.height());
        let horizontal = view.inner.lyrics_scroll.hadjustment();
        assert!(
            horizontal.upper() <= horizontal.page_size() + 1.0,
            "lyrics overflow horizontally"
        );
        let adjustment = view.inner.lyrics_scroll.vadjustment();
        // Retain the actual mapped scene and row positions from each paint.
        // Texture downloads happen after playback so sampling cannot drive it.
        let frames = Rc::new(RefCell::new(Vec::new()));
        let clock = window.frame_clock().unwrap();
        let handler = clock.connect_after_paint({
            let frames = frames.clone();
            let controller = Rc::downgrade(this);
            let lyrics = view.inner.clone();
            let surface = surface.clone();
            let host = host.clone();
            move |clock| {
                let Some(snapshot) = controller.upgrade().and_then(|c| c.music_snapshot()) else {
                    return;
                };
                if !matches!(
                    snapshot.state,
                    PlaybackState::Playing | PlaybackState::Paused
                ) {
                    return;
                }
                let Some(position) = snapshot.position else {
                    return;
                };
                // After GTK has painted the mapped sidebar, snapshot_child
                // returns its cached node, including its normal theme and
                // clipping. Observing the subtree leaves toplevel damage and
                // its idle publication schedule out of the sampling path.
                let scene = gtk::Snapshot::new();
                host.snapshot_child(&surface, &scene);
                if let Some(node) = scene.to_node() {
                    frames.borrow_mut().push(Frame {
                        frame_time: clock.frame_time(),
                        position,
                        playing: snapshot.state == PlaybackState::Playing,
                        node,
                        scroll: adjustment.value(),
                        active: lyrics.active.get(),
                        center_error: lyrics.active.get().filter(|&i| i >= 11).and_then(|i| {
                            let label = lyrics.visible_lines.borrow().get(&i)?.upgrade()?;
                            let b = label.compute_bounds(&lyrics.lyrics_scroll)?;
                            Some(
                                (b.y() + b.height() / 2.
                                    - lyrics.lyrics_scroll.height() as f32 / 2.)
                                    .abs(),
                            )
                        }),
                        settled: lyrics.line_transitions.borrow().is_empty(),
                        rows: rows.each_ref().map(|r| r.compute_bounds(&host).unwrap()),
                        scales: labels.each_ref().map(label_scale),
                    });
                }
            }
        });
        this.toggle_music();
        // Observe uninterrupted Playing through both the adjacent and grouped
        // natural handoffs before preparing the deep, adjacent Follow passage.
        pump_until(|| {
            frames.borrow().last().is_some_and(|f| {
                f.playing
                    && f.active == Some(5)
                    && f.settled
                    && f.position >= Duration::from_millis(3900)
            })
        });
        let first_passage_end = frames.borrow().len();
        this.pause_music();
        pump_until(|| {
            this.music_snapshot()
                .is_some_and(|s| s.state == PlaybackState::Paused)
        });
        let previous_seek = this.music_snapshot().unwrap().seek_serial;
        this.seek_music(Duration::from_millis(5050));
        pump_until(|| {
            this.music_snapshot()
                .is_some_and(|s| s.seek_serial > previous_seek)
                && frames.borrow().last().is_some_and(|f| {
                    !f.playing
                        && f.active == Some(11)
                        && f.settled
                        && f.center_error.is_some_and(|error| error <= 2.0)
                })
        });
        let follow_start = frames.borrow().len() - 1;
        this.toggle_music();
        pump_until(|| {
            let frames = frames.borrow();
            frames.last().is_some_and(|f| {
                f.active == Some(12)
                    && f.settled
                    && f.position >= Duration::from_millis(6350)
                    && view.inner.scroll_transition.get().is_none()
            })
        });
        clock.disconnect(handler);
        this.pause_music();
        pump_until(|| {
            this.music_snapshot()
                .is_some_and(|s| s.state == PlaybackState::Paused)
        });
        let samples = frames.take();
        let optical: Vec<_> = samples[..first_passage_end]
            .iter()
            .filter(|f| f.playing)
            .collect();
        for (old, next, old_index, next_index, timestamp, name) in [
            (0, 1, 0, 1, 1400, "adjacent"),
            (1, 2, 1, 5, 3300, "simultaneous"),
        ] {
            // Locate the actual LRC handoff and its settled GTK presentation.
            // Only the scene pixels below determine the two physical scales.
            let changed = optical
                .iter()
                .position(|f| f.active == Some(next_index))
                .unwrap();
            assert!(optical[changed].position >= Duration::from_millis(timestamp));
            let before = optical[..changed]
                .iter()
                .rposition(|f| f.active == Some(old_index) && f.settled)
                .unwrap();
            let after = optical[changed..]
                .iter()
                .rposition(|f| f.active == Some(next_index) && f.settled)
                .unwrap()
                + changed;
            verify_transition(
                window,
                &optical[before..=after],
                [old, next],
                [panel(), viewport],
                motion,
                name,
                theme,
            );
        }
        verify_follow(&samples[follow_start..], samples[0].scroll, motion);
    }
    settings.set_gtk_enable_animations(true);
    style.set_color_scheme(original_scheme);
    // Let the decoded seven-second MP3 finish normally. Both EOS and a
    // subsequent Previous must follow the new occurrence and retire old LRC.
    this.toggle_music();
    let plain_for = |id: &str| {
        this.model
            .borrow()
            .queue
            .current()
            .is_some_and(|track| track.id == id)
            && view.inner.lyrics.borrow().as_ref().is_some_and(|lyrics| {
                lyrics.synchronized.is_empty() && lyrics.plain.contains(&format!("text {id}"))
            })
            && view.inner.active.get().is_none()
    };
    pump_until(|| plain_for("44"));
    this.skip_music(false, false);
    pump_until(|| plain_for("46"));
    assert_eq!(fixture.counts.lock().unwrap().external, 1);
    ui.close_for_smoke()(true);
}

// A confirmed paused seek materializes the grouped winner. Start at its
// actual centered paint, then follow its adjacent row during ordinary Resume.
fn verify_follow(samples: &[Frame], initial_scroll: f64, motion: bool) {
    let baseline = &samples[0];
    assert_eq!(baseline.active, Some(11));
    assert!(baseline.scroll - initial_scroll > 20.0 && !baseline.playing);
    assert!(baseline.center_error.is_some_and(|error| error <= 2.0));
    let changed = samples.iter().position(|f| f.active == Some(12)).unwrap();
    assert!(samples[changed..].iter().all(|f| f.playing));
    assert!(
        samples
            .last()
            .unwrap()
            .center_error
            .is_some_and(|error| error <= 2.0),
        "the adjacent line should finish centered"
    );
    let passage = &samples[changed - 1..];
    let start = passage.first().unwrap().scroll;
    let end = passage.last().unwrap().scroll;
    assert!(
        end - start > 20.0,
        "ordinary Follow must move: {start} → {end}"
    );
    if motion {
        let mut positions = Vec::new();
        for frame in passage {
            if frame.scroll > start + 1.0
                && frame.scroll < end - 1.0
                && positions
                    .last()
                    .is_none_or(|previous: &f64| (frame.scroll - previous).abs() > 1.0)
            {
                positions.push(frame.scroll);
            }
        }
        assert!(
            positions.len() >= 3,
            "Follow jumped to the next center: {positions:?}"
        );
        assert!(
            passage.windows(2).all(|pair| {
                let delta = pair[1].scroll - pair[0].scroll;
                delta >= -1.0 && delta <= (end - start) * 0.5
            }),
            "Follow reversed or jumped over half the displacement in one frame: {:?}",
            passage
                .iter()
                .map(|f| (f.frame_time, f.scroll))
                .collect::<Vec<_>>()
        );
    }
}
