<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Cloud UI evidence

Current **v0.1 Gate A** screenshots and playback states are in
[the Gate A gallery](gate-a/README.md). They replace the following historical
images as evidence for the current application.

## v0.0 UI/licensing baseline

Captured on 2026-10-24 from the UI/licensing correction in PR #2. These are real,
unretouched application-window screenshots in isolated Xvfb/Openbox sessions,
not user desktop captures. No account, token, public API request or physical
audio output was used in the capture matrix.

Debian 13, GTK 4.18.6, Libadwaita 1.7.6, Xvfb 21.1.16, Openbox 3.6.1,
1920×1080 at 96 dpi. Light/dark are Libadwaita's native themes. The capture driver
uses Cairo with `GSK_DEBUG=full-redraw` to avoid incremental X11/Cairo residue,
and reads the visible application rectangle with `import -screen -window`.
It does not modify the application's production rendering settings or retouch
PNG content. The window manager's maximized-state atoms were checked for the
maximized cases.

The [manifest](capture-manifest.json) records measured dimensions, binary/source
fingerprints and SHA-256 values for the 22 matrix/keyboard frames. The source
fingerprints identify the reviewed working tree rather than implying the older
base commit already contained this layout. The separate About/Legal image checks
the unchanged license dialog configuration in the same review.

## Inspected matrix

| Size / state | Light overview | Light diagnostics | Dark overview | Dark diagnostics |
| --- | --- | --- | --- | --- |
| 420×720 | [PNG](light/compact-overview.png) | [PNG](light/compact-diagnostics.png) | [PNG](dark/compact-overview.png) | [PNG](dark/compact-diagnostics.png) |
| 900×650 | [PNG](light/medium-overview.png) | [PNG](light/medium-diagnostics.png) | [PNG](dark/medium-overview.png) | [PNG](dark/medium-diagnostics.png) |
| 1440×900 | [PNG](light/wide-overview.png) | [PNG](light/wide-diagnostics.png) | [PNG](dark/wide-overview.png) | [PNG](dark/wide-diagnostics.png) |
| WM-maximized, 1920×1080 | [PNG](light/maximized-overview.png) | [PNG](light/maximized-diagnostics.png) | [PNG](dark/maximized-overview.png) | [PNG](dark/maximized-diagnostics.png) |

The overview is top-aligned, with bounded connection content and shortcuts to
implemented diagnostics. At widths up to 600 logical pixels, navigation is below
the content; wider windows use the header switcher. Content becomes a single
column at widths up to 850 logical pixels. No custom application CSS is used.

Keyboard checks used Alt+1/Alt+2, Tab/Shift+Tab, Enter/Space and Ctrl+Q. Enter
started the real silent GStreamer test; its EOS result is visible in the
[light](light/keyboard-activation.png) and [dark](dark/keyboard-activation.png)
activation frames. Focus is visible in the separate
[light](light/keyboard-focus-diagnostics.png) and
[dark](dark/keyboard-focus-diagnostics.png) frames.

The compact diagnostics page intentionally scrolls. Its lower library card is
reachable by keyboard, as shown in the
[light](light/compact-diagnostics-bottom.png) and
[dark](dark/compact-diagnostics-bottom.png) frames. Content above the viewport
is intentionally clipped during scrolling; the fixed HeaderBar and navigation
remain intact. The audio result survives resizing from medium to compact.

The native [About → Legal page](about-license.png) displays **GNU General Public
License, version 3 or later**.

Reproduce with `scripts/capture-ui.sh build/ui-captures` after building and
installing the tools listed in [the quality gate](../UI_QUALITY_GATE.md). CI
retains a newly generated matrix as a per-revision artifact for seven days.
Review new frames rather than reusing these images as evidence for later code.

These cloud checks do **not** establish repeated visual acceptance on Fedora 44 /
KDE Plasma / Wayland / PipeWire. That gate remains pending, especially the
previously reported top-edge title/window-control problem, HiDPI and live resize.
Third-party theme icons and fonts retain their own licenses; Karin's license does
not relicense those components or the user's private screenshots.
