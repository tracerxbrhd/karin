// SPDX-License-Identifier: GPL-3.0-or-later
//! Small native presentation rules. Colors follow the active GTK/Adwaita theme.

pub(super) fn install() {
    thread_local! { static INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    INSTALLED.with(|installed| {
        if installed.replace(true) { return; }
        let provider = gtk::CssProvider::new();
        provider.load_from_string(r#"
            .music-shell { background: @window_bg_color; }
            .music-workspace { background: @window_bg_color; }
            .music-header { min-height: 48px; }
            .music-rail { background: @window_bg_color; }
            .music-rail list { background: transparent; }
            .music-nav-item { margin: 3px 8px; border-radius: 12px; padding: 0; }
            .music-nav-item:selected { background: alpha(@accent_bg_color, .18); color: @accent_color; }
            .music-information-view { background: @window_bg_color; border-left: 1px solid @borders; }
            .music-track-list { background: transparent; border: none; box-shadow: none; }
            .music-track-row { padding: 0; border: none; border-radius: 10px; }
            .music-track-row:hover { background: alpha(@theme_fg_color, .055); }
            .music-track-row.music-current, .music-queue-row.music-current-row {
                background: alpha(@accent_bg_color, .16);
            }
            .music-track-title { font-weight: 600; }
            .music-track-artist, .music-track-duration { font-size: .92em; }
            .music-track-duration { color: alpha(@theme_fg_color, .65); }
            .music-unavailable { opacity: .55; }
            .music-row-badge { font-size: .85em; color: @accent_color; }
            .music-collection-title { font-size: 1.65em; font-weight: 700; }
            .music-liked-cover { background: alpha(@accent_bg_color, .14); color: @accent_color; border-radius: 16px; }
            .music-collection-facts { font-size: .92em; }
            .music-catalog-tile, .karin-playlist-tile { padding: 8px; border-radius: 12px; }
            .music-catalog-tile:hover, .karin-playlist-tile:hover { background: alpha(@theme_fg_color, .055); }
            .music-catalog-title { font-weight: 600; }
            .music-catalog-subtitle { font-size: .9em; }
            .music-catalog-grid, .karin-playlist-grid { background: transparent; }
            .music-catalog-results { background: transparent; }
            .music-catalog-grid-row { padding: 0; background: transparent; }
            .music-catalog-grid > flowboxchild { padding: 0; }
            .music-artist-art { border-radius: 999px; }
            .music-home-hero-art, .music-collection-hero image, .music-collection-hero picture,
            .karin-playlist-cover, .music-catalog-tile picture { border-radius: 10px; }
            .karin-track-pane-header { min-height: 44px; }
            .karin-track-pane-cover { border-radius: 12px; }
            .karin-track-pane-link { min-height: 24px; padding: 2px 0; font-weight: normal; }
            .karin-track-pane-actions button { min-width: 30px; min-height: 30px; }
            .music-player { background: @window_bg_color; }
            .music-player-main { min-height: 64px; }
            .music-player-main button.flat { padding: 4px; }
            .music-player-main .music-transport button { min-width: 24px; }
            .music-player-title { font-weight: 600; }
            #music-current-like.liked { color: @accent_color; background: alpha(@accent_bg_color, .10); border-radius: 999px; }
            .music-player-artist { font-size: .9em; }
            .music-player-timeline scale { padding-top: 0; padding-bottom: 0; }
            .music-player-timeline label { font-size: .85em; color: alpha(@theme_fg_color, .7); }
            .music-transport button { border-radius: 999px; }
            .music-transport button:checked, #music-repeat.active { color: @accent_color; background: alpha(@accent_bg_color, .12); }
            .music-player-compact .music-player-main { min-height: 60px; }
            .music-repeat-one-badge { font-size: .75em; font-weight: 800; color: @accent_color; background: @window_bg_color; border-radius: 3px; padding: 0 1px; }
            #music-current-info { padding: 0; }
        "#);
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    });
}
