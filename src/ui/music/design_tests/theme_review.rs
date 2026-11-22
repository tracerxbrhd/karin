// SPDX-License-Identifier: GPL-3.0-or-later
//! Matched native theme observations; original local catalog/art/audio only.
use super::*;
use crate::settings::{ColorScheme, UiStyle};
use gtk::gdk;

fn styles() -> Vec<(&'static str, UiStyle)> {
    ["system", "liquid-glass"]
        .into_iter()
        .filter_map(|name| {
            serde_json::from_value(json!(name))
                .ok()
                .map(|style| (name, style))
        })
        .collect()
}

fn apply(ui: &crate::ui::MainWindow, style: UiStyle, dark: bool) {
    let mut values = ui.music.controller.persistence.borrow().values.clone();
    values.ui_style = style;
    values.appearance = if dark {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    };
    ui.music.controller.persistence.borrow_mut().values = values.clone();
    super::super::super::style::apply(&values, &ui.window);
    pump(|| adw::StyleManager::default().is_dark() == dark);
}

pub(super) fn startup_style(ui: &crate::ui::MainWindow) {
    if let Ok(name) = std::env::var("KARIN_TEST_THEME_STYLE") {
        let style: UiStyle = serde_json::from_value(json!(name)).unwrap();
        apply(ui, style, false);
    }
}

fn glass(ui: &crate::ui::MainWindow) -> Option<Value> {
    let widget = descendants(ui.window.upcast_ref())
        .into_iter()
        .find(|w| w.widget_name() == "karin-liquid-glass-background")?;
    let mut stats = serde_json::Map::new();
    stats.insert("visible".into(), json!(widget.is_visible()));
    stats.insert("mapped".into(), json!(widget.is_mapped()));
    for name in [
        "preparation-count",
        "snapshot-count",
        "blur-pass-count",
        "cache-hit-count",
        "cache-entry-count",
        "cache-pixel-bytes",
    ] {
        if widget.find_property(name).is_some() {
            stats.insert(name.to_owned(), json!(widget.property::<u64>(name)));
        }
    }
    Some(Value::Object(stats))
}

fn texture(window: &adw::ApplicationWindow) -> gdk::Texture {
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let image = paintable.current_image();
    let snapshot = gtk::Snapshot::new();
    snapshot.scale(window.scale_factor() as f32, window.scale_factor() as f32);
    image.snapshot(
        &snapshot,
        image.intrinsic_width() as f64,
        image.intrinsic_height() as f64,
    );
    window
        .renderer()
        .unwrap()
        .render_texture(snapshot.to_node().unwrap(), None)
}

fn frame(window: &adw::ApplicationWindow) {
    let seen = Rc::new(Cell::new(false));
    let clock = window.frame_clock().unwrap();
    let handle = clock.connect_after_paint({
        let seen = seen.clone();
        move |_| seen.set(true)
    });
    window.queue_draw();
    pump(|| seen.get());
    clock.disconnect(handle);
}

