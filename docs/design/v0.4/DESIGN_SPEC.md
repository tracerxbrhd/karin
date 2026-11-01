<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin v0.4 — final collection and transport refinement

## Authority and scope

The owner's latest [full task](CODEX_TASK.md), starting from
`2e3ae41a8480b8f5f437f739c02cb925af69e884` in existing **Draft PR #15**, supersedes
conflicting previous v0.4 requirements. This is a refinement of the working
music client, not a replacement application or a new concept from scratch.
No new PR, merge or branch deletion. Earlier runtime, cloud/CI and owner results
remain historical evidence; none automatically accept this iteration.

Use the two supplied side-by-side screenshots as **composition evidence**, not
pixel templates. The owner compares Cassette on the left with Karin on the right:
a collection needs its own expressive identity, useful local search and coherent
transport grouping. The primary agent inspected the private screenshots; this
specification does not claim independent pixel inspection by every contributor.
They contain desktop/personal material and are not copied into this repository.
No reference art, account content, icon paths or branding is an application asset.
Previously inspected public sources remain qualified in
[UX_RESEARCH.md](UX_RESEARCH.md), including the distinction between Cassette's
released screenshots and its unfinished main-branch rewrite.

Keep Rust/GTK4/Libadwaita/GStreamer, current authentication/persistence, MusicService,
StreamResolver, PlaybackController, queue/policy, MPRIS and cache boundaries.
The old permanent-footer/rail/dropdown/page-button sketches are **historical**
where they conflict with the rules below. The implementation review preserves
their executed results separately from new evidence.

## Current execution checkpoint

