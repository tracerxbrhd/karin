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

Gate A is the preliminary authorization/playback-feasibility gate: an independent
local-media GStreamer engine and an evidence-backed Music integration decision.
The Gate A [authorization decision](adr/0006-music-authorization-feasibility.md)
recorded the initial Music authorization blocker. Gate B1 now uses the
owner-confirmed multiuser registration for an isolated local S256 PKCE diagnostic;
actual token issuance, Music account/catalog access and production callback/session
acceptance remain pending. See [ADR-0008](adr/0008-local-oauth-pkce-diagnostic.md).
Music sign-in and stream resolution remain unavailable in the application. The
[playback foundation](adr/0007-playback-controller.md) does not complete the end-to-end v0.1 acceptance above.

## Later Releases
UX polish, richer library features, lyrics, settings,
and experimentally validated integrations.
Offline functionality requires separate legal and technical review.
