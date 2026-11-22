// SPDX-License-Identifier: GPL-3.0-or-later
//! Original, static Karin scenery beneath the existing native application.
//!
//! GSK blurs this small procedural scene once per scheme/scale combination.
//! Subsequent snapshots only draw the cached texture. Foreground widgets,
//! artwork, the desktop and playback do not participate in the preparation.

use std::{
    cell::{Cell, RefCell},
    sync::LazyLock,
};

use adw::prelude::*;
use gtk::{gdk, glib, graphene, gsk, subclass::prelude::*};

const WIDTH: f32 = 512.0;
const HEIGHT: f32 = 384.0;
const BLUR_RADIUS: f64 = 14.0;
const CACHE_SLOTS: usize = 4;

#[derive(Clone)]
struct PreparedTexture {
    texture: gdk::Texture,
    pixel_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) struct RenderStats {
    pub preparation_count: u64,
    pub snapshot_count: u64,
    pub blur_pass_count: u64,
    pub cache_hit_count: u64,
    pub cache_entries: u64,
    pub cache_pixel_bytes: u64,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct LiquidGlass {
        pub enabled: Cell<bool>,
        pub dark: Cell<bool>,
        pub high_contrast: Cell<bool>,
        pub(super) textures: RefCell<[Option<PreparedTexture>; CACHE_SLOTS]>,
        pub preparations: Cell<u64>,
        pub snapshots: Cell<u64>,
        pub blurs: Cell<u64>,
        pub hits: Cell<u64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LiquidGlass {
        const NAME: &'static str = "KarinLiquidGlassBackground";
        type Type = super::LiquidGlass;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for LiquidGlass {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: LazyLock<Vec<glib::ParamSpec>> = LazyLock::new(|| {
                [
                    "preparation-count",
                    "snapshot-count",
                    "blur-pass-count",
                    "cache-hit-count",
                    "cache-entry-count",
                    "cache-pixel-bytes",
                ]
                .map(|name| glib::ParamSpecUInt64::builder(name).read_only().build())
                .to_vec()
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _: usize, spec: &glib::ParamSpec) -> glib::Value {
            // These counters are polled by acceptance observers. Notifications
            // during drawing would add needless work to ordinary playback.
            let stats = self.obj().stats();
            match spec.name() {
                "preparation-count" => stats.preparation_count.to_value(),
                "snapshot-count" => stats.snapshot_count.to_value(),
                "blur-pass-count" => stats.blur_pass_count.to_value(),
                "cache-hit-count" => stats.cache_hit_count.to_value(),
                "cache-entry-count" => stats.cache_entries.to_value(),
                "cache-pixel-bytes" => stats.cache_pixel_bytes.to_value(),
                _ => unreachable!("unknown glass presentation property"),
            }
        }

        fn dispose(&self) {
            self.textures.replace(Default::default());
        }
    }

    impl WidgetImpl for LiquidGlass {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            // The existing application, measured by GtkOverlay, owns layout.
            (0, 0, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            if !self.enabled.get() || widget.width() <= 0 || widget.height() <= 0 {
                return;
            }
            self.snapshots.set(self.snapshots.get().saturating_add(1));
            let bounds =
                graphene::Rect::new(0.0, 0.0, widget.width() as f32, widget.height() as f32);
            if self.high_contrast.get() {
                snapshot.append_color(
                    &if self.dark.get() {
                        tint([0, 0, 0], 1.0)
                    } else {
                        tint([255, 255, 255], 1.0)
                    },
                    &bounds,
                );
                return;
            }
            let scale = widget.scale_factor().clamp(1, 2);
            let slot = usize::from(self.dark.get()) * 2 + (scale - 1) as usize;
            let cached = self.textures.borrow()[slot].clone();
            let texture = match cached {
                Some(texture) => {
                    self.hits.set(self.hits.get().saturating_add(1));
                    texture
                }
                None => {
                    let Some(renderer) = widget.native().and_then(|native| native.renderer())
                    else {
                        snapshot.append_color(&base_color(self.dark.get()), &bounds);
                        return;
                    };
                    let texture = prepare(&renderer, self.dark.get(), scale);
                    self.preparations
                        .set(self.preparations.get().saturating_add(1));
                    self.blurs.set(self.blurs.get().saturating_add(1));
                    self.textures.borrow_mut()[slot] = Some(texture.clone());
                    texture
                }
            };
            let device_scale = widget
                .native()
                .and_then(|native| native.surface())
                .map_or_else(
                    || f64::from(widget.scale_factor()),
                    |surface| surface.scale(),
                );
            if device_scale == 1.0 || device_scale == 2.0 {
                // Sample only this already blurred bitmap in device pixels.
                // Cancelling the surface scale avoids TextureScaleNode's extra
                // HiDPI offscreen pass. Native foreground widgets keep their
                // normal scale and are never part of this sampling operation.
                let scale = device_scale as f32;
                snapshot.save();
                snapshot.scale(scale.recip(), scale.recip());
                snapshot.append_scaled_texture(
                    &texture.texture,
                    gsk::ScalingFilter::Nearest,
                    &graphene::Rect::new(0.0, 0.0, bounds.width() * scale, bounds.height() * scale),
                );
                snapshot.restore();
            } else {
                // Preserve the original path for unverified fractional scales.
                snapshot.append_texture(&texture.texture, &bounds);
            }
        }
    }
}

glib::wrapper! {
    pub struct LiquidGlass(ObjectSubclass<imp::LiquidGlass>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl LiquidGlass {
    pub(super) fn new() -> Self {
        let background: Self = glib::Object::new();
        background.set_widget_name("karin-liquid-glass-background");
        background.set_can_target(false);
        background.set_can_focus(false);
        background.set_focusable(false);
        background.set_hexpand(true);
        background.set_vexpand(true);
        background.update_state(&[gtk::accessible::State::Hidden(true)]);
        background.set_visible(false);
        background
    }

    pub(super) fn set_enabled(&self, enabled: bool) {
        if self.imp().enabled.replace(enabled) != enabled {
            self.set_visible(enabled);
            self.queue_draw();
        }
    }

    pub(super) fn set_dark(&self, dark: bool) {
        if self.imp().dark.replace(dark) != dark {
            self.queue_draw();
        }
    }

    pub(super) fn set_high_contrast(&self, high_contrast: bool) {
        if self.imp().high_contrast.replace(high_contrast) != high_contrast {
            self.queue_draw();
        }
    }

    pub(super) fn stats(&self) -> RenderStats {
        let imp = self.imp();
        let textures = imp.textures.borrow();
        RenderStats {
            preparation_count: imp.preparations.get(),
            snapshot_count: imp.snapshots.get(),
            blur_pass_count: imp.blurs.get(),
            cache_hit_count: imp.hits.get(),
            cache_entries: textures.iter().flatten().count() as u64,
            cache_pixel_bytes: textures
                .iter()
                .flatten()
                .map(|texture| texture.pixel_bytes)
                .sum(),
        }
    }
}

fn tint([red, green, blue]: [u8; 3], alpha: f32) -> gdk::RGBA {
    gdk::RGBA::new(
        f32::from(red) / 255.0,
        f32::from(green) / 255.0,
        f32::from(blue) / 255.0,
        alpha,
    )
}

fn base_color(dark: bool) -> gdk::RGBA {
    tint(if dark { [17, 28, 47] } else { [225, 234, 245] }, 1.0)
}

fn prepare(renderer: &gsk::Renderer, dark: bool, scale: i32) -> PreparedTexture {
    let snapshot = gtk::Snapshot::new();
    snapshot.scale(scale as f32, scale as f32);
    snapshot.push_blur(BLUR_RADIUS);
    draw_scene(&snapshot, dark);
    snapshot.pop();
    let viewport = graphene::Rect::new(0.0, 0.0, WIDTH * scale as f32, HEIGHT * scale as f32);
    opaque_cache_texture(renderer.render_texture(snapshot.to_node().unwrap(), Some(&viewport)))
}

fn opaque_cache_texture(texture: gdk::Texture) -> PreparedTexture {
    // GSK's offscreen render exports an alpha-capable texture, including when
    // every pixel is opaque. TextureNode determines opacity from its format,
    // so retaining that format prevents ordinary opaque-background culling.
    // Normalize once per bounded cache key, preserving the real blurred RGB
    // pixels. Sampling is selected separately; never discard actual alpha.
    let width = texture.width() as usize;
    let height = texture.height() as usize;
    let mut downloader = gdk::TextureDownloader::new(&texture);
    downloader.set_format(gdk::MemoryFormat::R8g8b8a8Premultiplied);
    let (bytes, stride) = downloader.download_bytes();
    let rgba = bytes.as_ref();
    let mut rgb = Vec::with_capacity(width * height * 3);
    for row in 0..height {
        for pixel in rgba[row * stride..row * stride + width * 4]
            .as_chunks::<4>()
            .0
        {
            if pixel[3] != 255 {
                return PreparedTexture {
                    texture,
                    pixel_bytes: (width * height * 4) as u64,
                };
            }
            rgb.extend_from_slice(&pixel[..3]);
        }
    }
    PreparedTexture {
        texture: gdk::MemoryTexture::new(
            width as i32,
            height as i32,
            gdk::MemoryFormat::R8g8b8,
            &glib::Bytes::from_owned(rgb),
            width * 3,
        )
        .upcast(),
        pixel_bytes: (width * height * 3) as u64,
    }
}

fn draw_scene(snapshot: &gtk::Snapshot, dark: bool) {
    // Extend the opaque base past the blur kernel so the window edges stay
    // fully covered. This scene is original geometry, without external assets.
    let bounds = graphene::Rect::new(-48.0, -48.0, WIDTH + 96.0, HEIGHT + 96.0);
    let far = tint(if dark { [34, 26, 50] } else { [237, 226, 244] }, 1.0);
    snapshot.append_linear_gradient(
        &bounds,
        &graphene::Point::new(0.0, 0.0),
        &graphene::Point::new(WIDTH, HEIGHT),
        &[
            gsk::ColorStop::new(0.0, base_color(dark)),
            gsk::ColorStop::new(1.0, far),
        ],
    );
    let cyan = tint(
        if dark { [32, 109, 135] } else { [91, 185, 222] },
        if dark { 0.56 } else { 0.52 },
    );
    let violet = tint(
        if dark {
            [101, 73, 143]
        } else {
            [179, 151, 218]
        },
        if dark { 0.50 } else { 0.48 },
    );
    for (center, horizontal, vertical, color) in [
        (graphene::Point::new(48.0, 56.0), 300.0, 236.0, cyan),
        (graphene::Point::new(486.0, 356.0), 340.0, 262.0, violet),
    ] {
        let mut clear = color;
        clear.set_alpha(0.0);
        snapshot.append_radial_gradient(
            &bounds,
            &center,
            horizontal,
            vertical,
            0.0,
            1.0,
            &[
                gsk::ColorStop::new(0.0, color),
                gsk::ColorStop::new(1.0, clear),
            ],
        );
    }
    // A restrained luminous ribbon leaves a visible optical transition after
    // real GSK blur, without animating or repainting with the current track.
    let ribbon = graphene::Rect::new(136.0, -80.0, 84.0, 310.0);
    snapshot.save();
    snapshot.rotate(32.0);
    snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(ribbon, 42.0));
    snapshot.append_linear_gradient(
        &ribbon,
        &graphene::Point::new(136.0, -80.0),
        &graphene::Point::new(220.0, 230.0),
        &[
            gsk::ColorStop::new(0.0, tint([242, 252, 255], if dark { 0.04 } else { 0.38 })),
            gsk::ColorStop::new(0.50, tint([207, 236, 251], if dark { 0.10 } else { 0.46 })),
            gsk::ColorStop::new(1.0, tint([221, 218, 251], 0.0)),
        ],
    );
    snapshot.pop();
    snapshot.restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "native GTK texture format and transparency preservation"]
    fn native_glass_cache_rgb_identity_and_alpha_preservation() {
        gtk::init().unwrap();
        let pixels = vec![12, 34, 56, 255, 65, 87, 109, 255];
        let source: gdk::Texture = gdk::MemoryTexture::new(
            2,
            1,
            gdk::MemoryFormat::R8g8b8a8Premultiplied,
            &glib::Bytes::from_owned(pixels.clone()),
            8,
        )
        .upcast();
        let cached = opaque_cache_texture(source);
        assert_eq!(cached.pixel_bytes, 6);
        let mut downloader = gdk::TextureDownloader::new(&cached.texture);
        downloader.set_format(gdk::MemoryFormat::R8g8b8a8Premultiplied);
        let (bytes, _) = downloader.download_bytes();
        assert_eq!(bytes.as_ref(), pixels);

        let source: gdk::Texture = gdk::MemoryTexture::new(
            2,
            1,
            gdk::MemoryFormat::R8g8b8a8Premultiplied,
            &glib::Bytes::from_owned(vec![12, 34, 56, 255, 65, 87, 109, 128]),
            8,
        )
        .upcast();
        let cached = opaque_cache_texture(source.clone());
        assert_eq!(cached.texture, source);
        assert_eq!(cached.pixel_bytes, 8);
    }
}
