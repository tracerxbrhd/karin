<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0027: My Wave on the shared player

Status: Ordinary Home integration; owner-accepted Native/installed Flatpak baseline
Date: 2026-11-04
Updated: 2026-11-05 (Europe/Samara)
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

The owner's first Fedora test confirmed a positive live chain but left one
HTTP 400 unidentified because its log omitted the event. The repeat at
[`a69dc79`](https://github.com/tracerxbrhd/karin/commit/a69dc79686ba066f194fb3b28b5deaa318016d3c)
confirmed real session creation, normalization, heard Music, all four events
`radioStarted`/`trackStarted`/`skip`/`trackFinished` with HTTP 200, and several
further batches. It then identified an accepted terminal `skip` followed by
`radioFinished` HTTP 400 during ordinary source replacement. The additional
event came from `WaveCoordinator::finish()`; the skip was not the refusal.
Cassette's constant for `radioFinished` does not establish server support: its
reviewed [Flow](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/player/mods/flow.vala#L69)
does not send it, and the reviewed modern MarshalX event contract omits it.
The live result now justifies removing that event, while retaining the accepted
`event.from` shape and the four confirmed events.

The owner subsequently retested corrected candidate
[`2e4b978`](https://github.com/tracerxbrhd/karin/commit/2e4b9786bb475abd9fc7843503da5bf0d243e9ba)
on Fedora and reported no new problems. Together with the earlier live chain,
this authorizes ordinary integration. The general report does not establish
unreported individual scenarios or acceptance of a new installed v0.5 Flatpak.

## Decision

Home starts with the accepted musical Wave Hero and secondary existing library
routes. Wave is available after ordinary Music-validated sign-in, with no token
input, alternative sign-in or feature flag. Only the user's Listen action starts
a session; startup, restored sign-in and Home navigation issue no Wave requests.
One primary action reflects the actual shared player: Listen, Pause or Resume;
a failure permits a deliberate fresh session. Only pending startup has Cancel;
there is no persistent Finish/Stop control or experimental notice in the Hero.

`--experimental-wave` is temporarily accepted as a deprecated no-op alias so
previous Fedora launch commands keep working. It enables nothing, creates no
separate application instance and is not required for ordinary or Flatpak use.
GApplication forwards activation to the existing instance without a Wave-enabling
action. No radio feature preference, session ID or batch marker is persisted.
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

The Wave buffer holds at most 100 positions with up to 25 prior
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

Pause keeps the radio context and excludes paused time. MPRIS Stop or source
replacement under the same account may drain ordered final track reports once,
then cancel locally; there is no separate user-facing Finish operation. Local
retirement sends no `radioFinished`: remove that variant from the outgoing
protocol and coordinator rather than retain an unused compatibility branch.
An interrupted, actually started cycle may send one terminal `skip`, preserving
its original batch marker and actual Playing time. An already naturally finished
cycle sends no additional `skip`; an unheard cycle invents no feedback. This
applies to every shared retirement path, including Stop before playback starts,
without leaving the local coordinator stuck in retiring/finishing. Local
completion does not claim remote deletion.
Account change, logout and close cancel queued work and fence late results;
they schedule no final reports with retired authorization. An admitted bounded
HTTP request cannot be unsent or allowed to update a new context/account/window.
Current audio may stay controllable after feedback failure, while further radio
admission stops.

Provider failures produce one diagnostic carrying the submitted local event,
origin (playback/selection/retirement), completion-time local lifecycle phase and Stop/source-change
reason where relevant. A pending typed failure remains primary if cancellation
is observed before its main-context callback. Diagnostics expose only those
fixed classifications, known HTTP status and bounded counts, never identifiers, tokens, URLs, raw JSON or listening payloads.
Radio markers/listening state are memory-only; protected sign-in storage stays
unchanged. MPRIS keeps ordinary safe track metadata and opaque track paths.

## Acceptance

Exercise normalization/status/availability, real Playing/Pause/seek/EOS, missed
GTK snapshots, duplicates, bounded work, cancellation, source switching, mode
restoration and uncertain feedback without replay. Native synthetic media must
use the shared player. Executed results belong in [VALIDATION](../VALIDATION.md).

The owner's final [Native/Flatpak procedure](../BUILDING.md#my-wave-local-acceptance)
checks ordinary no-argument startup, sign-in/restoration without automatic radio,
explicit Listen, transport/EOS/continuation, source replacement, MPRIS and
logout/restart. Require no `radioFinished` request or unhandled new refusal.
Retain tests for pending feedback/batch cancellation, exact-once terminal
reporting and honest HTTP 400 handling for every supported event. Fixtures do
not establish individual desktop acceptance. The owner subsequently confirmed
ordinary Native and installed Flatpak operation; Issue #16 is closed. That report
does not invent special failure/race results beyond the automated evidence.
Public distribution remains separate Issue #5
in [API_FEASIBILITY](../API_FEASIBILITY.md). No DRM bypass, audio persistence or
new Flatpak permissions follows from this decision.
