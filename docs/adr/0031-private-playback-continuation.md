<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0031: Private playback continuation

Status: accepted and normally merged through PR #28 at `2ab6d28`; exact
`949d1f9` passed the owner's v0.7 Native/installed-Flatpak acceptance (68/68).

## Context

The accepted player/cache path can reuse audio after restart, but the ordinary
queue and position were memory-only. Saving a GStreamer URI, service response or
radio session would leak transient credentials or resume an invalid session.

## Decision

`playback_session` stores a bounded, versioned, checksummed record under the
private XDG data directory, partitioned by the validated account hash. Atomic
0600 files in a 0700 directory contain service track selectors, typed source
identity, original source positions, current index, manual additions, position,
Shuffle future/history/RNG and Repeat mode. They contain no titles, credentials,
CDN URLs, XML, cookies, radio IDs or queue occurrence tokens. Queue restoration
creates fresh opaque occurrence tokens and an independent restored-view binding.

Filesystem work runs on application-held workers. Event changes are debounced
750 ms, playback is sampled every five seconds and close captures before engine
shutdown. Generation-bound save tickets, shared per-directory worker ordering,
atomic tombstones and the existing durable forget barrier prevent late saves
from reviving forgotten state. A replacement window waits off GTK for a pending
close transaction. Interactive-login restoration starts after its protected
credential-save barrier confirms. A concrete queue captured during fast close
waits asynchronously for that receipt; an empty not-yet-restored queue never
erases the previous record. Failure remains visible; playback does not depend on storage.

Only a currently validated Music account may load its record. MusicService
normalizes up to 500 selectors in batches of 50. Missing/unavailable tracks are
pruned by original occurrence index, preserving duplicates and the surviving
Shuffle plan. An unavailable current track selects a surviving item at position
zero. Saved position is bounded by fresh known duration. No source collection
fetch, resolver, media GET, player preparation, Wave session or autoplay occurs
as part of restoration. A collection label describes saved provenance, never
asserts that the saved sequence is the current server playlist.

The footer and MPRIS expose the restored selection as Stopped. Explicit Play
uses the accepted fresh-access/cache/resolver path and applies saved seek before
Playing where supported. Nonseekable resources start at the beginning with a
visible explanation. Native and MPRIS use the same controller.

Explicit Stop stops audio and tombstones continuation/position, retaining the
visible in-memory queue; closing cannot recreate it until explicit new playback.
Logout and remembering-login off delete continuation in the durable forget
transaction. Restart repair with an unknown account clears only recognized
private records in the dedicated playback directory on a bounded worker; unsafe
or unexpected entries leave the forget barrier active. Cache Clear is independent and does not erase playback intent.
Successful Wave replacement deletes ordinary continuation; radio state is never
serialized or restarted. A failed Wave startup does not erase ordinary state.

## Consequences

This extends the memory-only policy of ADR-0018/0023 for ordinary playback only.
The snapshot is private personal state, not encrypted credentials or an offline
library. Restore still needs current online Music validation and track access.
A failed current track lookup leaves the queue unknown, not empty. Periodic
snapshot writes remain suspended while restoration awaits Retry, including an
inherited storage retry, so an HTTP failure cannot publish a null tombstone.
The native recovery test verifies unchanged private record bytes and restores
the same queue/position through the real Retry action without audio requests.

Process-restart tests use original permitted local audio and synthetic accounts.
The owner separately accepted real Native/installed-Flatpak v0.7 behavior at
`949d1f9`; that report does not turn each synthetic failure case into a live test.
See [VALIDATION](../VALIDATION.md#owner-acceptance-of-pr-28--2026-11-14).
