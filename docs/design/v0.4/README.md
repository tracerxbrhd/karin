<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin v0.4 — Native Music visual references

**Status:** approved design direction, illustrative mockups; **not** implemented GTK screenshots and not new functionality.

Selected by the project owner: **B — Native Music**, based on the previously agreed composition. The priority is a beautiful, compact, understandable native player with **minimal visible text**.

## Design references

### Wide (1440 × 900) — dark and light

![Wide dark Native Music concept](desktop-wide-dark.svg)

![Wide light Native Music concept](desktop-wide-light.svg)

### Medium (900 × 650), right pane intentionally stays open

![Medium window with persistent nonmodal track information](medium-pinned-pane-dark.svg)

### Compact (420 × 720), information remains an explicit view

![Compact track-information presentation](compact-track-pane-dark.svg)

### Interaction / state invariants

![Persistent pane behavior: changing tracks updates its content, outside clicks do not close it](track-pane-behavior.svg)

## Reading these references

- These are **original self-contained vector sketches**, not screenshots, a GTK template, live Yandex artwork, or a license to copy any third-party UI.
- All musical names and cover graphics are synthetic illustrative placeholders. Do not ship demo catalog data or artwork as service content.
- Typography, geometry, hierarchy, cover sizes, composition, emphasis, responsive placement and control grouping are the **visual targets**. SVG icon paths and exact pixels are **not** a production asset pack.
- Colors are example light/dark theme cues; use Libadwaita style variables and GTK CSS classes in production, not a hard-coded pseudo-themed application.
- **Behavioral rules in [DESIGN_SPEC.md](DESIGN_SPEC.md) outrank an illustrative image.** If a mock visually suggests text or an action that current confirmed APIs cannot supply, omit it rather than fabricate it.
- A good implementation preserves native HeaderBar, focus and screen-reader labels, obeys reduced motion, and passes actual GTK screenshot review.
- The right pane must be **nonmodal with respect to outside clicks**, stay open until explicit close, and update to the new playing track on Next, queue selection, EOS, MPRIS and direct Play. It must never close and reopen merely to switch tracks.

The actionable Codex instruction is [CODEX_TASK.md](CODEX_TASK.md), and the authoritative product/interaction brief is [DESIGN_SPEC.md](DESIGN_SPEC.md).

The separate [implementation review](IMPLEMENTATION_REVIEW.md) records actual
GTK before/after evidence, source fingerprints, corrections and pending checks.
It does not turn these five SVG references into application screenshots or
replace owner Fedora visual acceptance.

## Privacy and attribution

Original mockup geometry and abstract artwork were authored for Karin. No private desktop screenshots, user account data, service album art, credential material, font binaries, or copied Cassette assets are included.

All original SVG and Markdown files are GPL-3.0-or-later, following the project license.
