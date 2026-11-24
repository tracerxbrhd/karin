// SPDX-License-Identifier: GPL-3.0-or-later
//! Non-secret application identity and defaults.

pub const APP_ID: &str = "io.github.tracerxbrhd.Karin";
pub const APP_NAME: &str = "Karin";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SOURCE_REVISION: &str = env!("KARIN_SOURCE_REVISION");
pub const SOURCE_STATUS: &str = env!("KARIN_SOURCE_STATUS");
pub const BUILD_PROFILE: &str = env!("KARIN_BUILD_PROFILE");
pub const BUILD_COMPILER: &str = env!("KARIN_BUILD_COMPILER");
pub const BUILD_TARGET: &str = env!("KARIN_BUILD_TARGET");
pub const BUILD_FEATURES: &str = env!("KARIN_BUILD_FEATURES");

/// Runtime packaging is independent of the public source revision.
pub fn package_kind() -> &'static str {
    if std::path::Path::new("/.flatpak-info").is_file() {
        "Flatpak"
    } else {
        "Native"
    }
}

pub fn build_identity() -> String {
    format!(
        "Karin {VERSION}\nSource commit: {SOURCE_REVISION}\nSource state: {SOURCE_STATUS}\nPackage: {}\nProfile: {BUILD_PROFILE}\nCompiler: {BUILD_COMPILER}\nTarget: {BUILD_TARGET}\nFeatures: {BUILD_FEATURES}",
        package_kind()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_id_is_valid_for_dbus() {
        assert!(gtk::gio::Application::id_is_valid(APP_ID));
    }
}
