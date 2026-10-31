// SPDX-License-Identifier: GPL-3.0-or-later
//! Native cover widgets; network and decoding remain on a bounded worker.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use gtk::{gdk, glib};

use crate::artwork::{ArtworkCancellation, ArtworkError, ArtworkLoader, CoverArt};

/// A scale-aware thumbnail with a native symbolic fallback. The loader belongs
/// to the session controller and must be cleared on logout/close.
pub(super) fn thumbnail(
    loader: &ArtworkLoader,
    cover: Option<&CoverArt>,
    logical_size: i32,
) -> gtk::Overlay {
    let size = logical_size.clamp(24, 256);
    let overlay = gtk::Overlay::new();
    overlay.set_size_request(size, size);
    overlay.set_hexpand(false);
    overlay.set_vexpand(false);
    overlay.set_halign(gtk::Align::Start);
    overlay.set_valign(gtk::Align::Center);
    overlay.set_overflow(gtk::Overflow::Hidden);
    overlay.add_css_class("card");
    let fallback = gtk::Image::from_icon_name("audio-x-generic-symbolic");
    fallback.set_pixel_size((size / 2).max(16));
    fallback.set_hexpand(true);
    fallback.set_vexpand(true);
    fallback.add_css_class("dim-label");
    overlay.set_child(Some(&fallback));
    let picture = gtk::Picture::new();
    picture.set_content_fit(gtk::ContentFit::Cover);
    picture.set_can_shrink(true);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    picture.set_visible(false);
    overlay.add_overlay(&picture);
    let Some(cover) = cover else {
        return overlay;
    };
    let generation = Rc::new(Cell::new(0_u64));
    let active = Rc::new(RefCell::new(None::<ArtworkCancellation>));
    let load = {
        let loader = loader.clone();
        let cover = cover.clone();
        let picture = picture.downgrade();
        let fallback = fallback.downgrade();
        let generation = Rc::clone(&generation);
        let active = Rc::clone(&active);
        let overlay = overlay.downgrade();
        move |scale: i32| {
            if !overlay.upgrade().is_some_and(|widget| widget.is_mapped()) {
                return;
            }
            if let Some(request) = active.borrow_mut().take() {
                request.cancel();
            }
            let current = generation.get().wrapping_add(1);
            generation.set(current);
            let loader = loader.clone();
            let cover = cover.clone();
            let picture = picture.clone();
            let fallback = fallback.clone();
            let generation = Rc::clone(&generation);
            let active = Rc::clone(&active);
            let overlay = overlay.clone();
            let epoch = loader.epoch();
            glib::spawn_future_local(async move {
                // Rows outside the scroll viewport can be mapped. Wait until
                // this thumbnail is actually visible before starting HTTP.
                loop {
                    let Some(widget) = overlay.upgrade() else {
                        return;
                    };
                    if generation.get() != current || !widget.is_mapped() || loader.epoch() != epoch
                    {
                        return;
                    }
                    if visible_in_viewport(&widget) {
                        break;
                    }
                    drop(widget);
                    glib::timeout_future(std::time::Duration::from_millis(100)).await;
                }
                let mut requested = Err(ArtworkError::Busy);
                for _ in 0..3 {
                    if generation.get() != current || loader.epoch() != epoch {
                        return;
                    }
                    requested = loader.request(&cover, size.saturating_mul(scale), epoch);
                    if !matches!(requested, Err(ArtworkError::Busy)) {
                        break;
                    }
                    glib::timeout_future(std::time::Duration::from_millis(100)).await;
                }
                let Ok(request) = requested else { return };
                active.replace(Some(request.cancellation()));
                let result = loop {
                    if generation.get() != current
                        || loader.epoch() != epoch
                        || !overlay.upgrade().is_some_and(|widget| widget.is_mapped())
                    {
                        return;
                    }
                    if let Some(result) = request.try_result() {
                        break result;
                    }
                    glib::timeout_future(std::time::Duration::from_millis(25)).await;
                };
                active.borrow_mut().take();
                let (Some(picture), Some(fallback), Ok(image)) =
                    (picture.upgrade(), fallback.upgrade(), result)
                else {
                    return;
                };
                let bytes = glib::Bytes::from_owned(image.bytes.clone());
                let texture = gdk::MemoryTexture::new(
                    image.width,
                    image.height,
                    if image.alpha {
                        gdk::MemoryFormat::R8g8b8a8
                    } else {
                        gdk::MemoryFormat::R8g8b8
                    },
                    &bytes,
                    image.rowstride,
                );
                picture.set_paintable(Some(&texture));
                picture.set_visible(true);
                fallback.set_visible(false);
            });
        }
    };
    let load = Rc::new(load);
    overlay.connect_map({
        let load = Rc::clone(&load);
        move |widget| load(widget.scale_factor())
    });
    overlay.connect_unmap(move |_| {
        generation.set(generation.get().wrapping_add(1));
        if let Some(request) = active.borrow_mut().take() {
            request.cancel();
        }
    });
    overlay.connect_scale_factor_notify({
        let load = Rc::clone(&load);
        move |widget| load(widget.scale_factor())
    });
    // Weak GTK references avoid retaining a replaced row until HTTP finishes.
    // Session invalidation also discards a late successful decode.
    overlay
}

