<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Application screenshots

The first three images are unchanged GTK captures from the owner-accepted
[`97093ee`](https://github.com/tracerxbrhd/karin/commit/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d)
application. They were captured on 2 November 2026 (Europe/Samara) under Xvfb
with GTK 4.18.6, Libadwaita 1.7.6 and scale 1. Selecting them for the current
documentation did not change the application source or generate new captures.

All library metadata is synthetic and the striped covers are original test
artwork. No personal account, private desktop screenshot or service music is
included. The images illustrate the UI; they do not establish hardware or
cold-animation performance. [Validation](../VALIDATION.md) records that scope.

| Image | Pixels | Theme | Original in Git |
| --- | --- | --- | --- |
| [Liked tracks](liked.png) | 850 × 750 | Light | [Source](https://github.com/tracerxbrhd/karin/blob/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d/docs/design/v0.4/evidence-final-refinement/current/static-scale1/matrix-reference-panel-closed-scale1-light.png) |
| [Playlists](playlists.png) | 1440 × 900 | Light | [Source](https://github.com/tracerxbrhd/karin/blob/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d/docs/design/v0.4/evidence-final-refinement/current/static-scale1/surface-playlists-tab-wide-scale1-light.png) |
| [Track information](track-info.png) | 1440 × 900 | Dark | [Source](https://github.com/tracerxbrhd/karin/blob/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d/docs/design/v0.4/evidence-final-refinement/current/static-scale1/matrix-wide-panel-track-scale1-dark.png) |

SHA-256, in the same order:

```text
491dd30c6a432eab6975024605cf529cea482da4ed1fc8b77ca981d426f24a85  liked.png
27012b0c5bd61f7e9006b14d8b2262e308c086674b8206876aeedf3979884812  playlists.png
732489ecfbd7f38d7af605450ff2e5a5a88a9d72fc016fa6119ed427cf399bc1  track-info.png
```

## PR #29 search and synchronized lyrics

These 20 November 2026 (Samara) working-tree captures use GTK 4.18.6,
Libadwaita 1.7.6, Cairo and Xvfb at 2×. The strips preserve the native pixels
and add phase labels in the surrounding margins. Original synthetic GTK
fixture captures and their composition are included under the
[repository license](../../LICENSE).

- On a 1920×1080 logical window, the same crop follows the three tabs, the
  revealing entry and the fully open field. The window's 322-pixel tab slot
  and 34-pixel toggle keep their logical bounds throughout.
  [Global search](v08-global-search.png) ·
  [Layout and input fixture](../../src/ui/music/search_tests/header.rs).
- During local GStreamer playback of the original test signal and LRC,
  “Журавли” shrinks and “Фонари” grows while the third line stays still.
  The three crops show the same pixels of the pinned panel across the
  handoff, with the intermediate frame selected by painted width. In this
  static-font run, GTK draws the Pango glyphs at subpixel offsets and applies
  the theme foreground once. Variable fonts use the `wght` axis.
  The playback fixture executable had SHA-256
  `ffd99c3a1ea4ae62f931a458992323ec9471e3a4e0dce236607df9906d1b7bc7`.
  [Synchronized lyrics](v08-karaoke.png) ·
  [Player fixture](../../src/ui/music/track_extras/tests/playing_frames.rs).

## My Wave Home preview

[Light](wave-home-light.png) and [dark](wave-home-dark.png) are new, unedited
850×750 GTK captures from the v0.5 production-integration working tree on
4 November 2026 (UTC), using GTK 4.18.6, Libadwaita 1.7.6, Xvfb and scale 1.
They show the synthetic Playing presentation state of the native Home fixture,
not a real provider session, an audible track or Fedora acceptance. No personal
account/library or private desktop is present. The static wave motif is original
GTK decoration. The same fixture also exercises actual mouse/keyboard activation,
all states, four window sizes, maximized, both themes and scales.

The complete test executable had SHA-256
`a29799a6b161110260b88e52a28cce63e48d2de31fb9ad9839f3186d0550c1ef`;
the captures precede subsequent lifecycle-test-only adjustments. The three
Home/style source files match their compiled receipt. See
[validation](../VALIDATION.md#production-integration-revision) for scope.

SHA-256:

```text
73401e1e9e0ff873609f9c6d97380b3864d40b5075cd53d710f4c0680a429b7f  wave-home-light.png
6a1a6d9301d4f8c188718c8197f0ba5e4bcccc9437b2ab168876b28c05664a9a  wave-home-dark.png
```
