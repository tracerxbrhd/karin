<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin v0.4 — Complete Music Experience

## Authority and acceptance

This specification follows the owner's **Complete Music Experience Redesign**
request of 2026-11-01, preserved in [CODEX_TASK.md](CODEX_TASK.md). It supersedes
preliminary Native Music B requirements and illustrative references wherever they
conflict. Starting revision: `a9eed4514802df84d73b0608dd1a62930fc1af76`.
The owner did **not** accept that revision's UI/UX. Successful technical checks
and earlier real-service acceptance remain historical evidence, not acceptance
of this redesign. [UX_RESEARCH.md](UX_RESEARCH.md) separates observations,
current source inspection, fresh starting GTK captures and design decisions.

Work only in `feature/v0.4-native-music-ui`, existing **Draft PR #15**. No new PR,
merge or branch deletion. Keep it Draft until owner Fedora visual acceptance.
This target and its original SVGs do not certify the running implementation.

## One music workspace

Four cooperating regions: compact expandable rail; main music workspace;
**one universal right panel**; permanent Player Bar. The rail contains only
**Home, Search, Library**. Queue is no longer a primary route/rail item.
Settings and technical diagnostics belong in the application/account menu.

Retain native HeaderBar/window controls, concise real errors, Retry, confirmations,
tooltips and accessible names. Name a page once: no duplicated Home/Your Music
heading. In compact dedicated panel views, title and explicit Back reflect
Track Info or Queue; closing restores the route title and usable focus.

Main content grows with the available viewport. Remove an arbitrary narrow
centered clamp that creates a large empty strip beside the right panel. Few
collections retain natural tile sizes instead of stretching to giant cards.
Align central content, panel and footer with a small 4/8/12/16/24px spacing scale.

## Visual system

- Use Libadwaita theme colors, semantic classes and native symbolic icons; proper
  system/light/dark, HiDPI, contrast, focus and reduced-motion behavior.
- Let real artwork supply richness. Keep chrome quiet; avoid repeated borders,
  heavy per-track cards, glass, blur, decorative gradients and artificial gaps.
- Native title/heading/body/caption hierarchy: stronger track title, legible
  artist, quiet duration/actions. Constrained lists ellipsize with full accessible
  text/tooltips; heroes and information may wrap meaningful long metadata.
- Target 60–66px song rows with 44–52px art. Highlight the actual occurrence,
  not every matching Track ID. Use consistent hover, focus and selected states.
- Natural-size collection tiles roughly 144–184px square where suitable;
  compact consistent labels and responsive wrapping. Collection hero art roughly
  96–144px with concise facts and one supported primary action.
- Missing artwork has an honest native fallback. No reference image, copied logo,
  synthetic catalog or demo art enters the ordinary release as service content.

## Universal panel: state, presentation and ownership

Logical state is explicit: **Closed | TrackInfo | Queue**. Separate user mode/open
intent, content availability, responsive presentation, information target, real
current queue occurrence and transient animation progress. GTK reveal state is
not the source of truth for the user's chosen mode.

Track Info and Queue share one stable outer surface, common header/close position,
width and transition. One queue model and one queue widget tree. Do not create
independent sidebars, rebuild/reparent the whole surface for a content update or
fight dismissal with an immediate reopen.

| Explicit action | Closed | Track Info open | Queue open |
| --- | --- | --- | --- |
| Current cover / show information | Open Track Info | Show current metadata; stay open | Switch to Track Info in place |
| Ctrl+I | Open available Track Info | Close explicitly | Switch to Track Info in place |
| Player Bar Queue / Ctrl+J | Open Queue | Switch to Queue in place | Close explicitly |
| Track Details | Open selected metadata without playing | Replace explicit selection | Switch to selected Track Info |
| X / compact Back | Remain closed | Close explicitly | Close explicitly |

