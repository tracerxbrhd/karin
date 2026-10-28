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
- Issue #7's Device Flow is a separate development-only experiment. Require both
  `experimental-device-auth` and `--experimental-music`; keep it out of public
  default/Flatpak builds. Never bundle official/borrowed OAuth credentials or
  add official-client headers. Application credentials are entered locally in
  memory only, not through files, environment, command arguments or Cloud/CI.
- Session and account/catalog acceptance are independent. Never turn metadata
  into playback entitlement. The inspected protected/signing delivery code is
  not an approved source; keep music playback unavailable until separately
  established. No raw provider messages, personal models or credentials in logs.
- Logout/close clears local data and invalidates stale UI work. No plaintext
  credential store, persistent token or refresh/revocation claim before a
  verified Secret Service lifecycle. Keep PKCE and developer audio independent.

## User Experience
- Cassette by Rirusha is an aesthetic reference, not a source to copy.
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

Also check `--all-features` Clippy/tests and build the opted-in experimental
configuration, then restore `--no-default-features` before public artifacts and
captures. Check feature/runtime gating and the public Flatpak policy; neither
real account access nor provider credentials belongs in automated checks.

Preserve the minimum Rust series declared in `Cargo.toml` (1.98). Run the pinned
Rust 1.98.1 CI checks as well as stable when changing Rust code, dependencies or
packaging; include Clippy, all tests and `cargo build --release --offline --locked`
after fetching or vendoring the locked dependencies. Keep the MSRV CI lane aligned
with the package metadata. Prove compatibility before lowering the declaration,
and never use `--ignore-rust-version` to bypass a compiler requirement.

Report executed checks, results, limitations, and remaining blockers.
A successful build does not prove GTK runtime, audio, or Yandex API integration.

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