The [exact C14 gallery](evidence-final-refinement/README.md) records 260 static
originals, 108 matched before/current GTK and PNG dimensions, and 240 initialized
panel frames across 48 strict trajectories. Six native stages pass with fatal
GTK warnings enabled; hidden-footer horizontal requests are also verified.
The [implementation review](IMPLEMENTATION_REVIEW.md#executed-c14-native-evidence-and-current-acceptance-boundary)
records passed regular checks on stable 1.99.1 and exact Rust 1.98.1, the retained
C12 result of 48 passes and 8 nonpassing cases, and the historical C13 targeted
regressions. The complete local C14 native run passes 56/56 (28 selectors at
both scales). Two navigating WebKit fixtures remain unsupported locally and
hosted-required. Hosted CI and archive validation are pending at documentation
freeze; their actual outcomes remain in the PR record.

The C14 collection-filter selector passes at 1× in 85.04 seconds and at 2× in
86.50 seconds. All 69 selected public originals are individually reviewed with matching SHA-256. Fresh-process
first-footer gaps of 900,018 μs and 850,017 μs, with one intermediate frame each,
remain unaccepted: lifecycle is not smoothness, and cause is unknown. Earlier
cold failures, including 116,669 μs, remain history. No Fedora acceptance is
inferred.

## Light primary navigation

Use three clearly named native tabs/segments: **Главная**, **Нравится**,
**Плейлисты**. Place them in or directly beside the compact HeaderBar so they
read as the main navigation, not another settings control. Keep native window
controls, accessible labels, selection/focus states and useful compact labels.
If the smallest HeaderBar cannot fit everything, adapt a short adjacent segment
row rather than clipping labels or reverting to a primary dropdown.

- **Главная:** a useful music starting page, not the previous giant Wave-only
  placeholder. Offer the existing real Liked/Playlists/Search routes and concise
  real content where useful without duplicating the complete neighboring pages.
  Reserve a modest read-only Wave section with an honest unavailable status.
  No functional radio Play, fake station/recommendation, hidden Wave request or
  claim that service radio works. Future native sections do not require a plugin
  framework or an application-wide navigation rewrite.
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

Use a small spacing scale near 4/8/12/16/24 logical pixels. Quiet song rows keep roughly
60–66px height and 44–52px art, legible artist/duration, exact-current-occurrence
emphasis and contextual secondary actions. Natural-size playlist/album tiles
wrap without stretching one item into a giant card. Do not restore an arbitrary
narrow main-content clamp or an unexplained strip beside the panel.

Empty/few/large collections, long names, missing art and unavailable tracks are
normal states, not reasons to insert unsupported content. Only the compact
read-only Wave status on Home is explicitly authorized as an unimplemented
feature; it does not authorize fake related songs, radio, offline audio or lyrics.

## Liked collection identity and truthful facts

Liked is a full musical collection. Group an original native heart cover,
**Мне нравится** title, concise facts and the existing Play/Refresh actions as
one hero above the quiet track list and local search field. The theme's symbolic
heart and modest theme-accent surface are original Karin composition, not a
Cassette graphic. Do not fetch service artwork merely to decorate this identity.

On wide content the cover is around 192 logical pixels; medium around 144;
compact around 88 with metadata/actions beside it. Use actual collection width,
including space reserved by an open panel, not just the whole window width.
Compact must not become a huge vertically stacked poster. Keep short title,
facts and actions readable at normal font sizes and avoid unexplained gaps.

Facts describe **all admitted source occurrences**, not the currently filtered
subset or the first provider page. While remote continuation, omissions, local
limits or uncertain/error state remain, label the count as **Загружено**. State a
total only when the source is actually complete. A collection duration is allowed
only for complete data with every duration known and a checked non-overflowing
sum; repeated occurrences count separately. Missing duration or overflow removes
the total, not the whole collection. Empty/error/loading remain truthful, and
the existing feed/controller owns Play sensitivity and request generations.

## Search inside an opened collection

Provide one reusable compact native search field below the hero for Liked and
every opened owned/liked playlist whose tracks are available. Reuse it for other
compatible full track lists. It is separate from global Yandex Music search and
does not appear on each grid tile. Use a search icon, short **Поиск по трекам**
placeholder and clear action; it must not dominate the screen.

Filter by known title, artist and album as text changes, with Unicode
**NFKC → GLib casefold → NFKC**, including Cyrillic and compatibility capitals
such as mathematical 𝐀/plain a; all query tokens must match.
Build the bounded text index once per admitted Track rather than recreating
Track models on every edit. Enter is not required for each query.
Clearing restores the source view; changing source retires the old query
and replies. Empty-result copy distinguishes no loaded matches from no matches
in an entirely complete collection. Do not invent an unverified server search
endpoint or send collection/metadata requests per character. The unchanged
bounded artwork loader may load covers when retained rows become visible;
that is not a collection-search or page request.

Preserve bounded continuous loading: a partial filter covers only admitted data,
later authorized pages extend matches gradually, and the coverage limit remains
visible. A partial filtered view offers **Искать в следующей части**; it admits
one bounded read per user action as the newest task permits. This is
not a return of unfiltered page navigation or an automatic drain of all pages.
Tail read Retry remains distinct from continuing a successful partial search.

A result retains its original absolute source position, including duplicate Track IDs.
Click/Enter/Space plays that exact visible occurrence. New playback follows a
clear bounded visible-result-order context, with at most 500 entries around its
admitted result window; filtering/clearing/appending alone never replaces the
already playing queue. The stable source identity remains separate from query/
window state, retaining original occurrence provenance. Manual additions retain their
pre-existing physical-index Info/MPRIS fallback; earlier-row deletion can change
that opaque identity without restarting audio. Stable manual-entry IDs are
outside this refinement (ADR-0025). Share source Track models
where possible. No collection sorting is added: provider order stays intact,
avoiding a claim of globally sorted remote data from an incomplete local prefix.

## Playlist grid refinement

Keep owned/liked sections, real covers, confirmed creation and existing open/
loading/error/keyboard behavior. Increase native covers to approximately
180–210 pixels wide, 145–175 medium, and a width-fitting compact grid. A sparse
single playlist must retain its natural card size. Creation uses the same canvas
and spacing rhythm as neighboring covers. Resize existing artwork rather than
recreating thumbnails or issuing new HTTP merely because a window changes size.

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
actions must remain visible above the active footer at 420×720,720×650 and
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
motion and focus-safe show/hide. The native inner footer child must release its horizontal minimum/natural
request while initially or fully closed; reveal shows it only after applying
actual host layout, and partial-close frames remain intact.
No visible Stop button; controller/MPRIS Stop
remain supported for system commands, logout and shutdown.

Wide: current metadata/real Like at left; genuinely centered Shuffle, Previous,
Play/Pause, Next, Repeat; Queue/Volume at right, functional progress/time. Text
length must not move transport. Keep the independent Like immediately associated
with metadata instead of marooned at the far edge of a large empty column.
Prefer the stock recognizable heart when the active icon theme supplies it;
confirmed active color/state, tooltip and accessible action convey membership.
Changing a glyph does not change the existing server confirmation semantics.
Use consistent symbolic sizes, comfortable hit areas and tighter spacing between
related controls. The wide footer uses equal native side/center columns so long
metadata cannot move the transport center. A width-bounded timeline and time sit
under its controls. Place Like as a separate nearby sibling of the natural-size
metadata target, not a nested button or far-away column-edge ornament.
Compact keeps useful metadata/Like/utilities above the same five inline
Shuffle/Previous/Play-Pause/Next/Repeat controls and a full-width bounded timeline;
this is one responsive view, not independently stateful players. Secondary
volume uses its normal native popover. Start loading uses a stable-size native spinner,
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

Create fresh source-matched real GTK evidence against the **2e3ae41** comparison
revision. Historical a70/C9/C10 images remain evidence of their own source only.
Inspect the new Liked hero, active Liked filter, opened-playlist filter, larger
playlist covers, useful Home, wide/compact transport, hidden idle player,
TrackInfo/Queue, light/dark and HiDPI. Include **420×720,850×750,900×650,
1440×900, 1650×930, 1920×1080**, true maximize and established boundary cases.
Compare logical sizes rather than the accidental relative width of two private
reference windows. Open the images themselves; allocation checks alone cannot
accept the new hierarchy, density, heart identity or footer composition.

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
Retain the observed roughly 117 ms initial footer frame gap and unknown cause.
Investigate and fix a demonstrated cause if one is found; do not increase the
240 ms duration, add an artificial delay or claim a new cold PASS from a warm
repeat. Ordinary fresh-start Fedora appearance remains an independent gate.

Continuous-list fixtures must cross multiple real local HTTP pages, prove one
application per page, true end, tail error/Retry, stable scroll, old-row activation,
duplicates, cancellation/stale replies and preserved queue/audio. Like tests use
real typed local fixtures, not fake production data. Dislike limitations remain
visible in development/acceptance notes, not as a pretend working button.
Add collection-filter Unicode/Cyrillic/title/artist/album, clear/empty/partial,
continued append/Retry, query/source isolation and duplicate-occurrence playback
fixtures. Confirm a filtered Play uses result order without modifying an already
playing context. Check truthful hero totals/duration and empty/error action
sensitivity. Density checks must allow the newly requested expressive hero plus
local field while requiring useful whole rows, not squeeze it back into the old
tiny summary merely to retain an obsolete row count.

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
