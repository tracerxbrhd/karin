<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin contributor instructions

Karin is an independent native Linux client for Yandex Music. Keep changes
reviewable and consistent with the owner-accepted v0.8 application.

## Architecture and ownership

- Keep Rust 1.98 compatibility (exact 1.98.1 and stable), GTK4/Libadwaita,
  GStreamer, reqwest/serde, GIO D-Bus/MPRIS, protected storage and Cargo/Flatpak.
  WebKitGTK6 is sign-in only. Never use `--ignore-rust-version` or casually raise
  dependency/runtime requirements.
- Separate widgets, state, API, audio and platform adapters. Keep blocking I/O
  and wire parsing off GTK. Preserve bounded work/cancellation, weak UI owners,
  account/request/source generations and deferred membership reads without
  reentrant controller borrows. Late work cannot update a replaced view/session.
- Read [ARCHITECTURE](docs/ARCHITECTURE.md) and relevant [ADRs](docs/adr/) before
  changing contracts; record significant decisions concisely.

## Security and service boundaries

- Never collect passwords, import sessions, expose secrets/log personal models or
  send real credentials/account content to Cloud/CI/GitHub. Use synthetic fixtures.
- Preserve ordinary **Войти → Яндекс → validated Music account → library** with
  the attributed Cassette-compatible provider. No registration, pasted tokens,
  technical credentials or manual codes in normal UI. Own-client PKCE is separate.
- Follow [AUTHENTICATION_REVIEW](docs/AUTHENTICATION_REVIEW.md): ephemeral WebKit,
  exact origins/callback/state, scoped in-memory cookies and cancellation. Never
  inspect password fields, inject login-reading JavaScript, persist cookies,
  weaken TLS or disable sandboxing.
- Store only Music-validated tokens in supported protected storage, with no
  plaintext fallback, invented refresh or remote revocation. Permit memory-only
  login. Keep durable forgetting barriers and visible failure/Retry; remember-off
  preserves live audio/session. Logout clears personal state; close may retain
  permitted credentials/cache.
- Metadata is not playback entitlement. Follow
  [STREAM_DELIVERY_REVIEW](docs/STREAM_DELIVERY_REVIEW.md); no DRM bypass, extracted
  keys, arbitrary URIs, forwarded OAuth/cookies to media hosts, persisted signed
  addresses or redistribution of music.
- Follow [AUDIO_CACHE_REVIEW](docs/AUDIO_CACHE_REVIEW.md): separate current Music
  access, authenticated offer/variant, integrity, HTTP storage policy, reuse proof
  and content restrictions. HTTP caching is no legal license; absence of a bespoke
  storage-grant API is no blanket cache ban. Honor explicit refusal/`no-store`;
  `no-cache` needs revalidation and `must-revalidate` applies once stale. Match a
  fresh complete retained MP3 before XML/CDN resolution. Logical selection is not
  immutable byte identity, and replica validators are not globally comparable.
  Missing modern receipt clocks require strict proof. Only pre-schema-2/unknown
  original-status records may migrate after inconclusive proof via a complete new
  response; never stamp old bytes fresh. Expired modern replacement requires fresh
  current probe and actual GET policy. Protect retained records through quota
  reservation, atomic publication and rollback; preserve admitted online playback
  when caching fails.
- Preserve [LICENSING](docs/LICENSING.md), standard GPL, SPDX and third-party
  attribution/provenance. Source licensing, OAuth identity and service/content
  distribution permission remain distinct.

## Music and presentation contracts

- Keep one MusicQueue/PlaybackController/GStreamer engine. MPRIS exports actual
  snapshots and confirmed seek/volume, opaque track paths and decoded local covers.
  Never widen Flatpak D-Bus/filesystem permissions.
- Mouse/Enter/Space plays the exact occurrence; separate Info never plays or opens
  a standalone track route. Preserve duplicates, opaque occurrence IDs and separate
  original source positions. Ordinary views: 2,000 positions/100 pages. Index:
  20,000 original positions, 50-position batches, final membership recheck and
  200-row windows. Playback context: 500. Filtering/sorting/indexing/appending and
  server edits cannot rewrite the active queue.
- The collection entry fills its content column beside a compact native sort
  button. Unicode title/artist/album matching is immediate; eligible incomplete
  index work starts after settled search intent/Enter, suspends on clear/leave and
  reuses admitted data under the latest query. No startup or per-keystroke scans.
  Date sorting needs valid timestamps; total duration needs complete known data.
- One global entry shares the measured tab slot and group center at all widths.
  Toggle/Ctrl+F/Home focus it without changing route until query results. Clear
  restores the source route with the field open; Escape/toggle/background dismiss
  and retire work while preserving interactive targets and deliberate navigation.
