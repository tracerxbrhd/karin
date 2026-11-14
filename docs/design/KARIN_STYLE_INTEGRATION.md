<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Integrating the future Karin presentation

The current **System** presentation is the working GTK4/Libadwaita application.
The designer's new Karin materials have not been supplied. **Karin is not an
implemented theme**: its disabled Preferences row says «В разработке». Do not
replace that row with an apparently working selector for a copy of System.

`UiStyle::{System,Karin}` is independent of `ColorScheme::{System,Light,Dark}`.
`ColorScheme` aliases the existing `Appearance` type, so its persisted
`appearance` field and values remain compatible. Settings schema 3 reads schemas
1 and 2, retaining remembered-login/deletion intent, aggregate cache policy and
the pinned pane choice. A stored Karin request is retained but explicitly falls
back to System until materials are integrated. An invalid style alone cannot
erase those other settings. Credentials, cached media and playback restoration
remain in their own stores.

## Presentation boundary

`src/ui/style.rs` installs the existing System CSS from
`resources/styles/system.css`. Selectors are scoped to `.karin-style-system`
under `.karin-presentation-root`; a future provider uses `.karin-style-custom`.
`resources/styles/karin.css` is a comment-only integration entry point and is
**not loaded**. It contains no provisional designer work.

`style::apply` changes the presentation root and Adwaita color scheme. It does
not construct a service, session, player, cache or queue. Existing UI builders,
view models and callbacks remain the integration boundary for any replacement
view; views send the same typed user intents to the current coordinator.
Replace only the widgets required by approved designs, preserving focus,
selection, scroll, open-pane intent and in-flight request ownership. Keep
MusicService, PlaybackController, Wave, MPRIS and cache contracts shared.

To enable Karin, implement its real provider/views, both schemes and accessibility
states first, then change `UiStyle::is_available`, the native Preferences row and
`style::apply` together. Availability is not permission to instantiate another
application or add API calls to CSS/view builders. Do not enable it merely by
loading System CSS with another class name.

## Materials required from the designer

| Area | Required states/materials |
| --- | --- |
| Main music workspace | Home/My Wave, liked tracks, playlist grid/detail, album/artist detail, unified search and pagination |
| Playback | Hidden/active/paused/error footer, progress/seek, volume, shuffle/repeat, queue, pinned current-track pane and follow/manual intent |
| Account and editing | Signed-out/loading/signed-in, auth dialog/cancellation, playlist create/rename/reorder/delete/confirmation and ambiguous-write errors |
| Preferences | Independent style and color choices, session restoration, shared cache quota/Clear/Retry and unavailable protected storage |
| Component states | Loading, empty, partial, stale, unavailable, error/Retry, current occurrence, selected/hover/focus/disabled; no fake content |
| Responsive layout | About 420×720, 900×650, 1440×900 and maximized; 1×/2×; long translated titles and keyboard-only navigation |
| Colors | Light, dark and high contrast; semantic foreground/background/surface/border/accent/error/success roles with contrast targets |
| Typography/spacing | System-font fallback, relative type scale, weight/line-height roles, spacing/radius/target-size roles, text scaling constraints |
| Assets | Editable originals, licensed icons/illustrations, symbolic variants and scalable/HiDPI exports, attribution and permission to distribute |

Use the native semantic palette where possible (`@window_bg_color`,
`@theme_fg_color`, `@borders`, `@accent_color`, `@accent_bg_color`). System CSS
already uses those roles and relative font sizes. Designer values should be
localized to the branded resource and semantic widget classes; introduce only
tokens that the actual materials need. Avoid globally overriding GTK controls,
font families, Adwaita CSS, focus rings or unrelated application windows.

## System preference behavior

System color uses `AdwStyleManager::Default`; explicit light/dark choices use
`ForceLight`/`ForceDark`. Returning to System resumes Adwaita's supported system
settings source. There is no `gtk-application-prefer-dark-theme` override.
System accent support depends on the installed Libadwaita/portal; inherited
semantic accent colors continue to work without raising the current API floor.
Fonts, scale and reduced motion stay owned by GTK. High-contrast changes update
one presentation class, retaining opaque text and an outline for current rows.

This does not promise full Qt/Breeze CSS inheritance on KDE. If a desktop portal
does not expose a preference, Adwaita/GTK's normal fallback remains in effect.
Do not compensate by scraping another toolkit's private settings.

## Verification before enabling branded style

Run the repository UI quality gate on the actual widgets, including mouse and
keyboard actions, screen-reader names/roles, visible focus, text/zoom, both
schemes, high contrast, reduced motion and 1×/2×. Inspect real screenshots for
density and clipping. Confirm transitions while audio plays, a queue and cache
exist and MPRIS is connected; none may restart or change due to presentation.
Restart and check both independent preferences, including migrated settings.

The existing native Preferences fixture also runs fresh-process synthetic
Adwaita system-light/dark and high-contrast cases, manual overrides and return to
System, unchanged font/reduced-motion settings and unavailable-style fallback.
These debug-setting fixtures are not a KDE/GNOME portal certification. Native
and installed Flatpak acceptance must check live desktop preference changes and
actual media playback. The external Karin design cannot be visually accepted
before its real resources exist.
