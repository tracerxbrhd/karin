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
- [ ] Public and `experimental-device-auth` configurations checked separately
- [ ] Native Music HTTP-fixture test explicitly executed under Xvfb
- [ ] Default/Flatpak excludes experimental authentication and bundled identities
- [ ] Documentation distinguishes passed, failed, blocked and unrun checks

## UI quality (required for substantial UI changes)

Follow `docs/UI_QUALITY_GATE.md`. If there is no UI change,
state that explicitly; do not mark unexecuted visual checks as passed.

- [ ] Real screenshots inspected at about 420×720, 900×650 and 1440×900
- [ ] Actual maximized window inspected, or the environment limitation recorded
- [ ] Light and dark themes covered for overview and diagnostics
- [ ] Resize transitions, selected-page preservation and adaptive navigation checked
- [ ] Header/title/window controls, typography, spacing and text clipping checked
- [ ] Keyboard navigation, visible focus and activation checked
- [ ] GTK structure/allocation/navigation checks accompany screenshots
- [ ] Only clean cloud screenshots shared; no private user/desktop captures added
- [ ] Pending Fedora/KDE/Wayland visual acceptance explicitly listed
- [ ] No fictitious release catalog/recommendations or implied unimplemented playback; unavailable controls explain the blocker

## Licensing and scope

- [ ] New original sources carry `SPDX-License-Identifier: GPL-3.0-or-later`
- [ ] Third-party licenses/notices and dependency distribution obligations preserved
- [ ] GPL is not described as permission to access Yandex Music or its content
- [ ] No credentials, account data or copyrighted music added
- [ ] Music authorization/stream claims have permission and scoped evidence; unavailable capabilities remain explicit
- [ ] OAuth changes use local fixtures and hidden-input checks; no real account/credentials in Cloud or CI
- [ ] Registration, token issuance, ID-client binding and Music access are reported separately; manual full-URL state validation is distinguished from an automatic production callback
- [ ] Experimental application credentials/tokens are memory-only; logout/cancel/late responses and new-session cache isolation tested

## Evidence and remaining manual checks

Link screenshots/results and list the checks still needed on Fedora. Leave
unperformed manual acceptance pending. Do not merge without the required review.
