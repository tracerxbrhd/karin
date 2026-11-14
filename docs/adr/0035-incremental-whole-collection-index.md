<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0035: Explicit incremental indexing of complete supported collections

Status: v0.8 implementation decision; live large-library acceptance is separate.
Date: 2026-11-14
Extends: ADR-0016, ADR-0024, ADR-0025 and ADR-0031

## Context

The accepted loaded-collection filter was honest about its 2,000-position prefix,
but could not find a known track outside that prefix. Raising the number of
native rows or fetching the complete collection on startup/every keystroke would
create different performance and privacy problems. Provider track IDs also do
not identify duplicate occurrences or prove current collection membership.

## Decision

Add `music::library_index` with a process-only current collection manifest and
independent canonical batches. An explicit “Искать во всей коллекции” intent
reads the existing liked membership endpoint or playlist `richTracks=false`
manifest once, under current account/source binding. Limit this specialized
compact response to 4 MiB and 20,000 original positions; retain the ordinary
1 MiB API ceiling elsewhere. Validate identifiers, owner/kind where supplied,
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

The user can pause/resume indexing and Retry a failed read. Opening the app or
typing a character does not start it. Query changes apply to already admitted
metadata; ongoing explicitly requested batches use the current query when
admitted. Preserve progress and distinguish incomplete coverage, omissions,
errors, no matches and a fully read collection. Do not claim no remote match
from an unfinished prefix.

Keep the index in the current application's account-bound RAM and reuse it
across ordinary navigation. Do not add a persistent database, cache quota,
startup whole-library scan or per-character membership/metadata requests. The
existing metadata/artwork/audio stores retain their shared CacheBudget and
HTTP/current-access policy. Logout, source/account replacement, refresh and
collection changes retire old index work. Normal navigation preserves useful
filter/sort/result-window and scroll state without changing playback.

Build Unicode-normalized/casefolded title/artist/album search keys once per
admitted track. Sort title or artist with deterministic original-position ties;
provider and reverse-provider sequence remain explicit alternatives. Added-date
sorting is offered only when every manifest occurrence supplies a valid RFC3339
`TrackShort.timestamp`. The admitted RFC3339 subset validates calendar/leap days
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
must locate a result past the old prefix, sort and navigate while asserting no
per-key network work or active queue/worker change. Fixtures and source decisions
are not live-library acceptance; executed results belong in
[VALIDATION](../VALIDATION.md), with one
[Native/installed-Flatpak procedure](../BUILDING.md#v08-final-fedora-acceptance).
