<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin v0.4 — Cassette-like refinement

Current authority is the [latest full owner task](CODEX_TASK.md) and
[DESIGN_SPEC.md](DESIGN_SPEC.md), starting from
`a70bbf714e3d7ea775c03839eb01f0b1d9dd4f62` in existing **Draft PR #15**.
This refines the working client; it does not restart the application or replace
its music, session, queue or audio architecture. No new PR or automatic merge.

## Current direction

- Light native **Моя волна / Нравится / Плейлисты** tabs, coherent Search access,
  no dropdown as the main destination switch.
- An explicitly unavailable Wave product placeholder, real liked music and
  owned/liked playlist sections. No invented radio, related songs or catalog.
- Continuous track loading with bounded service batches, stable scroll and
  inline loading/end/Retry instead of track Previous/Next page buttons.
- One pinned Track Info / Queue panel, visually composed as a musical sidebar;
  outside clicks never dismiss it and all of its surface moves together.
- **Initially hidden Player Bar**: reveal smoothly on real play, retain through
  pause/loading/active Retry, hide safely on full Stop/no active track. The whole
  current cover/title/artist region opens Info; independent actions retain focus.
- Direct server-confirmed Like. Hide unconfirmed Dislike rather than showing a
  pretend working control. Published upstream endpoints and Karin validation are
  different facts; document the remaining contract uncertainty accurately.
- Preserve real Shuffle/Repeat, numeric Repeat One, MPRIS, protected session,
  cache, playlist creation and the existing GStreamer/StreamResolver path.

The private Cassette screenshot supplied in chat is a composition reference:
light upper segments, clean collection hero/rows, coherent centered information
and compact transport. The primary agent inspected it; it is **not copied into
this repository**. Do not copy its service art, personal content, branding or
literal layout. GTK theme metrics and Karin's supported behavior determine the
implementation.

## Evidence and acceptance

[IMPLEMENTATION_REVIEW.md](IMPLEMENTATION_REVIEW.md) separates the new refinement
from prior a70/C10 and a9 checks. Existing screenshots and green CI are historical
before evidence; they do not prove the hidden-player, full metadata click area,
new navigation or continuous-list behavior. Fresh source-matched GTK captures
and real mouse/keyboard/scroll/lifecycle checks are required.

The current implementation uses a fixed native segment row, quiet 64-pixel
liked-track rows, owned/liked playlist sections, height-adaptive centered Info,
an initially hidden footer and a separate direct Like action. The
[current review](IMPLEMENTATION_REVIEW.md#c9-current-visual-evidence) identifies
the exact C9 source, native evidence audit and 11-original final-source visual
review separately from the broader C7 checkpoint; it does not silently transfer
old results to a newer revision. The
[selected actual GTK gallery](evidence-refinement/README.md) identifies its own
source and capture scope. Prior sketches remain historical below.

Show idle/active/paused/error player states, footer appearance/disappearance,
Info/Queue transitions, Wave/Liked/Playlists/Search, continuous append/Retry/end,
compact/light/dark/HiDPI and the established geometry matrix. Inspect actual
intermediate motion frames with honest renderer/initialization provenance.
Earlier cold first-paint failures and warm-proof limitations remain recorded;
do not turn them into a cold-start PASS or add production prewarming.
The C7 default 2× motion phase miss is retained separately from the qualified
paired 1×/2× `LP_NUM_THREADS=2` initialized-renderer pass. The latter is a test
resource-control result, not an application workaround or cold-start acceptance.
C9 repeats the complete strict panel scope with fresh source-matched artifacts;
its raw first footer reveal still records a 116.7 ms paint gap, so cold first-use
smoothness is explicitly not accepted.

Cloud/Xvfb does not accept Fedora/KDE/Wayland/PipeWire, real Music, physical media
keys, account writes or current Flatpak. Keep PR #15 Draft for owner review.

## Historical original sketches

The six vectors below describe the **preceding complete-redesign target**, not
the current refinement. Their permanent footer, left rail and collection controls
are superseded where they conflict with this task. Retained for design history:

- [Wide dark](desktop-wide-dark.svg)
- [Wide light](desktop-wide-light.svg)
- [Wide Queue](desktop-wide-queue-dark.svg)
- [Medium panel](medium-pinned-pane-dark.svg)
- [Compact panel](compact-track-pane-dark.svg)
- [Panel continuity](track-pane-behavior.svg)

They remain original illustrative vectors, not GTK screenshots or approved final
pixels. Earlier [public-source research](UX_RESEARCH.md) documents what was
actually inspected and the distinction between Cassette release images and main
source. Its older target decisions likewise do not override the latest brief.

All original Karin materials use GPL-3.0-or-later. Private screenshots, account
contents, tokens, service covers and music files do not belong in public evidence.
