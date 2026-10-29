<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0014: Explicit library writes with independent confirmation

Status: Accepted for the Issue #9 prototype; provider acceptance remains local
Date: 2026-10-29
Supersedes: ADR-0009's read-only limit for the scoped confirmed like/unlike methods

## Decision

Extend the existing MusicService in a small `music::interactions` module. A
validated token-bound account determines every membership/write path. Read the
compact liked-track library with exact UID and unsigned revision validation;
unknown or malformed membership is never guessed false. Preserve original
owner/kind IDs when listing liked playlists; a liked playlist is not an owned
playlist and does not acquire mutation privileges.

A deliberate like/unlike sends one documented-style form POST, then independently
reads membership and requires a non-regressing revision and the requested state.
Only that confirmation changes the visible favorite state. Do not retry writes
automatically or treat a lost/malformed response as proof nothing changed.
Separate explicit rejection/conflict, unknown write outcome and unavailable or
mismatched confirmation. A rejected/expired session is still propagated to logout.
Recovery refreshes state with GET; it does not replay the POST.

Keep reads and writes off GTK. A dedicated bounded read lane coalesces obsolete
membership checks. Admit only one explicit write at a time with visible feedback;
catalog/search/detail operations remain independent. Session/ticket generations,
weak control references and logout/close invalidation prevent late UI updates.
A request already accepted by the server cannot be recalled by local logout;
no rollback or remote revocation is claimed. Invalidate cached liked data after
every write outcome rather than presenting stale Home/library information.

Use original implementation and endpoint/schema analysis from pinned Cassette
and yamu references, retaining GPL SPDX notices. No default client identity,
credential, signing code, lyrics/protected delivery implementation or API-client
dependency is imported. Logs contain only fixed operation/category and numeric
status; private models, IDs, responses, URLs and tokens remain excluded.

## Acceptance

Loopback tests cover account binding, revision/schema/conflicts, no retry,
read-after-write, 401 confirmation, ambiguous outcomes and privacy. A separate
whole-window GTK fixture covers actual controls, disabled duplicate activation,
GET-only recovery, liked-playlist details and late results after logout. Live
like/unlike/liked-playlist behavior requires the consenting owner's Fedora run.
These operations establish no audio source, audible acceptance, service identity
distribution approval or persistent session lifecycle.
