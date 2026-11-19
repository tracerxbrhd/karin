// SPDX-License-Identifier: GPL-3.0-or-later
//! Measure ink in the mapped window's painted rows in physical device pixels.

use super::*;
use gtk::gdk;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
struct Ink {
    row_px: [usize; 2],
    exposed_px: [usize; 2],
    ink_px: [usize; 2],
    first_word_px: [usize; 2],
    margins_px: [usize; 4],
    first_word_coverage: f64,
    contrast: u8,
}

fn measure(texture: &gdk::Texture, scale: i32, row_px: [usize; 2]) -> Ink {
    let (w, h) = (texture.width() as usize, texture.height() as usize);
    let mut data = vec![0; w * h * 4];
    texture.download(&mut data, w * 4);
    assert_eq!(data[3], 255, "capture includes the native pane background");
    let background = &data[..3];
    let contrast = |x, y| {
        let i = (y * w + x) * 4;
        data[i..i + 3]
            .iter()
            .zip(background)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap()
    };
    // A threshold relative to each row's own background works with light,
    // dark and high contrast. It ignores fringe antialiasing around glyphs.
    let ink = |x, y| contrast(x, y) >= 36;
    let mut span_x = vec![false; w];
    let mut span_y = vec![false; h];
    let mut peak = 0;
    for (y, row) in span_y.iter_mut().enumerate() {
        for (x, col) in span_x.iter_mut().enumerate() {
            if ink(x, y) {
                *row = true;
                *col = true;
                peak = peak.max(contrast(x, y));
            }
        }
    }
    let range = |v: &[bool]| {
        (
            v.iter().position(|&p| p).expect("visible lyric glyphs"),
            v.iter().rposition(|&p| p).unwrap(),
        )
    };
    let (left, right) = range(&span_x);
    let (top, bottom) = range(&span_y);
    let margins_px = [left, top, w - 1 - right, h - 1 - bottom];
    if margins_px.contains(&0)
        && let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR")
    {
        texture
            .save_to_png(std::path::Path::new(&directory).join("edge-failure.png"))
            .unwrap();
    }
    assert!(
        margins_px.iter().all(|&n| n > 0),
        "ink hits a row edge: {margins_px:?}, row {w}×{h}"
    );
    // Isolate the first painted word even when the long Unicode sentence
    // changes line breaks. Its em space and interline leading stay blank.
    let scale = scale as usize;
    // Include detached dots and accents. Only one blank scanline may
    // separate the descenders and ascenders of two wrapped baselines.
    let body = (top + 8 * scale).min(bottom + 1);
    let first_end = span_y[body..=bottom]
        .iter()
        .position(|&p| !p)
        .map_or(bottom + 1, |delta| body + delta);
    let first_x: Vec<_> = (0..w)
        .map(|x| (top..first_end).any(|y| ink(x, y)))
        .collect();
    let (word_start, line_right) = range(&first_x);
    let word_end = first_x[word_start..]
        .windows(8 * scale)
        .position(|blank| blank.iter().all(|&p| !p))
        .map_or(line_right + 1, |delta| word_start + delta);
    let word_y: Vec<_> = (top..first_end)
        .map(|y| (word_start..word_end).any(|x| ink(x, y)))
        .collect();
    let (word_top, word_bottom) = range(&word_y);
    let first_word_px = [word_end - word_start, word_bottom - word_top + 1];
    // Sum antialiased ink relative to the solid stroke in this row. This
    // normalizes away opacity, and the bounding area normalizes away scale.
    let painted: u64 = (top + word_top..=top + word_bottom)
        .map(|y| {
            (word_start..word_end)
                .map(|x| u64::from(contrast(x, y)))
                .sum::<u64>()
        })
        .sum();
    Ink {
        row_px,
        exposed_px: [w, h],
        ink_px: [right - left + 1, bottom - top + 1],
        first_word_px,
        margins_px,
        first_word_coverage: painted as f64
            / f64::from(peak)
            / (first_word_px[0] * first_word_px[1]) as f64,
        contrast: peak,
    }
}

