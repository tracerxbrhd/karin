// SPDX-License-Identifier: GPL-3.0-or-later
//! Focused System contrast and footer regressions using original local fixtures.
use super::*;
use crate::settings::{ColorScheme, UiStyle};

fn apply(ui: &crate::ui::MainWindow, dark: bool) {
    let mut values = ui.music.controller.persistence.borrow().values.clone();
    values.ui_style = UiStyle::System;
    values.appearance = if dark {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    };
    ui.music.controller.persistence.borrow_mut().values = values.clone();
    super::super::super::style::apply(&values, &ui.window);
    pump(|| adw::StyleManager::default().is_dark() == dark);
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
            "system-core-failure",
            &json!({"declared_rgba":declared_rgba,"bounds":[x0,y0,x1,y1],"samples":samples.len(),"effective_alpha":alpha,"core_count":cores.len(),"core_coordinates":cores}),
        );
        if let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") {
            painted
                .save_to_png(std::path::Path::new(&directory).join("system-core-ink.png"))
                .unwrap();
            background
                .save_to_png(std::path::Path::new(&directory).join("system-core-background.png"))
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
            "system-readability-failure",
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

fn pixel_at(
    window: &adw::ApplicationWindow,
    image: &gdk::Texture,
    pixels: &[u8],
    x: f32,
    y: f32,
) -> [u8; 4] {
    let (dx, dy) = window.surface_transform();
    let scale = window.scale_factor() as f32;
    let x = ((x + dx as f32) * scale).floor() as i32;
    let y = ((y + dy as f32) * scale).floor() as i32;
    assert!(x >= 0 && x < image.width() && y >= 0 && y < image.height());
    let offset = (y as usize * image.width() as usize + x as usize) * 4;
    // GdkTexture::download uses native-endian premultiplied ARGB32.
    [
        pixels[offset + 2],
        pixels[offset + 1],
        pixels[offset],
        pixels[offset + 3],
    ]
}

fn downloaded(image: &gdk::Texture) -> Vec<u8> {
    let stride = image.width() as usize * 4;
    let mut pixels = vec![0; stride * image.height() as usize];
    image.download(&mut pixels, stride);
    pixels
}

fn color_distance(left: [u8; 4], right: [u8; 4]) -> u8 {
    left.into_iter()
        .zip(right)
        .map(|(left, right)| left.abs_diff(right))
        .max()
        .unwrap()
}

#[allow(deprecated)] // GTK 4.14's native named palette has no replacement query API.
fn assert_header_surface(ui: &crate::ui::MainWindow) -> Value {
    let native = ui
        .window
        .style_context()
        .lookup_color("view_bg_color")
        .unwrap();
    assert!(native.alpha() >= 0.99);
    let expected = [native.red(), native.green(), native.blue(), native.alpha()]
        .map(|component| (component * 255.0).round() as u8);
    let bounds = ui.header.compute_bounds(&ui.window).unwrap();
    let image = texture(&ui.window);
    let pixels = downloaded(&image);
    // Start title buttons are disabled. These are three blank parts of the
    // actual mapped header, clear of its centered title and trailing controls.
    let samples = [24.0, 32.0, 40.0].map(|x| {
        pixel_at(
            &ui.window,
            &image,
            &pixels,
            bounds.x() + x,
            bounds.y() + bounds.height() / 2.0,
        )
    });
    for painted in samples {
        assert!(
            color_distance(painted, expected) <= 2,
            "the native header must share the content's surface, not paint a contrast strip: {painted:?} != {expected:?}"
        );
    }
    json!({"native_view_rgba":expected,"painted_header_rgba":samples})
}

