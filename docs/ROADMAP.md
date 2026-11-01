<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin Roadmap

## Accepted v0.2 foundation

[PR #13](https://github.com/tracerxbrhd/karin/pull/13) is merged into `master` at
`09f76355684873f32aaf83b8050e047df6077143`. The owner reports **17 PASS, 0 FAIL
and 1 N/D** on Fedora 44 / KDE Plasma / Wayland / PipeWire, including the Flatpak.
Real sign-in, library/search, entitled Music playback, contextual queues,
transport/seek/volume, MPRIS registration, KDE Media Widget and shutdown form
the accepted baseline. Hardware media keys are N/D. This supersedes older UI,
search and packaging gaps for their recorded revisions; the complete dated
history remains in [VALIDATION.md](VALIDATION.md).

Preserve the accepted native stack, Cassette-compatible voluntary sign-in,
bounded identity-neutral MusicService, guarded StreamResolver, existing
GStreamer worker, 500-position PlaybackContext and in-process MPRIS adapter.
No generated audio, replacement provider or metadata-only queue may stand in
for that working service path. A new revision still requires regression checks.

## v0.3 — Seamless Experience: functional acceptance

Delivered through one `feature/v0.3-seamless-experience` branch and
[PR #14](https://github.com/tracerxbrhd/karin/pull/14). On **2026-10-31** the owner
accepted runtime `02325489d23ead9da4471de9cc35e2c063ac8bab`: persistent sign-in
and restart restoration, real covers in lists/Player Bar/right pane, actual
Music playback, pane mechanics and KDE Media Widget cover/metadata. The owner
also reported successful final Flatpak authorization, playback and system
integration, with no blocking functional defect reported. Exact scope and
historical failures remain in [VALIDATION.md](VALIDATION.md).

PR #14 is merged into `master` at `5e85e8d`; its accepted runtime remains the
separately identified `0232548` above. The feature branch was retained. The
sections below record the delivered implementation scope and its continuing
constraints; new v0.4 code still needs its own verification and owner acceptance.

### A. Persistent Session and Settings

Add asynchronous secure-store discovery, saved-token recovery and independent
Music account validation. Store only the needed token after accepted Music
validation; keep WebKit cookies, sessionid, password and temporary OAuth material
ephemeral. Support the available native Secret Service and investigate the
Flatpak Secret Portal/runtime contract without assuming either exists.

Keep restoring, authenticated, invalid session and unavailable storage distinct.
A missing/locked/broken backend permits ordinary sign-in without plaintext
fallback. Remember-login is effective by default only when secure storage is
supported; turning it off deletes saved credentials while the current in-memory
session can continue. Logout stops playback, invalidates old work, clears
personal presentation and confirms saved-secret deletion; unknown/failed deletion
must remain visible. Do not claim refresh or server revocation.

Persist actual application preferences, with private atomic XDG storage and
safe recovery from invalid settings. GTK must not wait for disk, keyring or
network work. [ADR-0018](adr/0018-persistent-session-and-settings.md) records the
new lifecycle boundary; the existing authentication identity is retained.

### B. Bounded Metadata and Artwork Cache

Add private per-account metadata and normalized-cover persistence, independent
of GTK. Use TTL, explicit refresh, byte limits, LRU-like eviction, atomic writes,
corruption recovery, bounded jobs and generation-aware cancellation. Disabling
cache prevents persistent reads and new writes; existing records remain until
explicit clearing. Clearing cache must preserve the secure session and current
playback. Live authorization, membership, playlist revision and write confirmation
remain server facts, not cached assertions.

Expose enabled state, size limit, actual usage and clear progress/errors in
Storage preferences. Label network-fallback records as saved/potentially stale.
[ADR-0019](adr/0019-bounded-account-cache.md) defines policy and isolation.

Persistent audio is a separate permission/entitlement problem. Implement it only
if a permitted third-party download/storage contract and enforceable offline
rights lifecycle are established. Otherwise keep audio off disk, omit an inert
Offline switch and record the exact blocker in
[AUDIO_CACHE_REVIEW.md](AUDIO_CACHE_REVIEW.md). That blocker does not defer
metadata, artwork, session or workspace work.

### C. Music Workspace

Add an adaptive track information pane with actual cover/title/artist/album,
explicit actions and only verified navigable identities. Keep click/Enter/Space
as playback and Details as its separate action. Use an appropriate compact
presentation, and preserve the accepted player bar and loaded-context behavior.

Present playlists as adaptive native cover tiles with real metadata, long-title
and missing-cover handling, loading/empty/error states and keyboard activation.
Expose creation only through a source-confirmed server contract and independent
server confirmation; an ambiguous write is not a local success or automatic
retry. [ADR-0020](adr/0020-seamless-music-workspace.md) records these UI choices.

My Wave requires a complete radio-session/batch/listening-feedback contract,
not a renamed ordinary queue. Similar tracks require a confirmed real endpoint.
Keep unsupported functions hidden and document precise gaps; do not add fake
recommendations or decorative disabled controls. The updated
[capability audit](CASSETTE_PARITY.md) separates upstream source evidence, Karin
implementation, fixture checks and live acceptance.

### D. Integration and Regression

Run stable and exact Rust 1.98.1 formatting, warning-free Clippy, default/all-
feature/minimal tests/builds, offline release and packaging/license checks.
Exercise secure-store recovery/failure/delete races, cache isolation/TTL/
corruption/limits/clear/disable and settings persistence with synthetic local
fixtures. Reuse the accepted GTK/WebKit/GStreamer/MPRIS lifecycle regressions.

Inspect fresh actual screenshots and GTK structure at approximately 420×720,
900×650, 1440×900 and maximized, in light/dark themes and scale 2, including
keyboard/focus, track pane, playlist tiles and preferences. Xvfb does not establish
Fedora Wayland acceptance. New native and Flatpak runs must check real music,
session recovery, storage availability, cache behavior and shutdown.

The integration PR records implemented code, executed commands/results, inspected
images, actual native/Flatpak storage evidence, blocked service features and
[one Fedora acceptance procedure](BUILDING.md#v03-seamless-experience-acceptance-on-fedora).
The separate v0.3 owner report accepts its stated functional scope, not every
unreported backend/failure case or future code change.

## v0.4 — Complete Music Experience in existing Draft PR #15

The owner rejected the UI/UX of `a9eed4514802df84d73b0608dd1a62930fc1af76`
on Fedora despite its successful cloud checks. The new
[design specification](design/v0.4/DESIGN_SPEC.md) and
[implementation task](design/v0.4/CODEX_TASK.md) supersede conflicting earlier
Native Music mockups. Preserve that failure and the accepted v0.3 service/audio
baseline as different evidence. Continue only in
[`feature/v0.4-native-music-ui` / Draft PR #15](https://github.com/tracerxbrhd/karin/pull/15);
no new PR, merge or branch deletion is authorized.

The delivery scope is one coherent native client, not a CSS-only revision:

- Three primary destinations: Home, Search and Library. Real covers, liked
  tracks and available playlists supply Home's hierarchy; no giant duplicate
  shortcut cards, fabricated recommendations or repeated page headings.
- One universal right panel with `Closed`, `TrackInfo` and `Queue` modes.
  Cover/Ctrl+I and footer Queue/Ctrl+J select content in the same container.
  Mode changes do not close/reopen it; outside input, navigation and resizing
  preserve mode. The entire background/header/text/art/action surface moves
  together. Queue is no longer a separate primary destination.
- A permanent, centered Player Bar with actual metadata/artwork, seek/progress,
  volume and queue. Functional Shuffle and Repeat Off/Queue/One replace the
  visible Stop control; controller and MPRIS Stop remain supported.
- A bounded occurrence-based shuffle plan and history over the existing loaded
  PlaybackContext. Repeat One handles EOS at the same occurrence; explicit
  Next/Previous still navigate. Repeat Queue wraps loaded available items.
  MPRIS writable Shuffle/LoopStatus and native buttons share that policy.

[ADR-0022](adr/0022-universal-music-panel.md) and
[ADR-0023](adr/0023-local-playback-order-and-mpris-modes.md) record these decisions.
Preserve MusicService, guarded StreamResolver, GStreamer, session/cache,
server-confirmed writes and the 500-position/partial-context boundary.
Unsupported offline audio, My Wave, lyrics and recommendations remain outside
this iteration.

Implementation, executed regression evidence and owner acceptance remain
separate gates. The expanded [quality gate](UI_QUALITY_GATE.md#v04-native-music-gate)
requires fresh equal-size before/after GTK evidence at the six required explicit sizes, owner-wide 1650×930 and
true maximize, both themes and scales, all three panel modes, populated/error
states and intermediate animation frames. Current strict motion evidence is
explicitly graphics-initialized: raw setup is separate, only the following
unchanged normal native cycle is measured, and cold first-use responsiveness
remains unaccepted pending the fresh-start Fedora gate. No production
prewarming or backend change is introduced. Run stable and exact Rust 1.98.1,
default/all/minimal tests, release, GTK/WebKit/GStreamer/MPRIS and available
Flatpak checks. Results belong in
[VALIDATION.md](VALIDATION.md#v04-complete-music-experience-redesign), not in a
claim inferred from the previous CI. Use the
[single Fedora guide](BUILDING.md#v04-native-music-acceptance-on-fedora) for a new
real Music, desktop and visual acceptance. Keep PR #15 Draft until supplied.

## Independent release conditions

Code licensing, working account access and technical playback are distinct from
public OAuth-registration/service-distribution permission and content rights.
Keep GPL-3.0-or-later, third-party notices, the GNOME 50 / Freedesktop 25.08
foundation and Rust 1.98.1 compatibility. No real account or secret enters Cloud,
CI or GitHub; audio tests use original generated media and loopback fixtures.

v0.3 is not a public-release declaration. The supplied functional acceptance and
explicit PR #14 merge authorization do not establish public OAuth distribution
rights, offline-audio rights or a later release approval. New runtime changes
still require their own checks and acceptance.
Earlier v0.0/v0.1 feasibility decisions and test histories remain in the ADRs and
validation record; superseded restrictions do not replace the current task.