fn luminance(rgb: [f64; 3]) -> f64 {
    let linear = rgb.map(|v| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
}

#[derive(serde::Serialize)]
struct TextContrast {
    ratio: f64,
    declared_rgba: [f32; 4],
    effective_alpha: f64,
    core_pixels: usize,
    painted_core_rgb: [u8; 3],
    painted_core_background_rgb: [u8; 3],
    worst_background_rgb: [u8; 3],
}

fn text_contrast(ui: &crate::ui::MainWindow, label: &gtk::Label) -> TextContrast {
    assert!(label.is_mapped());
    let original = label.opacity();
    let color = label.color();
    let declared_rgba = [color.red(), color.green(), color.blue(), color.alpha()];
    let foreground = [
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
    ];
    let bounds = label.compute_bounds(&ui.window).unwrap();
    // Paired native scenes preserve the allocation and all underlying layers.
    // GTK's get_opacity excludes CSS opacity, so infer effective opacity from
    // the painted stroke cores, including the CSS and ancestor composition.
    let painted = texture(&ui.window);
    let width = painted.width() as usize;
    let height = painted.height() as usize;
    let mut ink = vec![0; width * height * 4];
    painted.download(&mut ink, width * 4);
    label.set_opacity(0.0);
    frame(&ui.window);
    let background = texture(&ui.window);
    assert_eq!(
        (background.width(), background.height()),
        (width as i32, height as i32)
    );
    let mut blank = vec![0; ink.len()];
    background.download(&mut blank, width * 4);
    label.set_opacity(original);
    frame(&ui.window);
    let (dx, dy) = ui.window.surface_transform();
    let scale = ui.window.scale_factor() as f32;
    let x0 = ((bounds.x() + dx as f32) * scale).max(0.0) as usize;
    let y0 = ((bounds.y() + dy as f32) * scale).max(0.0) as usize;
    let x1 = (x0 + (bounds.width() * scale) as usize).min(width);
    let y1 = (y0 + (bounds.height() * scale) as usize).min(height);
    let rgb = |data: &[u8], x: usize, y: usize| {
        let i = (y * width + x) * 4;
        [data[i + 2], data[i + 1], data[i]]
    };
    let normalized = |rgb: [u8; 3]| rgb.map(|v| v as f64 / 255.);
    let mut samples = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let bg = normalized(rgb(&blank, x, y));
            let actual = normalized(rgb(&ink, x, y));
            let direction: [f64; 3] = std::array::from_fn(|i| foreground[i] - bg[i]);
            let denominator: f64 = direction.iter().map(|v| v * v).sum();
            if denominator < 0.04 {
                continue;
            }
            let alpha = (0..3)
                .map(|i| (actual[i] - bg[i]) * direction[i])
                .sum::<f64>()
                / denominator;
            let error = (0..3)
                .map(|i| (actual[i] - bg[i] - alpha * direction[i]).abs())
                .fold(0., f64::max);
            if alpha > 0.02 && alpha <= 1.01 && error <= 3. / 255. {
                samples.push((x, y, alpha.min(1.)));
            }
        }
    }
    assert!(
        samples.len() >= 16,
        "not enough actual glyph ink for contrast"
    );
    let mut coverage: Vec<_> = samples.iter().map(|p| p.2).collect();
    coverage.sort_by(f64::total_cmp);
    // Stroke interiors form a plateau above their antialiased fringes. A high
    // quantile avoids depending on a single noisy extreme device pixel.
    let alpha = coverage[(coverage.len() - 1) * 98 / 100];
    let stroke: std::collections::BTreeMap<_, _> = samples
        .iter()
        .map(|&(x, y, alpha)| ((x, y), alpha))
        .collect();
    let cores: std::collections::BTreeSet<_> = samples
        .iter()
        .filter(|&&(x, y, coverage)| {
            if (coverage - alpha).abs() > 0.0125 {
                return false;
            }
            // At 1x a small time glyph has isolated fully covered pixels;
            // require repeated cores inside connected painted strokes rather
            // than an impossible run of three equal-coverage device pixels.
            [
                (x + 1, y),
                (x.saturating_sub(1), y),
                (x, y + 1),
                (x, y.saturating_sub(1)),
            ]
            .into_iter()
            .filter(|p| stroke.get(p).is_some_and(|v| *v >= alpha * 0.5))
            .count()
                >= 2
        })
        .map(|p| (p.0, p.1))
        .collect();
    if cores.len() < 4 {
        write_json(
            "theme-core-failure",
            &json!({"declared_rgba":declared_rgba,"bounds":[x0,y0,x1,y1],"samples":samples.len(),"effective_alpha":alpha,"core_count":cores.len(),"core_coordinates":cores}),
        );
        if let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") {
            painted
                .save_to_png(std::path::Path::new(&directory).join("theme-core-ink.png"))
                .unwrap();
            background
                .save_to_png(std::path::Path::new(&directory).join("theme-core-background.png"))
                .unwrap();
        }
    }
    assert!(
        cores.len() >= 4,
        "no repeated connected stroke cores for contrast"
    );
    let contrast = |a: [f64; 3], b: [f64; 3]| {
        let a = luminance(a);
        let b = luminance(b);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    };
    let mut worst = f64::INFINITY;
    let mut worst_background_rgb = [0; 3];
    for y in y0..y1 {
        for x in x0..x1 {
            let bg_rgb = rgb(&blank, x, y);
            let bg = normalized(bg_rgb);
            let fg = std::array::from_fn(|i| foreground[i] * alpha + bg[i] * (1. - alpha));
            let ratio = contrast(fg, bg);
            if ratio < worst {
                worst = ratio;
                worst_background_rgb = bg_rgb;
            }
        }
    }
    let &(core_x, core_y) = cores
        .iter()
        .min_by(|&&(ax, ay), &&(bx, by)| {
            contrast(
                normalized(rgb(&ink, ax, ay)),
                normalized(rgb(&blank, ax, ay)),
            )
            .total_cmp(&contrast(
                normalized(rgb(&ink, bx, by)),
                normalized(rgb(&blank, bx, by)),
            ))
        })
        .unwrap();
    TextContrast {
        ratio: worst.min(contrast(
            normalized(rgb(&ink, core_x, core_y)),
            normalized(rgb(&blank, core_x, core_y)),
        )),
        declared_rgba,
        effective_alpha: alpha,
        core_pixels: cores.len(),
        painted_core_rgb: rgb(&ink, core_x, core_y),
        painted_core_background_rgb: rgb(&blank, core_x, core_y),
        worst_background_rgb,
    }
}

