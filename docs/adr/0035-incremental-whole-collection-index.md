<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0035: Search-driven incremental indexing of complete supported collections

Status: v0.8 implementation decision; functionality owner-accepted.
Date: 2026-11-14
Refined: 2026-11-18
Extends: ADR-0016, ADR-0024, ADR-0025 and ADR-0031

## Context

The accepted loaded-collection filter was honest about its 2,000-position prefix,
but could not find a known track outside that prefix. Raising the number of
native rows or fetching the complete collection on startup/every keystroke would
create different performance and privacy problems. Provider track IDs also do
not identify duplicate occurrences or prove current collection membership.

## Decision

Use `music::library_index` with a process-only current collection manifest and
independent canonical batches. A settled nonempty query or Enter in an eligible
incomplete collection starts or resumes that index. The existing liked
membership endpoint or playlist `richTracks=false` supplies a manifest under
current account/source binding. Limit this specialized compact response to
4 MiB and 20,000 original positions. Ordinary requests retain the 1 MiB API
ceiling. Validate identifiers, owner/kind where supplied,
revision and complete count. A larger/unsupported response is an explicit limit,
not a claim that the supported prefix is the whole remote library.

Normalize consecutive slices through the existing `/tracks` batch, at most
50 original positions per worker request. Keep every original selector position
separate even when IDs repeat; missing canonical rows leave holes, not shifted
occurrence identities. One active batch and the normal request generations
bound work. Read the manifest again before declaring complete coverage and
require identical full sequence, source/account/token binding and revision.
A changed membership is a visible restart/refresh condition, not mixed old/new
pages or an implicit replay of a mutation.

The shared track-list toolbar spans the available track-list column. It allocates
the native sort button its compact width and gives the entry the remaining space. Query changes filter admitted title, artist and album metadata
immediately. About 300 ms of quiet input starts the eligible whole-collection
work, with Enter applying the query immediately. Index read batches are
independent of query text. Each admitted batch is projected using the latest
query, without restarting a manifest or issuing one HTTP request per keystroke.
Opening the app or a list alone does not start indexing. Global catalog search
uses its own entry in the top navigation. It presents results on submission
after roughly 350 ms of quiet input or immediately on Enter, retaining its
request generations, sections, paging and Retry.

Clearing the query or leaving the view stops indexing while retaining admitted
metadata. Return alone preserves view state without starting more reads.
After clearing a partial index, «Загрузить ещё» at the end of the ordinary
list admits one batch of up to 50 positions. Reaching the end still requires
the final membership recheck.
Contextual progress and compact Pause, Continue or Retry actions communicate
incomplete coverage, omissions, errors, changed membership, the technical limit
and a fully read collection. A user pause or failure requires an intentional
resume or Retry. An unfinished prefix cannot establish no remote match.

Keep the index in the current application's account-bound RAM and reuse it
across ordinary navigation. Do not add a persistent database, cache quota,
startup whole-library scan or per-character membership/metadata requests. The
existing metadata/artwork/audio stores retain their shared CacheBudget and
HTTP/current-access policy. Logout, source/account replacement, refresh and
collection changes retire old index work. Normal navigation preserves useful
filter/sort/result-window and scroll state without changing playback.

Build Unicode-normalized/casefolded title/artist/album search keys once per
admitted track. Sort title or artist with deterministic original-position ties.
Provider and reverse-provider sequence remain menu alternatives. Mark the
selected order and retain it across ordinary navigation. Order projections use
maintained keys so GTK input does not repeatedly sort all matching tracks.
Added-date sorting is available when every manifest occurrence supplies a valid
RFC3339 `TrackShort.timestamp`. The admitted subset validates calendar/leap days
for 1970–9999, up to nine fractional digits and Z/explicit offsets; leap-second
60 is conservatively unavailable. Missing/unsupported timestamps disable date
choices without rejecting the track, guessing a date or relabelling provider
order. Indexing and sorting are local projections,
not remote membership mutations.

Display bounded 200-row result windows rather than constructing 20,000 native
rows. Explicit Play snapshots at most 500 matches in the selected projection
window, retaining each original source position below 20,000. Positions need
not be monotonic after sorting, but remain unique occurrence provenance. The
active queue/controller/MPRIS are immutable during search, sort, batch admission
and navigation; only an explicit Play creates a new context. Manual queue edits
and restoration use the same aligned provenance defined in ADR-0036/0031.

## Consequences and verification

This extends ADR-0024/0025's display-only prefix with a deliberate complete
supported-collection path. Their bounded ordinary loading behavior remains the
startup path, while indexed results have their own bounded display window.
“Complete” describes an admitted, rechecked membership snapshot, not an offline
right or a promise that the remote collection cannot change later.

Use synthetic manifests beyond 2,000 positions, repeated/composite selectors,
omissions, wrong owners, count/revision/sequence changes, timestamp combinations,
query changes, cancellation and late account/logout replies. Native input tests
must type and locate a result past the old prefix, exercise Enter, debounce,
clear and cancellation, and use the sort menu by mouse and keyboard. They assert
bounded requests and preserve the active queue, worker and occurrence identity.
Record executed checks in [VALIDATION](../VALIDATION.md) and check future relevant regressions
with the [Native/installed-Flatpak guide](../BUILDING.md#desktop-regression-checks).
