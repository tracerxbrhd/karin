<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# v0.0 foundation validation

This report separates initial cloud checks, user-reported Fedora acceptance and
checks of the revised UI. A result for one environment or revision is not evidence
that another has passed.

## User-reported Fedora acceptance of the original foundation

The user reported the following results on Fedora 44 / KDE Plasma / Wayland /
PipeWire while reviewing PR #2. These are manual observations supplied by the
user, not a repeat Fedora session performed by the cloud agent.

| Observation | Reported result |
| --- | --- |
| GTK4 application startup | Passed |
| Anonymous API diagnostic | HTTP 200; no authorization established |
| Silent GStreamer diagnostic | Passed |
| Physical synthetic test signal | Audible |
| D-Bus and Secret Service | Detected |
| Original visual layout | **Failed manual acceptance; sent for correction** |

The supplied screenshots are review evidence only. They include desktop elements
and have not been added to the public repository. They do not constitute approval
of the revised layout.

## Revised UI acceptance status

Repeat manual visual acceptance on Fedora 44 / KDE Plasma / Wayland is
**pending**. Do not mark it passed from cloud screenshots. The required size,
theme, keyboard, focus and clipping checks are defined in
[UI_QUALITY_GATE.md](UI_QUALITY_GATE.md).

### Revised cloud structural checks

The revised GTK smoke test has passed both under raw Xvfb and under managed
Xvfb/Openbox. It exercises 420×720, 900×650 and 1440×900 allocations in both themes
and both pages, checks HeaderBar height (at least 48), header child bounds,
horizontal overflow, text ellipsis/height, adaptive navigation visibility, page
selection and preservation during resize, focus and weak-reference cleanup.
These are structural results, not
automatic approval of visual quality or of KDE window decorations.

The capture helper is `scripts/capture-ui.sh build/ui-captures`, invoked through
`/workspace/karin-environment/run-visual` in the prepared cloud image. It uses
Xvfb 21.1.16 and Openbox 3.6.1 on a 1920×1080 virtual screen, ImageMagick 7.1.1.43,
xdotool 3.20160805.1 and the native SVG loader. Normal capture sizes are exactly
420×720, 900×650 and 1440×900; the actual WM-maximized allocation is 1920×1080.
The screenshots use the clean application rectangle, with no user account or
desktop data. Binary fingerprints and geometry accompany the generated captures.
The helper forces a full Cairo redraw during capture to avoid stale X11 partial
updates after resizing or changing pages; it does not edit the captured pixels
or change the renderer used by a normal application launch.

The final clean PNGs were inspected and published in [docs/ui](ui/README.md):
22 matrix/keyboard frames and one separate About/Legal image. No user-supplied
desktop screenshots were copied into the repository. The
[capture manifest](ui/capture-manifest.json) records actual dimensions and
source/binary/image fingerprints for the matrix.

| Cloud visual case | Light and dark overview/diagnostics result |
| --- | --- |
| Compact, 420×720 | Inspected: bottom navigation, readable single-column content and fixed header; lower diagnostics reachable by scrolling |
| Medium, 900×650 | Inspected: header navigation, bounded components and readable two-column content |
| Wide, 1440×900 | Inspected: top-aligned hierarchy, bounded widths, intact title/controls and no stretched empty-state layout |
| WM-maximized, 1920×1080 | Inspected: actual window-manager maximize atoms verified; header, controls and content remain intact |

Tab and Shift+Tab produced visible focus. The sequence exercised Enter and Space;
Enter started the real silent GStreamer diagnostic and produced a visible EOS
success result. Six Tab steps in
compact diagnostics exposed the lower native-library card. The audio result
survived resizing from 900×650 to 420×720; Ctrl+Q closed the process with exit
code 0. Final images were checked for text, hierarchy, header/control clipping
and clean content rather than treating a successful capture script as signoff.
These cloud observations do not pass the pending Fedora visual gate.

### Revised source verification

The revised application source passed the following reruns in the cloud. These
are separate executions from the initial foundation baseline below.

