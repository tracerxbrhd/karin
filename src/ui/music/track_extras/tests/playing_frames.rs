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
    node: gsk::RenderNode,
    scroll: f64,
    active: Option<u32>,
    settled: bool,
    rows: [graphene::Rect; 3],
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
        settle();
        let rows = [0, 1, 5].map(|index| {
            let label = view
                .inner
                .visible_lines
                .borrow()
                .get(&index)
                .and_then(glib::WeakRef::upgrade)
                .unwrap();
            assert!(label.is_mapped());
            label.parent().expect("measured lyric row")
        });
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
                if snapshot.state != PlaybackState::Playing {
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
                        node,
                        scroll: adjustment.value(),
                        active: lyrics.active.get(),
                        settled: lyrics.line_tick.borrow().is_none(),
                        rows: rows.each_ref().map(|r| r.compute_bounds(&host).unwrap()),
                    });
                }
            }
        });
        this.toggle_music();
        pump_until(|| {
            let frames = frames.borrow();
            frames.last().is_some_and(|f| {
                f.active == Some(11)
                    && f.settled
                    && f.position >= Duration::from_millis(5500)
                    && view
                        .inner
                        .scroll_animation
                        .borrow()
                        .as_ref()
                        .is_none_or(|a| a.state() != adw::AnimationState::Playing)
            })
        });
        clock.disconnect(handler);
        this.pause_music();
        pump_until(|| {
            this.music_snapshot()
                .is_some_and(|s| s.state == PlaybackState::Paused)
        });
        let samples = frames.take();
        verify_follow(&samples, motion);
        for (old, next, old_index, next_index, timestamp, name) in [
            (0, 1, 0, 1, 1400, "adjacent"),
            (1, 2, 1, 5, 3300, "simultaneous"),
        ] {
            // Locate the actual LRC handoff and its settled GTK presentation.
            // Only the scene pixels below determine the two physical scales.
            let changed = samples
                .iter()
                .position(|f| f.active == Some(next_index))
                .unwrap();
            assert!(samples[changed].position >= Duration::from_millis(timestamp));
            let before = samples[..changed]
                .iter()
                .rposition(|f| f.active == Some(old_index) && f.settled)
                .unwrap();
            let after = samples[changed..]
                .iter()
                .rposition(|f| f.active == Some(next_index) && f.settled)
                .unwrap()
                + changed;
            let frames: Vec<_> = samples[before..=after].iter().collect();
            verify_transition(
                window,
                &frames,
                [old, next],
                [panel(), viewport],
                motion,
                name,
                theme,
            );
        }
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

// The 5 s boundary advances across a timestamp group to row 11. The
// remaining stanzas make this a real, nonzero Follow movement in both widths.
fn verify_follow(samples: &[Frame], motion: bool) {
    let changed = samples.iter().position(|f| f.active == Some(11)).unwrap();
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
