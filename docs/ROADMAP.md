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
[ADR-0010](adr/0010-single-purpose-sign-in-and-provider-gate.md) removes the
developer-credential/code workflow from the GUI. One native sign-in dialog now
reports the provider block; generic automatic callback/PKCE infrastructure and
optional isolated WebKit support are covered by synthetic checks. Cassette-style
live sign-in is not achieved. Karin still needs an authorized Music client/grant,
a supported registered callback and provider confirmation for the chosen browser
path before account/library/search acceptance can be performed. End users must
not register applications or copy codes, URLs or tokens to sign in.

The default/public build has no activated auth provider or persistent session. A permitted
source, audible service playback, verified credential lifecycle, write methods,
MPRIS and My Wave remain separate gates. The [delivery review](STREAM_DELIVERY_REVIEW.md)
records the concrete audio blocker; generated media does not complete v0.1.

## Later Releases
UX polish, richer library features, lyrics, settings,
and experimentally validated integrations.
Offline functionality requires separate legal and technical review.
