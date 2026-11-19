<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0038: Bounded source renewal for the same paused occurrence

Status: v0.8 implementation decision; execution and Fedora acceptance are separate
Date: 2026-11-14
Extends: ADR-0015, ADR-0023, ADR-0027 and ADR-0031

## Context

At `927e5e9`, Play and Toggle called native Resume directly for a paused source.
The resolver's monotonic admission can expire while that occurrence is paused;
the later command then reports `SourceExpired` and the UI requires another Play.
That is a source-lifecycle defect, not evidence that the Music account lost
rights. Replacing the error text, extending the signed address's lifetime or
periodically resolving during Pause would not implement correct continuation.

The working cache/resolver, exact queue occurrences, Wave listening ledger and
MPRIS adapter already share one player. Replacing their logical track/cycle just
to renew transport would produce false transitions and lose listening state.

## Decision

Expose safe expiry readiness in the playback snapshot without exposing a URI.
Pause remains Paused as admission ages; an expiry discovered while idle retires
unusable native transport without inventing a Playing or access-success state.
No network timer refreshes paused media. A still-admitted source resumes normally.

An explicit Play/Resume retains the last trustworthy native position and the
current opaque queue occurrence. If admission is already expired, use the
existing background Music access/offer and AudioCache/local-first resolution
path before native Resume. If `SourceExpired` is first observed during that
Resume, the same intent may perform one bounded renewal. Other access/network
failures are reported normally; an expired or refused address is never replayed
as an identity, region or protection fallback. The existing bounded pre-Playing
cache-decode fallback stays distinct from repeating an expiry attempt.

`ui::music::player::source_renewal` owns a transient renewal ticket, account and
playback-intent generations, exact occurrence, desired position and autoplay
intent. It uses the existing one-running resolution lane. Repeated Play updates
that intent instead of submitting parallel HTTP work. Pause, Stop, replacement,
logout, account change and close fence old results; an explicit Seek during
renewal supplies the latest target. Editing unrelated future queue items does
not change the selected occurrence or restart it.

`PlaybackController::renew_source` replaces the admitted source inside the same
worker, retires its old pipeline/bus, prerolls and restores position before
autoplay. A separate source revision identifies that native preparation; the
logical playback generation, cumulative worker-observed Playing time and queue
identity remain stable. Volume, Shuffle, Repeat and continuation do not reset.
Internal restoration does not increment the user seek serial or emit a fake
MPRIS Seeked. A genuine user Seek keeps its normal acknowledgement path. Wave
keeps its existing cycle, original batch marker and actual-listening ledger;
renewal creates no radioStarted, trackStarted, skip or new radio session.

While admission/preroll is pending, the UI presents retained paused intent and
“Возобновляем воспроизведение…”, not fabricated Playing. Success clears the
owned failure. If the fresh source cannot restore a nonzero position, remain
paused with an actionable explanation instead of silently playing from zero.
If a previously playing source has no trustworthy native position, hold Pause
and explain that limitation before resolving; do not invent a zero position.
A failed renewal exposes an honest error/Retry without a retry loop.

TLS, exact origins/paths, no-redirect checks, current access and the original
admission lifetime are unchanged. Tokens, signed URLs and descriptors stay in
memory and never enter persistence, logs or D-Bus. There is no second engine,
stream cache, saved renewal ticket or additional Flatpak permission.

## Consequences and verification

Tests must use original local audio and controlled source ageing, including
10/30/60-minute equivalents, without sleeping for those durations. Verify valid
Resume, preknown and late expiry, admitted cached and fresh remote sources,
failure/no-loop, nonseekable restoration, exact duplicates, modes/volume/MPRIS,
latest-intent cancellation, Wave event counts and natural EOS to the next item.
These are production-path fixtures, not real Music rights or heard service audio.

The final [combined Fedora procedure](../BUILDING.md#v08-final-fedora-acceptance)
requires heard continuation after an actual long Pause on Native and installed
Flatpak. Record the duration and revision actually tested. Authored tests and
this ADR do not establish PASS; final executed evidence belongs to
[VALIDATION](../VALIDATION.md) and the exact-head PR checks. The earlier unexplained
Cloud cache/EOS sample remains separate from this owner-reported expiry defect.
