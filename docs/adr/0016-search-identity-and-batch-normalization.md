<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0016: Minimal search identity and batched track normalization

Status: Accepted for the PR #13 correction; live corrected-search acceptance pending
Date: 2026-10-31

## Context

The owner accepted real Yandex Music playback, controls, queue and cleanup at
`8f1b09d` on Fedora. The same run still rejects track search with HTTP 200 /
`track-wire` / `track-fields-unsupported`: own track identity passes the initial
check, but remaining search display metadata cannot deserialize. No private
response body or specific culprit field was supplied. Repeatedly guessing an
optional display field is not a stable search contract.

The existing account-bound `MusicService` `/tracks` lookup already normalizes
tracks used by the working Library/details/player path. Preserve it, the accepted
AuthProvider and the GStreamer integration. Keep transport/schema concerns out
of GTK and retain existing network and privacy limits.

## Decision

Deserialize each track search result as a minimal `SearchTrackHit` with one
validated own ID. Ignore all other well-formed JSON search fields, including
display labels, nested credits, duration, artwork and search availability.
Missing, invalid or ambiguous identity remains a contract failure; malformed
JSON, pagination and envelope errors are not converted into successful data.

Accept at most **50 hit positions** per track page and expand their distinct
validated IDs through one existing `/tracks` batch, retaining the existing
**50 distinct ID** limit. A normal page uses at most one search GET and one
normalization POST, with the already validated token-bound
account cache; there are no per-row requests or additional account probes.
Retain bounded bodies, request deadlines, no redirects and no automatic retries.
Recheck account/session validity across the operation and discard stale results.

Do not invent an unsupported page-size parameter, silently truncate a returned
page, add an internal cursor or increase the batch contract to 1000 IDs. A track
page with more than 50 positions fails with a finite size error before the POST,
even if many hits repeat the same ID. This also bounds amplification when full
normalized models are restored to duplicate positions. Album/artist wire-page
bounds remain unchanged.

Use the normalized records as the sole source of display/playback models.
Preserve search order, duplicate positions and existing composite-ID matching.
Missing/null lookup records are omitted with an explicit count; a normalized
`available=false` track remains visible and disabled. Do not manufacture labels,
playability or a placeholder track from the search hit. Provider totals/page
boundaries control pagination, rather than the count left after normalization.
Lookup/normalization failures have a separate finite `track-lookup` diagnostic
stage; safe stage/category/numeric HTTP details are disclosed only on demand.

Ordinary search has one plain-language failure state with explicit Retry, not
duplicated global/inline technical messages. A found normalized track follows
the existing detail and explicit Play paths. Account logout moves behind an
explicit menu item and native confirmation with Cancel as the safe default;
cancellation preserves the session, queue and active playback.

## Validation and consequences

Test odd-shaped display fields without making their exact shape the parser's
contract. Cover strict IDs, composite/duplicate ordering, page boundaries,
one-batch request counts, oversized pages, missing/null/unavailable normalized
records, lookup failures, cancellation and account changes. Native fixtures must
exercise actual search input, opening a result and explicit playback, plus safe
logout cancellation/confirmation and keyboard focus.

Stable and Rust 1.98.1, GTK/WebKit/GStreamer, offline release, licensing, CI and
fresh visual evidence remain required. These fixtures do not prove corrected
live search on Fedora. The owner's earlier audible-track PASS remains valid for
its revision, while the updated build needs one search/UX/sound-regression run.
Record outcomes in [VALIDATION.md](../VALIDATION.md) and PR #13.

No new dependency, OAuth identity, source resolver, audio mechanism, branch or PR
is required. The correction does not grant public registration/service/content
distribution rights or establish current Flatpak acceptance. After owner
acceptance, retargeting existing PR #13 to `master` and closing superseded PRs
requires explicit approval; no base change or merge occurs as part of this work.
