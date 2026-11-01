<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Reviewed native redesign captures

These are **36 untouched actual GTK captures**, with original synthetic metadata
and generated cover art, licensed GPL-3.0-or-later. No real account, service art,
credential, signed resource, private desktop or music recording is included.
The [previous gallery](../evidence/README.md) remains historical.
The retained C10 set below does not establish cold first-use performance.
See the newer [explicitly initialized normal-motion evidence](motion/README.md)
for recorded setup, fresh-cache measurements and the remaining cold-opening gate.

Before is unchanged production source `a9eed4514802df84d73b0608dd1a62930fc1af76`,
with an archived test-only size/motion adapter. Queue is honestly shown in its
old central route. After is the final C10 source-frozen implementation: the exact
Rust/Cargo hashes and both copied executable hashes are in
[provenance.json](provenance.json). Hosted evidence records its own commit and
executable; a rebuild need not have the same executable hash.

Environment: GTK 4.18.6, Libadwaita 1.7.6, Xvfb/Openbox and NGL
(`GskGLRenderer`) on Mesa **software** rendering, not physical GPU or Wayland.
The full local set has 96 matched matrix cases, 84 current state supplements and
240 normal-motion frames. Every paired matrix has the same physical dimensions;
nonmaximized client allocations are 10 logical pixels smaller than the outer
window. 1×/2×, light/dark and actual maximize are recorded separately.

## Same-size before and after

| View | Before | After |
| --- | --- | --- |
| Home, 1440×900 | ![Before Home](before-home-dark.png) | ![After Home](after-home-dark.png) |
| Owner-wide information, 1650×930 | ![Before wide information](before-owner-wide-track-dark.png) | ![After wide information](after-owner-wide-track-dark.png) |
| Queue, 900×650 | ![Before central Queue](before-medium-queue-dark.png) | ![After right Queue](after-medium-queue-dark.png) |
| Compact information, 420×720 | ![Before compact](before-compact-track-light.png) | ![After compact](after-compact-track-light.png) |
| Light information, 1440×900 | ![Before light](before-wide-track-light.png) | ![After light](after-wide-track-light.png) |
| Closed panel, 900×650 | ![Before library](before-medium-closed-dark.png) | ![After library](after-medium-closed-dark.png) |

## Actual whole-surface motion

Five **different rendered frame times and indices** from a normal 240 ms native
animation. Labels denote approximate **openness**, not elapsed time: easing makes
these different. [motion-geometry.json](motion-geometry.json) records actual
progress, mapped children and fixed surface-relative allocations. Closing must
be viewed in the reverse order, 100 → 75 → 50 → 25 → 0.

| Opening 0% | ≈25% | ≈50% | ≈75% | 100% |
| --- | --- | --- | --- | --- |
| ![Open 0](animation-wide-dark-track-opening-000-scale1.png) | ![Open 25](animation-wide-dark-track-opening-025-scale1.png) | ![Open 50](animation-wide-dark-track-opening-050-scale1.png) | ![Open 75](animation-wide-dark-track-opening-075-scale1.png) | ![Open 100](animation-wide-dark-track-opening-100-scale1.png) |

| Closing 100% | ≈75% | ≈50% | ≈25% | 0% |
| --- | --- | --- | --- | --- |
| ![Close 100](animation-wide-dark-track-closing-100-scale1.png) | ![Close 75](animation-wide-dark-track-closing-075-scale1.png) | ![Close 50](animation-wide-dark-track-closing-050-scale1.png) | ![Close 25](animation-wide-dark-track-closing-025-scale1.png) | ![Close 0](animation-wide-dark-track-closing-000-scale1.png) |

| Same outer Track Info surface | Same outer Queue surface |
| --- | --- |
| ![Track Info](animation-wide-dark-track-opening-100-scale1.png) | ![Queue](animation-wide-dark-queue-opening-100-scale1.png) |

The actual-input fixture additionally asserts that switching these modes while
open keeps progress at 1 and preserves the same surface allocation and header.

For comparison, the old observer labels **elapsed fractions**, not new-panel
openness. Its production surface stayed in place while its allocation reflowed:

| Old start | Old 25% time | Old 50% time | Old 75% time | Old end |
| --- | --- | --- | --- | --- |
| ![Old start](before-animation-opening-000-scale1.png) | ![Old quarter](before-animation-opening-025-scale1.png) | ![Old half](before-animation-opening-050-scale1.png) | ![Old three quarters](before-animation-opening-075-scale1.png) | ![Old end](before-animation-opening-100-scale1.png) |

## Music pages and modes

- [Playlist](surface-playlist-scale1-dark.png) and [unified search](surface-unified-search-scale1-light.png).
- Home with [no data](surface-home-empty-scale1-dark.png), [few playlists](surface-home-few-playlists-scale1-dark.png) and a [larger loaded collection](surface-home-large-collection-scale1-dark.png).
- Active [Shuffle + Repeat One](surface-player-shuffle-repeat-one-scale1-dark.png) with a real “1”, and [Shuffle + Repeat Queue](surface-player-shuffle-repeat-queue-scale1-light.png).
- [Long compact information](surface-long-metadata-compact-information-scale1-dark.png): normal vertical scrolling remains necessary; the small footer uses ordinary metadata ellipsis.

[Quality/review summary](quality-summary.json) records final counts, geometry
residuals and the independent image-review scopes.

PNG integrity is recorded in [SHA256SUMS](SHA256SUMS). Screenshots supplement the
allocation, actual-input, worker, queue, D-Bus and cleanup assertions; they do not
replace them. **Fedora/KDE/Wayland, real Music playback and the current Flatpak
still require the owner's new acceptance.**
