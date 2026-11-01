<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin v0.4 — Cassette-like refinement and continuous collections

## Authority and scope

The owner's latest [full task](CODEX_TASK.md), starting from
`a70bbf714e3d7ea775c03839eb01f0b1d9dd4f62` in existing **Draft PR #15**, supersedes
conflicting previous v0.4 requirements. This is a refinement of the working
music client, not a replacement application or a new concept from scratch.
No new PR, merge or branch deletion. Earlier runtime, cloud/CI and owner results
remain historical evidence; none automatically accept this iteration.

Use the supplied Cassette-like screenshot as a **composition reference**: light
upper segments, a real collection hero, calm song rows, one coherent music-info
sidebar and restrained transport. The primary agent inspected that screenshot;
it contains desktop/personal material and is not copied into this repository.
No reference art, account content, icon paths or branding is an application asset.
Previously inspected public sources remain qualified in
[UX_RESEARCH.md](UX_RESEARCH.md), including the distinction between Cassette's
released screenshots and its unfinished main-branch rewrite.

Keep Rust/GTK4/Libadwaita/GStreamer, current authentication/persistence, MusicService,
StreamResolver, PlaybackController, queue/policy, MPRIS and cache boundaries.
The old permanent-footer/rail/dropdown/page-button sketches are **historical**
where they conflict with the rules below. The implementation review preserves
their executed results separately from new evidence.

## Light primary navigation

Use three clearly named native tabs/segments: **Моя волна**, **Нравится**,
**Плейлисты**. Place them in or directly beside the compact HeaderBar so they
read as the main navigation, not another settings control. Keep native window
controls, accessible labels, selection/focus states and useful compact labels.
If the smallest HeaderBar cannot fit everything, adapt a short adjacent segment
row rather than clipping labels or reverting to a primary dropdown.

- **Моя волна:** an explicitly nonworking product placeholder authorized by this
  task. Show a brief truthful unavailable state. No functional Play, fake station,
  recommendations, remote Wave request or claim that service radio works.
- **Нравится:** real liked tracks and relevant actually returned liked entities,
  with clear small sections where useful. No collection-type dropdown.
- **Плейлисты:** owned and liked playlists in straightforward visual sections,
  preserving real creation/open actions and empty/loading/error states. Avoid
  dropdowns and unnecessary intermediate routes.
- Search remains a coherent HeaderBar action/view with a clear return path and
  the existing four real categories. Settings/account/diagnostics stay secondary.
  Queue is a footer/panel action, not a primary page.

Name a view once. Prefer the application name in the primary HeaderBar rather
than repeating the selected segment and collection label a third time. Compact
title/Back reflect the actual panel or search view;
closing restores the underlying tab and usable focus. Changing tabs, loading
another batch or searching never dismisses a pinned panel or restarts playback.

## Music composition and visual system

Refine the existing native spacing, typography and art hierarchy. Use Libadwaita
system/light/dark colors, symbolic icons, accessible names and normal hover/focus/
selected states. No glass, decorative gradients, fake catalog or heavy card wall.
Keep visible prose rare; retain real errors, Retry and confirmations.

Use a small spacing scale near 4/8/12/16/24 logical pixels. Collection art/title/
concise facts/primary Play form one compact hero. Quiet song rows keep roughly
60–66px height and 44–52px art, legible artist/duration, exact-current-occurrence
emphasis and contextual secondary actions. Natural-size playlist/album tiles
wrap without stretching one item into a giant card. Do not restore an arbitrary
narrow main-content clamp or an unexplained strip beside the panel.

Empty/few/large collections, long names, missing art and unavailable tracks are
normal states, not reasons to insert unsupported content. The Wave placeholder
is the only explicitly authorized new nonworking product destination; it does
not authorize fake related songs, radio, offline audio or lyrics elsewhere.

## Universal musical side panel

Retain one controlled `Closed | TrackInfo | Queue` surface, its shared header,
width policy and whole-surface opening/closing. Mode/open intent, responsive
presentation, availability, information target and actual queue occurrence remain
separate. Do not use transient GTK animation progress as the user's chosen mode.

TrackInfo ↔ Queue switches contents **in place**, without replaying the slide.
Background, edge, header, art, text and buttons move together on open/close.
Outside clicks, scroll, navigation, playback, pause/seek/volume, data updates and
resize preserve mode/open intent. Diagnostics may suspend it without forgetting
intent; logout/close retires personal content and pending jobs safely.

### Track Info refinement

