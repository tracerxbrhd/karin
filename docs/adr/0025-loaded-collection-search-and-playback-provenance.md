<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0025: Loaded collection search with original occurrence identity

Status: Accepted for final v0.4 implementation; executed and Fedora acceptance remain separate
Date: 2026-11-02
Extends: ADR-0024 bounded views and ADR-0017 exact playback occurrences

## Context

The final owner task continues Draft PR #15 from `2e3ae41`. Liked and every
opened accessible playlist need incremental local search without changing the
working MusicService, authentication or audio path. Existing confirmed contracts
provide liked/playlist pages and global catalog search; Karin has no verified
server-side collection-search contract. A global catalog result cannot stand in
for membership in one user's collection.

Filtering one loaded prefix must not assert that an entire remote collection
has no matches. A playlist can repeat the same Track ID, and a filtered row at
local position zero can represent original source position 500. Renumbering
those positions would confuse current-row, information and MPRIS identity.

## Decision

Use one UI/application-layer projection over the already admitted typed tracks.
Build a normalized search index once per original position, using Unicode
case folding and normalization for title, artists and known album. Keep an
ordered vector of matching original positions and a bounded reverse lookup;
reuse existing track rows/models instead of cloning the complete collection
on every keystroke or player tick. Preserve original provider order. Sorting
the complete remote collection is unconfirmed and is not presented as a working
global sort over an incomplete page.

Typing updates the loaded projection without collection-metadata requests. Query/layout changes must
not accidentally trigger scroll-driven continuation or restart scheduled
viewport filling. A deliberate next-part action or genuine forward scroll can
admit one existing bounded read. New results are indexed and filtered under
the current query. Existing bounded artwork work may resume when filtering maps a newly visible
cover; this is not an extra MusicService/page query or whole-library drain.
Source/session epochs retire old widgets and responses;
a different collection starts with an empty query. Read errors retain the
accepted prefix and explicit Retry. Empty matches, remote end, missing data,
stale membership and the local limit remain distinct states.

The existing limits remain: 50-position provider portions, at most 2,000
admitted positions and 100 pages per view, at most three initial viewport-fill
pages, one running/one latest pending request per lane and 500 queue positions.
Search does not expand those bounds or silently fetch the whole library.
The UI explicitly labels loaded-only coverage while further remote portions
or omitted records remain; a local zero is not a remote no-match claim.

New Play snapshots an aligned window of at most 500 matching occurrences,
including the selected row, in the visible result order. It carries the feed's
stable opaque identity and each item's absolute original admitted position.
The narrow validated PlaybackContext factory rejects length/position errors;
ordinary constructors, physical queue indices and policy ownership are retained.
Manual additions remain outside original source provenance. Source positions
are metadata, not a second playback order or another audio state.

Current-row, information and MPRIS identity use original position plus source
binding/Track ID, with a distinct manual fallback. Thus two equal IDs at global
0 and 500 do not alias merely because each filtered window selects local zero.
Removal/reindexing does not turn one original source occurrence into another.
Manual additions retain the pre-existing physical-index fallback; deleting an
earlier row can therefore change their opaque Info/MPRIS identity without
restarting playback. Stable manual-entry IDs are outside this refinement. Filter,
clear-query, append and view changes leave the active immutable queue and audio
unchanged; only explicit new playback replaces its context.

## Verification and limits

Use Unicode/Cyrillic/combining-character tests, exact duplicates across 500-item
boundaries, query clear/source reset, sparse/missing/unavailable records and
invalid provenance. Native fixtures must type in the real SearchEntry, exercise
actual row mouse/Enter/Space, hold continuation responses, retry failures and
verify no per-key collection-metadata work, no lost order and no queue/worker change from
filtering. Test delayed source/logout callbacks and identity shared with MPRIS.

The visible view remains a bounded GtkListBox, not unbounded virtualization.
These decisions do not establish an unverified server endpoint or guarantee
whole-library search beyond the admitted boundary. Source-matched screenshots,
structural checks and executed results belong in VALIDATION; real Fedora music,
first-cold footer/panel, physical desktop integration and current full Flatpak
remain separate owner gates. Preserve every earlier failure and do not alter
timing, sandbox, renderer or phase assertions merely to obtain green tests.