fn readable_samples(ui: &crate::ui::MainWindow, name: &str) -> Vec<Value> {
    let mut labels = vec![("footer-title", ui.music.controller.widgets.current.clone())];
    if ui.music.controller.widgets.position.is_mapped() {
        labels.push((
            "playback-time",
            ui.music.controller.widgets.position.clone(),
        ));
    }

    for (kind, class) in [
        ("home-description", "music-wave-subtitle"),
        ("visible-track-title", "music-track-title"),
        ("visible-track-artist", "music-track-artist"),
    ] {
        let found = descendants(ui.window.upcast_ref())
            .into_iter()
            .filter(|w| w.has_css_class(class) && w.is_mapped() && w.width() > 0 && w.height() > 0)
            .filter_map(|w| w.downcast::<gtk::Label>().ok())
            .find(|label| {
                let Some(b) = label.compute_bounds(&ui.window) else {
                    return false;
                };
                if b.x() < 0.
                    || b.y() < 0.
                    || b.x() + b.width() > ui.window.width() as f32
                    || b.y() + b.height() > ui.window.height() as f32
                {
                    return false;
                }
                if let Some(scroll) = label.ancestor(gtk::ScrolledWindow::static_type()) {
                    let Some(clip) = scroll.compute_bounds(&ui.window) else {
                        return false;
                    };
                    if b.y() < clip.y() || b.y() + b.height() > clip.y() + clip.height() {
                        return false;
                    }
                }
                if kind == "home-description" {
                    return true;
                }
                let row = label.ancestor(gtk::ListBoxRow::static_type());
                let picked = ui.window.pick(
                    f64::from(b.x() + b.width() / 2.),
                    f64::from(b.y() + b.height() / 2.),
                    gtk::PickFlags::DEFAULT,
                );
                picked.is_some_and(|picked| {
                    let actual = if picked.is::<gtk::ListBoxRow>() {
                        Some(picked)
                    } else {
                        picked.ancestor(gtk::ListBoxRow::static_type())
                    };
                    row.is_some() && actual == row
                })
            });
        if let Some(label) = found {
            labels.push((kind, label));
        }
    }
    let measured: Vec<_> = labels
        .into_iter()
        .map(|(kind, label)| {
            let measured = text_contrast(ui, &label);
            json!({"kind":kind,"contrast":measured.ratio,"painted":measured})
        })
        .collect();
    let failures: Vec<_> = measured
        .iter()
        .filter(|sample| sample["contrast"].as_f64().unwrap() < 4.5)
        .collect();
    if !failures.is_empty() {
        write_json(
            "theme-readability-failure",
            &json!({"capture":name,"samples":measured,"failed":failures}),
        );
    }
    assert!(
        failures.is_empty(),
        "readable text contrast below 4.5 at {name}: {failures:?}"
    );
    measured
}

