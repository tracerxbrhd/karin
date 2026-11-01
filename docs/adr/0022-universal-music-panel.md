<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0022: One controlled music-information and queue surface

Status: Accepted for the complete v0.4 redesign; execution and owner acceptance remain separate
Date: 2026-11-01
Extends: ADR-0017's queue/controller ownership and ADR-0021's controlled intent and occurrence target
Supersedes: ADR-0021's information-only presentation and the separate primary Queue route

## Context

The owner rejected the UI/UX of `a9eed45` on Fedora despite successful cloud
checks. The panel appeared to animate artwork separately from text/actions;
its relationship with the central music view was weak. Queue occupied a primary
navigation route even though it belongs to the current listening context. A
static endpoint screenshot did not establish coherent motion.

A fresh observer on the original `a9eed45` production layout reproduced the
allocation defect under GTK/Xvfb: in one 1440-pixel-wide cloud case, the
Revealer child surface stayed at x=1110 while its allocated width changed
1520 → 594 → 483 → 420 → 320. Left-aligned title x stayed 1131; the centered
cover moved through x=1742 → 1279 → 1224 → 1192 → 1142. Thus the old slide
changed the child's width rather than translating the whole surface: text
appeared at its final left edge while centered artwork travelled. This is an
executed cloud reproduction of the owner's symptom, not a claim about the
owner's exact Fedora coordinates or renderer. The recorded before frames and
after-paint geometry distinguish that cause from an animation-type guess.

The existing MusicService, guarded resolver, GStreamer worker, PlaybackContext,
protected session/cache and MPRIS are working foundations. This change concerns
native presentation and routing, not another service, engine or cloud queue.

## Decision

Use one permanent universal panel with an explicit user mode:

```text
Closed | TrackInfo | Queue
```

Keep mode/open intent separate from availability, animation progress,
wide/medium/compact presentation, information selection and actual playback.
One outer surface owns the background, border, header, close/Back control and
content. Track Info and the existing queue view occupy that same host; the queue
has one widget tree. There is no independently animated second pane or duplicate
primary Queue page.

| User action | Result |
| --- | --- |
| Current cover or explicit information, panel closed | Open TrackInfo |
| Current cover, Queue open | Switch content to TrackInfo in place |
| Current cover, TrackInfo open | Keep open and show current metadata |
| Ctrl+I | Explicit information toggle |
| Footer Queue / Ctrl+J, closed | Open Queue |
| Footer Queue / Ctrl+J, TrackInfo open | Switch content to Queue in place |
| Footer Queue / Ctrl+J, Queue open | Explicitly close |
| X or compact Back | Close with appropriate focus return |
| Outside click, scrolling, navigation, playback or resize | Preserve user mode |
| Diagnostics | Suspend presentation while retaining intent |
| Logout/account retirement | Close and retire personal information/work |

The left navigation contains Home, Search and Library. Queue entry points route
to the universal panel, including the existing Ctrl+J action. In compact mode
the panel is an explicit subview with Back, not a dismiss-on-backdrop dialog;
the permanent Player Bar remains available. Desktop geometry must leave actual
central actions reachable, including at the 720-pixel boundary and with expanded
navigation. Resizing does not change ownership or close/reopen the surface.
When actual music-content width falls below 620 logical pixels, use the compact
subview even if the window is nominally medium, for example at 720 pixels with
expanded navigation labels. This is a content-availability boundary, not a new
minimum window size. It preserves the same mode, progress and surface.

### Whole-surface motion

Animate the complete native surface as one geometric unit. Its background,
border, header, artwork, metadata and controls must have the same displacement
at every intermediate frame. Clipping belongs to the outer host, not separate
late visibility changes on individual children. Layout must not reveal text
instantly while only artwork moves, or cover central controls during a width
transition.

Use a small native `SlidingPanel` single-child GtkWidget as the geometry host,
driven by a 240 ms AdwTimedAnimation with EaseOutCubic. The child is allocated
once at the full final panel width/height; one GSK transform translates the
entire surface by `x = (1 - progress) × width` inside the clipped viewport.
This separates child allocation from reveal extent and removes the measured
inverse-width/reflow mechanism: the final wide child stays 320 pixels while
all its descendants share one translation. A change to a Revealer's transition
enum alone cannot establish this rigid-surface contract.

The central workspace reservation follows actual progress toward the final
320+20-pixel wide or 300+16-pixel medium split. It does not instantly shrink
the main view before a surface starts moving. The animation is native Adwaita
timing, not a custom frame-clock scheduler. Compact final width derives from
the available viewport rather than a desktop fixed width.

Switching TrackInfo ↔ Queue while open changes content inside the fixed host.
It does not replay opening, rebuild the outer panel, alter header/close position
or create another playback state. A short 100 ms GtkStack crossfade changes
content under the same fixed header/close controls. Use native GTK/Libadwaita animation and honor
reduced motion; normal-motion testing must retain animation. Concrete animation
geometry and frame evidence are recorded with the implementation review rather
than inferred from a transition enum or a fully opened capture.

### Information and queue ownership

Preserve `FollowingCurrent` versus `ExplicitSelection` and the existing
source/position/track occurrence identity. Explicit Details selects metadata
without playing. Actual current-occurrence changes, including duplicate IDs,
direct Play, Next/Previous, queue selection, EOS advancement and MPRIS, update
information atomically and resume following without closing the panel.
Pause/Seek/Volume and structural reindexing preserve explicit selection. A
Repeat One restart at the same occurrence is not a different queue position.
Old metadata/artwork completions remain epoch/session-checked.

Queue reads the same MusicQueue/PlaybackContext used by transport and MPRIS.
Current, upcoming and history presentation uses the local policy's exact
positions, including duplicates, manual additions, unavailable entries and a
visible partial-context boundary. Removal, clear, Play and Details dispatch
existing generation-bound actions; opening or switching the panel never starts
or stops sound. [ADR-0023](0023-local-playback-order-and-mpris-modes.md) defines
the shared playback order.

## Consequences and verification

The container/state refactor is necessary because retrying `show_sidebar(true)`
or instantly reopening a dismissed view cannot provide pinned nonmodal behavior
or coherent motion. It introduces no new runtime, API endpoint, credential
storage, permission or audio worker.

Capture actual opening/closing near 0/25/50/75/100 percent, using five distinct
rendered native frame times/indices per sequence and measured progress near
each target. Do not reuse one frame for several labelled quarters. Current
closing-gallery filenames describe openness, so chronological order is
100 → 75 → 50 → 25 → 0. Original before-observer filenames instead describe
elapsed quarters; label that difference explicitly rather than equating the
two progress measures. Assert common
surface-relative geometry for background/header/art/text/buttons, clipping and
usable central actions. Verify stable mode switches, normal and reduced motion
separately, actual mouse/keyboard, outside input, focus, all responsive crossings,
duplicate occurrences, delayed replies, logout and weak-reference cleanup.
Compare fresh matched before/current frames across the complete
[quality gate](../UI_QUALITY_GATE.md#v04-native-music-gate).

The earlier `a9eed45` cloud PASS and owner UI FAIL remain historical results.
Executed current checks belong in [VALIDATION.md](../VALIDATION.md); this ADR
is not a test PASS or Fedora/KDE/Wayland acceptance. Existing PR #15 stays Draft
pending the owner's new native/Flatpak review. No merge or new PR is authorized.
