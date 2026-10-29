<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin Roadmap

## v0.0 — Technical Foundation
- Validate API capabilities and limitations.
- Investigate compliant authentication and audio playback.
- Establish Rust/GTK4/Libadwaita architecture.
- Implement and verify the initial native application shell.
- Establish automated formatting, linting, build, and tests.
- Record blockers and technical decisions.

Acceptance: a reproducible validated foundation, with real
integration evidence and explicit unresolved blockers.

## v0.1 — Minimum Viable Player
- Authentication, if feasible and permitted.
- Search and basic personal library.
- Real audio playback and playback controls.
- Queue and My Wave, subject to API validation.
- Initial MPRIS integration.
- Responsive user interface and error handling.

Acceptance: working end-to-end features verified on Fedora.

Gate A established an independent GStreamer engine. Gate B1's own-client PKCE
and Yandex ID checks passed on the owner's Fedora desktop; Music status/about
both returned HTTP 403. An isolated unmodified yamu experiment accessed account,
search and metadata, but did not establish permission to redistribute its OAuth
identity or a permitted Music stream.

The current [vertical slice](adr/0009-music-vertical-slice.md) adds a typed
MusicService, read-only library/search/details UI and metadata queue.
[ADR-0011](adr/0011-cassette-session-auth-prototype.md) supersedes the earlier
blocked-provider decision while retaining the single-purpose native login dialog.
The default Cassette-compatible provider loads actual Yandex login in ephemeral
WebKit and attempts the published session-cookie → OAuth sessionid chain before
MusicService account validation and real library/search loads. End users do not
register apps or copy codes, URLs or tokens. The known own-client PKCE 403 is kept
as separate evidence rather than reused as the main login path.

Actual Karin login, Music access and library/search acceptance remain pending
one substantive Fedora run; cloud fixtures are insufficient. Code attribution,
unresolved OAuth identity/service distribution conditions and concrete technical
errors are documented separately. No persistent sessions or public-release
approval are claimed. A permitted source, audible service playback, verified
credential lifecycle, write methods, MPRIS and My Wave remain separate gates.
The [delivery review](STREAM_DELIVERY_REVIEW.md) records the concrete audio blocker;
generated media does not complete v0.1.

## Later Releases
UX polish, richer library features, lyrics, settings,
and experimentally validated integrations.
Offline functionality requires separate legal and technical review.
