<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin v0.4 — final Cassette-inspired collection refinement

Current authority is the [latest full owner task](CODEX_TASK.md) and
[DESIGN_SPEC.md](DESIGN_SPEC.md), starting from
`2e3ae41a8480b8f5f437f739c02cb925af69e884` in existing **Draft PR #15**.
This refines the working client; it does not restart the application or replace
its music, session, queue or audio architecture. No new PR or automatic merge.

## Current direction

- Light native **Главная / Нравится / Плейлисты** tabs, coherent Search access,
  no dropdown as the main destination switch.
- An expressive Liked collection hero with an original native heart, honest
  loaded/complete counts and duration only when every complete-source value is
  known. Quiet 64-pixel rows stay compact and useful.
- A reusable local track filter inside Liked and each opened playlist, distinct
  from global service search. Partial coverage, continued batches, duplicate
  occurrence identity and the already playing queue remain explicit and safe.
  Typing issues no MusicService/collection-metadata reads or whole-library drain;
  the existing bounded cover loader may resume for newly visible retained rows.
- A useful Home with existing real music routes and a modest unavailable Wave
  status, rather than a giant empty placeholder. No invented radio or catalog.
- Larger responsive owned/liked playlist covers with unchanged confirmed
  creation/open behavior; a sparse item retains natural size.
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

Footer refinement associates the native heart action with current metadata,
keeps transport genuinely centered, groups progress/time with it and retains
the same five reachable transport controls in the compact two-row presentation.
It is one existing player model, not another engine.

The two private side-by-side Cassette/Karin screenshots supplied for this task
are composition evidence, especially collection identity, playlist covers and
transport grouping. The primary agent inspected them; they are **not copied
into this repository**. Contributors who did not receive their pixels do not
claim independent inspection. Do not copy service art, personal content,
branding or literal layout. GTK theme metrics and Karin's supported behavior
determine the implementation.

## Evidence and acceptance

The [current actual GTK gallery](evidence-final-refinement/README.md) records
source-frozen **C14**, its 111-entry source manifest, seven equal-size Karin
before/current comparisons and untouched selected originals. The complete native
output has **260 static images**, **108 matched actual GTK/PNG dimension pairs**
and **240 strict initialized panel frames / 48 trajectories**. Six native stages
pass with fatal GTK warnings enabled. The exact 2e3 before production has one
fully disclosed additional 850×750 test case in the before harness. Static PNGs are X11 window captures
under full redraw; motion/raw-footer images render retained native after-paint
scene nodes. These are different from direct ordinary framebuffer-damage or
Fedora performance acceptance.

[IMPLEMENTATION_REVIEW.md](IMPLEMENTATION_REVIEW.md) records current implemented
composition and executed image review separately from prior checkpoints. C12's
complete local native run had **48 PASS / 8 nonpassing**; those exact failed logs/
source/results remain visible. C13 passed 12 targeted regressions and its own
six-stage proof. Its results are historical. C14
[regular checks](evidence-final-refinement/metadata/current/regular-result.json)
pass on stable Rust 1.99.1 and exact Rust 1.98.1: formatting, strict Clippy,
builds and tests (379 default, 387 all-feature and 365 minimal-feature library
tests, plus four CLI tests for each configuration/compiler). Locked offline MSRV
release, terminal/GStreamer/generated-media/GTK smoke checks and the unchanged
license inventory (207 lockfile packages; 178 for Linux) also pass.
**All 56 local native executions on C14 pass**: 28 selectors at both scales,
with a [verified full ledger](evidence-final-refinement/metadata/current/functional-results.jsonl)
and [summary](evidence-final-refinement/metadata/current/functional-summary.json).
Two navigating WebKit fixtures remain unsupported locally and hosted-required.
Hosted CI and downloaded Fedora archive validation are pending at documentation
freeze; their actual revision-specific outcomes remain in the PR record. All 56
current cases ran on C14; older green results are not relabeled final. The
[previous gallery](evidence-refinement/README.md) likewise remains historical.

**Fresh-process first-footer smoothness is still unaccepted.** C14's actual
first mouse Play records **900,018 μs / 850,017 μs** adjacent native gaps at 1×/2×,
with only one intermediate each and observer maxima 518/282 μs. Lifecycle and
released hidden horizontal requests pass; neither is a smoothness PASS. Cause
is unknown. No delay, longer animation or production prewarming is added.
Earlier C12/C13 cadence, 116,669 μs observations and strict sampling failures
remain preserved, not overwritten.

Cloud/Xvfb does not accept Fedora/KDE/Wayland/PipeWire, real Music, physical media
keys, account writes or current Flatpak. Keep PR #15 Draft for owner review.

The C14 collection-filter selector passes at **1× in 85.04 seconds** and
**2× in 86.50 seconds**, with six actual partial/Retry/append photos and exact source/log/result proof.
All **69 selected public originals** are individually opened across A/C/D/E,
with no new separate structural P0/P1; see the [gallery review](evidence-final-refinement/README.md#independent-original-image-review-and-remaining-acceptance).
This does not infer hosted CI or Fedora acceptance, or accept cold cadence.

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
