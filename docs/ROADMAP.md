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

## Completed v0.5 integration

[My Wave](https://github.com/tracerxbrhd/karin/issues/16) is available from Home
after ordinary Music-validated sign-in. The owner retested corrected candidate
`2e4b978` on Fedora without reporting new problems; earlier real audio, four
supported feedback events and continuation were confirmed. Radio starts only
from **Слушать**, using the existing queue/player/MPRIS. Pause retains its
context; ordinary music or MPRIS Stop retires it locally without `radioFinished`.
Startup, restored sign-in and Home navigation do not start or restore radio.

PR #25 was squash-merged at `860672b`; the owner accepted ordinary Native and
installed Flatpak behavior, and #16 is closed. This is an overall owner report,
not a claim that every optional failure/backend/hardware case was individually
tested. This is not a public release/version/tag decision. See
[VALIDATION](VALIDATION.md), [the acceptance procedure](BUILDING.md#my-wave-local-acceptance)
and [ADR-0027](adr/0027-experimental-wave-lifecycle.md).

## v0.6: reactions and separately gated audio storage

The Dislike branch extends account-bound Like/Unlike with a distinct disliked-track
collection and removal. Valid dual reads and revision acknowledgment determine
paired confirmation; unavailable Dislike leaves the independently confirmed
Like-only path usable. Unknown state and ambiguous writes stay explicit. The Player Bar
adds an independent Dislike target and delegates confirmed current-occurrence
Dislike addition to existing Next without changing the audio path or Wave protocol.
The owner confirmed live server reactions and their interaction at `1877a6c`.
PR #27 merged at `ddf93ea`; on 2026-11-06 the owner confirmed corrected
Dislike→Next, ordinary queue and Wave behavior on Fedora Native and installed
Flatpak without remarks. [Issue #17](https://github.com/tracerxbrhd/karin/issues/17)
is closed. Special race/failure cases remain separately automated evidence,
not newly inferred owner tests. See the
[procedure](BUILDING.md#track-reaction-acceptance) and
[ADR-0028](adr/0028-confirmed-track-reactions.md).

## Completed v0.6 private audio cache

[PR #26](https://github.com/tracerxbrhd/karin/pull/26) merged with its development
history at [`1fd4028`](https://github.com/tracerxbrhd/karin/commit/1fd40289771af5a627e05eaaa47d2e6381de4b89).
The owner accepted `c1de869` on 2026-11-11: real Native playback/background MP3
saving/local hits/restart-local playback, and installed Flatpak playback/cache/
restart-local hits. Downloading/Saved graphics are provisionally accepted, with
further visual refinement reserved for the incoming design. [Issue #20](https://github.com/tracerxbrhd/karin/issues/20)
is technically complete; public service/content distribution #5/#22 is separate.

One enabled-by-default 2 GiB budget covers metadata, artwork and eligible private
HTTP audio. The six limits, measured usage, LRU, Clear/Retry and disk headroom
remain shared. Current Music access/selection, integrity and reuse policy decide
local-first playback; the existing GStreamer player and independent downloader
remain. See [ADR-0029](adr/0029-synthetic-audio-cache-foundation.md),
[AUDIO_CACHE_REVIEW](AUDIO_CACHE_REVIEW.md) and [VALIDATION](VALIDATION.md).
This acceptance does not establish offline access lifetimes, a public service
license, a release/version/tag or every unreported failure/backend case.

## v0.7: playback restoration, playlist editing and appearance

One branch, `feature/v0.7-playback-playlists-appearance`, starts at the accepted
merge and combines #21 and #19 with independent UI-style/color-scheme settings.
Restore a bounded same-account queue/order/position without autoplay or saved
media/Wave resources; confirm each enabled playlist write with current ownership,
revision semantics and independent readback; keep the working System presentation
and prepare the unavailable Karin Style for real designer materials. These new
functions require their own final implementation/CI and one desktop acceptance.

## Further directions

- [Lyrics and related tracks](https://github.com/tracerxbrhd/karin/issues/18): confirm permitted endpoints/resources before
  exposing a user workflow; no extracted official-client signing material.
- [Playlist editing](https://github.com/tracerxbrhd/karin/issues/19): extend beyond
  confirmed creation only with ownership/revision and ambiguous-write recovery.
- Later offline audio needs separately confirmed access lifetimes and renewal
  rules. Accepted online-revalidated caching does not establish that feature.
- [Restart playback state](https://github.com/tracerxbrhd/karin/issues/21): define
  bounded same-account context/position restoration without storing media URLs.
- Whole-collection search/sorting: provide complete coverage or label a loaded
  prefix honestly without silently changing the playing queue.

Each direction needs its own confirmed contract and acceptance criteria.
Unsupported actions remain absent.
See [API_FEASIBILITY.md](API_FEASIBILITY.md) and
[AUDIO_CACHE_REVIEW.md](AUDIO_CACHE_REVIEW.md).

[The backlog audit](ISSUES_AUDIT.md) records closed milestones, remaining
criteria and separate external, technical-debt and future-feature priorities.

## Public distribution

Resolve the OAuth-registration and Music API distribution conditions tracked in
[Issue #5](https://github.com/tracerxbrhd/karin/issues/5), separately from technical
interoperability. GPL source reuse and private account success do not establish
provider approval.

A [public release](https://github.com/tracerxbrhd/karin/issues/22) also needs a version/tag decision, packaging/runtime checks,
corresponding source and preserved third-party notices. Add a Flathub link only
after publication. No release date is scheduled.