Cover, title, artist, real known album and verified actions form one centered,
readable musical cluster rather than unrelated technical rows. Keep their
alignment and comfortable text width coherent in a wide sidebar and the larger
compact dedicated view. The header is quiet; no duplicate large title or full
second transport console. Unsupported related/recommendation blocks stay absent.
Artwork must also respond to available **height**: normal title/artist/album and
actions must remain visible above the active footer at 420×720, 720×650 and
900×650. Long genuine metadata may scroll; it must not force short ordinary
metadata actions below the fold merely to preserve an oversized cover.

Explicit Details never plays. A current metadata-block click shows current
information; a new actual playback occurrence (including repeated IDs at another
position) resumes following without closing. Pause/Seek/Volume preserve explicit
other-track selection. Retain session generations, detail epochs and artwork
cancellation against mixed metadata, late replies and old-account data.

### Queue refinement

The same panel presents current / upcoming / history in a clear musical order,
with current artwork/emphasis and compact supported actions. Keep count/source/
partial boundary and Clear reachable above the scrolling rows. Select/play,
manual add/remove, Details and clear keep their existing safe semantics and
occurrence/session guards. Queue order reflects actual Shuffle/Repeat policy.
It is not the Yandex synchronized cloud queue. The 500-position PlaybackContext
and honest loaded-prefix boundary remain intact.

### Entry and responsive rules

The **whole current-track metadata region**—cover, title, artist and surrounding
metadata space—is one clickable/focusable target for Track Info. A click while
already in Track Info keeps it open; from Queue it switches in place. Current
cover is no longer the only hit area. Keep Like and other independent actions as
separate targets, not interactive buttons nested inside a metadata button.

Ctrl+I / explicit close / compact Back remain intentional controls. Footer Queue
and Ctrl+J open Queue in this same surface; repeated Queue can explicitly close.
On compact layouts use the dedicated explicit panel view if a usable side column
cannot fit. Preserve mode/target across resize. The Player Bar remains usable
**when active**, not forcibly visible merely because a panel is open.

## Player Bar lifecycle and interaction

**Idle is hidden.** With no active track/playback intent, the bottom bar occupies
no permanent empty strip and contributes no hidden focus targets. Metadata-only
Details, browsing a library, opening an empty queue or selecting an unplayed
item must not pretend a player session has started.

After real track activation, reveal the existing bar smoothly from below using
native GTK/Libadwaita animation; the whole footer, not isolated controls, appears
coherently. Active loading/playing/paused states retain it. A failed active play
attempt retains understandable error/explicit Retry controls; a full Stop,
cleared active track, logout or closed session may hide it. Pause **never** hides
it. Do not add fake restoration or autoplay: an active restored state may show
only if that state really exists.

Footer visibility consumes the existing application/player snapshot and explicit
play intent, not a second audio state. No panel/visibility animation restarts the
worker, replaces the queue or changes MPRIS playback status. Retain reduced
motion and focus-safe show/hide. No visible Stop button; controller/MPRIS Stop
remain supported for system commands, logout and shutdown.

Wide: current metadata/real Like at left; genuinely centered Shuffle, Previous,
Play/Pause, Next, Repeat; Queue/Volume at right, functional progress/time. Text
length must not move transport. Compact keeps useful metadata and primary
controls in a readable two-row composition, with honest native overflow for
modes/volume as necessary. Start loading uses a stable-size native spinner,
without changing the bar's natural height midway through the reveal. Repeat One
retains a literal visible **1**, independent of the theme's repeat-song icon.

## Direct Like; honest Dislike boundary

Reuse the existing server-confirmed Like contract. The footer's real action
reads membership if needed and performs the typed write/readback asynchronously;
update state only after confirmation. Show loading/failure and retain the previous
confirmed state on errors. A single direct action must not become a decorative
heart or require opening a developer-style action menu. Account/track generations
retire old replies; never automatically repeat an ambiguous POST.

Published clients expose Dislike endpoints, so lack of implementation must not
be described as “the Yandex API has no Dislike.” However, Karin has no verified
matching write/state/side-effect contract for this integration. **Hide unconfirmed
Dislike in ordinary UI**, as the task permits. Do not infer dislike from removing
a like, fabricate local success or present an enabled decorative control. Record
what was verified and what remains unknown separately from service restrictions.

## Continuous track lists, bounded work

Replace explicit Previous/Next track-page controls with incremental scroll-driven
loading wherever track pagination existed: liked tracks, relevant track search
and album/artist/playlist details. Preserve genuine four-category search, Back,
query and entity navigation. Non-track entity pagination may remain where needed;
it is not a reason to retain rough track-page switching.

