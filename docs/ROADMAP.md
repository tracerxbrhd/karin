<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Roadmap

## v0.4: accepted functionality, integration under review

The owner accepted `97093ee` on Fedora 44/KDE Plasma/Wayland/PipeWire for native
functionality and the current UI/UX, with separate successful Flatpak acceptance.
[PR #15](https://github.com/tracerxbrhd/karin/pull/15) prepares integration into
`master`; merge still requires explicit owner approval. Acceptance is not a public
release: Cargo remains at `0.0.0`, and no v0.4 release/tag or Flathub publication
is implied. See [VALIDATION.md](VALIDATION.md) for exact evidence.

The accepted client includes protected sign-in, liked tracks/playlists, catalog
and loaded-collection search, real playback with contextual queues/Shuffle/Repeat,
MPRIS, metadata/artwork caching and the adaptive native music workspace.

## Maintenance priorities

- Investigate cold first-appearance frame gaps without attributing an unproven
  driver cause or changing production timing to conceal them. Desktop acceptance
  does not resolve the separate cloud observation.
- Improve refusal-driven cache invalidation when storage I/O is busy or fails.
- Consider stable identities for manually appended queue items; original source
  occurrences already retain their identity through filtering and removal.
- Preserve Rust 1.98.1 compatibility, native GTK/WebKit/GStreamer lifecycle,
  protected-session cleanup and the narrow Flatpak permissions as dependencies
  and runtime components evolve.

These are follow-ups with explicit scope, not claims of new blocking functional
defects. [LIMITATIONS.md](LIMITATIONS.md) records their current boundaries.

## Features requiring separate contracts

My Wave needs a confirmed radio-session, batch and listening-feedback lifecycle;
a station list or ordinary queue is insufficient. Recommendations, similar tracks
and lyrics need independently confirmed permitted endpoints. Dislike must have
verified semantics distinct from removing a Like. Full collection search/sorting
must not present one loaded prefix as the complete remote library.

Persistent audio requires service permission, a storage contract and enforceable
offline entitlement/expiry behavior before implementation. The existing MP3 path
is not that permission. See [API_FEASIBILITY.md](API_FEASIBILITY.md) and
[AUDIO_CACHE_REVIEW.md](AUDIO_CACHE_REVIEW.md). Unverified features stay absent or,
for the explicitly labelled Wave section, visibly unavailable.

## Public release

A release decision must settle package version/tag consistency, authentication
distribution conditions, packaging and the supported runtime matrix. Retain GPL
text and third-party attribution. Add a Flathub link only after publication; no
release date is scheduled.
