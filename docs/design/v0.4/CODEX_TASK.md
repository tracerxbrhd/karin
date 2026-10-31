<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# CODEX TASK — Karin v0.4: implement approved Native Music UI

**Repository:** https://github.com/tracerxbrhd/karin

**Work on the existing branch:** `feature/v0.4-native-music-ui`.

**Continue the already-open Draft PR containing this design package. Do not create another PR. Do not merge or delete the branch.**

## Your authoritative input

1. **[DESIGN_SPEC.md](DESIGN_SPEC.md)** — complete approved behaviors, layout, responsive rules, UX, architecture and acceptance.
2. **[README.md](README.md)** — embeds the five original SVG references:
   - `desktop-wide-dark.svg`
   - `desktop-wide-light.svg`
   - `medium-pinned-pane-dark.svg`
   - `compact-track-pane-dark.svg`
   - `track-pane-behavior.svg`
3. Current `AGENTS.md`, `docs/ARCHITECTURE.md`, `docs/UI_QUALITY_GATE.md`, `docs/VALIDATION.md` and ADR-0017/0020.
4. Source code at latest `master` plus the design-docs branch.

**The owner's accepted choice is B — Native Music, with a minimum of visible text.** Do not reinterpret the instruction as one of the alternative A/C styles. These images formalize the *previously approved structural sketch*, not a requirement to copy third-party music apps.

## Nonnegotiable behavior of the right pane

- Opening it pins it until explicit X / Ctrl+I (or compact explicit back). Outside clicks, scrolls, route changes and track changes must not dismiss it.
- When an actual current track changes, its art/title/artist/album/actions update **in place**, without closing the pane, without restarting playback and without stale mixed content.
- Explicitly viewing a different track's metadata must not play it. Next actual current-item change returns to the current track.
- Preserve openness across wide/medium/compact breakpoints. A collapsed modal dismiss-on-outside overlay that defeats this requirement is not acceptable.
- Preserve the accepted opening/closing animation, without a forced close/reopen workaround.
- Test actual GTK input events, not only programmatic properties.

## Required work

1. Establish coherent light/dark typography, spacing, icon/button styles and reusable layout components. Do not hardcode dark-theme mock colors into the live app.
2. Redesign main shell: compact expandable navigation, expressive central music workspace, responsive right pane, compact persistent player bar.
3. Implement the pane's **controlled nonmodal open intent** and current-track following semantics; audit `src/ui.rs`, `src/ui/music/workspace.rs`, `track_pane.rs` and associated tests.
4. Redesign the pane: hero art, track/artist/album, compact verified actions, no duplicate full transport interface.
5. Redesign Home, Library, album and playlist headers, track rows, playlist tile grid, unified search, queue and Player Bar in the same Native Music visual system.
6. Retain loading/empty/error/retry, safe account/logout, accessible names, tooltips, keyboard, reduced motion, HiDPI, long metadata and absence of art.
7. Preserve existing working music, OAuth, persistence, cache and MPRIS integrations; no new unverified API features.
8. Update `docs/ROADMAP.md`, `docs/ARCHITECTURE.md`, `docs/UI_QUALITY_GATE.md`, `docs/VALIDATION.md`, `docs/BUILDING.md`, and an ADR if a significant container/state decision changes.

## Review before claiming completion

- View actual application captures (not the supplied SVGs) at 420×720, 900×650, 1440×900 and maximized, both themes, scale 1 and 2. Inspect populated library, playlist, search, queue, right pane open/closed, preferences, empty/error states and keyboard focus.
- Check the medium nonmodal pane on real GTK: clicking outside **cannot dismiss it**.
- Check pane tracking for mouse/keyboard, queue, Next/Previous, EOS and MPRIS, including duplicate track identity and stale asynchronous results.
- Check readability with at least 6 fully visible track rows at 900×650 with pane closed when accessible.
- Run formatting, warning-free Clippy, default/all/minimal tests/builds, Rust 1.98.1, native GTK fixtures, GStreamer/MPRIS, Flatpak if available, and CI. Distinguish executed tests from pending Fedora/Wayland/owner acceptance.
- Compare equal-sized before/after GTK screenshots and include visual artifacts in the PR; do not substitute SVG mockups as proof of running code.
- No user's personal screenshots, tokens, account names, track lists, or copyrighted music in public evidence.

**Deliverable:** one substantial updated Draft PR with functioning native UI code, design-quality screenshot review, behavioral regression tests, precise remaining limitations and short Fedora acceptance guide. Do not stop at plans or documentation. Do not merge without owner approval.
