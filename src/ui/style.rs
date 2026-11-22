// SPDX-License-Identifier: GPL-3.0-or-later
//! Presentation-only appearance. Native preferences remain owned by Adwaita;
//! no service, session, queue or playback object is created or replaced here.

use adw::prelude::*;

use crate::settings::{ColorScheme, Preferences, UiStyle};

mod liquid_glass;

const COMPONENT_CSS: &str = include_str!("../../resources/styles/components.css");
const SYSTEM_CSS: &str = include_str!("../../resources/styles/system.css");
const GLASS_CSS: &str = include_str!("../../resources/styles/liquid-glass.css");
const LYRICS_CSS: &str = include_str!("../../resources/styles/lyrics.css");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AppliedAppearance {
    pub requested: UiStyle,
    pub effective: UiStyle,
    pub color_scheme: ColorScheme,
}

/// Attach the current native presentation before the first frame. Future view
/// builders share the same controller/callbacks and receive their own root
/// class; the reserved branded resource is not a second application.
pub(super) fn attach(window: &adw::ApplicationWindow) {
    let attached = window.has_css_class("karin-presentation-root");
    window.add_css_class("karin-presentation-root");
    if !attached {
        window.add_css_class("karin-style-system");
    }
    window.remove_css_class("karin-style-custom");
    let manager = adw::StyleManager::default();
    refresh(window, &manager);
    if !attached {
        let weak = window.downgrade();
        let contrast = manager.connect_high_contrast_notify(move |manager| {
            if let Some(window) = weak.upgrade() {
                refresh(&window, manager);
            }
        });
        let weak = window.downgrade();
        let dark = manager.connect_dark_notify(move |manager| {
            if let Some(window) = weak.upgrade() {
                refresh(&window, manager);
            }
        });
        let handlers = std::cell::RefCell::new(Some((contrast, dark)));
        window.connect_destroy(move |_| {
            if let Some((contrast, dark)) = handlers.borrow_mut().take() {
                manager.disconnect(contrast);
                manager.disconnect(dark);
            }
        });
    }
}

fn refresh(window: &adw::ApplicationWindow, manager: &adw::StyleManager) {
    if manager.is_high_contrast() {
        window.add_css_class("karin-high-contrast");
    } else {
        window.remove_css_class("karin-high-contrast");
    }
    if manager.is_dark() {
        window.add_css_class("karin-color-dark");
    } else {
        window.remove_css_class("karin-color-dark");
    }
    if let Some(background) = background(window) {
        background.set_dark(manager.is_dark());
        background.set_high_contrast(manager.is_high_contrast());
        let enabled =
            window.has_css_class("karin-style-liquid-glass") && !manager.is_high_contrast();
        background.set_enabled(enabled);
        background.set_visible(enabled);
    }
}

fn background(window: &adw::ApplicationWindow) -> Option<liquid_glass::LiquidGlass> {
    window
        .content()?
        .downcast::<gtk::Overlay>()
        .ok()?
        .child()?
        .downcast()
        .ok()
}

/// Preserve the existing measured toolbar, widgets and event targets. The only
/// new child is a non-interactive, application-owned background behind it.
pub(super) fn wrap(
    window: &adw::ApplicationWindow,
    content: &impl IsA<gtk::Widget>,
) -> gtk::Overlay {
    let layer = gtk::Overlay::new();
    layer.set_widget_name("karin-presentation-layers");
    let background = liquid_glass::LiquidGlass::new();
    background.set_visible(false);
    layer.set_child(Some(&background));
    layer.add_overlay(content);
    layer.set_measure_overlay(content, true);
    layer.set_clip_overlay(content, false);
    // attach() precedes the shell construction. Apply the same live palette
    // after the layer has been attached to the window in build_with_music_pane.
    let weak = window.downgrade();
    layer.connect_map(move |_| {
        if let Some(window) = weak.upgrade() {
            refresh(&window, &adw::StyleManager::default());
        }
    });
    layer
}

pub(super) fn apply(values: &Preferences, window: &adw::ApplicationWindow) -> AppliedAppearance {
    install();
    let selected = AppliedAppearance {
        requested: values.ui_style,
        effective: values.ui_style.effective(),
        color_scheme: values.appearance,
    };
    attach(window);
    let glass = selected.effective == UiStyle::LiquidGlass;
    if glass {
        window.remove_css_class("karin-style-system");
        window.add_css_class("karin-style-liquid-glass");
    } else {
        window.remove_css_class("karin-style-liquid-glass");
        window.add_css_class("karin-style-system");
    }
    adw::StyleManager::default().set_color_scheme(adwaita_scheme(selected.color_scheme));
    refresh(window, &adw::StyleManager::default());
    selected
}

fn adwaita_scheme(scheme: ColorScheme) -> adw::ColorScheme {
    match scheme {
        ColorScheme::System => adw::ColorScheme::Default,
        ColorScheme::Light => adw::ColorScheme::ForceLight,
        ColorScheme::Dark => adw::ColorScheme::ForceDark,
    }
}

pub(super) fn install() {
    thread_local! { static INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    INSTALLED.with(|installed| {
        if installed.replace(true) {
            return;
        }
        let provider = gtk::CssProvider::new();
        // One provider, installed once. Switching styles changes only scoped
        // classes; no old provider can accumulate or override the next style.
        provider.load_from_string(&format!(
            "{COMPONENT_CSS}\n{SYSTEM_CSS}\n{GLASS_CSS}\n{LYRICS_CSS}"
        ));
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_availability_and_color_scheme_are_independent() {
        assert!(UiStyle::System.is_available());
        assert!(UiStyle::LiquidGlass.is_available());
        assert!(!UiStyle::Karin.is_available());
        assert_eq!(UiStyle::Karin.effective(), UiStyle::System);
        assert_eq!(
            adwaita_scheme(ColorScheme::System),
            adw::ColorScheme::Default
        );
        assert_eq!(
            adwaita_scheme(ColorScheme::Light),
            adw::ColorScheme::ForceLight
        );
        assert_eq!(
            adwaita_scheme(ColorScheme::Dark),
            adw::ColorScheme::ForceDark
        );
    }
}
