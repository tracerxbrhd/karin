<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0024: Bounded continuous track views and stable playback provenance

Status: Accepted for the v0.4 refinement; executed checks and owner acceptance remain separate
Date: 2026-11-01
Extends: ADR-0016's bounded normalization and ADR-0017's exact PlaybackContext occurrences
Supersedes: Visible track-page navigation in the earlier v0.4 workspace
Updated by: ADR-0025 for collection filtering, explicit filtered continuation and stable feed/absolute occurrence identity

## Context

The latest owner request continues PR #15 from `a70bbf7`. Track lists must feel
continuous rather than require previous/next page buttons. The existing API
still has bounded page/batch contracts, optional unavailable entries, strict
identifiers and cancellation/account generations. Removing buttons cannot mean
fetching an entire remote library, duplicating a returned page, losing scroll
position or silently expanding an already playing queue.

The same request replaces the permanently visible idle footer and the old
mandatory Home/Search/Library routes with activity-dependent transport and
Wave/Liked/Playlists. Those presentation choices do not change MusicService,
StreamResolver, GStreamer or local Shuffle/Repeat ownership.

## Decision

Use controller-owned bounded continuous track views over the existing read
contracts. Keep source/account/query identity, admitted next portion, retained
occurrences, in-flight work and terminal/error state distinct. Scroll demand
admits a portion; repeated lower-edge notifications must not enqueue unbounded
work. Accept a returned portion only under its current request/session/source
generation and advance the cursor only when that portion is admitted.

Appending retains existing rows and the user's scroll anchor. New query/source,
refresh, logout and close retire old replies. An error leaves prior rows usable
and requires an explicit read Retry; repeated scroll events must not become an
automatic network-error loop. Keep loading, remote end, partial data and any
local resource/window bound distinguishable. Retain at most **2,000 rendered
positions** and admit at most **100 provider pages** per feed, with the existing
**50-position portions**. Both bounds apply even to empty/sparse provider pages;
a local limit is never proof that the remote collection ended. This uses bounded
append-only GtkListBox rows, not an unbounded or virtualized infinite catalog.
The supported sources are liked tracks, full track search, playlist/artist
tracks and 50-position slices of already admitted album metadata.

A new source may fill an unfilled viewport from at most **three provider pages
in total**. After that, actual forward scroll/wheel or PageDown/End near the
lower threshold admits the next portion, including when a sparse page produces
no useful scroll range. There is no manual Continue or previous/next track-page
control and no automatic empty-page drain. The one-flight and 100-page budgets
still apply. Owned and liked playlist **entity** sections retain separate Show
More reads; removing track pagination must not truncate collections to their
first playlist page.

Known Like/Unlike mutation can change liked-list positions. Before and after
the one POST, all outcomes mark the liked feed `restart_required`: keep prior
rows labelled library-changed and stop continuation. Explicit Refresh starts
page zero under a new feed epoch; never append new-library offsets to the old
snapshot. Invalidating a read also releases its loading/admission state rather
than stranding a spinner. This refresh does not apply retired results, replay
a POST or change the active playback context.

The controller uses **12 bounded read/write lanes**, each with one running and
one latest pending job. Playlist-section refresh uses its own `PlaylistSections`
lane rather than the liked-track Library lane, so a confirmed Like invalidation
does not retire the refresh callback and leave owned/liked playlist loading
flags stranded. The two playlist Show More lanes remain independent. Playlist
creation retires those refresh/continuation tickets and resets view flags;
logout/close invalidate all lanes and clear retained personal hero/feedback.
This is request/view lifecycle separation, not an audio-engine change.

Provider paging remains an internal bounded transport detail. The UI does not
expose previous/next track-page switches, fake an unsupported next cursor or
introduce per-row normalization calls. Applied provider page identity prevents a replayed portion from appending twice;
legitimate repeated Track IDs remain distinct occurrences and are not deduped by
ID. Preserve the existing
batched canonical search lookup and honest unavailable/missing-record state.

Each feed retains bounded provenance for its admitted pages, independent of
other read lanes. Aggregate those records while their rows remain displayed:
any stale page retains the saved-data warning, and storage-error flags combine
without recording URLs or personal fields. A new feed resets the origin set.
Already accepted album metadata seeds its local slices; combined owned/liked
playlist sections use the origins of the sections actually displayed. Recheck
cache leases before admitting asynchronous replies. Already admitted RAM rows
may remain after storage Clear; that does not authorize a retired new reply.

