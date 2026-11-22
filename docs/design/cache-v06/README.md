<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Unified cache preferences

These curated captures illustrate the shared cache controls introduced in v0.6.
They use synthetic settings, without accounts, credentials, provider content or
private desktop material. They are historical examples, not current v0.8 UI or
physical Fedora/Wayland acceptance.

| Capture | Logical window | Scale | Selected value |
| --- | --- | --- | --- |
| [Default quota, light](medium-2gb-light.png) | 900×650 | 1 | 2 ГБ, enabled |
| [Unlimited, dark](medium-unlimited-dark.png) | 900×650 | 1 | Unlimited, enabled |
| [Wide, dark](wide-dark.png) | 1440×900 | 1 | 500 МБ, disabled |
| [Compact HiDPI, light](compact-hidpi-light.png) | 420×720 | 2 | 500 МБ, disabled |
| [Ordinary maximized release](maximized-ordinary-light.png) | 1920×1080 | 1 | Signed out, Preferences open |

The first four are 6 November 2026 Xvfb/Cairo captures from native GTK input
fixtures. The fifth is a separately maximized ordinary release under isolated
XDG, D-Bus, Xvfb and Openbox. The [original immutable record](https://github.com/tracerxbrhd/karin/blob/42f811be955247efde93bcc2ef6df1f700cc856a/docs/design/cache-v06/README.md)
retains capture conditions, measured geometry and executable hashes.
Current regression assertions live in [Preferences tests](../../../src/ui/preferences/tests.rs);
full generated matrices belong in CI artifacts. [VALIDATION](../../VALIDATION.md)
records accepted real audio-cache functionality separately.