fn assert_panel_corners(ui: &crate::ui::MainWindow, high_contrast: bool) -> Option<Value> {
    let this = &ui.music.controller;
    if this.panel_mode.get() == workspace::PanelMode::Closed {
        return None;
    }
    let panel = this
        .info_host
        .borrow()
        .as_ref()
        .unwrap()
        .revealer()
        .unwrap();
    assert!(panel.is_child_revealed());
    let surface = panel.surface().unwrap();
    let bounds = surface.compute_bounds(&ui.window).unwrap();
    let external_spacing = assert_information_spacing(ui);
    let original = surface.opacity();
    assert!(original >= 0.99);
    let painted = texture(&ui.window);
    let ink = downloaded(&painted);
    // Paired native scenes keep panel mode, allocation and content unchanged.
    // Hiding only its paint gives the true backing pixels at the same corner,
    // including the compact host; it does not synthesize a background color.
    surface.set_opacity(0.0);
    frame(&ui.window);
    let background = texture(&ui.window);
    assert_eq!(
        (painted.width(), painted.height()),
        (background.width(), background.height())
    );
    let blank = downloaded(&background);
    surface.set_opacity(original);
    frame(&ui.window);
    let gap = external_spacing["right_gap_logical"].as_f64().unwrap() as f32;
    let gap_x = bounds.x() + bounds.width() + gap / 2.0;
    let gap_y = bounds.y() + 24.0;
    let gap_painted = pixel_at(&ui.window, &painted, &ink, gap_x, gap_y);
    let gap_backing = pixel_at(&ui.window, &background, &blank, gap_x, gap_y);
    assert!(gap_painted[3] >= 254 && gap_backing[3] >= 254);
    assert!(color_distance(gap_painted, gap_backing) <= 1);
    let samples = [
        ("top-left-outside", 2.0, 1.0),
        ("top-right-outside", bounds.width() - 3.0, 1.0),
        ("top-left-inside", 12.0, 1.0),
        ("top-right-inside", bounds.width() - 13.0, 1.0),
        ("left-body-inside", 2.0, 24.0),
    ]
    .map(|(name, x, y)| {
        let x = bounds.x() + x;
        let y = bounds.y() + y;
        let painted = pixel_at(&ui.window, &painted, &ink, x, y);
        let backing = pixel_at(&ui.window, &background, &blank, x, y);
        assert!(
            painted[3] >= 254 && backing[3] >= 254,
            "no transparent pane gap"
        );
        (name, painted, backing, color_distance(painted, backing))
    });
    for (name, _, _, delta) in &samples[..2] {
        assert!(
            *delta <= 1,
            "rounded {name} must expose its real backing paint"
        );
    }
    if !high_contrast {
        // Normal System surfaces deliberately differ. The inward samples
        // must remain opaque panel paint, proving a curve rather than a hole
        // or a missing panel. High Contrast may use identical native fills.
        for (name, _, _, delta) in &samples[2..] {
            assert!(*delta >= 3, "opaque {name} is missing its pane surface");
        }
    }
    Some(json!({
        "panel_mode":format!("{:?}",this.panel_mode.get()),
        "surface_bounds":[bounds.x(),bounds.y(),bounds.width(),bounds.height()],
        "paired_native_pixels":samples,"high_contrast":high_contrast,
        "external_spacing":external_spacing,"right_gap_native_pixel":gap_painted
    }))
}

