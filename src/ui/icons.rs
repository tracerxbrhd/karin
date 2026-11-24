// SPDX-License-Identifier: GPL-3.0-or-later
//! The application's selected MIT Tabler vectors, embedded and recolored by GTK.

use gtk::{gio, gio::prelude::FileExt};

const RESOURCE_PATH: &str = "/io/github/tracerxbrhd/Karin/icons";

fn register() {
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| {
        gio::resources_register_include!("karin.gresource")
            .expect("the compiled icon resource is valid");
    });
}

pub(super) fn install() {
    register();
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let theme = gtk::IconTheme::for_display(&display);
    if !theme
        .resource_path()
        .iter()
        .any(|path| path.as_str() == RESOURCE_PATH)
    {
        theme.add_resource_path(RESOURCE_PATH);
    }
}

/// Used by the existing packaged smoke check after GTK has a display. The
/// vectors must come from this executable, rather than an installed icon pack.
pub(super) fn verify() -> bool {
    install();
    let Some(display) = gtk::gdk::Display::default() else {
        return false;
    };
    let theme = gtk::IconTheme::for_display(&display);
    let path = format!("{RESOURCE_PATH}/scalable/actions");
    let Ok(files) = gio::resources_enumerate_children(&path, gio::ResourceLookupFlags::NONE) else {
        return false;
    };
    !files.is_empty()
        && files.iter().all(|file| {
            let Some(name) = file.strip_suffix(".svg") else {
                return false;
            };
            if !name.starts_with("karin-") || !theme.has_icon(name) {
                return false;
            }
            let icon = theme.lookup_icon(
                name,
                &[],
                24,
                1,
                gtk::TextDirection::Ltr,
                gtk::IconLookupFlags::FORCE_SYMBOLIC,
            );
            icon.is_symbolic()
                && icon.file().is_some_and(|source| {
                    source.uri().as_str() == format!("resource://{path}/{file}")
                })
        })
}

/// An explicit packaged smoke check of real native rendering. Normal playback
/// never renders test textures or reads pixels back from the GPU.
pub(super) fn verify_rendering(window: &impl gtk::prelude::IsA<gtk::Window>) -> bool {
    use gtk::prelude::*;
    let window: &gtk::Window = window.as_ref();
    let theme = gtk::IconTheme::for_display(&WidgetExt::display(window));
    let foreground = window.color();
    [
        "karin-search-symbolic",
        "karin-plus-symbolic",
        "karin-x-symbolic",
        "karin-heart-symbolic",
        "karin-heart-filled-symbolic",
        "karin-file-text-symbolic",
        "karin-playlist-symbolic",
    ]
    .into_iter()
    .all(|name| {
        let icon = theme.lookup_icon(
            name,
            &[],
            24,
            window.scale_factor(),
            gtk::TextDirection::Ltr,
            gtk::IconLookupFlags::FORCE_SYMBOLIC,
        );
        render_icon_pixels(window, &icon, 24, foreground).is_some_and(|(pixels, extent)| {
            pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] >= 96)
                && contour_is_valid(name, &pixels, extent) == Some(true)
        })
    })
}

fn render_icon_pixels(
    window: &gtk::Window,
    icon: &gtk::IconPaintable,
    size: i32,
    foreground: gtk::gdk::RGBA,
) -> Option<(Vec<u8>, usize)> {
    use gtk::prelude::*;
    let scale = window.scale_factor();
    let snapshot = gtk::Snapshot::new();
    snapshot.scale(scale as f32, scale as f32);
    icon.snapshot_symbolic(
        &snapshot,
        f64::from(size),
        f64::from(size),
        &[foreground; 4],
    );
    let extent = size * scale;
    let crop = gtk::graphene::Rect::new(0.0, 0.0, extent as f32, extent as f32);
    let texture = window
        .renderer()?
        .render_texture(snapshot.to_node()?, Some(&crop));
    if (texture.width(), texture.height()) != (extent, extent) {
        return None;
    }
    let extent = usize::try_from(extent).ok()?;
    let mut pixels = vec![0; extent * extent * 4];
    texture.download(&mut pixels, extent * 4);
    Some((pixels, extent))
}

