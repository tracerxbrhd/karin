// SPDX-License-Identifier: GPL-3.0-or-later
//! Inspect the ordinary mapped application's rendered text during real playback.

use super::*;
use gtk::{gdk, graphene, gsk};

struct Frame {
    frame_time: i64,
    position: Duration,
    node: gsk::RenderNode,
}

pub(super) fn ordinary_lyrics_playback(app: &adw::Application) {
    let fixture = Fixture::new();
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
    let originally_dark = style.is_dark();
    for motion in [true, false] {
        if !motion {
            settings.set_gtk_enable_animations(false);
            style.set_color_scheme(if originally_dark {
                adw::ColorScheme::ForceLight
            } else {
                adw::ColorScheme::ForceDark
            });
            pump_until(|| style.is_dark() != originally_dark);
        }
        let serial = this.music_snapshot().unwrap().seek_serial;
        this.seek_music(Duration::from_millis(160));
        pump_until(|| {
            this.music_snapshot()
                .is_some_and(|s| s.seek_serial > serial)
                && view.inner.active.get() == Some(0)
        });
        settle();
        let region = |row| {
            let label = view
                .inner
                .visible_lines
                .borrow()
                .get(&row)
                .and_then(glib::WeakRef::upgrade)
                .unwrap();
            assert!(label.is_mapped());
            let bounds = label.compute_bounds(window).unwrap();
            let accent = accent_layer(&label);
            assert_eq!(
                accent.text(),
                label.text(),
                "accent must paint the same bound line"
            );
            assert_eq!(
                accent.compute_bounds(window),
                Some(bounds),
                "accent must fit its own measured row"
            );
            let viewport = view.inner.lyrics_scroll.compute_bounds(window).unwrap();
            assert!(
                bounds.y() >= viewport.y()
                    && bounds.y() + bounds.height() <= viewport.y() + viewport.height()
            );
            bounds
        };
        let regions = [region(0), region(1), region(5)];
        // Sampling the complete application's painted GSK nodes avoids styling
        // labels in isolation. Download only the two row regions after playback.
        let frames = Rc::new(RefCell::new(Vec::new()));
        let clock = window.frame_clock().unwrap();
        let handler = clock.connect_after_paint({
            let frames = frames.clone();
            let paintable = gtk::WidgetPaintable::new(Some(window));
            let controller = Rc::downgrade(this);
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
                // Observe one paintable across the run. Attaching a fresh
                // paintable to the full window every frame needlessly
                // invalidates GTK's observer and snapshot caches under HiDPI.
                let image = paintable.current_image();
                let scene = gtk::Snapshot::new();
                image.snapshot(
                    &scene,
                    f64::from(image.intrinsic_width()),
                    f64::from(image.intrinsic_height()),
                );
                if let Some(node) = scene.to_node() {
                    frames.borrow_mut().push(Frame {
                        frame_time: clock.frame_time(),
                        position,
                        node,
                    });
                }
            }
        });
        this.toggle_music();
        pump_until(|| {
            this.music_snapshot().is_some_and(|s| {
                s.state == PlaybackState::Playing
                    && s.position.is_some_and(|p| p >= Duration::from_millis(2950))
            }) && view.inner.active.get() == Some(5)
        });
        clock.disconnect(handler);
        this.pause_music();
        pump_until(|| {
            this.music_snapshot()
                .is_some_and(|s| s.state == PlaybackState::Paused)
        });
        assert_eq!(
            regions,
            [region(0), region(1), region(5)],
            "row capture geometry moved"
        );
        let samples = frames.take();
        for (old, next, milliseconds, name) in
            [(0, 1, 1000, "adjacent"), (1, 2, 2200, "simultaneous")]
        {
            let transition = Duration::from_millis(milliseconds);
            let start = transition - Duration::from_millis(350);
            let end = transition + Duration::from_millis(600);
            let frames: Vec<_> = samples
                .iter()
                .filter(|f| (start..=end).contains(&f.position))
                .collect();
            verify_transition(window, &frames, [regions[old], regions[next]], motion, name);
        }
    }
    settings.set_gtk_enable_animations(true);
    style.set_color_scheme(original_scheme);
    // Let the decoded four-second MP3 finish normally. Both EOS and a
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

