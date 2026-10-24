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

## User Experience
- Cassette by Rirusha is an aesthetic reference, not a source to copy.
- Follow GNOME HIG and Libadwaita conventions where suitable.
- Support light/dark themes, HiDPI, keyboard navigation, and accessibility.
- Design loading, empty, offline, unauthorized, and error states.
- Prioritize responsiveness and clarity over decorative effects.

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
