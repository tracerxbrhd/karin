<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0002: Service ownership and bounded workers

Status: Accepted; current implementation contract.

## Decision

Separate GTK presentation, application intents, typed MusicService/API access,
session/storage, queue and the one playback worker. Widgets consume domain values; they do
not construct HTTP requests or own a second playback state machine.

Run bounded blocking HTTP, parsing and filesystem work on GIO/application-held workers,
returning results to the main context. GTK widgets/textures stay on GTK. Keep one running
and one latest pending request per ordinary lane (currently 16), independent
navigation/membership/editor lanes and a separate serialized Wave feedback FIFO. Writes
and feedback cannot be coalesced or replayed as reads. Reqwest owns its HTTP runtime; add
no top-level daemon or competing async runtime.

Account, request, source and playback-intent generations fence old results. Cancellation
retires presentation immediately even if an admitted bounded HTTP request takes until its
deadline to settle. Use weak UI ownership; delayed work cannot resurrect a closed window,
retired view or replaced session. Deferred membership reads must run outside synchronous
map/render/controller borrows.

The audio worker owns GStreamer. Shutdown bypasses command capacity and reaches Null
without GTK joining the worker. Application holds/work fences let admitted durable
transactions finish before a replacement window starts conflicting I/O. A queued request
is not evidence that cleanup or persistence succeeded.

## Performance and verification

Initialize a new catalog row's current marker directly; reconcile the full set only when
its real context/occurrence/presentation changes. Unchanged player ticks must not rescan
all rows. Preserve exact duplicate identity and bounded weak-row cleanup; do not add a
second marker truth model or perform disk work per row.

Source/ELF identity and environment accompany timings. Do not hide cold renderer costs
with production prewarm, forced renderers, longer animations or weaker assertions.
Preserve synthetic-only privacy, bounded work and lifecycle checks in
[BUILDING](../BUILDING.md); platform scope remains in [LIMITATIONS](../LIMITATIONS.md).