#[test]
#[ignore = "isolated Xvfb/Openbox System contrast and footer geometry; run at 1x and 2x"]
fn native_system_visual_surfaces() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let _manager = WindowManager::start();
    let (fixture, _app, ui, liked, _album) = populated_window(false);
    let this = &ui.music.controller;
    // The full layout matrix and native panel/LRC motion tests own their
    // respective contracts. This small set samples each surface for painted
    // AA contrast and keeps real footer hit boxes clear of the window edge.
    let settings = gtk::Settings::default().unwrap();
    settings.set_gtk_enable_animations(false);
    let high_contrast = adw::StyleManager::default().is_high_contrast();
    if std::env::var("ADW_DEBUG_HIGH_CONTRAST").as_deref() == Ok("1") {
        assert!(
            high_contrast,
            "the requested High Contrast source must be active"
        );
    }
    set_panel(&ui, workspace::PanelMode::Closed);
    resize(&ui, 420, 720);
    for dark in [false, true] {
        apply(&ui, dark);
        assert_footer_geometry(&ui);
        assert_hidden_footer_geometry(&ui);
    }
    activate_design_player_observed(&ui, &fixture, false);
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
    for dark in [false, true] {
        apply(&ui, dark);
        for (width, height, shape, state) in [
            (900, 650, "medium", "home"),
            (1440, 900, "wide", "liked"),
            (900, 650, "medium", "playlist"),
            (420, 720, "compact", "info"),
            (420, 720, "compact", "lyrics"),
            (1920, 1080, "large", "queue"),
            (0, 0, "maximized", "home"),
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
                "system-{}-{shape}-{state}-scale{}",
                if dark { "dark" } else { "light" },
                ui.window.scale_factor()
            );
            save(&ui, &name);
            let readability = readable_samples(&ui, &name);
            let header_surface = assert_header_surface(&ui);
            let panel_corners = assert_panel_corners(&ui, high_contrast);
            assert!(ui.window.has_css_class("karin-style-system"));
            assert_eq!(
                ui.window.has_css_class("karin-high-contrast"),
                high_contrast
            );
            assert!(!settings.is_gtk_enable_animations());
            records.push(json!({
                "capture":name,"dark":dark,"shape":shape,"state":state,
                "logical_size":[ui.window.width(),ui.window.height()],
                "scale":ui.window.scale_factor(),"readable_text_samples":readability,
                "high_contrast":high_contrast,"reduced_motion":true,
                "footer_safe_geometry":safe_footer_geometry(&ui),
                "header_surface":header_surface,"panel_corners":panel_corners
            }));
        }
    }
    // Check the real Default preference as well as the forced light/dark
    // palettes above. Run this bounded case last: it must not alter the
    // first-Play sequence or stand in for the separate cold observer.
    set_panel(&ui, workspace::PanelMode::Closed);
    if ui.window.is_maximized() {
        ui.window.unmaximize();
        pump(|| !ui.window.is_maximized());
    }
    resize(&ui, 420, 720);
    let mut values = this.persistence.borrow().values.clone();
    values.ui_style = UiStyle::System;
    values.appearance = ColorScheme::System;
    this.persistence.borrow_mut().values = values.clone();
    super::super::super::style::apply(&values, &ui.window);
    // The current track remains available from Home. Open through the same
    // footer action without adding an unrelated collection-route transition.
    set_panel(&ui, workspace::PanelMode::TrackInfo);
    assert_footer_geometry(&ui);
    let name = format!(
        "system-default-compact-info-scale{}",
        ui.window.scale_factor()
    );
    save(&ui, &name);
    records.push(json!({
        "capture":name,"preference":"system","dark":adw::StyleManager::default().is_dark(),
        "shape":"compact","state":"info",
        "logical_size":[ui.window.width(),ui.window.height()],
        "scale":ui.window.scale_factor(),"readable_text_samples":readable_samples(&ui,&name),
        "high_contrast":high_contrast,"reduced_motion":!settings.is_gtk_enable_animations(),
        "footer_safe_geometry":safe_footer_geometry(&ui),
        "header_surface":assert_header_surface(&ui),
        "panel_corners":assert_panel_corners(&ui,high_contrast)
    }));
    assert_eq!(
        this.music_snapshot().unwrap().state,
        crate::playback::PlaybackState::Paused
    );
    assert_eq!(
        this.model.borrow().queue.current().unwrap().id,
        liked.items[0].id
    );
    write_json(
        "system-visual-surfaces",
        &json!({"scope":"native original fixtures; no live account or physical GPU claim","cases":records}),
    );
    let closed = ui.close_for_smoke();
    pump(|| closed(false));
    assert!(closed(true));
}
