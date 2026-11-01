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
  Shuffle and LoopStatus are writable views of the local queue's playback
  policy, not separate desktop-player state. Keep Stop available through MPRIS
  and the controller while omitting a visible Stop button from the music bar.
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
  docs/AUDIO_CACHE_REVIEW.md; hide unsupported offline and recommendation actions.
  The latest owner request explicitly permits one honest “Моя волна” landing
  placeholder. Label it unimplemented; do not fake stations, recommendations,
  playback, radio-session progress or endpoint support.

## User Experience
- The final Cassette-inspired refinement starts at `2e3ae41` in Draft PR #15.
  Its latest owner task supersedes the earlier Wave-only landing: primary tabs
  are “Главная”, “Нравится” and “Плейлисты”. Home uses real working shortcuts
  and a compact truthful unavailable Wave section, never invented recommendations.
  Liked has an original native heart collection hero and a reusable loaded-track
  search shared by opened playlists and compatible collection views. Case-fold
  and normalize Unicode; typing must not fetch all remote pages. Label incomplete
  coverage, preserve source occurrences and active playback, and admit any next
  portion only within existing network/view limits. Total duration is shown only
  for complete admitted data with every duration known and a checked sum.
  Filtered Play snapshots matching original occurrences in provider order, at
  most 500; view/query changes never rewrite the playing queue. Preserve the
  normal 240 ms SlideUp, pinned Info/Queue, confirmed Like and exact MPRIS identity.
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
- Inspect actual application screenshots at approximately 420×720, 850×750, 900×650,
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
- ADR-0020 covers the v0.3 track pane, playlist grid and persistent preferences.
  [ADR-0022](docs/adr/0022-universal-music-panel.md) extends ADR-0021's controlled
  nonmodal intent into one universal panel: `Closed`, `TrackInfo` or `Queue`.
  One native surface owns the background, border, header, artwork, text and
  controls throughout opening/closing. Check actual intermediate frames; an
  already-open image or a partially moving cover is insufficient. Mode changes
  replace content in place without repeating the slide. Outside clicks,
  navigation, playback and responsive changes preserve the chosen mode. The
  footer and Ctrl+J open Queue; it is not a primary sidebar destination.
  Explicit Details never plays; a new actual queue occurrence, including
  duplicate IDs, EOS and MPRIS, resumes following in place. Structural queue
  reindexing alone is not a new occurrence. No pane operation creates a second
  playback state or restarts audio. Retain click/Enter/Space playback.
  Mutation UI must wait for server confirmation, never fabricate local success or
  automatically repeat an ambiguous POST. The current Fedora acceptance guide is
  docs/BUILDING.md#v04-native-music-acceptance-on-fedora.
- [ADR-0023](docs/adr/0023-local-playback-order-and-mpris-modes.md) keeps local
  Shuffle and Repeat Off/Queue/One over exact queue occurrences, preserving the
  physical PlaybackContext order and its 500-position/partial boundary. Shuffle
  uses a bounded upcoming plan and actual history; repeat-one affects natural
  EOS, not explicit Next/Previous. Never turn a resolver refusal into an automatic
  retry loop or fetch an entire remote collection to complete a local cycle.