fn save(ui: &crate::ui::MainWindow, name: &str) {
    frame(&ui.window);
    if let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        texture(&ui.window)
            .save_to_png(std::path::Path::new(&directory).join(format!("{name}.png")))
            .unwrap();
    }
}

fn write_json(name: &str, value: &Value) {
    eprintln!("{name}: {value}");
    if let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{name}.json")),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "isolated Xvfb/Openbox populated theme captures; run at 1x and 2x"]
fn native_theme_matched_visual_surfaces() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let _manager = WindowManager::start();
    let (fixture, _app, ui, liked, _album) = populated_window(false);
    let this = &ui.music.controller;
    activate_design_player_observed(&ui, &fixture, false);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    let playlist = this
        .model
        .borrow()
        .playlists
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .items[0]
        .clone();
    let mut records = Vec::new();
    for (style_name, style) in styles() {
        for dark in [false, true] {
            apply(&ui, style, dark);
            for (width, height, shape) in [
                (420, 720, "compact"),
                (900, 650, "medium"),
                (1440, 900, "wide"),
                (1920, 1080, "large"),
                (0, 0, "maximized"),
            ] {
                set_panel(&ui, workspace::PanelMode::Closed);
                if width == 0 {
                    ui.window.maximize();
                    pump(|| ui.window.is_maximized());
                } else {
                    if ui.window.is_maximized() {
                        ui.window.unmaximize();
                        pump(|| !ui.window.is_maximized());
                    }
                    resize(&ui, width, height);
                }
                for state in [
                    "home", "liked", "playlist", "info", "lyrics", "queue", "player",
                ] {
                    set_panel(&ui, workspace::PanelMode::Closed);
                    match state {
                        "home" => primary_destination(&ui, "home"),
                        "playlist" => {
                            this.open_playlist(playlist.clone(), 0);
                            pump(|| {
                                this.widgets.stack.visible_child_name().as_deref() == Some("detail")
                                    && !this.model.borrow().busy
                            });
                        }
                        _ => primary_destination(&ui, "library"),
                    }
                    match state {
                        "info" => set_panel(&ui, workspace::PanelMode::TrackInfo),
                        "lyrics" => {
                            set_panel(&ui, workspace::PanelMode::Lyrics);
                            pump(|| {
                                tests::contains_text(
                                    this.track_pane.extras.lyrics_widget.upcast_ref(),
                                    "Original lyric verse 1",
                                )
                            });
                        }
                        "queue" => set_panel(&ui, workspace::PanelMode::Queue),
                        _ => {}
                    }
                    assert_footer_geometry(&ui);
                    let name = format!(
                        "theme-{style_name}-{}-{shape}-{state}-scale{}",
                        if dark { "dark" } else { "light" },
                        ui.window.scale_factor()
                    );
                    save(&ui, &name);
                    let primary = text_contrast(&ui, &this.widgets.current);
                    let contrast = primary.ratio;
                    let readability = readable_samples(&ui, &name);
                    let high_contrast = adw::StyleManager::default().is_high_contrast();
                    if let Some(background) = glass(&ui) {
                        let visible = background["visible"].as_bool().unwrap();
                        assert_eq!(
                            visible,
                            style_name == "liquid-glass" && !high_contrast,
                            "background visibility at {name}"
                        );
                        if high_contrast {
                            assert_eq!(
                                background["preparation-count"],
                                json!(0),
                                "HC background prepared at {name}"
                            );
                        } else if style_name == "liquid-glass" {
                            assert!(
                                background["blur-pass-count"].as_u64().unwrap() > 0,
                                "glass has no prepared blur at {name}"
                            );
                        }
                    }
                    assert!(
                        contrast >= 4.5,
                        "primary text contrast at {name}: {contrast:.2}"
                    );
                    records.push(json!({"capture":name,"style":style_name,"dark":dark,"shape":shape,"state":state,"logical_size":[ui.window.width(),ui.window.height()],"scale":ui.window.scale_factor(),"readable_text_samples":readability,"primary_text_worst_contrast":contrast,"primary_painted":primary,"high_contrast":adw::StyleManager::default().is_high_contrast(),"reduced_motion":!gtk::Settings::default().unwrap().is_gtk_enable_animations(),"glass":glass(&ui)}));
                }
            }
        }
    }
    assert_eq!(
        this.music_snapshot().unwrap().state,
        crate::playback::PlaybackState::Paused
    );
    assert_eq!(
        this.model.borrow().queue.current().unwrap().id,
        liked.items[0].id
    );
    write_json(
        "theme-visual-matrix",
        &json!({"scope":"native original fixtures, no live account or physical GPU claim","cases":records}),
    );
    let closed = ui.close_for_smoke();
    pump(|| closed(false));
    assert!(closed(true));
}

