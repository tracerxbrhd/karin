<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0036: Exact-occurrence editing of the upcoming queue

Status: v0.8 implementation decision; executed checks and owner acceptance are separate.
Date: 2026-11-14
Extends: ADR-0017, ADR-0023, ADR-0026 and ADR-0031

## Context

The accepted contextual queue has stable opaque occurrence identities, local
Shuffle/Repeat policy, MPRIS and private continuation. Adding controls must not
replace its player or confuse repeated provider Track IDs with queue entries.
A user's future-order edit must not restart the currently playing occurrence,
replay listening history or rewrite a server playlist.

## Decision

Keep operations in MusicQueue and the existing Queue presentation. Append adds
a distinct manual occurrence at the end of the ordinary upcoming plan. Play
Next inserts a new manual occurrence directly after current, ahead of the
remaining Shuffle plan and any forward-history replay. An existing noncurrent
ordinary occurrence can instead be promoted to next without cloning it.

Move by stable source and target OccurrenceId, never a provider ID or stale
physical row number. Reordering applies only to editable upcoming occurrences;
the current occurrence cannot be dragged away. Remap physical indices in current
selection, Shuffle remaining/history and source positions together. A manual
future edit folds forward-history replay back into the editable future while
retaining actual past history. Toggling Shuffle/Repeat and natural EOS retain
the established policy; Repeat One does not make a future edit restart current.
Removing an entry removes only that occurrence, even when IDs repeat.

Manual entries may be interleaved with source entries. `loaded_items` counts
entries with original provenance rather than asserting a contiguous prefix.
Original source positions stay attached to their occurrences across permutations;
they are unique, bounded below 20,000 and need not be monotonic in edited or
sorted playback order. The queue itself remains bounded at 500. Persistence
retains its compatible schema, validates aligned selectors/positions/policy and
restores edited order plus Shuffle/history through fresh occurrence tokens.
No gesture payload, token address or serialized object pointer is stored.

GTK drag-and-drop uses an in-process typed value with weak list ownership,
account/controller generation, the context presentation snapshot and the exact
occurrence. There is no textual serializer/MIME payload accepting outside IDs.
A replaced account/window/context or late gesture cannot retarget another queue.
Accessible Alt+Up/Down and named native menu move actions call the same methods;
keyboard and mouse are equivalent intents.

After an admitted edit, update the existing Queue view, continuation and MPRIS
capabilities from the same state. The current occurrence/source/controller and
its playback position remain intact; no resolver or GStreamer prepare/play is
required merely to edit future order. Details, reactions, search and server
playlist mutations retain their independent contracts.

Do not apply these manual reorder/Play Next operations to My Wave. Its batch
markers and feedback lifecycle describe the admitted radio sequence; rewriting
that sequence would require a separate verified protocol change. Existing Wave
controls remain available, and an ordinary explicit context replaces Wave by
the accepted retirement path.

## Consequences and verification

Domain tests cover duplicates, exact removal, promotion, physical permutations,
all local mode combinations, forward/past history and continuation round trips.
Native fixtures exercise actual drag-and-drop, keyboard alternatives, paused and
playing edits, restored queues and stale gestures. Assert stable current
occurrence, worker/source/position and coherent MPRIS; a list reorder is not
proof of audible playback. Tests use original synthetic media and accounts.

Execution results belong in [VALIDATION](../VALIDATION.md); the one real
Native/installed-Flatpak acceptance remains in
[BUILDING](../BUILDING.md#v08-final-fedora-acceptance). The owner-accepted v0.7
restoration is preserved, not inferred acceptance of these new v0.8 gestures.