| Check | Revised-source result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed, exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed, no warnings, exit 0 |
| `cargo test --workspace --locked` | **23 passed, 0 failed, 0 ignored** |
| `cargo build --workspace --locked` | Passed, exit 0 |
| `./target/debug/karin --audio-self-test` | Passed: synthetic GStreamer pipeline reached EOS through `fakesink`, exit 0 |
| Revised GTK smoke test | Passed: both pages/themes at three sizes, header/title/menu bounds, wrapping/ellipsis/overflow, focus and weak-reference cleanup, exit 0 |
| Dependency-license inventory regeneration and comparison | Passed: regenerated reviewed inventory matches, exit 0; this is not final-package legal clearance |
| Flatpak manifest parsing | Passed, exit 0; full sandboxed package build remains blocked |
| Final Flatpak source preparation | Passed, exit 0, including locked source preparation and notice generation; not a package build |
| GitHub Actions for the revised source | The authoritative per-commit result is in [PR #2 checks](https://github.com/tracerxbrhd/karin/pull/2/checks); local results above do not imply a hosted-runner outcome |

The native [About → Legal screenshot](ui/about-license.png) was inspected and
shows **GNU General Public License, version 3 or later**. This agrees with GTK's
configured license type and the approved source/metadata notices.

## Initial cloud baseline

The following baseline checks were executed on 2026-10-24 at foundation commit
`dc82909`, using `/workspace/karin-environment/run` from the repository root.

### Environment

Debian 13 x86_64; Rust/Cargo 1.99.1 stable; GTK 4.18.6; Libadwaita 1.7.6;
GStreamer 1.26.2; GCC 14.2.0; pkg-config 1.8.1. Development/test profiles omit
debug information to reduce the cost of compiling gtk-rs in cloud runners.
Release builds use Cargo's normal release profile.

### Executed checks

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed, no warnings |
| `cargo test --workspace --locked` | **23 passed, 0 failed, 0 ignored**; binary and documentation targets contain 0 tests |
| `cargo build --workspace --locked` | Passed |
| `./target/debug/karin --audio-self-test` | Passed: 40 synthetic buffers through `fakesink`, EOS observed, pipeline reset to `Null` |
| `GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- xvfb-run -a ./target/debug/karin --smoke-test` | Passed: real window realized, diagnostics navigation selected, window/key widgets released after close |
| `./target/debug/karin --probe-service` | Real anonymous HTTP 200, `Anonymous`, `service_available: Some(false)`; this is not login or playback |
| `python3 packaging/flatpak/prepare-source.py` | Passed: locked Cargo source snapshot prepared outside tracked source in ignored `build/flatpak-source` |
| `flatpak-builder --show-manifest packaging/flatpak/io.github.tracerxbrhd.Karin.json` | Passed: manifest parsed; no package build implied |
| Bubblewrap namespace capability check | Blocked: kernel/container denied namespace creation; no sandbox bypass attempted |

Tests cover domain transitions and retry/duplicate behavior, application-service
boundaries, actual local HTTP responses (401/403/451/503, redirects, malformed and
oversized bodies, disconnect, timeout and a slow-drip total deadline), UI state
presentation, application identifiers, and GStreamer EOS/error/timeout cleanup.
HTTP fixtures are test-only and use loopback; no real API, account credentials or
sound device is required by the automated tests.

The GTK smoke check explicitly observes weak references after closing the real
window, guarding against permanent widget reference cycles in signal handlers.
It does not replace target-desktop accessibility or rendering acceptance.

### Acceptance mapping

| Criterion | Evidence |
| --- | --- |
| Real Rust project and modular boundaries | Single Cargo package, committed lockfile, modules described in [architecture](ARCHITECTURE.md) |
| Native GTK4/Libadwaita shell builds and runs | Successful build and Xvfb smoke check |
| Isolated API adapter and real vertical slice | UI action → `ConnectionService` → `MusicApi`/`YandexApi` → `ConnectionReport` → UI; same service exercised by the live CLI probe |
| Independent audio scenario | UI worker action and CLI self-test use the same finite GStreamer diagnostic |
| Loading, unauthorized/unavailable and error states | Typed state transitions and presentation tests; UI permits retry |
| Honest service capabilities | [Feasibility matrix](API_FEASIBILITY.md) distinguishes observation, documentation and blockers |
| Reproducible dependency/build workflow | Cargo.lock, `--locked` commands, CI definition and vendored offline Flatpak source preparation |
| CI | GitHub Actions workflow provided; hosted-runner outcome must be read from the PR checks, not inferred from local results |

## Unrun/manual acceptance

Full Flatpak package build/install, repeat visual acceptance of the revised UI on
Fedora 44/KDE/Wayland, HiDPI and accessibility acceptance remain unverified. The
user-reported functional and audible-output observations above remain distinct
from those pending checks. No OAuth flow,
subscribed account, music stream, personal library, My Wave, Secret Service
credential lifecycle or MPRIS playback integration was exercised. See
[limitations](LIMITATIONS.md) and [build instructions](BUILDING.md).
