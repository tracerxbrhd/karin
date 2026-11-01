<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0023: Local occurrence-based playback order and MPRIS modes

Status: Accepted for v0.4 implementation; execution and owner acceptance remain separate
Date: 2026-11-01
Extends: ADR-0017's PlaybackContext and shared MPRIS/controller ownership

## Context

Karin's accepted service path resolves authorized resources and plays them
through one GStreamer worker. Its queue previously selected loaded positions in
sequence; optional MPRIS Shuffle/LoopStatus were deliberately omitted. The
complete v0.4 redesign requires real Shuffle and Repeat, not decorative buttons.

Track IDs are not occurrence identities: a playlist may contain the same ID
twice. Reordering or cloning the full Track collection on every transition
would weaken source provenance, duplicate handling and the 500-position bound.
These modes are local playback policy, not additional Music API capabilities or
a synchronized Yandex queue.

## Decision

Own order policy in MusicQueue over the existing PlaybackContext. Retain its
physical source order and shared immutable Track list. Store bounded indices for
the shuffled future plan and actual playback history, using exact occurrences
and skipping unavailable positions. Upcoming/history queries do not consume
random state or clone all Track models. Fisher-Yates uses unbiased bounded
draws; tests can seed reproducible sequences.

Expose typed `RepeatMode::Off`, `Queue` and `One`, with the native button cycle
Off → Queue → One → Off. Expose Shuffle independently. Current occurrence,
source binding, resolver work and PlaybackController are unchanged when a mode
is toggled; only future navigation and capability snapshots change.

### Transition semantics

| Event | Policy |
| --- | --- |
| Enable Shuffle | Keep current item; plan remaining available exact occurrences without repetition within the cycle |
| Disable Shuffle | Keep current item/audio; future navigation uses original context order |
| Explicit Next/Previous | Navigate according to current order/history, including with Repeat One |
| Previous under Shuffle | Revisit actual bounded playback history; subsequent Next consumes that history before the remaining plan |
| Natural EOS, Repeat Off | Advance to the next available planned item; end when the loaded cycle is exhausted |
| Natural EOS, Repeat One | Restart the same actual occurrence from the beginning |
| Natural EOS, Repeat Queue | Wrap the available loaded context; Shuffle starts a new permutation |
| Shuffled new cycle with multiple available items | Avoid immediate repetition of the current occurrence |
| Append/remove/manual selection | Reconcile exact indices, history and remaining plan without restoring stale positions |
| Ordinary clear or replacement context | Retain app-session modes; retire old context plan/history |
| Logout/account change or app close | Reset modes and retire personal queue state |

Repeat One uses the ordinary guarded resource resolution/playback path once
when EOS requests another play. A refusal or engine error is a visible failed
state, not another natural-completion signal and not an automatic retry loop.
An empty/all-unavailable context cannot generate an unbounded loop. Repeat
Queue covers only admitted loaded positions and manual additions; it never
fetches an entire remote collection or hides partial paging.

Modes are in-memory local policy. No new preference file, remote queue state,
stored media URL or audio cache is introduced. `next_track`/`previous_track`
separate explicit navigation from `advance_eos`; capability queries and the
universal Queue view consume the same policy as actual playback.

### MPRIS and native controls

Publish writable `Shuffle: boolean` and `LoopStatus: string` with the official
values `None` ↔ Off, `Playlist` ↔ Queue and `Track` ↔ One. Validate types/values,
apply writes to MusicQueue on the existing UI bridge and publish actual changed
properties. Native buttons and external KDE/playerctl writes therefore share
one source of truth. Recompute CanGoNext/CanGoPrevious from policy and real
player/session capability, rather than exposing a second desktop-only mode.

Retain the current Play/Pause/Stop, Seek/SetPosition, Volume, metadata and
worker-acknowledged Seeked contracts. Stop stays available through MPRIS and the
controller despite removal of its visible footer button. No second engine,
TrackList, arbitrary OpenUri, daemon or broad session-bus permission is added.
No credentials, account IDs or signed media addresses enter D-Bus properties.

## Verification and consequences

Test seeded cycles and exact duplicate positions; available/unavailable and
single-item contexts; manual history; append/removal of current/noncurrent
positions; clear/replacement/logout; the 500-position/partial boundary; all mode
combinations and EOS versus explicit navigation. Assert mode toggles do not
restart current audio or replace its source/context.

Use generated local media and a counted loopback resolver to exercise actual
EOS, one requested repeat, refusal/error boundaries and shared transport.
Use isolated session D-Bus to test property reads/writes, invalid values,
PropertiesChanged, capabilities and cleanup. Compare native and external state,
including in HiDPI/compact controls. Follow the
[quality gate](../UI_QUALITY_GATE.md#v04-native-music-gate) and record executed
results in [VALIDATION.md](../VALIDATION.md); this decision is not a test PASS.

The owner must separately confirm modes, KDE integration and audible real Music
on the current Fedora native/Flatpak build. Cloud generated audio does not
replace that criterion. Public distribution, persistent audio and unverified
service features retain their independent boundaries.
