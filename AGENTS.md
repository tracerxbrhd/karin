<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin — Codex Instructions

## Product
Karin is an independent, open-source, native Linux client exclusively for Yandex Music.

The priorities are user experience, reliability, performance, and maintainable architecture.

## Approved Stack
- Rust (stable)
- GTK4, gtk-rs, Libadwaita
- Blueprint when appropriate
- GStreamer
- reqwest and serde
- D-Bus / MPRIS
- Secret Service for credentials
- Cargo and Flatpak

Do not replace this stack without a documented technical reason.
Do not introduce Electron or web-based application shells.

## Architecture
- Separate UI, domain, application services, network API, audio, and platform integrations.
- Keep Yandex API transport details out of UI components.
- Use typed models and explicit error handling.
- Never block the GTK main thread with network requests.
- Avoid unnecessary services, dependencies, and abstractions.
- Document significant decisions in docs/adr/.

## Service Integration and Security
- Treat the Yandex Music API as unofficial and unstable.
- Verify actual endpoint behavior before claiming support.
- Validate authentication and playback independently.
- Do not bypass DRM, access controls, or other protections.
- Never collect account passwords.
- Never commit, log, or expose access tokens and other secrets.
- Never distribute copyrighted audio content.
- Do not use fake production API responses to conceal unsupported features.
- Flag legal or technical blockers explicitly.
- Gate B1's `--oauth-diagnostic` is a consenting owner's local-only experiment.
  Never supply real OAuth inputs in Cloud/CI/GitHub or add a token-paste shortcut.
  Automated OAuth checks use loopback fixtures and synthetic terminal input only.
  Paste the full browser redirect only into the hidden local prompt; validate
  its exact origin/path and returned state before a token request. Treat the
  authorization code as opaque. Manual URL transfer is not an automatic
  production callback, and ID success does not establish Music access.
- The normal sign-in journey has one `Войти` action and a single-purpose native
  dialog. Never expose developer Client ID/secret, device-code copying, manual
  redirect transfer or token entry in that journey, including experimental modes.
  The legacy feature-gated Device Flow module is library-only, with no GUI entry.
  Do not add official-client request headers or unrelated OAuth identities.
- The owner explicitly permits adapting Cassette's published authentication chain
  for the functional prototype; a Karin-owned OAuth registration is no longer a
  prerequisite. The default `cassette-auth` provider must attempt real sign-in,
  not return a permanent local provider block or reuse the known ID 200/Music 403
  diagnostic as its normal path. Retain upstream GPL notices and attribution.
  Document the published client identity transparently; distinguish code-license
  permission, unresolved service/registration distribution conditions and actual
  technical errors. Public source alone does not establish service permission.
- WebKitGTK6 is limited to the authentication dialog; Karin remains native
  GTK/Libadwaita. Use a fresh ephemeral session, exact trusted origins/return path,
  random returned-state validation, bounded work and cancellation. Read only
  cookies applicable to the official token endpoint from this voluntary Karin
  session, in memory, for the scoped `sessionid` exchange. Never inspect DOM/form
  passwords, inject JavaScript, import browser profiles, persist cookies, log
  navigation URLs, or forward cookies to Music API. Sessionid is not code/PKCE;
  keep the independent own-client PKCE diagnostic unchanged.
  A Yandex page, session or token alone is not Music sign-in: validate Music
  before completing the dialog, then load real library data. RFC 8252 rejects
  embedded native OAuth; reviewed Yandex mobile SDK support does not establish
  Linux WebKit permission. Record that uncertainty without inventing a provider
  ban or approval. Report a failed chain's concrete stage and safe category.
