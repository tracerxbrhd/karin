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
- Type Cyrillic/Unicode title, artist and album queries; test clear/reset, empty
  matches, partial coverage, next-part errors/Retry and retired replies. Count
  HTTP work to exclude per-key page/N+1 reads. Retain the 2,000-position/100-page
  bounds and initial fill limit. See [ADR-0025](adr/0025-loaded-collection-search-and-playback-provenance.md).
- Test the whole metadata Info target and independent Like focus. Membership
  read/write/confirmation and playlist creation must not invent success or replay
  ambiguous POSTs. Logout needs a clear label and default Cancel confirmation.
- Selection alone cannot reveal an idle footer. Real resolver/generated-media
  playback must expose controls; Pause retains them, Stop/clear/logout hide them
  without a focus trap or hidden width constraint. Loading must not change footer
  height. Focus Seek at 850×750 and resize to 420×720/back without audio restart.
- Exercise queue edits, unavailable items, Next/Previous/EOS, Shuffle/Repeat and
  MPRIS transport/seek/volume/modes on the same player. Check session restore,
  cache clear/off and teardown for stale callbacks/private views.

## Pinned panel and motion

Info/Queue share one fixed-size surface. Outside clicks, navigation, resize and
playback preserve mode; explicit close/Back restores focus. Content switching
must not close/reopen the host. Current-occurrence changes resume following;
inspecting details must not play. See [ADR-0022](adr/0022-universal-music-panel.md).

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

Retain all native registrations: currently 30 functional and three modern visual/
raw selectors in both Rust lanes, with sandboxed WebKit, actual input, fatal
warnings and strict motion. Report namespace/SDK limits; do not disable sandbox
or remove tests. Commands/platform placement live in
[BUILDING.md](BUILDING.md) and the [workflow](../.github/workflows/ci.yml).

Cloud/CI use synthetic accounts, original art and generated audio. Never publish
personal screenshots/cache, credentials or signed resources. Keep few current
images in source; full matrices/geometry/logs belong in artifacts. Preserve key
results/failures with durable Git/PR references in [VALIDATION.md](VALIDATION.md),
not only expiring artifacts. Do not relabel old captures or FAILs. Desktop
acceptance does not resolve cloud cold-frame observations.