fn visible_in_viewport(widget: &gtk::Overlay) -> bool {
    if !widget.is_mapped() || widget.width() <= 0 || widget.height() <= 0 {
        return false;
    }
    let mut parent = widget.parent();
    while let Some(ancestor) = parent {
        if ancestor.is::<gtk::ScrolledWindow>()
            || ancestor.is::<gtk::Window>()
            || ancestor.overflow() == gtk::Overflow::Hidden
        {
            let Some(bounds) = widget.compute_bounds(&ancestor) else {
                return false;
            };
            let viewport = gtk::graphene::Rect::new(
                0.0,
                0.0,
                ancestor.width() as f32,
                ancestor.height() as f32,
            );
            if bounds
                .intersection(&viewport)
                .is_none_or(|area| area.width() <= 0.0 || area.height() <= 0.0)
            {
                return false;
            }
        }
        parent = ancestor.parent();
    }
    true
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    #[ignore = "separate GTK process under Xvfb; also run with GDK_SCALE=2"]
    fn artwork_thumbnail_maps_caches_hidpi_and_cancels_without_network() {
        gtk::init().expect("GTK display");
        let (loader, cover) = crate::artwork::cached_cover_fixture();
        let window = gtk::Window::builder()
            .default_width(300)
            .default_height(240)
            .build();
        let row = thumbnail(&loader, Some(&cover), 96);
        let picture = row
            .last_child()
            .unwrap()
            .downcast::<gtk::Picture>()
            .unwrap();
        assert!(!row.is_mapped());
        assert!(!picture.is_visible());
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let title = gtk::Label::new(Some("Synthetic track title"));
        title.set_hexpand(true);
        title.set_xalign(0.0);
        content.append(&row);
        content.append(&title);
        window.set_child(Some(&content));
        window.present();
        let context = glib::MainContext::default();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !picture.is_visible() && Instant::now() < deadline {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(picture.is_visible(), "cached synthetic cover should render");
        let texture = picture
            .paintable()
            .unwrap()
            .downcast::<gdk::MemoryTexture>()
            .unwrap();
        let target = 96 * row.scale_factor();
        let expected = match target {
            ..=128 => 128,
            129..=256 => 256,
            _ => 512,
        };
        assert_eq!(texture.width(), expected);
        assert_eq!(texture.height(), expected);
        assert_eq!((row.width(), row.height()), (96, 96));
        let bounds = title.compute_bounds(&content).expect("adjacent title");
        assert!(
            (bounds.x() - 108.0).abs() < 1.0,
            "cover must not reserve expanding blank space before its title"
        );
        let weak = row.downgrade();
        window.set_child(gtk::Widget::NONE);
        window.close();
        drop(row);
        drop(picture);
        drop(window);
        drop(content);
        drop(title);
        for _ in 0..10 {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            weak.upgrade().is_none(),
            "thumbnail must not retain GTK widgets"
        );
        loader.clear();
    }
}
