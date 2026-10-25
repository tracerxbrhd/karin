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
The current [authorization decision](adr/0006-music-authorization-feasibility.md)
keeps Music sign-in and stream resolution blocked pending confirmed grants and
permission. The [playback foundation](adr/0007-playback-controller.md) does not
complete the end-to-end v0.1 acceptance above.

## Later Releases
UX polish, richer library features, lyrics, settings,
and experimentally validated integrations.
Offline functionality requires separate legal and technical review.
