<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0027: Experimental My Wave on the shared player

Status: Accepted for opt-in implementation; live service acceptance pending
Date: 2026-11-04
Extends: ADR-0017, ADR-0023 and ADR-0026

## Context and evidence

Accepted ordinary Music playback does not prove access to personalized radio,
feedback or further batches. Reuse Karin's existing Wave adapter/coordinator.
The modern JSON session contract appears in
[Cassette v0.2.4](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/yam-client.vala#L948)
and [MarshalX](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/rotor_sessions.py).
This is published-source evidence, not a live Karin grant. Their optional
feedback `from` placement differs; retain the existing Cassette `event.from`
shape until a specific real result warrants changing it. Older station APIs
are not automatic fallbacks. No new OAuth identity, secret or dependency follows.

## Decision

Ordinary Home keeps Wave unavailable. Process-local `--experimental-wave`
exposes a labelled owner-local experiment through the existing validated Music
session, with no token input, alternative sign-in or persisted feature flag.
MusicService owns session/batch/feedback POSTs and one canonical lookup per
batch. Preserve validated IDs, exact composite precedence, provider order,
duplicates and honest omissions. Unknown/terminated status stops admission
before following requests.

Install a Wave PlaybackContext in the existing MusicQueue; keep the same
StreamResolver, PlaybackController/GStreamer, Player Bar, Info/Queue and MPRIS.
Browsing is not a source change; explicitly playing another collection leaves
Wave. Do not restore a radio session from cached tracks or restart it on launch.

## Occurrences, listening and bounds

Bind each original WaveBatchMarker to its opaque queue OccurrenceId, not its
service Track ID. Later batches cannot rewrite current provenance. Each new
playback source/intent cycle has a fresh observation clock, including a replay.
The worker's cumulative actual Playing clock excludes Pause, Loading and seek
distance independently of GTK scheduling. A positive ledger can prove Playing
that GTK missed; selection cannot. Emit Started before a terminal event when
such evidence exists, finish/skip each cycle once and observe EOS before Next.
Zero-heard terminal cycles invent no listening. This is state observation,
not sample-accurate physical audibility.

The experimental buffer holds at most 100 positions with up to 25 prior
positions retained; feedback FIFO holds at most 16 events. One provider operation
runs at a time, with one pending batch, one pending start and one retiring session.
Prefetch at three or fewer upcoming positions once per current occurrence;
coalesce pending Next demand rather than accumulate requests. Capacity/error
ends continuation, not an automatic loop. The ordinary 500-position bound stays.

Use provider order with Shuffle/Repeat off only for Wave, including MPRIS mode
writes. Restore prior ordinary modes on leaving; existing logout/close resets
remain. Previous uses retained history without rebuilding evicted batches.
Manual additions cannot silently join radio provenance. Ordinary contexts keep
their existing local policy.

## Failure, retirement and privacy

HTTP 200 with an admitted feedback envelope confirms receipt under this
contract, not recommendation effect. Compatible empty acknowledgments are
allowed; malformed, explicit-error or duplicate recognized error fields cannot
become success. An uncertain POST outcome retires continuation without replay.
A deliberate fresh-session action creates a new identity; local Busy is distinct
from an already-dispatched write.

Explicit Finish, MPRIS Stop or source replacement under the same account may drain ordered
final reports once, then cancel locally. This is not verified remote deletion.
Account change, logout and close cancel queued work and fence late results;
they schedule no final reports with retired authorization. An admitted bounded
HTTP request cannot be unsent or allowed to update a new context/account/window.
Current audio may stay controllable after feedback failure, while further radio
admission stops.

Diagnostics expose only fixed stage/category/event, known HTTP status and
bounded counts, never identifiers, tokens, URLs, raw JSON or listening payloads.
Radio markers/listening state are memory-only; protected sign-in storage stays
unchanged. MPRIS keeps ordinary safe track metadata and opaque track paths.

## Acceptance

Exercise normalization/status/availability, real Playing/Pause/seek/EOS, missed
GTK snapshots, duplicates, bounded work, cancellation, source switching, mode
restoration and uncertain feedback without replay. Native synthetic media must
use the shared player. Executed results belong in [VALIDATION](../VALIDATION.md).

Default enablement needs the owner's [Fedora procedure](../BUILDING.md#my-wave-local-acceptance):
real session, normalized tracks, heard Music, accepted feedback and a further
batch, plus transport/MPRIS and cleanup. Until then Issue #16 stays open and the
ordinary UI remains unavailable. Public distribution remains separate Issue #5
in [API_FEASIBILITY](../API_FEASIBILITY.md). No DRM bypass, audio persistence or
new Flatpak permissions follows from this decision.