- Session and account/catalog acceptance are independent. Never turn metadata
  into playback entitlement. Issue #9 supersedes the earlier audio deferral:
  real service playback is mandatory for v0.2. Investigate audio in parallel
  with catalog repairs. Bounded opt-in delivery-info requests may use the
  consenting owner's existing local session, without fetching media or
  disclosing IDs, response bodies or resource URLs. Do not import extracted
  signing/decryption secrets or infer that every signed resource uses DRM.
  A returned server-signed resource and an extracted client-side signing key
  are different mechanisms. Enable actual playback only for an established
  permitted source; preserve an unmet audible-player gate if none is confirmed.
  No raw provider messages, personal models or credentials in logs.
- Normal track-row activation (mouse, Enter and Space) starts the exact occurrence
  in a bounded PlaybackContext from its real loaded source. Details are explicit
  secondary actions. Preserve duplicate order, unavailable-item handling, original
  paging and the visible partial-context boundary; manual additions supplement it.
- MPRIS is an in-process GIO adapter to the same queue and PlaybackController,
  never a second engine. Publish actual worker state and confirmed seek/volume,
  opaque track object paths and decoded local cover files only. Never expose an
  OAuth value, cookie, account ID or signed media URL on D-Bus. Advertise only
  supported capabilities and retain the exact Flatpak MPRIS own-name permission.
- v0.3's protected session lifecycle is specified in ADR-0018. Persist only the
  Music-validated access token through native Secret Service or a Secret Portal
  master secret plus an authenticated encrypted envelope. Never use a plaintext
  fallback or add broad session-bus permissions. Probe, restore, validate, save
  and delete off the GTK thread; unavailable storage must allow memory-only
  sign-in. Do not invent token refresh or server revocation. Retain the current
  Cassette-compatible provider and keep the PKCE diagnostic independent.
- Normal close preserves the permitted stored session and cache. Explicit logout
  invalidates pending work, stops playback, clears personal views, verifies secret
  deletion and purges personal metadata/covers. An unresolved deletion or purge
  must stay visible and must not be reported as a completed durable logout.
  Turning off remember-login deletes stored credentials without stopping the
  active in-memory session or audio. Test late writes, cancellation, restart and
  replacement windows before claiming these transitions safe.
- ADR-0019 defines the bounded, account-isolated XDG metadata/cover cache. Keep
  mutable likes and playlist membership network authoritative; label network-error
  fallback as saved data. Recheck cache leases at the UI boundary. Disabling
  caching stops persistent reads/writes; clearing it must leave session and current
  playback intact. Never persist OAuth values, cookies, signed media URLs or audio.
- Persistent audio/offline playback requires an established service permission
  and offline entitlement lifecycle, not just a working MP3 URL. Follow
  docs/AUDIO_CACHE_REVIEW.md; hide unsupported offline and recommendation actions
  rather than presenting nonfunctional controls.

## User Experience
- Cassette by Rirusha is an aesthetic reference and, for the explicitly scoped
  authentication prototype, a GPL-licensed protocol implementation reference.
  Preserve notices for adapted components; do not copy its persistent cookie
  storage, substring return matching, URL logging or close-before-validation.
- Follow GNOME HIG and Libadwaita conventions where suitable.
- Support light/dark themes, HiDPI, keyboard navigation, and accessibility.
- Design loading, empty, offline, unauthorized, and error states.
- Prioritize responsiveness and clarity over decorative effects.
- Every substantial UI change must pass the visual and structural checks in
  [docs/UI_QUALITY_GATE.md](docs/UI_QUALITY_GATE.md) before being called ready.
- Inspect actual application screenshots at approximately 420×720, 900×650,
  1440×900 and maximized, in light and dark themes. Check navigation adaptation,
  HeaderBar/title/window controls, spacing, text wrapping, scrolling and focus.
- Exercise keyboard navigation and resize transitions. Screenshots supplement
  GTK allocation, navigation and lifecycle checks; neither replaces the other.
- Use Xvfb captures when available and state their environment. Xvfb does not
  establish Fedora/KDE/Wayland visual acceptance. Record manual acceptance as
  pending until someone actually performs it on the target desktop.
