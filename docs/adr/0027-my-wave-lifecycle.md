<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0027: My Wave on the shared player

Status: Accepted; current ordinary Home contract.

## Decision

Only explicit Listen after Music validation creates a modern radio session. Startup,
restored login, navigation and queue restoration do not. Reuse MusicService, MusicQueue,
resolver/cache, GStreamer, Player Bar, Info and MPRIS; there is no radio daemon, alternate
login or persisted radio state. The old `--experimental-wave` flag is a no-op
compatibility alias.

The modern JSON `/rotor/session/new`, session `/tracks` and `/feedback` flow uses strict
status/IDs and one canonical lookup per batch. Preserve original order, composite-selector
precedence, omissions and duplicate occurrences. Unknown/terminated status stops
admission. Retain Cassette's established `event.from` shape; older station routes are not
fallback endpoints.

Bind each original batch marker to its queue OccurrenceId. The worker's cumulative actual
Playing ledger excludes Pause, Loading and seek distance; GTK scheduling or selection is
not evidence of listening. Emit `radioStarted`, `trackStarted`, `skip` and `trackFinished`
in admitted order, with Started before a terminal event when actual Playing proves a
missed snapshot. Each cycle terminates at most once. EOS precedes Next; unheard cycles and
naturally completed tracks gain no extra skip. There is no outgoing `radioFinished` or
invented negative-feedback event.

Keep at most 100 positions including up to 25 past positions, a 16-event FIFO, one
provider operation and bounded pending batch/start/retirement work. Prefetch at three or
fewer upcoming positions once per current occurrence; coalesce Next demand rather than
accumulating operations. Wave uses provider order with Shuffle/ Repeat off, including
external MPRIS writes; leaving restores ordinary modes. Previous uses retained history,
not rebuilt evicted batches. Manual additions/ reordering cannot acquire radio provenance.

## Failure and retirement

One POST is one intent. An admitted HTTP 200 acknowledgment is receipt, not recommendation
effect; malformed/explicit-error envelopes are failures. Ambiguous POSTs stop continuation
without replay. Explicit Retry starts a new session. Pause preserves context. Same-account
Stop/source replacement may drain ordered final heard-track reports once before local
retirement, never a session-finished event. Logout/account change/close cancel queued
reports and fence late results without sending new requests under retired authorization.
In-flight HTTP cannot be unsent. A feedback failure may leave current audio controllable
while stopping further radio admission.

Diagnostics retain the first typed failure and emit fixed event/origin/lifecycle/
reason/category, HTTP status and bounded counts only. Never log IDs, markers, URLs,
bodies, tokens or listening payloads. Markers/ledgers remain memory-only.

## Evidence and verification

[Pinned Cassette session
shape](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/yam-client.vala#L948)
and [modern
MarshalX](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/rotor_sessions.py)
are technical references, not service permissions. [BUILDING](../BUILDING.md) retains
shared-player, feedback and lifecycle regressions.
