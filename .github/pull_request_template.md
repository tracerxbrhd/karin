<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

## Change

Describe the concrete behavior and why it is needed.

## Verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] `cargo build --workspace --locked`
- [ ] Pinned Rust 1.98.1: formatting, Clippy without warnings, all tests and build
- [ ] Rust 1.98.1 `cargo build --release --offline --locked` after fetch/vendor
- [ ] GTK smoke test and normal window/resource cleanup
- [ ] Synthetic GStreamer diagnostic through `fakesink`
- [ ] Generated local WAV through `--playback-self-test`, including transport and worker cleanup
- [ ] GitHub Actions result inspected for this revision
- [ ] Default Cassette-compatible session provider, minimal no-WebKit and `experimental-device-auth` configurations checked separately
- [ ] Native Music HTTP-fixture test explicitly executed under Xvfb
- [ ] Track search parses only validated identities, then one bounded batch; order, pagination, missing data and no N+1 verified
- [ ] Search Retry, explicit Play/details/back and queue actions exercised with real mouse/keyboard input
- [ ] Account-menu logout requires native confirmation; Cancel/Escape/default response preserve the session and audio
- [ ] Concurrent search/library/detail actions, latest pending query and logout/stale-result isolation checked under Xvfb
- [ ] Delivery-probe changes tested with bounded local fixtures; no credentials, resource URLs, provider payloads or media in reports/artifacts
- [ ] `auth-webkit` checked on stable/MSRV with one GTK dependency family; startup/cancel and sandbox-enabled local navigation/session-cookie results recorded separately
- [ ] Default/Flatpak enables `cassette-auth` and excludes legacy Device Flow; exact reused application identity, source attribution and public-distribution uncertainty documented separately
- [ ] Documentation distinguishes passed, failed, blocked and unrun checks

## v0.3 persistence and cache (when changed)

- [ ] One v0.3 branch and one Draft PR to `master`; accepted v0.2 owner evidence is distinguished from new checks
- [ ] Saved tokens are used/saved only after current Music validation; native secure-store and Flatpak portal paths have explicit availability/failure states and no plaintext fallback
- [ ] Restore, invalid token, network failure, locked/corrupt/unavailable store, cancellation and late operations tested with synthetic fixtures
- [ ] Remember-off preserves the live session when intended, confirms saved-credential deletion and persists the forget barrier before async cleanup
- [ ] Logout cancels old work, stops audio, purges personal cache and verifies saved-credential absence; unknown deletion/clear is not reported as final success
- [ ] Settings corruption/unavailability cannot silently bypass unresolved forget; theme/cache/pane choices survive restart and obsolete saves cannot overwrite newer choices
- [ ] Account-scoped metadata/cover cache hit/miss/TTL/corruption/byte bounds/eviction tested; no token, cookie, signed media URL or audio stored
- [ ] Mutable reads remain network-first; explicit Refresh/membership/write confirmation bypass cache; only Network/Timeout can use visibly saved/stale fallback
- [ ] Disable stops persistent content reads/writes without implicit deletion; usage and clear show actual results/errors
- [ ] Clear/account switch/logout races reject old leases; ordinary cache clearing preserves credentials, active audio/context and live MPRIS cover
- [ ] Artwork persistence respects explicit HTTP restrictions and shorter source freshness; graphical material is not relicensed as Karin assets
- [ ] Native D-Bus secure-storage fixture runs only in its isolated guarded session bus; ordinary automation never restores a personal token/cache or unlocks a real wallet
- [ ] Native KWallet/GNOME and current Flatpak Secret Portal/backend tests are separately executed or explicitly pending/unavailable
- [ ] Persistent-audio permission/offline entitlement review recorded; no hidden media downloads or inert Offline switch

## UI quality (required for substantial UI changes)

Follow `docs/UI_QUALITY_GATE.md`. If there is no UI change,
state that explicitly; do not mark unexecuted visual checks as passed.

- [ ] Real screenshots inspected at about 420×720, 850×750, 900×650 and 1440×900
- [ ] Actual maximized window inspected, or the environment limitation recorded
- [ ] Light and dark themes covered for overview and diagnostics
- [ ] Resize transitions, selected-page preservation and adaptive navigation checked
- [ ] Header/title/window controls, typography, spacing and text clipping checked
- [ ] Signed-in search/list density measured at compact/medium/wide sizes; visible content and action clarity reviewed
- [ ] Keyboard navigation, visible focus and activation checked
- [ ] GTK structure/allocation/navigation checks accompany screenshots
- [ ] Adaptive track pane checked at wide and compact sizes, including selected/playing distinction, source occurrence, verified navigation and open/close without audio changes
- [ ] Playlist tile layout/long titles/missing covers/loading/error/keyboard activation checked; Create uses one write plus server confirmation and never a fake local entry
- [ ] Persistent preferences, restore/cleanup errors, cache enable/limit/usage/clear and asynchronous updates inspected in both themes/scales
- [ ] Current/next/unavailable queue markers and actual bottom scroll reachability reviewed without changing the accepted player composition
- [ ] Unconfirmed Dislike/similar tracks/offline audio stay hidden; the owner-requested Wave landing is explicitly unimplemented, without fake playback/recommendations
- [ ] Single native sign-in dialog checked at all sizes/themes; no developer credentials/code/token fields, stale signed-out errors or completion before Music account validation
- [ ] Only clean cloud screenshots shared; no private user/desktop captures added
- [ ] Pending Fedora/KDE/Wayland visual acceptance explicitly listed
- [ ] No fictitious release catalog/recommendations or implied unimplemented playback; unavailable controls explain the blocker

