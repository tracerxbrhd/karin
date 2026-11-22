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
Style changes update root classes and the existing background leaf.
`AdwStyleManager` remains responsible for color selection and high contrast.
Switching styles never recreates widgets, MusicService, the queue, Wave, MPRIS
or PlaybackController.

One non-interactive background leaf sits behind the existing measured toolbar
in a native GtkOverlay. Its input/accessibility state cannot cover controls.
The original application content still determines window and footer geometry.
System hides this leaf completely. Foreground controls and text are ordinary
GTK/Libadwaita widgets in either style.

## Rendering boundary

Liquid Glass uses an original neutral scene: an opaque base, soft cyan/violet
fields and a restrained light ribbon. GSK actually blurs that scene at a
512×384 reference size, with retained 1×/2× cache resolutions. Each light/dark
and resolution combination is prepared and read back once on a cache miss.
The texture is retained as RGB only if every pixel has alpha 255, preserving
its blurred color values and exposing its opacity to GSK. Otherwise the
original texture and alpha are retained; no flattening color or forced opacity
replaces them.

The four cache slots retain 5.625 MiB of RGB pixel data when all combinations
are visited, or at most a nominal 7.5 MiB of RGBA8 fallback pixel data. These
figures exclude temporary preparation/readback buffers and renderer/driver
allocations; they are not RSS or GPU-memory limits.

At an actual native surface scale of exactly 1 or 2, only this background
bitmap uses device-pixel coordinates and nearest sampling. The inverse scale
inside its snapshot preserves the logical bounds and avoids an extra HiDPI
offscreen sampling pass. Other scales retain the original linear path; the
fractional-scale optimization is not claimed. Foreground widgets keep normal
GTK logical coordinates and rendering. Resize and player/Lyrics updates reuse
the retained texture; they do not prepare, read back or blur it again.
There is no decorative timer, artwork request, screen capture, custom render
thread or new persistent cache. No third-party graphic asset is copied.

The right pane and player use opaque matte colors and thin native edges,
without full-surface gradients or outer blurred shadows. The header, Home
hero and original backdrop retain the glass treatment, highlights and depth.
System uses opaque computed native colors and boundaries for its pane/player.
Text, buttons and covers are never submitted to the background blur.
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
