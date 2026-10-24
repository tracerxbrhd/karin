//! Non-secret application identity and defaults.

pub const APP_ID: &str = "io.github.tracerxbrhd.Karin";
pub const APP_NAME: &str = "Karin";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_id_is_valid_for_dbus() {
        assert!(gtk::gio::Application::id_is_valid(APP_ID));
    }
}
