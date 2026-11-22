<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# UI quality gate

Substantial UI changes require real native image inspection, GTK structure and
input checks. A successful build or screenshot alone proves neither usability
nor desktop performance. The owner accepted v0.8 functionality; this gate covers
changed behavior without requesting a full retake of accepted work.

<a id="v04-native-music-gate"></a>

## Layout and visual checks

Inspect compact (420×720), reference (850×750), medium (900×650), wide
(1440×900), large (1920×1080) and actual maximized windows, both schemes and
relevant 1×/2× cases. Preserve fixture-specific 720×650/1280×800/1650×930 bounds.
Match actual logical allocations, content, theme and scale rather than requested
outer size. Cover Home/Wave, collections, Search, Info/Lyrics/Queue and
hidden/active/paused/error footer, with long/missing/partial data and Retry.

- Inspect density, hierarchy, contrast, separators, clipping, wrapping, scrolling,
  focus and all hit targets. At 900×650, Liked retains three complete 64-pixel
  rows, hero ≤32% of viewport height and filter ≤56 pixels. Global search keeps
  its density guards. Geometric containment alone does not establish visual quality.
- System retains its distinct surface levels and native semantics. Measure painted
  primary-text contrast ≥4.5:1. High Contrast uses opaque surfaces/visible outlines;
  Reduced Motion uses immediate endpoints. Karin Style stays unavailable.
- The complete Seek/control target retains at least 24 logical pixels of inner
  bottom spacing. Hidden footer height is zero; loading cannot change its height.
  Check focus/controls while resizing reference → compact → wide with no restart.
- Use the native [input helper](../src/ui/music/test_input.rs) for popovers; missing
  cross-native bounds never justify skipped containment or guessed coordinates.

## Input, state and accessibility

Exercise mouse, Tab, Enter, Space and meaningful keyboard alternatives with the
real mapped widgets. Inspect ordinary executable AT-SPI names/roles as well as
widget focus: the root must expose navigation, search, playback and Lyrics.

- Track activation selects the exact occurrence, including duplicates across
  filtered windows; Info/reaction targets remain independent. Verify selection,
  current marker and queue/player/MPRIS identity survive projections and future
  queue edits. Unchanged player ticks must not rescan every marker.
- Collection search fills its content column beside the compact native sort
  button. Measure allocations with pane open/closed, then type Unicode title/
  artist/album, choose sorts by mouse/keyboard and exercise index intent, Pause/
  Continue/Retry/clear/leave. Count HTTP work for fast typing and reuse past 2,000
  positions. Preserve bounds and final membership recheck in
  [ADR-0035](adr/0035-incremental-whole-collection-index.md).
- Global search entry/tabs share the same measured slot, group center and header
  height. Inspect closed/intermediate/open/reverse frames, both schemes and scales:
  pages crossfade in that slot without sliding beside each other. Test toggle/
  Ctrl+F/Home, debounce/Enter, results/paging/Retry/detail navigation, clear versus
  dismiss, entry padding/menu/result clicks, rapid reversal and late logout replies.
- Check confirmed reactions, playlist writes and GET-only recovery; unknown state
  must not look unmarked or replay a write. Confirmed current Dislike advances
  once; Like/removal/refusal and stale duplicate replies do not. Logout confirmation
  defaults to Cancel. Detailed write contracts are in
  [API_FEASIBILITY](API_FEASIBILITY.md) and [PLAYLIST_MANAGEMENT](PLAYLIST_MANAGEMENT.md).
- Check queue input, modes, transport/seek/volume/EOS, MPRIS, session restore and
  teardown through the one player. Cache indicators follow actual complete records
  across rows/pane/footer, including restart/Clear/eviction/account fences. A Stored
  icon is not proof of local playback. No per-row network or GTK disk I/O is allowed.
- Verify independent color preference persistence while audio/search/pane state
  remains intact. A removed/unknown style falls back without resetting other
  preferences. Test the unavailable Karin row and ordinary System startup.

## Lyrics and pinned-panel motion

Info/Lyrics/Queue share one surface; outside clicks/navigation/resize preserve
intent. Footer Lyrics/Back returns to Info in place; explicit Close restores focus.
Current-occurrence changes resume following; inspection alone never plays.
[ADR-0022](adr/0022-universal-music-panel.md).

First exercise the ordinary executable's provider path: Yandex TEXT without
timing → disclosed LRCLIB consent → MusicService LRC → actual player position.
A renderer preseeded with LRC cannot prove this path. Check declined/saved/revoked
consent, missing/malformed timing and retired account/request/selection results.
Keep useful TEXT on lookup failure and successful captions minimal. Record
executable revision/package identity.

Play original audio/LRC through PlaybackController and the mapped native view.
At a natural line boundary capture before/start, nearest painted frames to
25/50/75% of the 300 ms presentation duration, and the endpoint. Measure actual
frame times, wall duration and both rows' painted glyph bounds, accounting for
scroll/device scale. The old/new line must shrink/grow together in size, weight
and contrast; inspect hierarchy, clipping, overlap and Follow movement. With
all tick intervals ≤30 ms verify expected wall duration; after a longer stall,
verify continued paired change and centering on subsequent paints.

Include long wrapped Cyrillic/Unicode, narrow/wide, both schemes, High Contrast
and scales 1/2. Reduced Motion reaches the legible enlarged endpoint on its first
paint. Check pause/resume, seek, equal timestamps, next/previous/EOS, source
renewal, recycled rows, rapid selection, close/logout and manual scroll/Follow.
TEXT remains selectable without pretending to have timing.
[ADR-0034](adr/0034-bounded-track-lyrics-and-similar.md).

Observe panel opening/closing at 0/25/50/75/100% using five distinct native frame
times/indices; closing is chronological 100→75→50→25→0. Background/border/header/
art/text/actions translate together under normal 240 ms timing with stable
relative geometry and clipping. Initialized strict motion follows the recorded
setup open and bounded mapped renders; it proves no cold-start smoothness.
The separate fresh-process first-Play observer retains actual request/worker and
raw paint gaps before graphics initialization. Do not hide failure through delays,
longer motion, retries, synthetic frames, renderer changes or production prewarming.

## Evidence and CI

Record source/binary identity, actual geometry/libraries/theme/scale/renderer,
animation and Mesa worker settings. Static captures use full redraw; motion/cold
use ordinary GTK damage. Render-node snapshots are no physical-framebuffer proof.
Matched performance observations retain actual wall/CPU/frame data.

Required CI covers every native registration: full functional execution once,
modern visual/strict motion with targeted HiDPI/High Contrast/Lyrics, and exact
MSRV compile/runtime compatibility. The registry rejects missing/unregistered
selectors. The [extended native workflow](../.github/workflows/extended-native.yml) runs the
current signed-out gallery manually; historical comparison sources remain in Git.
Underlying regression assertions remain required. Ignored/skipped cases are no
PASS. [BUILDING](BUILDING.md) and the [workflow](../.github/workflows/ci.yml)
contain executable commands.

Use synthetic accounts, original art/text/audio and local HTTP only. Private
captures require publication permission; no credentials, private cache or signed
resources belong in evidence. Keep few curated images in Git and full matrices/
logs in bounded CI artifacts. Preserve essential outcomes with immutable links
in [VALIDATION](VALIDATION.md). Report SDK/namespace limits without weakening
sandboxing. Desktop acceptance does not resolve the Cloud cold-frame investigation.