### Catalog growth and playback are separate

Starting a row snapshots the intended loaded source/window with the exact
selected occurrence. Appending display data must not replace or extend the
currently playing PlaybackContext, restart audio or alter MPRIS identity. A
later explicit Play may create a fresh context from the then-loaded source,
bounded to the existing **500-position block containing the selected row**:
`start = floor(index / 500) × 500`. Its opaque source binding includes the feed
epoch and window offset. Mark partial when the window omits loaded or remote
positions. At most four shared Arc context windows are cached, avoiding repeated
full-context cloning during periodic row synchronization. Rows created before an append must not keep stale actions pointing only to an
obsolete page merely because their widgets are retained.

PlaybackContext carries bounded original-position provenance in parallel with
its admitted items: source positions initially identify the original context
occurrences; manual additions have no source position. Removing an item removes
its corresponding provenance entry without renumbering the remaining source
occurrences. The mapping stays within the same 500-position bound and contains
no new network request, audio state or queue ordering policy.

Visible current-row matching uses source/window binding plus original position,
not mutable queue index or Track ID alone. Thus removing an earlier item does
not highlight a different repeated track, and a manual duplicate does not
pretend to be the original catalog occurrence. Shuffle/Repeat and actual queue
history still use the same accepted queue/controller; provenance is a narrow
domain addition for correct UI identity, not a second playback engine.

### Related presentation boundaries

The footer starts hidden without a real playback intent, uses a native SlideUp
transition for an active request/source, remains available during Pause and a
retryable request failure, and hides after explicit Stop/logout/queue clear.
Selection-only metadata is not playback. The artwork/title/artist block is one
accessible Track Info target; Like is a separate real confirmed action.
Unconfirmed Dislike is omitted rather than confused with removing a Like;
published third-party endpoint variants are evidence of implementations, not
proof of their matching Karin semantics or side effects. Direct Like reuses the
existing confirmed read/write/read boundary. Its lazy visible-map read runs
outside the synchronous render/map stack with weak ownership and session/track
guards, so revealing the real player cannot reenter a borrowed controller.

Track Info sizes its already-mounted cover from actual available panel width
and height after measuring readable metadata/actions, within 96–256 logical
pixels. Resize does not recreate art or start a fetch; unusually long content
remains scrollable. Cache provenance follows all retained admitted page origins,
not whichever request finished last.

The explicit Wave tab is the owner's authorized unimplemented landing only.
It does not establish a radio session, recommendations, API support or playable
station. The one pinned TrackInfo/Queue surface and existing modes remain.

## Verification and consequences

Use counted local HTTP and actual scroll/input fixtures for multiple portions,
repeated threshold demand, empty/end/local-limit cases, failure/read Retry,
query/account changes and delayed responses. Assert bounded request/row work,
stable scroll position, exact duplicates and no N+1 requests. Activate old and
new rows after append; verify their context window and original position.
Mix fresh, network and stale/error pages; later network replies must not hide
a warning for retained stale rows. Check local album slices, combined playlist
sections, new-feed resets and cache clear/disable while a reply is in flight.
Exercise current/noncurrent queue removal, manual duplicates and Shuffle/Repeat
without misleading row highlighting or spontaneous context replacement.

Capture actual hidden/appearing/paused/failed/footer states with a real fixture
resolver and generated-media worker. Test the full metadata target by mouse,
Enter/Space and separate Like focus. New before evidence uses the unchanged
a70 production source and original harness, not a9 or a relabelled older after
image. [VALIDATION.md](../VALIDATION.md) records executed results; this ADR is
not an implementation PASS. Keep per-candidate source manifests and binary
fingerprints: a successful visual run or compiler matrix for C7 must not be
presented as an executed check of a later lifecycle correction.

Keep prior CI and cold-paint failures. Graphics-initialized right-panel proof
does not accept first-use responsiveness or the new footer's motion. Follow the
[quality gate](../UI_QUALITY_GATE.md#v04-native-music-gate) and the separate
[Fedora guide](../BUILDING.md#v04-native-music-acceptance-on-fedora). PR #15 stays
Draft; this decision authorizes neither a new PR nor merge/public release.