- Request bounded existing-service batches, one in-flight continuation per
  source/generation. Provider batches retain the existing 50-position contract.
  Initially prefill at most three provider pages; thereafter an actual wheel,
  PageDown or End gesture near the lower threshold admits a continuation, even
  if a sparse viewport still has zero scroll range. Stop at the real end.
  Do not fetch an entire remote collection in a hidden loop or block GTK.
- Append each provider page/position range at most once. Preserve legitimate
  duplicate Track IDs at distinct playlist positions; deduplicate retries/page
  application, not musical occurrences merely by identifier.
- Keep already displayed rows and scroll position while appending. Show concise
  tail Loading, End or an actionable inline Retry; an error must not erase the
  useful loaded prefix or be disguised as successful end-of-list.
- Bind replies to the current account, tab/query/detail source and request
  generation. Reject retired/out-of-order results. Cancel safely on navigation,
  logout and close, and do not let a shared busy flag discard user actions.
- Old rows activated after a new batch use the current loaded source and exact
  original position. New explicit playback takes the aligned **500-position
  window containing that position**, with source offset, original positions and
  an opaque epoch; it does not always select the first 500 or renumber identity.
  Scrolling never replaces or restarts the already active immutable context.
  Partial/context-window limitations remain honest.
- The view is bounded to **2000 rendered positions and 100 provider pages**.
  Reaching a safety bound is a labeled partial limit, not successful end-of-list.
  No manual Continue or track-page button is reintroduced for sparse data; actual
  scroll gestures and error Retry retain their distinct meanings.
  Creation and mutable likes/membership remain network authoritative, with the
  existing cache provenance rules.

## Preserved local Shuffle and Repeat


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

## Preserved MPRIS 2.2


Use the same application policy and in-process session-bus adapter. Publish
read/write **Shuffle** and **LoopStatus** (`None`, `Track`, `Playlist`), validate
property writes, update GTK on external changes and emit correct
`PropertiesChanged` for GTK changes. `CanGoNext`/`CanGoPrevious` reflect actual
policy, available positions and player capability. Preserve Stop and all accepted
metadata/transport/position/volume behavior. No TrackList, daemon, broad bus
access, tokens, account IDs, cookies or signed addresses. Follow the
[official Player interface](https://specifications.freedesktop.org/mpris/latest/Player_Interface.html).

## Verification and evidence for this refinement

Create fresh source-matched real GTK evidence; historical a70/C10 screenshots are
before evidence only. Inspect wide dark, wide with Info, Queue, hidden idle bar,
visible active/paused bar, honest Wave placeholder, Liked, Playlists, Search,
compact and light/HiDPI states. Retain the established wide/medium/compact and
boundary-size matrix, owner-wide 1650×930, true maximize, both themes and scales.

Use actual mouse/keyboard to open Info from cover, title, artist and empty
metadata hit area; test separate Like/modes/queue focus, repeated activation,
outside clicks, tab navigation, resize, logout and late replies. Check the entire
footer bottom-up reveal/hide at intermediate native frames, pause persistence,
Stop/no-active hide and reduced motion separately. Preserve the universal panel's
whole-surface normal-motion checks and the exact documented initialization scope.
Earlier cold first-paint failures remain historical; warm measured cycles do not
establish ordinary cold-start responsiveness, and production must not prewarm.
The [implementation review](IMPLEMENTATION_REVIEW.md) records the current
source-matched execution scope, retained failures and first-reveal paint gaps;
a passing initialized-renderer panel matrix must not be used to accept a raw
first-footer sequence with missing smooth intermediate presentation.

Continuous-list fixtures must cross multiple real local HTTP pages, prove one
application per page, true end, tail error/Retry, stable scroll, old-row activation,
duplicates, cancellation/stale replies and preserved queue/audio. Like tests use
real typed local fixtures, not fake production data. Dislike limitations remain
visible in development/acceptance notes, not as a pretend working button.

Run stable and exact Rust 1.98.1 formatting, warning-free all-feature Clippy,
default/all/minimal tests, builds and relevant GTK/WebKit/GStreamer/MPRIS/policy
regressions, locked/offline release, license and Flatpak checks when available,
and all GitHub Actions. Preserve historical results and state current executed
checks separately. No warning/test suppression or weakened evidence standards.

Handoff includes coherent current screenshots, native footer/panel motion,
continuous-loading/input proof, verified artifact, role/review results and precise
limits. Fedora/KDE/Wayland/PipeWire, real-account Like/session/library behavior,
audible Music, cold first appearance and current Flatpak acceptance remain
owner-local. Cloud fixtures never certify them. PR #15 stays Draft.