fn cpu_ticks() -> u64 {
    let value = std::fs::read_to_string("/proc/self/stat").unwrap();
    let fields: Vec<_> = value
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
}

fn observe(
    ui: &crate::ui::MainWindow,
    name: &str,
    action: impl FnOnce(),
    done: impl Fn() -> bool,
) -> Value {
    let frames = Rc::new(RefCell::new(Vec::new()));
    let clock = ui.window.frame_clock().unwrap();
    let handle = clock.connect_after_paint({
        let frames = frames.clone();
        move |clock| frames.borrow_mut().push(clock.frame_time())
    });
    let old_glass = glass(ui);
    let started = Instant::now();
    let cpu = cpu_ticks();
    action();
    let action_return = started.elapsed().as_micros();
    pump(done);
    let wall = started.elapsed().as_micros();
    let cpu = cpu_ticks() - cpu;
    clock.disconnect(handle);
    let new_glass = glass(ui);
    if name != "scheme-light-to-dark"
        && let (Some(before), Some(after)) = (&old_glass, &new_glass)
    {
        assert_eq!(
            before["preparation-count"], after["preparation-count"],
            "background re-prepared during {name}"
        );
        assert_eq!(
            before["blur-pass-count"], after["blur-pass-count"],
            "background reblurred during {name}"
        );
    }
    let times = frames.borrow();
    let gaps: Vec<_> = times.windows(2).map(|p| p[1] - p[0]).collect();
    json!({"case":name,"observation_window_us":wall,"action_plus_layout_return_us":action_return,"process_cpu_ticks":cpu,"ticks_per_second":100,"painted_frames":times.len(),"frame_gap_us":gaps,"glass_before":old_glass,"glass_after":new_glass})
}

