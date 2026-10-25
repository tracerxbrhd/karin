<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Gate A — inspected cloud UI and local playback

Captured and inspected on **2026-10-25** from the Gate A source committed with
this evidence. These 26 PNGs are unretouched application-window captures from
isolated Xvfb/Openbox sessions. No personal desktop, account, credentials,
Yandex API call or physical audio output is present.

Debian 13; Rust/Cargo 1.98.1; GTK 4.18.6; Libadwaita 1.7.6; GStreamer 1.26.2;
Xvfb 21.1.16; Openbox 3.6.1; 1920×1080 at 96 dpi. Native light/dark themes and
Cairo/full-redraw are used by the capture driver only. The
[manifest](capture-manifest.json) records actual dimensions, real maximize atoms,
all Rust source fingerprints, rendered binary SHA256 and each PNG's SHA256.
Its base commit identifies the starting point; the dirty Gate A working tree
was built before these files were committed.

## Matrix

| Size/state | Light overview | Light diagnostics | Dark overview | Dark diagnostics |
| --- | --- | --- | --- | --- |
| 420×720 | [PNG](light/compact-overview.png) | [PNG](light/compact-diagnostics.png) | [PNG](dark/compact-overview.png) | [PNG](dark/compact-diagnostics.png) |
| 900×650 | [PNG](light/medium-overview.png) | [PNG](light/medium-diagnostics.png) | [PNG](dark/medium-overview.png) | [PNG](dark/medium-diagnostics.png) |
| 1440×900 | [PNG](light/wide-overview.png) | [PNG](light/wide-diagnostics.png) | [PNG](dark/wide-overview.png) | [PNG](dark/wide-diagnostics.png) |
| WM-maximized, 1920×1080 | [PNG](light/maximized-overview.png) | [PNG](light/maximized-diagnostics.png) | [PNG](dark/maximized-overview.png) | [PNG](dark/maximized-diagnostics.png) |

The new player is explicitly developer diagnostics with generated local WAV;
the Music-specific authorization blocker is visible. Content stays within the
compact width. Both themes retain readable text, native focus and intact
HeaderBar/title/window controls. Wide content is bounded and aligned near the
top. Lower compact diagnostics intentionally scroll; that viewport clipping
does not hide window chrome or remove implemented actions.

## Real transport and focus

Alt+1/Alt+2 select the actual pages. Enter prepares a temporary eight-second WAV
with silent output. Tab/Shift+Tab move focus; Enter starts the same `playbin`
controller used by tests; Space pauses it; Tab/Enter stops it. The images show
the actual status, transport labels, seek position and enabled/disabled controls.

| Observed state | Light | Dark |
| --- | --- | --- |
| Keyboard focus on the ready Play action | [PNG](light/keyboard-focus-diagnostics.png) | [PNG](dark/keyboard-focus-diagnostics.png) |
| Playing, Pause action and advancing position | [PNG](light/keyboard-activation.png) | [PNG](dark/keyboard-activation.png) |
| Paused, Resume action and retained position | [PNG](light/playback-paused.png) | [PNG](dark/playback-paused.png) |
| Stopped, position reset and Stop disabled | [PNG](light/playback-stopped.png) | [PNG](dark/playback-stopped.png) |
| Compact lower diagnostics/native-library card | [PNG](light/compact-diagnostics-bottom.png) | [PNG](dark/compact-diagnostics-bottom.png) |

Keyboard navigation reaches the desktop controls. Wheel scrolling then exposes
the final read-only native-library card; keyboard transport/focus checks are
separate from that scroll. Ctrl+Q closes the process successfully. No audible
test button is activated during capture.

The raw compact header's first 52 pixel rows match the corresponding overview
header in both themes. Some image previews omit repeated header content; the
original PNG pixels and GTK allocation checks confirm intact titles/controls.

Reproduce with `scripts/capture-ui.sh build/ui-captures` after building. See
[the quality gate](../../UI_QUALITY_GATE.md) and
[validation](../../VALIDATION.md). The GTK smoke test independently checks
geometry, navigation, actual local playback, close during playback, widget
release and worker completion; screenshots do not replace those assertions.

These cloud results do **not** establish Fedora/KDE/Wayland/PipeWire acceptance,
physical audibility, HiDPI or accessibility approval. Those Gate A checks remain
manual. Native theme icons/fonts retain their third-party licenses; Karin's
license does not relicense those components.
