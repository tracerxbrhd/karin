<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

## Change

Describe the concrete behavior and why it is needed.

## Verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] `cargo build --workspace --locked`
- [ ] GTK smoke test and normal window/resource cleanup
- [ ] Synthetic GStreamer diagnostic through `fakesink`
- [ ] GitHub Actions result inspected for this revision
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
- [ ] No fictitious catalog, recommendations or non-working player controls added

## Licensing and scope

- [ ] New original sources carry `SPDX-License-Identifier: GPL-3.0-or-later`
- [ ] Third-party licenses/notices and dependency distribution obligations preserved
- [ ] GPL is not described as permission to access Yandex Music or its content
- [ ] No credentials, account data or copyrighted music added

## Evidence and remaining manual checks

Link screenshots/results and list the checks still needed on Fedora. Leave
unperformed manual acceptance pending. Do not merge without the required review.