Outside clicks, scrolling, navigation, track changes, refresh, Pause, Seek,
Volume, queue edits and resizing preserve mode/open intent. Diagnostics may
suspend rendering while retaining intent. Logout clears personal content and
pending work and closes safely. Closing returns focus to a usable initiating
control or real main-page action; stale detached controls cannot affect a new
session.

### Whole-surface motion

Opening/closing translate the **whole fully laid-out surface**: background,
edge, header, art, text, buttons and queue rows. Descendants retain coherent
relative geometry throughout. Merely animating art or changing a Revealer
transition type is insufficient. Audit allocation/clipping, Overlay/Revealer,
ScrolledWindow and child visibility before choosing the container.

Prefer GTK/Libadwaita; justify any narrowly scoped custom layout/frame-clock
geometry. Do not jump the main area's width, overlap live central controls,
spawn text early, reveal buttons late, flicker or reset scroll on every update.
Switching TrackInfo ↔ Queue while open changes contents inside the same surface,
optionally with a short crossfade, without replaying the slide or moving the
shared header/close. Current-track changes update atomically in place and never
restart sound or close/reopen the surface.

### Track Info

Large cover, title, artist, real known album and a concise set of verified actions;
no duplicate full transport console. Separate `FollowingCurrent` and
`ExplicitSelection`. Explicit Details never plays. The next actual current
occurrence change, including a repeated ID at a different position, resumes
following; Pause/Seek/Volume preserve explicit selection. Keep session generations,
detail epochs and artwork cancellation against mixed metadata and late replies.

### Queue

Show current occurrence prominently, then **Next**, with history/manual positions
where useful. Upcoming order must match the real local policy, including Shuffle.
Keep select/play, add, remove, clear, Details, actual position, unavailable items
and partial-collection boundary, with concise contextual actions. Stale controls
retain generation/occurrence guards. Details switches this same panel to TrackInfo.

The local PlaybackContext is at most 500 loaded positions, not Yandex's synced
cloud queue. No hidden fetch of an entire remote playlist. Selection/advancement
updates active row, footer, next occurrence and MPRIS together while Queue stays
open. Clear/removal has a defined safe transport result, without retry loops.

## Real local Shuffle and Repeat

Use a typed policy over existing MusicQueue/PlaybackContext. Do not shuffle the
physical source list, fabricate buttons or create another player. Repeat cycles
**Off → Queue → One → Off** with accurate tooltip/accessibility and a visible
“1” distinction for One.

- Off ends at the last available position.
- One restarts the same exact occurrence from zero on natural EOS. Explicit
  Next/Previous still transition; failed resolution is visible, never infinitely
  automatically retried.
- Queue wraps from the last available loaded position to the first, including
  manual additions; entirely unavailable or empty queues cannot loop requests.
- Shuffle keeps the current occurrence on enable. Use bounded proper random
  permutation of available occurrences, retaining duplicate IDs and no repeats
  within a cycle. Previous uses actual bounded history; Next retraces forward
  history before consuming remaining positions. Disable without interrupting
  current playback and resume source order.
- Reconcile manual selection/add/remove. Removing another index preserves the
  remaining plan/history with adjusted indices; removing current uses a defined
  available successor/history fallback. Repeat Queue creates a new shuffled cycle.
  Tests can supply deterministic ordering. Do not clone full Track models merely
  to compute each next index.
- Mode choices survive ordinary clear/new context within the local session;
  replacement clears old history/remaining positions, and logout/close reset
  modes. Test all combinations, pause, explicit commands, EOS, new context,
  queue edits, unavailable-only data and cleanup.

PlaybackController, StreamResolver admission/cancellation and their existing
error/cleanup paths remain the foundation. These are local player modes, not
unverified Yandex API capabilities.

## Player Bar

Wide/medium: real art/title/artist plus a server-confirmed like if supported on
left; genuinely centered **Shuffle, Previous, Play/Pause, Next, Repeat**; Queue
and Volume at right. Metadata length must not move transport away from center.
Play/Pause is primary without an oversized decorative treatment. Progress,
elapsed and duration form a useful, legible seek area aligned with the grouping.

