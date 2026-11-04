<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Roadmap

## Completed v0.4

[PR #15](https://github.com/tracerxbrhd/karin/pull/15) was squash-merged into
`master` at [`562596b`](https://github.com/tracerxbrhd/karin/commit/562596b990b5cd87c763af41120bad7830fda551).
The owner accepted runtime `97093ee` on Fedora 44/KDE Plasma/Wayland/PipeWire
for native functionality and UI/UX, with separate successful Flatpak acceptance.
[VALIDATION.md](VALIDATION.md) records the scope; unreported individual scenarios
are not inferred. Cargo remains at `0.0.0`, with no public v0.4 release or Flathub
publication implied.

The baseline includes protected sign-in, library/playlists, global and
loaded-collection search, GStreamer playback, contextual queues, Shuffle/Repeat,
MPRIS, metadata/artwork caching and the adaptive Track Info/Queue workspace.

## Stability maintenance

The [stability change](https://github.com/tracerxbrhd/karin/pull/24) adds refusal
fences before cache I/O and stable queue occurrence IDs shared by Info/MPRIS.
Rejected metadata cannot return as fresh after failed removal; cold metadata
needs a new positive write. Source provenance, modes and the audio path remain
unchanged. [ADR-0026](adr/0026-refusal-fences-and-queue-occurrence-identity.md)
defines both contracts. The owner accepted `9d54834`; it was squash-merged at
`920fd71`. [VALIDATION.md](VALIDATION.md) separates the acceptance and tests.

- Continue the bounded [cold Player Bar investigation](https://github.com/tracerxbrhd/karin/issues/23).
  Software-render LLVM compilation contributes to the measured Cloud stall;
  its complete trigger/cause and ordinary Wayland behavior remain unproven.
  Preserve production timing, renderer policy and tests without artificial prewarm.
- Maintain Rust 1.98.1 compatibility, protected-session cleanup and the narrow
  native/Flatpak permission boundaries as dependencies evolve.

## Next functional directions

- [My Wave](https://github.com/tracerxbrhd/karin/issues/16): the owner confirmed
  session/audio/four supported feedback events/continuation; repeat the local
  retirement fix after the identified `radioFinished` HTTP 400 before default enablement.
- [Dislike](https://github.com/tracerxbrhd/karin/issues/17): verify semantics and
  confirmation independently of removing a Like.
- [Lyrics and related tracks](https://github.com/tracerxbrhd/karin/issues/18): confirm permitted endpoints/resources before
  exposing a user workflow; no extracted official-client signing material.
- [Playlist editing](https://github.com/tracerxbrhd/karin/issues/19): extend beyond
  confirmed creation only with ownership/revision and ambiguous-write recovery.
- [Offline audio](https://github.com/tracerxbrhd/karin/issues/20): establish storage permission and verifiable account/device/track
  entitlement, expiry and renewal. A working MP3 source is not that contract.
- [Restart playback state](https://github.com/tracerxbrhd/karin/issues/21): define
  bounded same-account context/position restoration without storing media URLs.
- Whole-collection search/sorting: provide complete coverage or label a loaded
  prefix honestly without silently changing the playing queue.

Each direction needs its own confirmed contract and acceptance criteria.
The v0.5 branch presents a musical Hero first on Home and uses the shared queue,
player and MPRIS behind `--experimental-wave`. Pause keeps the session; ordinary
music selection and MPRIS Stop retire radio without a separate Finish control.
The owner-confirmed positive chain and identified retirement-event refusal are recorded in
[VALIDATION](VALIDATION.md). Service access stays gated pending repeat acceptance;
see [ADR-0027](adr/0027-experimental-wave-lifecycle.md).
Unsupported actions remain absent.
See [API_FEASIBILITY.md](API_FEASIBILITY.md) and
[AUDIO_CACHE_REVIEW.md](AUDIO_CACHE_REVIEW.md).

## Public distribution

Resolve the OAuth-registration and Music API distribution conditions tracked in
[Issue #5](https://github.com/tracerxbrhd/karin/issues/5), separately from technical
interoperability. GPL source reuse and private account success do not establish
provider approval.

A [public release](https://github.com/tracerxbrhd/karin/issues/22) also needs a version/tag decision, packaging/runtime checks,
corresponding source and preserved third-party notices. Add a Flathub link only
after publication. No release date is scheduled.