#[test]
#[ignore = "fresh-process bounded theme work observation; ordinary frame damage and original media"]
fn native_theme_matched_work_observation() {
    let started = Instant::now();
    let cpu = cpu_ticks();
    gtk::init().unwrap();
    adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(true);
    let _manager = WindowManager::start();
    let (fixture, _app, ui, liked, _album) = populated_window_with_initial_selection(false, false);
    let first_window = json!({"case":"mapped-library-ready","wall_us":started.elapsed().as_micros(),"process_cpu_ticks":cpu_ticks()-cpu,"scope":"local fixture/server/audio construction plus first mapped loaded library, no prerender"});
    let this = &ui.music.controller;
    let requested = std::env::var("KARIN_TEST_THEME_STYLE").unwrap_or_else(|_| "system".into());
    let style: UiStyle = serde_json::from_value(json!(requested)).unwrap();
    apply(&ui, style, false);
    let mut records = vec![first_window];
    let started = Instant::now();
    let cpu = cpu_ticks();
    let first = this.widgets.library_rows.row_at_index(0).unwrap();
    observe_footer_transition_with_baseline(&ui, "theme-first-start", true, false, false, || {
        native_click(&ui, &first)
    });
    pump(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == crate::playback::PlaybackState::Playing)
    });
    records.push(json!({"case":"first-footer","wall_us":started.elapsed().as_micros(),"process_cpu_ticks":cpu_ticks()-cpu,"scope":"raw original first footer observer"}));
    this.pause_music();
    pump(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == crate::playback::PlaybackState::Paused)
    });
    primary_destination(&ui, "library");
    // A genuine long native row list uses the same factory/context bindings.
    let mut long = liked.items.clone();
    for i in 8..200 {
        let mut track = liked.items[i % 8].clone();
        track.id = format!("{}", 7000 + i);
        long.push(track);
    }
    this.render_liked(
        Ok(Page {
            items: long,
            has_more: false,
        }),
        0,
    );
    frame(&ui.window);
    let scrolled = this
        .widgets
        .library_rows
        .ancestor(gtk::ScrolledWindow::static_type())
        .unwrap()
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
    let adjustment = scrolled.vadjustment();
    let scroll_start = Instant::now();
    records.push(observe(
        &ui,
        "long-library-scroll",
        || {},
        || {
            let elapsed = scroll_start.elapsed().as_secs_f64();
            adjustment
                .set_value((adjustment.upper() - adjustment.page_size()) * (elapsed / 1.0).min(1.));
            elapsed >= 1.0
        },
    ));
    for (mode, name) in [
        (workspace::PanelMode::TrackInfo, "info-open"),
        (workspace::PanelMode::Lyrics, "info-to-lyrics"),
        (workspace::PanelMode::Queue, "lyrics-to-queue"),
        (workspace::PanelMode::TrackInfo, "queue-to-info"),
        (workspace::PanelMode::Closed, "info-close"),
    ] {
        let started = Instant::now();
        records.push(observe(
            &ui,
            name,
            || set_panel(&ui, mode),
            || started.elapsed() >= Duration::from_millis(400),
        ));
    }
    let started = Instant::now();
    records.push(observe(
        &ui,
        "global-search-open",
        || this.open_global_search(),
        || started.elapsed() >= Duration::from_millis(350),
    ));
    this.close_global_search(true);
    let serial = this.music_serial();
    let current_id = this.model.borrow().queue.current().unwrap().id.clone();
    records.push(observe(
        &ui,
        "next-track",
        || this.skip_music(true, false),
        || {
            this.music_serial() > serial
                && this
                    .model
                    .borrow()
                    .queue
                    .current()
                    .is_some_and(|track| track.id != current_id)
                && this.music_snapshot().is_some_and(|s| {
                    matches!(
                        s.state,
                        crate::playback::PlaybackState::Paused
                            | crate::playback::PlaybackState::Ready
                    )
                })
        },
    ));
    let started = Instant::now();
    records.push(observe(
        &ui,
        "scheme-light-to-dark",
        || apply(&ui, style, true),
        || started.elapsed() >= Duration::from_millis(350),
    ));
    write_json(
        "theme-work-observation",
        &json!({"style":requested,"renderer":ui.window.renderer().unwrap().type_().name(),"scale":ui.window.scale_factor(),"cases":records,"physical_gpu_performance":false}),
    );
    assert!(fixture.media_requests.load(Ordering::Acquire) > 0);
    drop(first);
    drop(adjustment);
    drop(scrolled);
    let closed = ui.close_for_smoke();
    pump(|| closed(false));
    assert!(closed(true));
}