**Remove visible Stop from the music footer.** Keep internal Stop and MPRIS Stop
for system commands, logout and shutdown; preserve existing transport semantics.

Compact keeps art, useful title, essential transport, Queue and functional seek.
Shuffle/Repeat can move to native overflow with honest current states where space
requires; Volume remains reachable. No clipping, excessive extra rows or modes
that disappear entirely. Footer remains available with either dedicated panel
view. UI indicators consume the same policy/controller, never a second playing
state.

## MPRIS 2.2

Use the same application policy and in-process session-bus adapter. Publish
read/write **Shuffle** and **LoopStatus** (`None`, `Track`, `Playlist`), validate
property writes, update GTK on external changes and emit correct
`PropertiesChanged` for GTK changes. `CanGoNext`/`CanGoPrevious` reflect actual
policy, available positions and player capability. Preserve Stop and all accepted
metadata/transport/position/volume behavior. No TrackList, daemon, broad bus
access, tokens, account IDs, cookies or signed addresses. Follow the
[official Player interface](https://specifications.freedesktop.org/mpris/latest/Player_Interface.html).

## Screens in the same system

**Home:** genuine artwork/collections and liked music, concise section headers
and useful Play/open actions. Show listening context only when it exists. Remove
three giant navigation shortcuts and repeated page headings. No invented recent
history, recommendations, My Wave or other fillers.

**Library:** compact understandable navigation for liked tracks, owned and liked
playlists, not a dominant dropdown. Real playlist art/title/count/actions form a
concise hero. Creation remains server-confirmed, with honest ambiguous/error
outcomes.

**Search:** four genuine independently handled categories, compact songs,
album/playlist grids and suitable artist treatment. Preserve batch normalization,
paging, Retry, Show All, Back, query/detail context and partial failures.

**All:** truthful loading/empty/saved-data/unauthorized/unavailable/error states;
protected login/session, cache, preferences and explicit safe logout. Technical
diagnostics remain outside music content. Offline audio, My Wave and lyrics
remain outside this task; no nonfunctional controls posing as support.

## Responsive and acceptance contract

Inspect 420×720, 720×650, 900×650, 1280×800, 1440×900, 1920×1080 and true
maximize, plus owner-reported 1650×930; light/dark and 1×/2×. Wide uses a right
column. Medium leaves main row actions usable. Compact uses a dedicated panel
view in the same logical system. Mode/target/open intent survive resizing both
ways, with the footer available. Useful song density and readable long metadata
matter as much as absence of clipping.

Capture populated Home/Library/playlist/Search, both panel modes, footer modes,
few/many collections, missing art, long titles, empty/unavailable/error/loading,
rapid switches and late replies. Synthetic data stays test-only. Compare fresh
same-sized before/after **real GTK** frames. Old SVGs/static final screenshots
cannot pass animation or new owner acceptance.

Record motion beginning, approximately 25/50/75 percent and fully open, plus
closing, with animations enabled. Assert shared relative geometry and inspect
actual frames; mode-switch must not replay the slide. Test reduced motion
separately, actual mouse/keyboard, focus, queue duplicates/edits/limit, mode/EOS
combinations, external MPRIS writes/signals and cleanup.

Run stable and exact Rust 1.98.1 fmt, warning-free all-feature Clippy,
default/all/minimal tests, ordinary build, locked/offline MSRV release, native
GTK/WebKit lifecycle, GStreamer/MPRIS, Flatpak when SDK exists and GitHub Actions.
Preserve historical results. Report authored code, executed fixtures and owner
acceptance distinctly. Handoff: reviewed real before/after images, motion
frames/video, test/subagent reviews, source-matched verified native artifact and
one Fedora 44 guide. Fedora/KDE/Wayland/PipeWire, audible Music and current
Flatpak acceptance stay owner-local; cloud frames cannot certify them.