- Preserve pinned Track Info/Lyrics/Queue intent across outside clicks/navigation/
  resize. New current occurrences resume following. Lyrics toggle/Back return to
  Track Info in place. Footer hides before Play and on Stop/clear/logout, remains
  through Pause/retryable failure, and has no visible Stop. Keep 24 logical pixels
  of inner bottom spacing and zero hidden height. Preserve System surface levels,
  dividers, player padding and minimal successful Lyrics/Similar captions.
- LRC uses actual player position for the exact following occurrence. Old glyphs
  shrink as new glyphs grow in size/weight/contrast; measure both painted rows in
  the same native frames. Reduced Motion/seek/selection fences apply immediate
  final state. Manual scroll suspends centering; rebound rows use current emphasis.
  Yandex supplement is literal TEXT without timing. LRCLIB requires explicit
  per-request consent or separately saved default-off automatic permission;
  automatic lookup is visible-Lyrics-only after missing synchronization, with
  account/request fences and effective revocation. No cached/persisted lyrics.
- Explicit Resume may renew an expired source once for the same occurrence,
  preserving confirmed position, modes/volume and Wave/MPRIS identity. Pause has
  no periodic network renewal or visible technical-expiry error. Preserve honest
  access/network/unseekable failures and original signed-source lifetimes.
- Continuation is bounded account-bound IDs-only worker-written state. Restore
  after fresh Music validation without autoplay/player preparation. Stop/forget/
  logout delete it; Wave never persists. Preserve exact duplicate/history state.
- System is the available/default UiStyle, independent of System/Light/Dark color.
  Karin remains reserved/unavailable until real designer materials exist. Do not
  retain or reintroduce the removed experimental presentation, renderer layers,
  effects or texture cache. A legacy `liquid-glass` settings value may appear only
  in safe fallback/migration coverage and must preserve all other preferences.
- Playlist writes use complete exact positions/revision, one POST per intent and
  independent readback. Unknown results permit GET-only reconciliation; metadata/
  deletion endpoints have no proven revision CAS. Never replay ambiguous writes.
- My Wave is ordinary signed-in Home after Music validation. Only Listen creates
  radio; startup/restore/Home do not. Keep original occurrence/batch markers,
  actual worker Playing ledger and bounded serialized four-event feedback. No
  session-finished event on local retirement, persisted radio data or POST replay.
  The old Wave flag is a no-op alias. Do not invent fake content or inert controls.
- Metadata/artwork/policy-admitted audio share one default-on 2 GiB budget and six
  limits including Unlimited. Preserve disk headroom, worker reservations, LRU,
  Clear/Disable generation fences and v1/v2-to-schema-3 migration. No disk scans
  during quota dragging. Catalog refusals fence before I/O, replies recheck key
  leases, cold metadata stays Stale until a positive write, and GTK's cache state
  mutex cannot span filesystem work.

## Workflow and verification

- The owner accepted v0.8 Fedora functionality and authorized normal merge of
  **PR #29** after the requested cleanup and final checks pass. This supersedes
  older separate-approval/pending-full-acceptance wording for that PR. Do not ask
  for a repeat of the accepted full matrix. No release/tag, new PR, v0.9, squash,
  rebase merge, force push or shared-history rewrite is authorized.
- For other work, follow the branch/PR and merge authorization actually supplied.
  Keep docs concise, audit consumers before deletion and verify tracked links/
  assets. Preserve essential history through immutable commits/PRs rather than
  giant run logs or expiring artifacts alone.
- Use [BUILDING](docs/BUILDING.md) for format, strict Clippy, default/all/minimal
  tests, stable/exact-1.98.1 compatibility, locked release, Flatpak preparation
  and license checks. Required native groups retain GTK/WebKit sandbox, audio/
  MPRIS/Wave/cache/restoration/playlist/Lyrics/search regressions; full extended
  galleries need not repeat on every toolchain. Ignored registration is no PASS.
- Substantial UI work follows [UI_QUALITY_GATE](docs/UI_QUALITY_GATE.md): actual
  captures/structure/input, compact through maximized, both schemes, targeted
  HiDPI, focus, High Contrast and normal/Reduced Motion. Do not relax timing,
  geometry, renderer or sandbox checks. Private captures require publication
  permission; Xvfb is separate from Fedora acceptance.
- Before handoff verify final-head CI, licenses, links, packaging and diff.
  Report only executed checks and confirmed GitHub state. Read
  [VALIDATION](docs/VALIDATION.md) and [LIMITATIONS](docs/LIMITATIONS.md): unresolved
  Cloud cold-rendering and public-distribution #5/#22 remain independent.