## Native Music v0.4 (when changed)

- [ ] Existing Draft PR #15 and its branch retained; no automatic merge
- [ ] Current task/references guide independent composition; 108 matched actual GTK before/after size/theme/scale/panel cases reviewed with source hashes and true maximize
- [ ] Home/Liked/Playlists native tabs and accessible unified Search replace primary dropdowns/rail
- [ ] Idle footer allocates no strip; actual Play reveals it from below, Pause retains it, Stop/clear/logout hide it; normal raw first-start/Stop frames and reduced motion checked separately
- [ ] Whole cover/title/artist/metadata area opens Track Info with mouse/Enter/Space; direct Like is separate, server-confirmed and never repeats an uncertain POST
- [ ] Continuous track batches preserve rows/scroll/occurrences, show loading/end/partial/error/Retry, reject retired work and retain 2000-position/100-page UI and 500-position queue bounds
- [ ] Liked heart hero shows truthful loaded/complete count and complete-only checked duration; inspect actual density, not just clipping
- [ ] Collection-local Unicode/title/artist/album search works in Liked and opened own/liked/search playlists; typing performs no collection-metadata requests and partial coverage/continuation/Retry stay honest
- [ ] Filter/clear/append/reset preserves active Queue/worker; explicit Play retains visible order, absolute duplicate provenance and 500-position windows without N+1
- [ ] Metadata-adjacent confirmed heart, centered five-button transport/timeline and Queue/Volume fit actual compact/850/wide layouts; playlist cover sizes and single/create tiles verified
- [ ] First-footer raw paint observation runs separately without pre-capture/resize/graphics initialization; historical 117 ms gap and pending Fedora cold acceptance retained
- [ ] Independent playlist-section refresh/continuation and Like/Liked reads do not strand loading; logout removes collection artwork/count/actions as well as rows
- [ ] Actual normal panel phase/geometry evidence records renderer, software-Mesa worker setting and graphics initialization; failed observations and cold first-use limits remain explicit
- [ ] Pinned information survives outside clicks, scroll, navigation and resize; X/Ctrl+I/compact Back are explicit closes
- [ ] Wide and medium reserve space; real GTK hit testing proves catalog Play/More remain reachable with information open
- [ ] Actual current occurrence, duplicate positions, Next/Previous/EOS/MPRIS update information in place; explicit Details survives Pause/Seek/Volume
- [ ] Pane toggles preserve worker/resolver identity; stale art/detail replies and logout/close cleanup tested
- [ ] Closed-pane whole-window pixels, compact focus, normal/reduced motion, 64px row density and visible footer errors/Retry checked at both scales
- [ ] Source-matched Fedora artifact and short v0.4 guide supplied; new owner visual/audio/KDE/Flatpak acceptance stays separate

## Licensing and scope

- [ ] New original sources carry `SPDX-License-Identifier: GPL-3.0-or-later`
- [ ] Third-party licenses/notices and dependency distribution obligations preserved
- [ ] GPL is not described as permission to access Yandex Music or its content
- [ ] No user credentials, session cookies, tokens, account data or copyrighted music added; published application constants are distinguished from user secrets
- [ ] Music authorization/stream claims have permission and scoped evidence; unavailable capabilities remain explicit
- [ ] OAuth changes use local fixtures and hidden-input checks; no real account/credentials in Cloud or CI
- [ ] Registration, token issuance, ID-client binding and Music access are reported separately; manual full-URL state validation is distinguished from an automatic production callback
- [ ] Separate own-client OAuth diagnostic credentials/tokens remain memory-only; ordinary protected-session logout/cancel/late responses and cache isolation are tested
- [ ] Login, current-dialog session, OAuth exchange, Music account acceptance and library/search are reported separately with safe concrete errors; third-party application identity/distribution status is not represented as established permission or RFC 8252 compliance

## Evidence and remaining manual checks

Link screenshots/results and list the checks still needed on Fedora. Leave
unperformed manual acceptance pending. Do not merge without the required review.

For changed music workflows, report actual track/album/artist search and personal
playlist contents separately from account/list acceptance. The v0.3 owner runtime at `0232548` is accepted and integrated at `5e85e8d`;
new v0.4 visual and functional regression acceptance remains separate. Audible service
playback is mandatory:
only an entitled owner hearing a real selected track through PipeWire/GStreamer
passes that criterion. A metadata probe, synthetic WAV or player screenshot does
not. Record the exact delivery stage if blocked; do not silently defer the gate.

For v0.3, use the single Fedora guide for native and Flatpak restore/forget,
settings/cache, pane/tiles and real playback/KDE regression. Report backend kind
and fixed availability/error categories only; no credential values, portal keys,
real cache records, media metadata dumps or personal screenshots. A source graph,
portal version or loopback fixture does not pass real system storage. Leave
unperformed media-key, wallet/portal and package checks pending/N/D.
