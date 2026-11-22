<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0039: Liquid Glass beside the native System presentation

Status: Implemented in Draft PR #29; final desktop presentation acceptance pending.
Date: 2026-11-22
Extends: ADR-0022, ADR-0033 and ADR-0034

## Decision

Add `UiStyle::LiquidGlass` with the `liquid-glass` wire value. System remains
the default. The independently reserved Karin design remains unavailable.
The existing schema 3, worker-owned atomic settings and independent
System/Light/Dark color preference remain in use. Unknown styles fall back
without resetting session, cache, Lyrics permission or other preferences.

One installed CSS provider contains shared native component metrics,
separately scoped System and Liquid Glass resources, and shared Lyrics rules.
Only the root style class changes. `AdwStyleManager` remains responsible for
color selection and high contrast. Switching styles never recreates widgets,
MusicService, the queue, Wave, MPRIS or PlaybackController.

One non-interactive background leaf sits behind the existing measured toolbar
in a native GtkOverlay. Its input/accessibility state cannot cover controls.
The original application content still determines window and footer geometry.
System hides this leaf completely. Foreground controls and text are ordinary
GTK/Libadwaita widgets in either style.

## Rendering boundary

Liquid Glass uses an original neutral scene: an opaque base, soft cyan/violet
fields and a restrained light ribbon. GSK actually blurs that scene into a
512×384 texture at the relevant device scale. The resulting texture is retained
per light/dark and 1×/2× combination: at most four textures, about 7.5 MiB of
pixel data if every combination is visited. Resize and player/Lyrics updates
only stretch the retained texture; they do not rebuild or blur the scene.
There is no decorative timer, artwork request, screen capture, custom render
thread or new persistent cache. No third-party graphic asset is copied.

Tinted CSS surfaces, highlights, contours and shadows provide the foreground
depth. Text, buttons and covers are never submitted to the background blur.
High Contrast disables the background and uses opaque semantic surfaces and
visible outlines. The scene is static, including with Reduced Motion.

This does not blur the desktop behind a Wayland window. The input is owned by
Karin; KDE effects, other windows and desktop contents are inaccessible to this
component. Native and Flatpak use the same code and existing permissions.

## Related UX corrections

Successful timed Lyrics hides routine status text and names its provider
compactly. TEXT retains “Без синхронизации”, explicit external lookup and its
existing consent/revocation contract. Similar success shows the heading and
rows; empty/error/loading/partial states retain their specific feedback.

Repeating the footer Lyrics action returns to Track Info in the already open
panel. Back does the same; Close remains explicit and Queue retains its intent.
The footer's inner bottom margin is 24 logical pixels. Its background extends
to the window edge, while its hidden host remains exactly zero height.

## Verification

Use settings round trips, ordinary executable preferences/restart and native
input tests. Retain the strict footer, panel-motion and provider-to-karaoke
regressions. Compare baseline System, revised System and Liquid Glass with
matched source, data, scale, renderer and process conditions. Record actual
frame/wall/CPU observations and background preparation counts; a green
functional result is not physical-GPU smoothness or Fedora acceptance.
Screenshots, contrast checks and final package identities belong in
[VALIDATION](../VALIDATION.md) and PR #29. No merge or release is implied.
