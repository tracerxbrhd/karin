<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# UI quality gate

Substantial UI changes need actual native image inspection, structural checks and
real input. Neither build nor screenshot alone proves quality. Acceptance of
`97093ee` in [VALIDATION.md](VALIDATION.md) does not waive later review.

<a id="v04-native-music-gate"></a>

## Visual matrix

Inspect 420×720, 850×750, 900×650, 1440×900, 1650×930, 1920×1080 and actual
maximize, plus fixture cases 720×650/1280×800; light/dark and scales 1/2. Match
before/current actual logical allocation, theme, scale and content role, not just
requested outer-window size.

Cover Home, Liked, playlists/details, Search, Info/Queue; hidden/active/paused/error
player; empty/partial/large data, missing art, long text, loading/Retry and focus.
Assess density, hierarchy, contrast and actions. At 900×650, Liked leaves three
complete 64-pixel rows, hero ≤32% viewport height and filter ≤56 pixels. Global
search retains its density checks. Bounds alone are not visual acceptance.

Check HeaderBar/window controls, spacing, wrapping, scrolling, hit targets and no
horizontal overflow. Compact footer controls stay reachable. Use the native
[input helper](../src/ui/music/test_input.rs) for popovers; missing cross-native
bounds do not justify skipping containment or guessing coordinates.

## Real interaction and state

- Activate exact track occurrences by mouse/Enter/Space, including duplicate IDs
  across filtered 500-position windows. Details must not play. Filtering,
  appending and navigation must not rewrite the active queue or restart audio.
- Inspect the shared one-row track filter and native sort button in all track
  feeds. Measure the entry and sort allocations at 420, 900, 1440 and 1920 logical
  pixels and maximized, at 1× and 2×. Check that the field grows with the list,
  shrinks when a pinned pane takes space and grows back on close. The sort
  button must stay square, reachable and inside the content column. Open and select sort options with
  mouse and Tab/Enter/Space. Verify selected order, active styling and date
  availability. Type Unicode title, artist and album queries, then check immediate
  local results, clear/reset, empty matches, honest partial coverage, compact
  Pause/Continue/Retry and retired replies. Count actual HTTP work for fast typing,
  Enter, clearing, navigation and reusing an index beyond 2,000 positions. Preserve
  the ordinary 2,000-position/100-page bounds, initial fill limit and indexed
  20,000-position/50-position-batch/200-row-window bounds. Exercise debounced
  global search from the top button, Ctrl+F and Home. Sample the moving icon and
  revealing field in native frames. Check focus, unchanged origin before input,
  sections/paging/Retry after submission, clear and Escape from a detail origin,
  and response retirement after a tab switch or logout.
  See [ADR-0035](adr/0035-incremental-whole-collection-index.md).
- Test the whole metadata Info target and independent Like/Dislike focus. A
  reaction click must not open Info; unknown Dislike stays hidden and pending
  state is not optimistic. An unavailable Dislike must not disable a separately
  confirmed Like-only workflow. Confirmed Dislike addition advances the unchanged
  current occurrence once through normal Next/Wave Skip. Removal, Like/Unlike,
  refusals and ambiguous writes must not advance; neither may a late reply after
  selecting a duplicate Track ID occurrence, logout or cancellation. Membership
  confirmation and playlist creation must not invent success or replay ambiguous
  POSTs. Logout needs a clear label and default Cancel confirmation.
- Selection alone cannot reveal an idle footer. Real resolver/generated-media
  playback must expose controls; Pause retains them, Stop/clear/logout hide them
  without a focus trap or hidden width constraint. Loading must not change footer
  height. Focus Seek at 850×750 and resize to 420×720/back without audio restart.
- Exercise queue edits, unavailable items, Next/Previous/EOS, Shuffle/Repeat and
  MPRIS transport/seek/volume/modes on the same player. Check session restore,
  cache clear/off and teardown for stale callbacks/private views.
- Cache indicators must follow actual complete AudioCache records across Liked,
  playlists/search, Queue/Wave, footer and Info. Check downloading → stored,
  replay/restart, multiple occurrences, Clear/LRU, failures and account/off fences.
  Stored means a local record; only actual cached-file playback can show the
  local-source cue. Inspect compact/light/dark/HiDPI density and accessibility,
  preserve row/Like/menu keyboard actions, and exclude per-row HTTP or GTK file I/O.

## Pinned panel and motion

Info/Lyrics/Queue share one surface. Outside clicks, navigation, resize and
playback preserve mode; explicit close/Back restores focus. Content switching
must not close/reopen the host. Current-occurrence changes resume following;
inspecting details must not play. See [ADR-0022](adr/0022-universal-music-panel.md).

Play original fixture audio through PlaybackController with Lyrics visible.
Across a natural LRC boundary compare rendered pixels inside both affected
rows before, during and after emphasis changes, accounting for Follow movement.
Require multiple distinct intermediate frames and stable row geometry.
Run the same passage with Reduced Motion and check immediate emphasis. Check seek, pause, next/previous/EOS, virtual row reuse, old-track
retirement and manual scrolling without recentering before Follow. Verify
instant current-line emphasis with reduced motion, readable High Contrast,
and plain selectable TEXT without synchronization. [ADR-0034](adr/0034-bounded-track-lyrics-and-similar.md).

With normal animation enabled, observe opening and closing near 0/25/50/75/100%
openness using five distinct native frame times and indices. Closing is presented
chronologically as 100→75→50→25→0. Background, border, header, art, text and actions
must translate together, with stable relative geometry and correct clipping.
A moving cover, fully open endpoint or disabled animation does not prove this.
Check reduced motion separately; keep normal 240 ms and phase/geometry guards.

The strict fixture measures after a recorded setup open, six bounded renders of
the mapped panel, then close. Setup changes no progress/clock and provides no
proof PNGs; its quarters are not assessed. Separately
observe fresh-process first Play before graphics initialization, using the actual
request/worker and raw paint gaps. Monotonic height/lifecycle success is not cold
smoothness. No delays, longer animation, conditional retries, synthetic frames or
production prewarming may conceal failure.

## Evidence and CI

Record source/binary identity, actual geometry, libraries, theme/scale, renderer,
animation and test environment. Static uses full redraw; motion/cold use ordinary
GTK damage. Render-node snapshots are not framebuffer-damage or physical-GPU proof.
Record Mesa worker limits explicitly.

Retain every native registration in both Rust lanes, including functional and
modern visual/raw selectors, sandboxed WebKit, actual input, fatal warnings and
strict motion. Record the final registered/executed counts with its source. Report namespace/SDK limits; do not disable sandbox
or remove tests. Commands/platform placement live in
[BUILDING.md](BUILDING.md) and the [workflow](../.github/workflows/ci.yml).

Cloud/CI use synthetic accounts, original art and generated audio. Never publish
personal screenshots/cache, credentials or signed resources. Keep few current
images in source; full matrices/geometry/logs belong in artifacts. Preserve key
results/failures with durable Git/PR references in [VALIDATION.md](VALIDATION.md),
not only expiring artifacts. Do not relabel old captures or FAILs. Desktop
acceptance does not resolve cloud cold-frame observations.
