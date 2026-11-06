<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Unified cache preferences

These are actual Xvfb captures of the native Rust/GTK/Libadwaita fixture on
2026-11-06. All values are synthetic. They contain no account, credential,
service response, downloaded Music audio or personal desktop material.
They supplement the [widget and input checks](../../../src/ui/preferences/tests.rs),
not Fedora/Wayland or installed Flatpak acceptance.

| Capture | Actual logical window | Scale | Selected value |
| --- | --- | --- | --- |
| [Default quota, light](medium-2gb-light.png) | 900×650 | 1 | 2 ГБ; enabled |
| [Unlimited, dark](medium-unlimited-dark.png) | 900×650 | 1 | Без ограничений; enabled |
| [Wide, dark](wide-dark.png) | 1440×900 | 1 | 500 МБ; disabled |
| [Compact HiDPI, light](compact-hidpi-light.png) | 420×720 | 2 | 500 МБ; disabled |

The same Preferences selector passed on stable 1.99.1 and Rust 1.98.1 at scales
1/2 with capture enabled and disabled: eight actual executions. Four capture
executions produced 112 images, including all six limits in both themes. The
persistent-session selector separately passed on both compilers at both scales:
four further executions. Inputs use actual pointer hits and Home/End/Right keys;
assertions cover snapping, accessibility role, callback counts, compact bounds,
Clear/Retry, focus and cleanup. The older clipped-switch regression uses a
separate 420×520 viewport because the shorter unified section correctly keeps
that switch visible at 420×720.

Capture runs use Cairo, full redraw and disabled animations for static inspection;
no-capture runs retain normal animations. The WebKit sandbox is not disabled.
The Preferences selector covers the three window sizes above. A separate ordinary
release check below covers actual maximization. Neither establishes a physical
KDE/Wayland result. See [BUILDING](../../BUILDING.md) for desktop acceptance.

Verified test-ELF SHA-256:

- Rust 1.98.1: `5ca18d0c92ae2ababd09fb188a63f1137643372fdf385ec7d9c512ce21a6bd0a`
- Stable: `455a6da1c0cb1882b4c23ba9a6cb452ee32678da325672cb328e46fba4ff2191`

Both binaries were freshly compiled after the final fixture changes; source hashes
and Cargo `fresh=false` records were retained with the complete execution receipts.
The images show the unified controls, while real service-audio persistence remains
disabled until its storage/reuse contract is established.

## Actual maximized ordinary release

[Maximized window, light](maximized-ordinary-light.png) shows the ordinary Rust
1.98.1 release, rather than a fixture snapshot. It ran under isolated XDG paths,
a fresh session D-Bus, Xvfb and Openbox, with no account or login attempt. Both
`_NET_WM_STATE_MAXIMIZED_VERT` and `_NET_WM_STATE_MAXIMIZED_HORZ` were confirmed
before and after opening Preferences; `xdotool`/`xwininfo` measured **1920×1080**
at **0,0**. The real Ctrl+comma accelerator opened Preferences. Actual wheel
input brought the whole cache group into view, then ImageMagick captured the X11
framebuffer. Escape and a window-manager close exited with status 0.

The release ELF SHA-256 was
`bd77319f67d3fc7260c51003641faa78a332971a7d8af5e97ebaf3d9cb4ac0e2`.
This independently executed application check is separate from the twelve Rust
native-fixture passes and does not claim new Fedora or installed Flatpak acceptance.
