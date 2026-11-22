// SPDX-License-Identifier: GPL-3.0-or-later
//! Presentation-only appearance. Native preferences remain owned by Adwaita;
//! no service, session, queue or playback object is created or replaced here.

use adw::prelude::*;

use crate::settings::{ColorScheme, Preferences, UiStyle};

const COMPONENT_CSS: &str = include_str!("../../resources/styles/components.css");
const SYSTEM_CSS: &str = include_str!("../../resources/styles/system.css");
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
    window.add_css_class("karin-style-system");
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
        let handler = std::cell::RefCell::new(Some(contrast));
        window.connect_destroy(move |_| {
            if let Some(contrast) = handler.borrow_mut().take() {
                manager.disconnect(contrast);
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
}

pub(super) fn apply(values: &Preferences, window: &adw::ApplicationWindow) -> AppliedAppearance {
    install();
    let selected = AppliedAppearance {
        requested: values.ui_style,
        effective: values.ui_style.effective(),
        color_scheme: values.appearance,
    };
    attach(window);
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
        // A single provider preserves native components and Lyrics geometry
        // across independent color-scheme changes.
        provider.load_from_string(&format!("{COMPONENT_CSS}\n{SYSTEM_CSS}\n{LYRICS_CSS}"));
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
