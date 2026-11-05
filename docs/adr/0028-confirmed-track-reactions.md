<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0028: Distinct, server-confirmed track reactions

Status: Live server reactions confirmed at `1877a6c`; revised Next/Flatpak acceptance pending
Date: 2026-11-05
Extends: ADR-0014 and ADR-0027

## Evidence and semantics

Unlike removes membership from the liked-track collection. It does not add a
Dislike. Account Dislike is the separate **Не рекомендовать** collection; removing
it does not create a Like. Keep both states independently typed and read from
the service, rather than deriving one by negating the other.

Published source inspected for this change:

- [Cassette v0.2.4](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/yam-client.vala#L578)
  has distinct disliked-track GET/add/remove helpers. Its singular routes differ
  from the batch route below; they are evidence of a separate collection, not
  fallback URLs to try after a refusal.
- [MarshalX track Dislike methods](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/likes.py#L451)
  corroborate `GET /users/{uid}/dislikes/tracks` with
  `if_modified_since_revision=0`, a `library` with UID/revision/track references,
  and one form POST to `/dislikes/tracks/add-multiple` or `/remove` with
  `track-ids`. The track write acknowledgment contains a revision.
- Its [Like](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/likes.py#L80)
  and [Dislike](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/likes.py#L569)
  notes describe clearing the opposite mark when adding one. Karin must confirm
  both collections; it cannot assert that effect from the POST alone.

These sources established the concrete interoperable contract. The owner then
confirmed real server reactions, opposite-mark interaction and existing functions
at `1877a6c`, and requested automatic Next after confirmed current-track Dislike.
This report does not establish a new revision's desktop/installed Flatpak acceptance
or API/public-distribution permission. No upstream implementation, identity or
dependency is imported; retain existing GPL/source attribution boundaries.

## Service and UI decision

Extend the existing `music::interactions` boundary with `TrackReactions` and
explicit Like/Dislike intents. Token-bound validation selects the account path;
callers cannot provide another account UID. Read compact memberships with strict
identity/schema/count/revision checks. Missing, conflicting or refused data is
unknown, never an invented unmarked state. Do not use metadata cache to confirm
reactions.

When both memberships are available, one deliberate action sends one POST, then
reads both collections independently.
Require the requested state, the documented opposite-mark effect when adding,
and a non-regressing target revision before reporting success. These GETs are
not an atomic server transaction: concurrent changes or contradictory replies
must remain visible as an unconfirmed/conflicting result. Never send a second
compensating mutation or retry an ambiguous POST. Recovery reads current state.

Failure of the new Dislike read must not disable existing Like/Unlike. If a fresh
independent `is_track_liked` read succeeds, retain the old confirmed Like control
and use `set_track_liked` with its existing membership/revision confirmation.
Dislike stays hidden and unknown; the Like-only result establishes no Dislike
state or opposite-mark effect. A later valid dual read enables paired reactions.
Both recovery paths use GET only, without replaying an uncertain mutation.

Keep work off GTK and admit replies only under the current account, reaction and
track generations. Serialize reaction writes, show pending state, keep Like and
Dislike as independent focus targets and prevent a reaction click from opening
Track Info. Unknown/unavailable Dislike is not exposed as a successful control.
Mapped detail Like controls are weak, account-generation-bound subscribers.
Before a write, retire their read tickets and mark matching controls unknown;
only confirmed server state or an explicit error may update them afterward.
Pending unrelated reads retired by the shared lane receive GET-only Retry,
not an endless spinner. No additional membership requests are needed to update
matching detail views, and logout/close clear these presentation subscriptions.

Only successful, dual-read-confirmed Dislike addition on the unchanged current
playback occurrence invokes the existing Next once. Capture the queue-owned
opaque occurrence identity and current playback intent when admitting the write;
Track ID alone is insufficient because queues permit duplicates and replay.
Recheck these guards together with account/request generations when admitting the
result. Update confirmed reaction presentation before delegating transport.

Removing Dislike, Like/Unlike, refusals and uncertain writes never advance.
A new occurrence, replaced playback intent, logout, account change, cancellation
or controller destruction retires the pending playback effect. GET-only recovery
after an ambiguous POST may display current marks but cannot replay the earlier
Next intent. Ordinary failures do not stop or rebuild the player/queue;
rejected/expired authorization still follows normal session teardown. An already
dispatched request cannot be unsent or update a replacement session.

## Wave boundary

[Modern SessionEvent documentation](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/rotor/session_event.py#L10)
lists recommendation-only `like`/`unlike`/`dislike`/`undislike` events and requires
actual played time for `dislike`. It explicitly distinguishes those events from
the account collection. A generic enum does not establish the exact negative
feedback lifecycle for Karin's live session. The owner's confirmed-Dislike Next
requirement is a local transport policy, separate from those protocol events.

Account reactions during Wave use the same confirmed collection operation. After
confirmed current-occurrence Dislike addition, delegate to existing Next: its
observed Playing ledger, original batch marker and terminal-once rules determine
whether the interrupted cycle sends the established Skip. Add no recommendation-
only negative event, replay, radio session or second player. The four-event Wave
protocol remains unchanged; extending it still needs separate evidence under
[ADR-0027](0027-experimental-wave-lifecycle.md).

## Validation

Use synthetic HTTP fixtures for distinct add/remove routes, both memberships,
revision conflicts, authorization refusals, malformed/oversized data, ambiguous
writes, no replay and obsolete-account results. Native checks must exercise
mouse/keyboard focus, pending state, compact layouts, Info isolation and playback
survival. Cover exactly one confirmed Dislike Next in ordinary/Wave contexts,
no Next for removal/Like/refused/uncertain outcomes, changed occurrences including
duplicate IDs, cancellation and logout. Executed results belong in
[VALIDATION](../VALIDATION.md); merely registering a test is not a pass.

The owner's `1877a6c` result confirms live server reactions. Repeat the revised
Next workflow in Native and installed Flatpak, recording revision, environment
and PASS/FAIL/not-tested without account identifiers, tokens, wire data, signed
URLs, personal screenshots or recordings. Issue #17 retains any unreported
acceptance; public-distribution conditions remain separate Issue #5.
