<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin contributor instructions

Karin is an independent native Linux client for Yandex Music. Keep changes small,
reviewable and consistent with the accepted application.

## Architecture

- Keep Rust, GTK4/Libadwaita, GStreamer, reqwest/serde, GIO D-Bus/MPRIS,
  Secret Service and Cargo/Flatpak. WebKitGTK6 is confined to sign-in.
- Preserve Rust 1.98 compatibility; check exact 1.98.1 and stable. Never use
  `--ignore-rust-version` or change dependencies/runtime requirements casually.
- Separate widgets, application state, API transport, audio and platform adapters.
  Keep wire parsing and blocking network/storage work off the GTK thread.
- Preserve cancellation, bounded work, account isolation, generations and weak UI
  ownership. Late responses must not affect a replaced view/session/window.
  Defer map-triggered membership work; avoid reentrant controller borrows in GTK.
- Read [ARCHITECTURE](docs/ARCHITECTURE.md) and relevant [ADRs](docs/adr/)
  before changing contracts; record significant decisions in an ADR.

## Security and service boundaries

- Never collect passwords, import other applications' sessions, expose secrets,
  log personal models or send real credentials/account data to Cloud/CI/GitHub.
  Tests use synthetic local fixtures, without a live account.
- Preserve ordinary **Войти → Яндекс → validated Music account → library** with
  the Cassette-compatible provider. No technical credentials, registration,
  pasted tokens or manual codes in the normal UI. The opt-in own-client PKCE
  diagnostic remains independent.
- Follow [AUTHENTICATION_REVIEW](docs/AUTHENTICATION_REVIEW.md): ephemeral
  WebKit session, exact origins/callback, state, cancellation and scoped in-memory
  cookies. Never read password fields, inject login-reading JavaScript, persist
  cookies, weaken TLS or disable the WebKit sandbox.
- Store only validated tokens through supported protected storage; no plaintext
  fallback or invented refresh/revocation. Allow memory-only login. Expose failed
  logout/deletion; forgetting a saved login or clearing cache must preserve active
  in-memory audio where specified. Normal close may retain permitted session/cache.
- Treat the API as unofficial and unstable. Metadata is not playback entitlement.
  Follow [STREAM_DELIVERY_REVIEW](docs/STREAM_DELIVERY_REVIEW.md); never bypass
  DRM/access controls, import extracted keys, redistribute music, forward OAuth/
  cookies to a CDN, or persist signed media URLs. Private audio caching separates
  current playback access, HTTP storage policy, representation revalidation and
  explicit content restrictions under [AUDIO_CACHE_REVIEW](docs/AUDIO_CACHE_REVIEW.md).
  Do not invent a provider entitlement or treat HTTP caching as a legal license.
  Do not use the absence of a bespoke storage-grant API as a blanket HTTP-cache
  rejection. Reject protected/unsupported resources and explicit storage refusals;
  honor `no-store`, require online validation before reuse and fall back online
  when the saved representation cannot be verified.
- Preserve attribution and explicit OAuth-identity uncertainties. Code licensing
  does not grant service/distribution permission or prove a provider ban.
  Follow [LICENSING](docs/LICENSING.md); keep standard GPL and third-party notices.

## Music and UI contracts

- Keep one queue/PlaybackController/GStreamer engine. MPRIS adapts actual snapshots
  and confirmed seek/volume; export supported capabilities, opaque track paths and
  decoded local covers only. Do not widen Flatpak D-Bus/filesystem permissions.
- Row click/Enter/Space plays the exact occurrence; Details is separate. Preserve
  duplicates and original source positions. Views stay bounded at 2,000 positions/
  100 pages; new contexts at 500. Filtering/appending must not rewrite active Queue.
  Info/MPRIS use queue-owned opaque occurrence IDs, never mutable indices or IDs
  of service tracks. Original source positions remain separate provenance.
- Search admitted collection metadata with Unicode matching and honest partial
  coverage. No whole-library drain on typing. Total duration needs complete data
  with every duration known and a checked sum.
- Preserve pinned Track Info/Queue intent across outside clicks/navigation/resize;
  actual new current occurrences resume following. Footer: hidden before Play,
  retained through Pause/retryable active failure, hidden on Stop/clear/logout.
  No visible Stop. Keep local Shuffle/Repeat and existing system controls.
- Mutations need server confirmation; never replay an ambiguous write. Use clear
  loading/empty/error/Retry states. No fake radio, recommendations, lyrics,
  offline mode or inert controls. My Wave is available in the ordinary signed-in
  app after Music validation. Only explicit Listen creates a radio session;
  startup, protected-login restore and opening Home must not start Wave.
  Do not send a session-finished event on local retirement or persist radio IDs
  and batch markers. The legacy Wave CLI flag is a no-op compatibility alias.
  Preserve its original occurrence/batch markers, actual worker Playing ledger,
  bounded serialized feedback and no replay after an ambiguous POST.
- Cache metadata, artwork and policy-admitted audio under one shared budget
  (default on/2 GiB, six discrete choices including Unlimited). Preserve worker
  reservations, disk headroom, generation fences and the v1 settings migration;
  do not reintroduce separate user quotas or run disk scans while dragging.
- Catalog refusals fence key epochs before cache I/O; cached UI replies recheck
  those leases. Cold metadata stays Stale until a current positive write. Never
  hold the cache state mutex used by GTK over filesystem operations.
- Follow [UI_QUALITY_GATE](docs/UI_QUALITY_GATE.md) for substantial UI changes:
  real captures plus GTK structure, compact/medium/wide/maximized, both themes,
  HiDPI, mouse/keyboard/focus and normal/reduced motion. Private screenshots need
  separate publication permission. Xvfb is not Fedora acceptance.

## Workflow and documentation

- Continue the branch/PR requested by the owner. Do not merge, close PRs, delete branches,
  rewrite history or publish releases without explicit authorization.
- Keep docs concise. Audit code/CI/packaging consumers before deleting a file;
  verify links and assets are tracked in Git. Preserve important history through
  immutable commits/PRs, not expiring artifacts alone. Retain SPDX/provenance.
- Distinguish authored code, executed tests and owner acceptance. See
  [VALIDATION](docs/VALIDATION.md): native/UI and Flatpak at `97093ee` are accepted;
  Dislike→Next and available `707539b` Native/installed-Flatpak functions are
  owner-accepted; `0709729` is the last fully accepted PR #26 revision. At
  `31d873d` the owner confirmed real first playback/background saving on Native,
  while local replay and restart failed with `resource-changed`. At `cac4d78`,
  real saving still worked, but descriptor-path/representation-changed caused
  repeated downloads; no local hit was confirmed. Revised validator scoping and
  retained cache indicators need separate acceptance.
  Unreported individual scenarios and unresolved Cloud cold gaps are not PASS.
  This is not a published release. Read [LIMITATIONS](docs/LIMITATIONS.md).

## Verification

Use [BUILDING](docs/BUILDING.md), including:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --all-features --locked
cargo test --workspace --no-default-features --locked
cargo build --workspace --locked
```

For Rust/dependency/packaging changes check stable and exact 1.98.1, feature
compatibility and locked/offline release; also run explicit task-required checks.
Keep complete native CI coverage, including sandboxed navigating WebKit and
modern static/motion/cold observers. Ignored tests are not executed tests. Never
relax timing, geometry, sandbox or renderer checks just to obtain green CI.

Before handoff verify final-head CI, links, license inventory and packaging.
Merge still needs a separate explicit instruction. Report actual checks and
remaining limitations.
