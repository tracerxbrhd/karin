<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0026: Refusal fences and exact queue occurrences

Status: Accepted; current account/queue/MPRIS contract.

## Account refusals

Catalog leases bind account/key epochs before I/O. A known refusal fences its key in
memory before attempted deletion; old readers/writers fail final lease checks even if
storage is Busy or deletion fails. A bounded worker writes a non-secret tombstone and
removes the record. Keep failed fences, bound their table and fail closed on exhaustion;
do not clear another account or loop.

Cold-process metadata is Stale even within its former TTL: only this process's successful
current-epoch write establishes a fresh hit. Tombstones are never served. Without a
durable marker, saved metadata may only be an honestly labelled fallback on eligible
Network/Timeout failure. It cannot confirm authentication, rights, membership or a write.
Artwork has its separate HTTP rules. No GTK cache mutex spans filesystem work; cache
failure must preserve admitted live playback.

## Occurrence and order ownership

MusicQueue owns one opaque reference-counted OccurrenceId for every admitted entry,
including equal service IDs and manual additions. Removal/reindexing does not change
surviving identity; new context/append allocates new tokens. The bounded PlaybackContext
and original source positions are provenance, not a second order model. Keep at most 500
entries; source positions below 20,000 remain aligned, unique and potentially nonmonotonic
after sorting/manual edits.

Shuffle plans exact future occurrences without repetition; Previous follows actual
history. Repeat Off/Queue/One affects EOS; explicit Next/Previous still navigate. Modes
change future policy without restarting current audio. Querying capabilities does not
consume random state. Empty/unavailable contexts cannot loop. Append, Play Next, remove
and reorder remap indices/history/provenance atomically. Upcoming edits leave current
occurrence/source/position unchanged; fold forward replay into editable future rather than
replaying past history. GTK drag/drop carries only a typed process-local value with weak
owner and account/context fences, not an outside-ID serializer. Keyboard actions use the
same operations. Manual edits are unavailable for Wave's admitted radio sequence.

## Desktop integration

Use the in-process GIO MPRIS 2.2 adapter at `/org/mpris/MediaPlayer2`, owning only
`org.mpris.MediaPlayer2.io.github.tracerxbrhd.Karin`. Transport, modes, Volume,
Seek/SetPosition and capabilities consume the same queue/controller as GTK. Publish actual
status/position/duration and worker-confirmed Seeked; reject stale track paths.
Shuffle/LoopStatus map to local modes. Rate is fixed at 1.0 (0 pauses). No second engine,
TrackList, arbitrary OpenUri or broad session bus is added. Metadata contains known
display fields, opaque track paths and bounded decoded local covers, never account IDs,
tokens, signed media addresses or `xesam:url`. Private temporary covers and bus
name/registration are removed on retirement.

[BUILDING](../BUILDING.md) retains account-fence, exact-occurrence, worker and private-bus
regressions.
