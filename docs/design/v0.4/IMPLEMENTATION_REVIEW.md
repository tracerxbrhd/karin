<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Native Music implementation review

This records actual GTK/Xvfb/Openbox review for
[Draft PR #15](https://github.com/tracerxbrhd/karin/pull/15), separately from the
[approved SVG designs](README.md) and owner-local Fedora acceptance. Captures
use isolated D-Bus, Cairo/full-redraw, synthetic MusicService metadata and
original fixture images decoded over loopback HTTP. They contain no real
account, service artwork, credential or music recording.

## Source and capture provenance

The accepted production baseline is
`5e85e8d21a0e7075787d1242ed5abae0166f000c`. Its archived production source was
rebuilt with the same isolated evidence harness used for current captures;
the baseline UI was not modified. The preserved local baseline test executable
has SHA-256:

```text
def3af020aa5609585a80b6deed0f1650e4ffa83df3f79ed8fc636756aee4a1d
```

The baseline and current **32-case matrices** were directly inspected at both
scales: 64 matrix PNGs, plus 40 populated/state supplements per revision. Every
matched pair has equal outer dimensions: 420×720, 900×650, 1440×900 and actual
1920×1080 maximize at 1×; twice those pixel sizes at 2×. GTK reports the same
logical geometry at both scales. Rows are 64 logical pixels; closed-pane views
show seven complete rows compact, six medium and all eight fixture rows wide
and maximized.

The preserved current capture executable has SHA-256
`7e4c827ec647ea51eb612185358084e6a3ece78a4d4d6d7760735e1f1d578998`.
Hosted artifacts record their own immutable source/executable fingerprints;
a newly built binary need not have this local test executable's hash.

The reproducible baseline helper was also executed with the final shared
harness: all 32 fresh baseline matrix images were byte-identical to the
reviewed baseline above. It forces a fresh archive compilation to prevent
same-package Cargo-output reuse from becoming false current evidence.

The final information-action corrections were followed by fresh captures of
all 32 current matrix cases and 40 supplements. These were compared with the
reviewed preceding captures; differences were limited to the native hover row
and blinking insertion caret, with no layout/artwork/metadata change.

## Inspected composition and corrections

| Area | Cloud review |
| --- | --- |
| Native shell and content | Current 1×/2× frames pass the cloud composition review: clear compact navigation, quieter song rows, actual cover hierarchy and persistent transport/timeline |
| Medium information and central actions | An actual input test exposed an overlaid pane hiding a played row's More action. Both desktop widths now reserve space: wide 320+20 pixels; medium 300+16. Corrected 1×/2× frames keep the music actions reachable |
| Collection header and fold | Header/action grouping was corrected to avoid folding controls over one another; current 1×/2× frames show the intended hierarchy |
| Density | Six complete 64-pixel song rows remain visible at the medium target with information closed; measured GTK content is 890×640 within the approximately 900×650 captured window |
| Compact information | Dedicated full-width information retains explicit Back/close and the permanent Player Bar; underlying music is hidden, without a dismissing scrim |
| Populated/state supplements | Home, playlist tiles, playlist/album detail, unified search, queue, Preferences, search failure/Retry, keyboard focus and empty search were inspected in both themes at 1×/2× |

The five original SVGs remain unchanged as design inputs. The screenshots use
real widgets and themed colors; synthetic striped covers demonstrate image
placement, not real service art. Composition review does not certify contrast
or readability for every accessibility setting or very large personal library.
Native pointer/keyboard tests additionally check 720/900-width central actions,
compact focus and weak-reference cleanup. Real repeated current-cover clicks
keep information open, return from explicit selection to current metadata and
preserve queue, worker generation, playback serial and resolver requests.
Explicit metadata remains reopenable through Ctrl+I before any track has played;
closing it returns focus to actual main-page controls when the footer is disabled.
Generated-media audio/EOS/MPRIS-follow and full-scale regressions are separately
recorded in [VALIDATION.md](../../VALIDATION.md).

Long two-line metadata can put the action row just below the initial medium
pane fold; the pane retains ordinary vertical scrolling. The additional
no-current/explicit-selection screenshots show this nonblocking case. Fedora
review should include wheel/keyboard access to these actions with actual fonts
and long personal titles, rather than treating below-fold content as truncation.

## Matched actual GTK examples

Untouched, reviewed cloud captures with information open; all names and covers
are synthetic. Click an image to inspect its original size. See the
[capture provenance](evidence/README.md) and the complete CI matrix for both scales.

| Case | Before | Native Music |
| --- | --- | --- |
| Compact, dark | ![Before compact](evidence/before-compact-dark.png) | ![Native Music compact](evidence/after-compact-dark.png) |
| Medium, dark | ![Before medium](evidence/before-medium-dark.png) | ![Native Music medium](evidence/after-medium-dark.png) |
| Wide, dark | ![Before wide dark](evidence/before-wide-dark.png) | ![Native Music wide dark](evidence/after-wide-dark.png) |
| Wide, light | ![Before wide light](evidence/before-wide-light.png) | ![Native Music wide light](evidence/after-wide-light.png) |

## Reproduction and remaining gates

The [quality gate](../../UI_QUALITY_GATE.md#v04-native-music-gate) requires
32 matched before/after cases: four sizes × two themes × two scales × pane
open/closed, plus page/state supplements. `native_design_matched_matrix_and_populated_surfaces`
records actual allocation, scale, maximize/pane state and fully visible row
count in `native-layout-scale*.jsonl`. The current images go to the CI artifact's
`fixtures/native-music-{1,2}/`; baseline images go to `before/scale{1,2}/`.

Run [the accepted-baseline helper](../../../scripts/capture-native-music-baseline.sh)
**last among Cargo steps**, after current-source tests/builds/captures:

```sh
scripts/capture-native-music-baseline.sh build/ui-captures
```

It rebuilds the archived baseline in temporary source space, immediately copies
the executable and runs both scales. Its shared target directory can replace
same-package crate outputs, so those outputs are not reused as current evidence.
`comparison-info.txt` records source/harness/executable fingerprints and library
versions. Normal current-source builds must rebuild after this comparison.
The pinned [CI workflow](../../../.github/workflows/ci.yml) retains PNGs, geometry,
JSONL and provenance in `karin-ui-<source SHA>`.

Stable 1.99.1 and exact 1.98.1 formatting, three-configuration strict Clippy,
ordinary tests/builds, MSRV offline ordinary/all-feature release, GStreamer and
GTK smoke checks pass. All 26 locally available named native fixtures pass at
both scales, including the targeted compact-error retest. Full navigating
WebKit sandbox fixtures run in hosted CI because this container cannot create
the required nested namespaces; the sandbox is not disabled. Full GNOME SDK 50
Flatpak building is unavailable here, separately from successful offline release.

See the immutable PR checks and source-matched Fedora artifact for hosted
results. **Owner Fedora/KDE/Wayland/PipeWire visual and real-music regression
acceptance remains pending.** Earlier v0.3 acceptance and cloud review cannot
pass that gate. PR #15 remains Draft, with no automatic merge or new PR.
