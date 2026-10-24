# v0.0 foundation validation

Executed on 2026-10-24 in the prepared Codex Cloud environment using
`/workspace/karin-environment/run` from the repository root.

## Environment

Debian 13 x86_64; Rust/Cargo 1.99.1 stable; GTK 4.18.6; Libadwaita 1.7.6;
GStreamer 1.26.2; GCC 14.2.0; pkg-config 1.8.1. Development/test profiles omit
debug information to reduce the cost of compiling gtk-rs in cloud runners.
Release builds use Cargo's normal release profile.

## Executed checks

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed, no warnings |
| `cargo test --workspace --locked` | **23 passed, 0 failed, 0 ignored**; binary and documentation targets contain 0 tests |
| `cargo build --workspace --locked` | Passed |
| `./target/debug/karin --audio-self-test` | Passed: 40 synthetic buffers through `fakesink`, EOS observed, pipeline reset to `Null` |
| `GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- xvfb-run -a ./target/debug/karin --smoke-test` | Passed: real window realized, diagnostics navigation selected, window/key widgets released after close |
| `./target/debug/karin --probe-service` | Real anonymous HTTP 200, `Anonymous`, `service_available: Some(false)`; this is not login or playback |
| `python3 packaging/flatpak/prepare-source.py` | Passed: locked Cargo source snapshot prepared outside tracked source in ignored `build/flatpak-source` |
| `flatpak-builder --show-manifest packaging/flatpak/io.github.tracerxbrhd.Karin.json` | Passed: manifest parsed; no package build implied |
| Bubblewrap namespace capability check | Blocked: kernel/container denied namespace creation; no sandbox bypass attempted |

Tests cover domain transitions and retry/duplicate behavior, application-service
boundaries, actual local HTTP responses (401/403/451/503, redirects, malformed and
oversized bodies, disconnect, timeout and a slow-drip total deadline), UI state
presentation, application identifiers, and GStreamer EOS/error/timeout cleanup.
HTTP fixtures are test-only and use loopback; no real API, account credentials or
sound device is required by the automated tests.

The GTK smoke check explicitly observes weak references after closing the real
window, guarding against permanent widget reference cycles in signal handlers.
It does not replace target-desktop accessibility or rendering acceptance.

## Acceptance mapping

| Criterion | Evidence |
| --- | --- |
| Real Rust project and modular boundaries | Single Cargo package, committed lockfile, modules described in [architecture](ARCHITECTURE.md) |
| Native GTK4/Libadwaita shell builds and runs | Successful build and Xvfb smoke check |
| Isolated API adapter and real vertical slice | UI action → `ConnectionService` → `MusicApi`/`YandexApi` → `ConnectionReport` → UI; same service exercised by the live CLI probe |
| Independent audio scenario | UI worker action and CLI self-test use the same finite GStreamer diagnostic |
| Loading, unauthorized/unavailable and error states | Typed state transitions and presentation tests; UI permits retry |
| Honest service capabilities | [Feasibility matrix](API_FEASIBILITY.md) distinguishes observation, documentation and blockers |
| Reproducible dependency/build workflow | Cargo.lock, `--locked` commands, CI definition and vendored offline Flatpak source preparation |
| CI | GitHub Actions workflow provided; hosted-runner outcome must be read from the PR checks, not inferred from local results |

## Unrun/manual acceptance

Full Flatpak package build/install, Fedora 44/KDE/Wayland, physical PipeWire
output, HiDPI and accessibility acceptance remain unverified. No OAuth flow,
subscribed account, music stream, personal library, My Wave, Secret Service
credential lifecycle or MPRIS playback integration was exercised. See
[limitations](LIMITATIONS.md) and [build instructions](BUILDING.md).
