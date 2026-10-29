<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Native sign-in correction — 2026-10-29

These are actual GTK renders from Debian 13 / GTK 4.18.6 / Libadwaita 1.7.6,
Xvfb/Openbox 3.6.1 at 96 dpi, Cairo/full-redraw. No real authorization input,
Yandex account, provider page or private desktop is captured.

**Public sign-in is blocked by the missing permitted Music provider.** The
dialog explains that fact; it is not a successful Yandex login or a disguised
ID-only flow. The HTTP 403 image is an explicitly synthetic loopback fixture,
not a new live provider result. No credentials/code/token fields are present.

| Selected frame | Source |
| --- | --- |
| [Compact login, light](compact-login-light.png) | Real public blocked dialog at 420×720 |
| [Medium login, dark](medium-login-dark.png) | Real public blocked dialog at 900×650 |
| [Wide diagnostics](wide-diagnostics-light.png) | Public 1440×900 view with corrected provider-block explanation |
| [Music permission denial](music-permission-denied-dark.png) | Native dialog after synthetic HTTP 403; fixture header is visible |
| [Library](library-light.png) | Actual MusicService and GTK callbacks against synthetic HTTP |
| [Search](search-dark.png) | Synthetic returned results, native focus and queue controls |

The full local review contains **62 PNGs**: 50 public frames and 12 explicitly
synthetic fixture frames. Both themes cover 420×720, 900×650, 1440×900 and actual
EWMH maximization to 1920×1080, including sign-in dialogs, keyboard actions,
scrolling and silent diagnostic states. The fixture also covers library/search/
queue/album, public provider blocking and Music-account denial.

[Capture metadata](capture-info.txt) records the dirty working tree over
`6ecfe75` and the actual binary/source hashes. That base is not misidentified as
the rendered implementation. Hosted CI captures the final source again; its
exact-revision result and artifact are linked from PR #8. Only selected clean
images are retained in the repository. Previous [music UI captures](../v0.1-music/README.md)
remain historical evidence.

Reproduce public captures with:

```sh
cargo +1.98.1 build --locked --no-default-features
scripts/capture-ui.sh build/ui-captures
```

Run the populated fixture separately with `KARIN_TEST_CAPTURE_DIR` as described
in [BUILDING.md](../../BUILDING.md). All HTTP is loopback and no synthetic
catalog/credential is included in normal builds. Frame inspection supplements
GTK allocation, callback, cancellation, logout and resource-release assertions.

Xvfb, integer GDK scaling and these synthetic fixtures do not establish Fedora/
KDE/Wayland, fractional HiDPI, live Yandex authorization, Music access, browser
policy approval or physical Music playback. See [VALIDATION.md](../../VALIDATION.md)
and the [provider investigation](../../AUTHENTICATION_REVIEW.md).