- Current v0.4 work follows the owner's complete music-experience redesign in
  [DESIGN_SPEC.md](docs/design/v0.4/DESIGN_SPEC.md), superseding conflicting older
  mockups: three navigation destinations, real music-led content, one universal
  right panel and functional local playback modes. The current final refinement from
  `2e3ae41` retains the earlier activity-dependent footer and native tabs, with
  “Главная”, “Нравится” and “Плейлисты” plus accessible unified Search. Do not
  use a dropdown as their primary switcher. The unavailable Wave section is a
  compact honest part of Home, not a working radio service.
  The Player Bar is initially hidden with no playback intent, reveals natively
  from below on a real playback request, remains through Pause and retryable
  request failure, and hides after explicit Stop/logout/queue clear. Selection
  alone must not expose an idle player or create playback. The whole current
  artwork/title/artist block is one accessible Track Info button; confirmed Like
  is a separate action, never a nested button. Unconfirmed Dislike stays hidden.
  Track lists load bounded portions continuously rather than through visible
  page switches. Preserve scroll position, exact occurrences, request/session
  epochs, explicit retry and the active context's 500-position/partial boundary.
  UI appends must not silently replace the active playback context. Follow
  [ADR-0024](docs/adr/0024-bounded-continuous-track-views.md): keep bounded source
  position provenance, compare stable feed identity+absolute original occurrence after queue
  edits, and mark manual additions separately instead of matching by Track ID.
  Retain meaningful errors, confirmations, tooltips and accessible labels. Original design SVGs
  are not running GTK screenshots or production artwork. First-map Like reads
  must be deferred beyond the synchronous GTK render/map stack, guarded by weak
  ownership and current session/track generations; never reborrow controller
  state mutably from a widget mapping inside a render. Keep footer membership
  reads separate from detail/catalog lanes and never replay an uncertain write.
  Inspect matched baseline/current GTK captures across required sizes plus owner-wide 1650×930,
  true maximize, themes, scales and
  Closed/TrackInfo/Queue, plus populated pages and input states. Capture motion
  near 0/25/50/75/100 percent with normal animation and verify reduced motion
  separately before claiming quality.
  Current strict motion proof is graphics-initialized: one unconditional real
  normal setup open/close plus six bounded native offscreen renders of the actual
  mapped panel; raw setup quarters are not assessed and no setup PNG/progress
  mutation supplies proof. Only the subsequent unchanged 240 ms native cycle is
  measured. Never call this cold responsiveness PASS or add production prewarming.
  Cold NGL/Vulkan failures on both old/modern stacks remain recorded; Fedora first
  appearance after ordinary fresh start is a separate unaccepted gate for both
  panel and footer. Raw footer frames with no height reversal do not prove smooth
  cold appearance: retain the C9 116,669 μs first-paint gap, the separate
  fresh-process observations in VALIDATION.md and their unknown causes.
  Record test-only Mesa worker settings explicitly. Motion fixtures unset
  `GSK_DEBUG` to use ordinary GTK damage; static resize captures retain
  `full-redraw`. Preserve the separate local full-redraw PASS, hosted `144492f`
  FAILs and one ordinary-damage control. Fresh full render-node snapshots are
  not direct native-framebuffer damage-correctness proof. A controlled `LP_NUM_THREADS=2`
  result does not erase a default-environment sampling failure or prove a driver
  cause, cold responsiveness, a production fix or Fedora performance.
  Work only in `feature/v0.4-native-music-ui` and existing Draft PR #15; leave
  it Draft pending owner Fedora visual acceptance. Do not create another PR,
  merge it or delete the branch without a new instruction.

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

Keep complete native CI coverage: retain the earlier 29 Ubuntu functional
selectors, compiler/package checks and both navigating WebKit fixtures, plus
both strict visual selectors on Debian 13 at both scales/compilers. Add the
refinement's actual-input/continuous/footer/membership regressions to the compiled
registration partition; the historical 31 = 29 + 2 count is not a ceiling or a
reason to omit new cases. The final refinement registers 33 selectors: 30
functional (including collection-filter input) plus three modern visual/raw
observers (including separate fresh-start footer capture). Report the actual
executed set, including the locally blocked navigating WebKit cases.
The recorded `709f5b3` GTK 4.14/Adwaita 1.5/Mesa strict-motion FAIL remains
scoped evidence, not a reason to drop tests or weaken timing/progress checks.
Modern test placement does not change production minimums, renderer or SDK.

Report executed checks, results, limitations, and remaining blockers.
A successful build does not prove GTK runtime, audio, or Yandex API integration.
The owner accepted merged master 09f7635 with 17 PASS, 0 FAIL and 1 N/D;
that evidence is a baseline, not acceptance of subsequent v0.3 changes. Distinguish
authored code, executed fixture checks, owner acceptance and blocked functionality.
The owner subsequently accepted v0.3 runtime 02325489d23ead9da4471de9cc35e2c063ac8bab
on 2026-10-31: persistence/restart, real covers in lists/player/pane, audible Music,
pane mechanics, KDE cover/metadata and the reported final Flatpak auth/playback/
system integration. Do not invent unreported backend/deletion/media-key details.
Keep historical FAILs intact. The pane's v0.3 visual design was not accepted;
v0.4 is its separately authorized UI/UX iteration, with new visual acceptance
still required. PR #14 integrated the accepted baseline at `5e85e8d`. Functional
acceptance and the explicit PR #14 squash authorization are not v0.4 merge,
public-release or service-distribution permission.
The first v0.4 implementation at `a9eed4514802df84d73b0608dd1a62930fc1af76`
passed its recorded cloud checks but failed owner Fedora UI/UX acceptance. Keep
that historical result. The owner's complete redesign supersedes the previous
visual target and requires fresh matched evidence, motion checks, real playback
modes and a new Fedora review; earlier green CI is not acceptance of this work.
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
