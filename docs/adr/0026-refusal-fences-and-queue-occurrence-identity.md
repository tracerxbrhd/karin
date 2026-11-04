<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0026: Refusal fences and stable queue occurrences

Status: Accepted for stability implementation; execution and desktop acceptance are separate
Date: 2026-11-04
Extends: ADR-0019 and ADR-0025

## Context

After the v0.4 merge, two local consistency defects remain. A confirmed Music
API refusal calls cache removal, but Busy/I/O failure can leave a fresh record
and its lease valid. A manual queue item uses its physical index for Info/MPRIS
identity, so deleting an earlier row reports a false track change. Neither fix
requires new service endpoints, an audio engine or a redesigned workspace.

## Cache decision

Narrow catalog leases to an account/key epoch before reading or fetching. A
known refusal fences that key in memory before storage admission; older cached
replies and writers can no longer pass their lease checks. A later successful
request must use the new epoch to persist a replacement. Account/policy/Clear
generations remain an independent outer fence.

On a worker, write a fixed-size, non-secret tombstone at the existing hashed
record path and attempt deletion. Retain a refusal fence when cleanup fails;
bounded later operations may retry, without a background loop or GTK disk I/O.
Bound the fence table and fail closed if its limit is exhausted. Never clear
another account as a recovery shortcut. Cache failure does not stop live audio.

Complete storage failure cannot persist a new tombstone across process death.
Therefore metadata inherited from another cache process is **Stale**, even if
its TTL remains positive. Only this process's successful current-epoch write
can establish a fresh metadata hit. A known tombstone is never served; without
a durable marker, cold metadata can only be a labelled saved-data fallback on
an eligible Network/Timeout error. Artwork retains its separate HTTP/TTL rules.
This deliberately trades some restart cache reuse for a defensible freshness
guarantee. Cached data never establishes authentication or playback entitlement.

## Queue decision

MusicQueue owns one opaque `OccurrenceId` per admitted entry, in a parallel
bounded vector. Each ID is a private reference-counted token, compared by
identity; it contains no service/account/source identifier. Appending allocates
a new token; removing an entry removes only its token. New contexts allocate a
new identity scope, while clear/logout release queue ownership. Live clones
prevent an old identity from being reused while a consumer still holds it.

Keep PlaybackContext, its shared Track list and original `source_positions` as
provenance. They are not a second playback order. Shuffle/history still operate
on their existing reconciled indices. Track Info and MPRIS compare the queue's
actual occurrence token instead of a mutable manual index. MPRIS continues to
publish its existing opaque serial path, never the internal token or provider
IDs. Reindexing keeps identity; selecting another duplicate changes it. No
separate identity registry, persisted queue, service call or audio worker exists.

## Verification and limits

Use synthetic storage failure, Busy, read/write/refusal races, restart, Clear,
policy/account/logout and old-lease tests. Check manual and loaded duplicates,
context replacement, history, modes, EOS and cleanup. A native private-bus
fixture must retain the same GStreamer worker and MPRIS path through reindexing,
accept the retained SetPosition, and reject it after an actual occurrence switch.
Record executed results in [VALIDATION](../VALIDATION.md).

The owner accepted the earlier v0.4 runtime; these fixes need a fresh scoped
[Fedora check](../BUILDING.md#stability-acceptance). The separate unresolved
Cloud cold-rendering observation and public service/distribution conditions
remain in [LIMITATIONS](../LIMITATIONS.md).
