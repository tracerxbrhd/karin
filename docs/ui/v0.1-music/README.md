<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# v0.1 Music UI review

These are real GTK captures from the prepared Debian 13 environment, with GTK
4.18.6, Libadwaita 1.7.6, GStreamer 1.26.2, Cairo and `GSK_DEBUG=full-redraw`.
No real account, authorization input, provider response or user desktop appears.

The reviewed matrix contains **42 application frames** in isolated Xvfb/Openbox
3.6.1 sessions and **8 native HTTP-fixture frames**. Both themes were inspected.
Application dimensions were 420×720, 900×650, 1440×900 and real WM maximization
to 1920×1080, with geometry/EWMH records. Library/search/queue/diagnostics,
keyboard focus/activation and the independent diagnostic playback states were
reviewed. The populated test window is 900×650 and explicitly labels its source.

| Selected frame | Meaning |
| --- | --- |
| [Compact public view](compact-public-light.png) | Ordinary signed-out application, light theme, 420×720 |
| [Wide diagnostics](wide-diagnostics-dark.png) | Ordinary application, dark theme, 1440×900 |
| [Library](library-light.png) | Real MusicPane against synthetic loopback HTTP, light theme |
| [Search](search-dark.png) | Synthetic HTTP results and native focus, dark theme |
| [Album](album-light.png) | Synthetic album and paged tracks, light theme |
| [Queue](queue-dark.png) | Synthetic returned metadata; playback remains explicitly unavailable |

**Fixture data is test-only, not a shipped demonstration catalog or evidence of
Yandex service acceptance.** The component test drives the actual MusicService,
GTK callbacks, paging, queue and logout/late-response behavior. Default builds
contain neither these data nor a way to paste a token. The generated WAV remains
in its separate developer diagnostics, never a substitute for a service track.

[Capture metadata](capture-info.txt) records the binary/source hashes and the
dirty working tree over `90a9799`, including the GTK 4.14 stack-width fix. It does
not misidentify that base commit as the rendered code. The source hashes identify
the reviewed UI. CI captures the exact PR head again and uploads the full matrix
as an artifact.
Only these selected, inspected frames are retained in source control.

Reproduce the public matrix using `scripts/capture-ui.sh build/ui-captures` after
building the public binary. To capture populated test components separately:

```sh
KARIN_TEST_CAPTURE_DIR="$PWD/build/ui-captures/fixtures" \
  GTK_A11Y=none GSK_RENDERER=cairo GSK_DEBUG=full-redraw \
  dbus-run-session -- xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  cargo +1.98.1 test --lib --all-features --locked \
  native_music_session_catalog_queue_and_logout -- --ignored --test-threads=1
```

The commands require Xvfb, ImageMagick and xdotool, and never contact Yandex.
Cloud review does not establish Fedora/KDE/Wayland visual acceptance, fractional
scaling, accessibility or audible Music. A separate GDK integer-scale-2 smoke
passed; real target-desktop HiDPI remains pending.