fn verify_transition(
    window: &adw::ApplicationWindow,
    frames: &[&Frame],
    rows: [graphene::Rect; 2],
    motion: bool,
    name: &str,
) {
    assert!(
        frames.len() >= 3,
        "capture natural Playing frames across {name}"
    );
    let scale = window.scale_factor();
    let renderer = window.renderer().unwrap();
    let crop = |frame: &Frame, rect: graphene::Rect| {
        let snapshot = gtk::Snapshot::new();
        snapshot.scale(scale as f32, scale as f32);
        snapshot.append_node(&frame.node);
        let scale = scale as f32;
        let crop = graphene::Rect::new(
            rect.x() * scale,
            rect.y() * scale,
            rect.width() * scale,
            rect.height() * scale,
        );
        renderer.render_texture(snapshot.to_node().unwrap(), Some(&crop))
    };

    let save_frame = |index: usize, phase: &str| {
        if let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") {
            let filename =
                format!("v08-playing-lyrics-{name}-motion-{motion}-{phase}-scale{scale}.png");
            crop(frames[index], rows[0].union(&rows[1]))
                .save_to_png(std::path::Path::new(&directory).join(filename))
                .unwrap();
        }
    };
    save_frame(0, "before");
    save_frame(frames.len() - 1, "after");
    let mut progress = Vec::new();
    for rect in rows {
        // Exclude the leading marker. Both independent comparisons must detect
        // intermediate color in actual glyphs, even when Follow holds still.
        let text = graphene::Rect::new(
            rect.x() + 20.,
            rect.y(),
            (rect.width() - 24.).min(230.),
            rect.height(),
        );
        let pixels = |texture: gdk::Texture| {
            let stride = texture.width() as usize * 4;
            let mut data = vec![0; stride * texture.height() as usize];
            texture.download(&mut data, stride);
            data
        };
        let before = pixels(crop(frames.first().unwrap(), text));
        let after = pixels(crop(frames.last().unwrap(), text));
        let mask = before
            .iter()
            .zip(&after)
            .enumerate()
            .filter_map(|(i, (&a, &b))| {
                // The first pixel is the row's empty top padding. Require ink
                // at both endpoints to exclude the fading background tint.
                let channel = i % 4;
                (a.abs_diff(b) >= 20
                    && a.abs_diff(before[channel]) >= 45
                    && b.abs_diff(after[channel]) >= 45)
                    .then_some(i)
            })
            .collect::<Vec<_>>();
        assert!(
            mask.len() >= 100,
            "{name}: only {} glyph channels changed visibly between endpoints (row {:?}, extent {:?}, maximum delta {})",
            mask.len(),
            rect,
            text,
            before
                .iter()
                .zip(&after)
                .map(|(&a, &b)| a.abs_diff(b))
                .max()
                .unwrap()
        );
        let denominator: f64 = mask
            .iter()
            .map(|&i| (f64::from(after[i]) - f64::from(before[i])).powi(2))
            .sum();
        progress.push(
            frames
                .iter()
                .map(|frame| {
                    let sample = pixels(crop(frame, text));
                    mask.iter()
                        .map(|&i| {
                            (f64::from(sample[i]) - f64::from(before[i]))
                                * (f64::from(after[i]) - f64::from(before[i]))
                        })
                        .sum::<f64>()
                        / denominator
                })
                .collect::<Vec<_>>(),
        );
    }
    for fractions in &progress {
        assert!(
            fractions.windows(2).all(|pair| pair[1] + 0.025 >= pair[0]),
            "{name}: a settled row flashed back toward its original emphasis"
        );
    }
    let mut intermediate = Vec::new();
    for (i, (&old, &new)) in progress[0].iter().zip(&progress[1]).enumerate() {
        if motion
            && (0.02..0.98).contains(&old)
            && (0.02..0.98).contains(&new)
            && intermediate
                .last()
                .is_none_or(|&prev: &usize| (new - progress[1][prev]).abs() > 0.02)
        {
            intermediate.push(i);
        }
        if !motion {
            assert!(
                !(0.02..0.98).contains(&old) && !(0.02..0.98).contains(&new),
                "Reduced Motion interpolated {name} text at frame {i}: {old:.2}, {new:.2}"
            );
        }
    }
    let longest_gap_us = frames
        .windows(2)
        .map(|p| p[1].frame_time - p[0].frame_time)
        .max()
        .unwrap_or_default();
    eprintln!(
        "Karin ordinary lyric playback: {name} motion={motion} captured={} distinct-intermediate={} max-frame-gap-us={longest_gap_us}",
        frames.len(),
        intermediate.len()
    );
    if motion {
        assert!(
            intermediate.len() >= 2,
            "{name}: both rendered glyph regions must interpolate over multiple distinct frames: {progress:.2?}"
        );
    }
    for (n, &index) in intermediate.iter().enumerate() {
        save_frame(index, &format!("during-{n:02}"));
    }
}
