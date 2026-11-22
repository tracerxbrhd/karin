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

These working-tree GTK strips use original synthetic metadata and text. Their
composition preserves the captured pixels without resampling and adds phase
labels only in the surrounding margins. The fixture captures and composition
are included under the [repository license](../../LICENSE).

- The 22 November 2026 (Samara) search strip was captured under Xvfb with
  GTK 4.18.6, Libadwaita 1.7.6, Cairo and scale 2. A 1920×1080 logical window
  uses two native GtkStack pages occupying the same 322×36 logical-pixel slot.
  The four 644×72 physical-pixel crops show closed, opening, open and closing
  states. During the native crossfade the two pages overlap in that single
  slot; they do not slide into adjacent regions. The crop omits the neighboring
  toggle. Its unchanged 34×34 logical-pixel bounds and the 362-pixel group's
  allocation are verified separately in the fixture's geometry JSON.
  [Global search](v08-global-search.png) ·
  [Layout and input fixture](../../src/ui/music/search_tests/header.rs) ·
  [Painted geometry fixture](../../src/ui/music/search_tests/geometry.rs).
- The 22 November 2026 (Samara) provider strip comes from the ordinary debug
  executable under its explicit local application fixture, GTK 4.18.6,
  Libadwaita 1.7.6, Cairo, Light and scale 1. It places two 320×702
  physical-pixel crops from the same right pane side by side. The first shows
  Yandex TEXT and the synchronized-text action. The second follows explicit
  native consent and the real MusicService LRCLIB request to the local fixture;
  it shows LRC applied at the existing player's position. The run also passed
  remembered consent after a process restart. Its executable SHA-256 was
  `ee660070126be53d2a952b7ea8809d476f0cdf38f17c292287ae067a55880e30`.
  This is an uncommitted-source development receipt, with a synthetic account,
  original text/audio and loopback HTTP. Its embedded base commit identifies
  `0bfc68a`; final exact-commit CI is separate. It is not Fedora or live-account
  acceptance. [TEXT to synchronized view](v08-lyrics-provider-flow.png) ·
  [Ordinary executable driver](../../scripts/test-application-fixture.py).
- The 20 November 2026 (Samara) lyrics strip was captured under Xvfb with
  GTK 4.18.6, Libadwaita 1.7.6, Cairo and scale 2. During local GStreamer
  playback of the original test signal and LRC, “Журавли” shrinks and “Фонари”
  grows while the third line stays still. The three crops show the same pixels
  of the pinned panel across the handoff, with the intermediate frame selected
  by painted width. In this static-font run, GTK draws the Pango glyphs at
  subpixel offsets and applies the theme foreground once. Variable fonts use
  the `wght` axis. The playback fixture executable had SHA-256
  `ffd99c3a1ea4ae62f931a458992323ec9471e3a4e0dce236607df9906d1b7bc7`.
  This earlier renderer image does not prove the current ordinary executable's
  provider/consent path. [Synchronized lyrics](v08-karaoke.png) ·
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

## v0.8 System and Liquid Glass

[Light comparison](v08-system-liquid-glass-light.png) ·
[Dark comparison](v08-system-liquid-glass-dark.png)

Each image compares the same Home and pinned Track Info content in System
(left) and Liquid Glass (right). The four complete 1440×900 native GTK captures
are reduced to 480×300 thumbnails with labels; the application pixels are not
reconstructed. The originals are retained by
[`native_theme_matched_visual_surfaces`](../../src/ui/music/design_tests/theme_review.rs),
which captures all seven music states at five window sizes in both styles and
color schemes. Stable gallery captures pause the existing GStreamer player and
use Reduced Motion. Separate native tests observe actual synchronized playback
and ordinary animation frames.

Captured on 22 November 2026 (UTC), GTK 4.18.6, Libadwaita 1.7.6, scale 1,
GskGLRenderer under Xvfb, using the original local catalog, artwork, audio and
lyrics fixtures. This is a precommit development build based on `d345fec`, with
modified sources; its native test ELF SHA-256 is
`d0256777225897a9cdacda0a8ac86e4625f4875ee60d13727e8309b084c62461`.
The matching 140-case run passed native layout, lifecycle and paired painted-text
contrast checks. It does not establish physical GPU performance or new Fedora,
installed Flatpak or live Yandex-account acceptance. Exact committed-package
verification is recorded separately in [validation](../VALIDATION.md).

SHA-256:

```text
b9e103b5a87041cf9f7f3e3b7c0291ce50860fd92aa9d41df36def9a7e24bbd1  docs/screenshots/v08-system-liquid-glass-light.png
6272f48a7abb2da687535b7fdb91ebef9ae0403db6a12b994ae50a535d64c3e5  docs/screenshots/v08-system-liquid-glass-dark.png

```