- Never add user screenshots containing personal information or desktop elements
  to a public repository without separate permission. Prefer clean cloud captures
  of Karin only, with no account data or unrelated desktop content.
- ADR-0020 covers the adaptive track pane, playlist grid and persistent preferences.
  Preserve the accepted player bar, context queue and click/Enter/Space playback.
  Mutation UI must wait for server confirmation, never fabricate local success or
  automatically repeat an ambiguous POST. The current Fedora acceptance guide is
  docs/BUILDING.md#v03-seamless-experience-acceptance-on-fedora.

## Licensing

- Karin's own code and original materials use GPL-3.0-or-later unless a file says otherwise.
- Add the corresponding SPDX identifier to new original source files and retain
  existing third-party license notices. Do not relabel dependencies as Karin code.
- Keep the standard GPLv3 text in LICENSE unchanged. GPL does not authorize
  access to Yandex Music, its unofficial API or any service content.

## Development Workflow
- Use master as the stable integration branch.
- Implement changes in dedicated feature/, fix/, or chore/ branches.
- Submit changes through Pull Requests.
- Keep commits focused and readable.
- Do not rewrite Git history or perform destructive operations without approval.
- Update documentation when behavior or architecture changes.
- Do not claim successful builds or tests without executing them.

## Verification
When a Rust workspace exists, run:
- cargo fmt --all -- --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- cargo build --workspace

Also check `--all-features` Clippy/tests and the `--no-default-features`
compatibility build. Default and Flatpak prototype artifacts enable
`cassette-auth` / `auth-webkit`; Device Flow stays excluded. Automated smoke and
capture paths must inject an offline provider or use explicitly identified
no-feature artifacts, never open real Yandex login. Test WebKit/session exchange
with loopback fixtures, synthetic cookies and tokens only. A real Fedora owner
run is separate; no account access or user secrets belong in Cloud/CI/GitHub.

Preserve the minimum Rust series declared in `Cargo.toml` (1.98). Run the pinned
Rust 1.98.1 CI checks as well as stable when changing Rust code, dependencies or
packaging; include Clippy, all tests and `cargo build --release --offline --locked`
after fetching or vendoring the locked dependencies. Keep the MSRV CI lane aligned
with the package metadata. Prove compatibility before lowering the declaration,
and never use `--ignore-rust-version` to bypass a compiler requirement.

Report executed checks, results, limitations, and remaining blockers.
A successful build does not prove GTK runtime, audio, or Yandex API integration.
The owner accepted merged master 09f7635 with 17 PASS, 0 FAIL and 1 N/D;
that evidence is a baseline, not acceptance of subsequent v0.3 changes. Distinguish
authored code, executed fixture checks, owner acceptance and blocked functionality.
The owner subsequently accepted v0.3 runtime 02325489d23ead9da4471de9cc35e2c063ac8bab
on 2026-10-31: persistence/restart, real covers in lists/player/pane, audible Music,
pane mechanics, KDE cover/metadata and the reported final Flatpak auth/playback/
system integration. Do not invent unreported backend/deletion/media-key details.
Keep historical FAILs intact. The pane's visual design is not accepted and moves
to a separate UI/UX iteration. Functional acceptance and the explicit PR #14
squash authorization are not public-release or service-distribution permission.
Do not claim a full current Flatpak build or a real wallet/portal test from source
inspection, host-library checks or synthetic D-Bus fixtures.

## Working Environment
- Primary validation: Fedora 44, KDE Plasma, Wayland, PipeWire, x86_64.
- Remain compatible with other supported Linux desktop environments.
- Flatpak is the preferred distribution format.

## Initial Milestone
v0.0 Technical Foundation:
- Research and validate API, authentication, and playback feasibility.
- Establish modular Rust architecture.
- Implement a native GTK4/Libadwaita application foundation.
- Validate GStreamer independently.
- Prepare reproducible build and test workflows.
- Clearly distinguish proven functionality from experimental behavior.