pub(super) fn verify_transition(
    window: &adw::ApplicationWindow,
    frames: &[&Frame],
    indices: [usize; 2],
    [panel, viewport]: [graphene::Rect; 2],
    motion: bool,
    name: &str,
    theme: &str,
) {
    assert!(frames.len() >= 3, "natural Playing frames across {name}");
    let scale = window.scale_factor();
    let renderer = window.renderer().unwrap();
    let crop = |frame: &Frame, rect: graphene::Rect| {
        let snapshot = gtk::Snapshot::new();
        let scale = scale as f32;
        snapshot.scale(scale, scale);
        snapshot.append_node(&frame.node);
        let crop = graphene::Rect::new(
            rect.x() * scale,
            rect.y() * scale,
            rect.width() * scale,
            rect.height() * scale,
        );
        renderer.render_texture(snapshot.to_node().unwrap(), Some(&crop))
    };

    let separation = frames[0].rows[indices[1]].y() - frames[0].rows[indices[0]].y();
    let ink: Vec<[Ink; 2]> = frames
        .iter()
        .map(|frame| {
            let [a, b] = indices.map(|i| frame.rows[i]);
            assert!(
                (b.y() - a.y() - separation).abs() <= 1.,
                "Follow or scaling shifted rows relative to one another"
            );
            assert!(
                a.y() + a.height() <= b.y(),
                "overlapping lyric row allocations"
            );
            [a, b].map(|rect| {
                assert!(
                    rect.y() >= viewport.y() - 0.5
                        && rect.y() + rect.height() <= viewport.y() + viewport.height() + 0.5,
                    "{name} row {rect:?} clips at viewport {viewport:?}"
                );
                let exposed = rect.intersection(&viewport).unwrap();
                let row_px =
                    [rect.width(), rect.height()].map(|n| (n * scale as f32).round() as usize);
                measure(&crop(frame, exposed), scale, row_px)
            })
        })
        .collect();
    let before = ink.first().unwrap();
    let after = ink.last().unwrap();
    for col in 0..2 {
        let (small, large) = if col == 0 {
            (&after[col], &before[col])
        } else {
            (&before[col], &after[col])
        };
        assert!(
            ink.iter().all(|pair| pair[col].row_px == small.row_px),
            "{name} row reflowed"
        );
        for axis in 0..2 {
            assert!(
                large.first_word_px[axis] as f64 >= small.first_word_px[axis] as f64 * 1.15,
                "{name} requires larger painted text on both axes: {small:?} → {large:?}"
            );
        }
        assert!(
            large.contrast >= small.contrast,
            "emphasis lost text contrast"
        );
        if !theme.starts_with("hc-") {
            assert!(large.contrast > small.contrast + 10, "flat text contrast");
        }
    }
    if name == "adjacent" {
        // Use the same two complete single words in both states. Averaging
        // limits integer hinting noise, including under High Contrast where
        // both painted endpoints have identical foreground/background colors.
        let active = (before[0].first_word_coverage + after[1].first_word_coverage) / 2.0;
        let resting = (after[0].first_word_coverage + before[1].first_word_coverage) / 2.0;
        assert!(
            active > resting + 0.02,
            "stroke coverage should grow: {resting:.3} → {active:.3}"
        );
        if motion {
            let intermediate_heights = |i: usize| {
                let bounds = [before[i].first_word_px[1], after[i].first_word_px[1]];
                ink.iter()
                    .map(|p| p[i].first_word_px[1])
                    .filter(|height| !bounds.contains(height))
                    .collect::<std::collections::BTreeSet<_>>()
            };
            for i in 0..2 {
                assert!(
                    intermediate_heights(i).len() >= 2,
                    "missing intermediate heights in row {i}: {:?}",
                    ink.iter()
                        .map(|pair| pair[i].first_word_px)
                        .collect::<Vec<_>>()
                );
            }
        }
    }
    if name == "simultaneous" {
        for sample in [before, after] {
            assert!(
                sample[1].ink_px[1] > 2 * sample[1].first_word_px[1],
                "long Unicode line did not wrap"
            );
        }
    }

    // Derive progress only from the widths of the two painted first words.
    // A color-only transition at a fixed font cannot satisfy the scale gate.
    let progress: Vec<_> = ink
        .iter()
        .map(|pair| {
            [0, 1].map(|i| {
                let width = |word: &Ink| word.first_word_px[0] as f64;
                (width(&pair[i]) - width(&before[i])) / (width(&after[i]) - width(&before[i]))
            })
        })
        .collect();
    if motion && !theme.starts_with("hc-") {
        assert!(
            ink.iter().zip(&progress).any(|(pair, fractions)| {
                (0..2).all(|i| {
                    let a = before[i].contrast;
                    let b = after[i].contrast;
                    (0.0..1.0).contains(&fractions[i])
                        && fractions[i] > 0.0
                        && (a.min(b) + 1..a.max(b)).contains(&pair[i].contrast)
                })
            }),
            "both growing and shrinking glyphs must also paint intermediate contrast"
        );
    }
    let last = frames.len() - 1;
    let mut phases = vec![("before", 0)];
    let start_us = frames.iter().find(|f| !f.settled).map(|f| f.frame_time);
    let duration_us = f64::from(LINE_TRANSITION_MS) * 1000.0;
    let time_fraction =
        |i: usize| start_us.map(|start| (frames[i].frame_time - start) as f64 / duration_us);
    if motion {
        assert!(
            start_us.is_some(),
            "at least one native GTK paint during the handoff"
        );
        for pair in progress.windows(2) {
            for i in 0..2 {
                // A rasterized edge can move by one device pixel under hinting.
                let extent = before[i].first_word_px[0].abs_diff(after[i].first_word_px[0]) as f64;
                assert!(
                    pair[1][i] + (1.5 * f64::from(scale)) / extent >= pair[0][i],
                    "{name} reverses {progress:.2?}"
                );
            }
        }
        for (phase, target) in [("early", 0.08), ("25", 0.25), ("50", 0.50), ("75", 0.75)] {
            let previous = phases.last().unwrap().1;
            let (index, error) = progress
                .iter()
                .enumerate()
                .skip(previous + 1)
                .filter(|(_, p)| {
                    if phase == "early" {
                        // On a physical pixel grid the first visible edge of
                        // one word can move a frame before the other one's.
                        p.iter().all(|&v| (0.0..1.0).contains(&v)) && p.iter().any(|&v| v > 0.0)
                    } else {
                        p.iter().all(|&v| v > 0.0 && v < 1.0)
                            && p.iter().zip(progress[previous]).all(|(v, prev)| *v > prev)
                    }
                })
                .map(|(i, _)| (i, (time_fraction(i).unwrap() - target).abs()))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .expect("distinct simultaneous old and new glyph widths at each quarter");
            let t = time_fraction(index).unwrap();
            assert!(
                error <= 0.17,
                "{name} lacks a painted frame near {phase}% of the native duration: t={t:.2}, progress={progress:.2?}"
            );
            // The production curve eases both ends. Compare the *painted*
            // change in each physical word width against that curve, allowing
            // two device pixels per logical pixel for hinting and thickening.
            let expected = (1.0 - (std::f64::consts::PI * t.clamp(0.0, 1.0)).cos()) * 0.5;
            for i in 0..2 {
                let extent = before[i].first_word_px[0].abs_diff(after[i].first_word_px[0]) as f64;
                let error_px = (progress[index][i] - expected).abs() * extent;
                assert!(
                    error_px <= 2.5 * f64::from(scale) + extent * 0.10,
                    "{name} {phase}% row {i} raster differed by {error_px:.1}px at t={t:.2}; width progress={:?}",
                    progress[index]
                );
            }
            phases.push((phase, index));
        }
    } else {
        let matches = |sample: &Ink, end: &Ink| {
            sample.first_word_px == end.first_word_px && sample.contrast.abs_diff(end.contrast) <= 2
        };
        for pair in &ink {
            assert!(
                pair.iter().zip(before).all(|(p, b)| matches(p, b))
                    || pair.iter().zip(after).all(|(p, a)| matches(p, a)),
                "reduced motion painted a partial transition: {pair:?}"
            );
        }
    }
    let endpoint = frames
        .iter()
        .zip(&ink)
        .position(|(f, pair)| {
            f.settled
                && f.active == frames[last].active
                && pair
                    .iter()
                    .zip(after)
                    .all(|(p, a)| p.first_word_px == a.first_word_px && p.contrast == a.contrast)
        })
        .unwrap();
    phases.push(("after", endpoint));
    let basename = format!("v08-playing-lyrics-{name}-{theme}-motion-{motion}-scale{scale}");
    let directory = std::env::var_os("KARIN_TEST_CAPTURE_DIR").map(std::path::PathBuf::from);
    let mut evidence = Vec::new();
    for (phase, index) in phases {
        let frame = frames[index];
        if let Some(directory) = &directory {
            crop(frame, panel)
                .save_to_png(directory.join(format!("{basename}-{phase}.png")))
                .unwrap();
        }
        let [old, new] = &ink[index];
        eprintln!(
            "{basename} {phase} at {} ms: {:?} → {:?}",
            frame.position.as_millis(),
            old.first_word_px,
            new.first_word_px
        );
        evidence.push(json!({
            "phase": phase, "frame_time_us": frame.frame_time, "playhead_ms": frame.position.as_millis(),
            "old": old, "new": new, "width_progress": progress[index], "scroll": frame.scroll,
            "phase_time_fraction": time_fraction(index),
        }));
    }
    if let Some(directory) = directory {
        let surface = window.surface().unwrap();
        let evidence = json!({
            "theme": theme, "scale": scale, "motion": motion, "transition": name,
            "gtk": [gtk::major_version(),gtk::minor_version(),gtk::micro_version()],
            "adwaita": [adw::major_version(),adw::minor_version(),adw::micro_version()],
            "renderer": renderer.type_().name(), "window_logical": [surface.width(),surface.height()],
            "panel_logical": [panel.width(), panel.height()], "painted_frames": frames.len(),
            "max_frame_gap_ms": frames.windows(2).map(|p| (p[1].frame_time-p[0].frame_time)/1000).max(), "phases": evidence,
        });
        std::fs::write(
            directory.join(format!("{basename}.json")),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
    }
}