/// These stable contour landmarks distinguish actual outlines from filled
/// silhouettes and empty strokes. Other icons retain the full ink/color checks.
fn contour_is_valid(name: &str, pixels: &[u8], extent: usize) -> Option<bool> {
    if extent == 0 || pixels.len() != extent * extent * 4 {
        return Some(false);
    }
    let alpha = |x: f64, y: f64| {
        let x = ((x * extent as f64 / 24.0) as usize).min(extent - 1);
        let y = ((y * extent as f64 / 24.0) as usize).min(extent - 1);
        pixels[(y * extent + x) * 4 + 3]
    };
    let ink = |x, y| alpha(x, y) >= 96;
    let clear = |x, y| alpha(x, y) <= 8;
    Some(match name {
        "karin-search-symbolic" => clear(10.0, 10.0) && ink(10.0, 3.0) && ink(18.0, 18.0),
        "karin-plus-symbolic" => {
            ink(12.0, 12.0)
                && ink(12.0, 7.0)
                && ink(7.0, 12.0)
                && ink(12.0, 17.0)
                && ink(17.0, 12.0)
                && clear(7.0, 7.0)
        }
        "karin-x-symbolic" => {
            ink(12.0, 12.0)
                && ink(7.0, 7.0)
                && ink(17.0, 17.0)
                && ink(7.0, 17.0)
                && ink(17.0, 7.0)
                && clear(12.0, 6.0)
        }
        "karin-heart-symbolic" => clear(12.0, 12.0),
        "karin-heart-filled-symbolic" => ink(12.0, 12.0),
        "karin-file-text-symbolic" => clear(12.0, 11.0) && ink(5.0, 12.0) && ink(12.0, 13.0),
        "karin-playlist-symbolic" => clear(14.0, 17.0) && ink(14.0, 14.0) && ink(8.0, 9.0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::{gdk, glib, prelude::*};
    use std::time::{Duration, Instant};

    fn icons() -> Vec<(String, String)> {
        let xml = roxmltree::Document::parse(include_str!("../../resources/karin.gresource.xml"))
            .unwrap();
        xml.descendants()
            .filter(|node| node.has_tag_name("file"))
            .map(|node| {
                let relative = node.text().unwrap();
                let name = relative
                    .rsplit('/')
                    .next()
                    .unwrap()
                    .strip_suffix(".svg")
                    .unwrap();
                (
                    name.to_owned(),
                    format!("/io/github/tracerxbrhd/Karin/{relative}"),
                )
            })
            .collect()
    }

    #[test]
    fn embedded_icon_sources_are_namespaced_symbolic_vectors() {
        register();
        let mut names = std::collections::BTreeSet::new();
        for (name, path) in icons() {
            assert!(name.starts_with("karin-") && name.ends_with("-symbolic"));
            assert!(names.insert(name.clone()), "duplicate icon name");
            let bytes = gio::resources_lookup_data(&path, gio::ResourceLookupFlags::NONE).unwrap();
            let text = std::str::from_utf8(bytes.as_ref()).unwrap();
            let svg = roxmltree::Document::parse(text).unwrap();
            let root = svg.root_element();
            assert_eq!(root.tag_name().name(), "svg");
            assert_eq!(root.attribute("width"), Some("24"));
            assert_eq!(root.attribute("height"), Some("24"));
            assert_eq!(root.attribute("viewBox"), Some("0 0 24 24"));
            if name == "karin-heart-filled-symbolic" {
                assert_eq!(root.attribute("fill"), Some("currentColor"));
            } else {
                assert_eq!(root.attribute("fill"), Some("none"));
                assert_eq!(root.attribute("stroke"), Some("currentColor"));
                assert_eq!(root.attribute("stroke-width"), Some("2"));
                assert_eq!(root.attribute("stroke-linecap"), Some("round"));
                assert_eq!(root.attribute("stroke-linejoin"), Some("round"));
            }
            for primitive in svg
                .descendants()
                .filter(|node| matches!(node.tag_name().name(), "path" | "circle" | "rect"))
            {
                let effective = |attribute| {
                    primitive
                        .ancestors()
                        .find_map(|node| node.attribute(attribute))
                };
                let filled = effective("fill").unwrap_or("black") != "none";
                let stroked = effective("stroke").unwrap_or("none") != "none";
                let classes: std::collections::BTreeSet<_> = primitive
                    .attribute("class")
                    .unwrap_or("")
                    .split_ascii_whitespace()
                    .collect();
                assert_eq!(
                    classes.contains("foreground-fill"),
                    filled,
                    "{name}: symbolic fill"
                );
                assert_eq!(
                    classes.contains("transparent-fill"),
                    !filled,
                    "{name}: empty fill"
                );
                assert_eq!(
                    classes.contains("foreground-stroke"),
                    stroked,
                    "{name}: symbolic stroke"
                );
                assert!(classes.iter().all(|class| matches!(
                    *class,
                    "foreground-fill" | "transparent-fill" | "foreground-stroke"
                )));
            }
            assert!(svg.descendants().all(|node| {
                !matches!(node.tag_name().name(), "image" | "script" | "foreignObject")
                    && node
                        .attributes()
                        .all(|attribute| !matches!(attribute.name(), "href" | "src"))
            }));
        }
        assert!(!names.is_empty());
        let license = include_str!("../../packaging/third-party-licenses/tabler-icons/LICENSE");
        assert!(license.contains("Copyright (c) 2020-2026 Paweł Kuna"));
        assert!(license.contains("MIT License"));
    }

    fn pump_until(predicate: impl Fn() -> bool) {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + Duration::from_secs(8);
        while !predicate() {
            assert!(
                Instant::now() < deadline,
                "native icon fixture did not settle"
            );
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn frame(window: &gtk::Window) {
        let seen = std::rc::Rc::new(std::cell::Cell::new(false));
        let clock = window.frame_clock().unwrap();
        let handler = clock.connect_after_paint({
            let seen = seen.clone();
            move |_| seen.set(true)
        });
        window.queue_draw();
        pump_until(|| seen.get());
        clock.disconnect(handler);
    }

    #[test]
    #[ignore = "requires an isolated GTK display; run the registered native fixture"]
    fn native_bundled_symbolic_icons_recolor() {
        gtk::init().unwrap();
        adw::init().unwrap();
        install();
        assert!(
            verify(),
            "the ordinary Native/Flatpak smoke uses these same resources"
        );
        let display = gdk::Display::default().unwrap();
        let theme = gtk::IconTheme::for_display(&display);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);
        let heading = gtk::Label::new(Some("Bundled symbolic icons"));
        content.append(&heading);
        let grid = gtk::Grid::new();
        grid.set_column_spacing(16);
        grid.set_row_spacing(16);
        grid.set_halign(gtk::Align::Center);
        content.append(&grid);
        let icons = icons();
        let images: Vec<_> = icons
            .iter()
            .enumerate()
            .map(|(index, (name, _))| {
                assert!(theme.has_icon(name), "bundled icon is missing: {name}");
                let image = gtk::Image::from_icon_name(name);
                image.set_tooltip_text(Some(name));
                grid.attach(&image, (index % 8) as i32, (index / 8) as i32, 1, 1);
                image
            })
            .collect();
        let window = gtk::Window::builder()
            .title("Karin bundled icons fixture")
            .default_width(480)
            .default_height(280)
            .child(&content)
            .build();
        window.present();
        pump_until(|| window.is_mapped() && window.renderer().is_some());
        assert!(
            verify_rendering(&window),
            "packaged smoke validates real symbolic contours"
        );
        let scale = window.scale_factor();
        let manager = adw::StyleManager::default();
        let previous_scheme = manager.color_scheme();
        let capture = std::env::var_os("KARIN_TEST_CAPTURE_DIR").map(std::path::PathBuf::from);
        if let Some(directory) = &capture {
            std::fs::create_dir_all(directory).unwrap();
        }
        let mut records = Vec::new();
        for (appearance, scheme) in [
            ("light", adw::ColorScheme::ForceLight),
            ("dark", adw::ColorScheme::ForceDark),
        ] {
            manager.set_color_scheme(scheme);
            frame(&window);
            let foreground = heading.color();
            for size in [16, 20, 24] {
                for image in &images {
                    image.set_pixel_size(size);
                }
                frame(&window);
                for (name, _) in &icons {
                    let paintable = theme.lookup_icon(
                        name,
                        &[],
                        size,
                        scale,
                        gtk::TextDirection::Ltr,
                        gtk::IconLookupFlags::FORCE_SYMBOLIC,
                    );
                    assert!(paintable.is_symbolic(), "{name} must recolor natively");
                    assert!(
                        paintable
                            .file()
                            .unwrap()
                            .uri()
                            .starts_with(&format!("resource://{RESOURCE_PATH}/")),
                        "{name} must come from the executable's resource, not the system"
                    );
                    let (pixels, extent) =
                        render_icon_pixels(&window, &paintable, size, foreground)
                            .expect("a loaded symbolic icon paints native pixels");
                    assert_eq!(extent, (size * scale) as usize);
                    let contour = contour_is_valid(name, &pixels, extent);
                    if let Some(valid) = contour {
                        assert!(
                            valid,
                            "{name} {appearance} must retain its outline/filled contour"
                        );
                    }
                    let mut painted = 0;
                    let mut max_error = 0;
                    for pixel in pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .filter(|pixel| pixel[3] >= 96)
                    {
                        painted += 1;
                        for (channel, color) in [
                            (2, foreground.red()),
                            (1, foreground.green()),
                            (0, foreground.blue()),
                        ] {
                            let expected = (color * f32::from(pixel[3])).round() as i16;
                            let error = (i16::from(pixel[channel]) - expected).unsigned_abs();
                            max_error = max_error.max(error);
                            assert!(error <= 3, "{name} {appearance} must use GTK foreground");
                        }
                    }
                    assert!(painted >= 4, "{name} must paint actual vector ink");
                    records.push(serde_json::json!({
                        "icon":name,"appearance":appearance,"logical_size":size,"scale":scale,
                        "high_contrast":manager.is_high_contrast(),"painted_pixels":painted,
                        "foreground":[foreground.red(),foreground.green(),foreground.blue(),foreground.alpha()],
                        "max_channel_error":max_error,"contour_valid":contour
                    }));
                }
            }
            if let Some(directory) = &capture {
                let paintable = gtk::WidgetPaintable::new(Some(&window));
                let image = paintable.current_image();
                let snapshot = gtk::Snapshot::new();
                snapshot.scale(scale as f32, scale as f32);
                image.snapshot(
                    &snapshot,
                    image.intrinsic_width().into(),
                    image.intrinsic_height().into(),
                );
                window
                    .renderer()
                    .unwrap()
                    .render_texture(snapshot.to_node().unwrap(), None)
                    .save_to_png(
                        directory.join(format!("symbolic-icons-{appearance}-scale{scale}.png")),
                    )
                    .unwrap();
            }
        }
        manager.set_color_scheme(previous_scheme);
        if let Some(directory) = &capture {
            std::fs::write(
                directory.join("symbolic-icons.json"),
                serde_json::to_vec_pretty(&records).unwrap(),
            )
            .unwrap();
        }
        window.close();
    }
}
