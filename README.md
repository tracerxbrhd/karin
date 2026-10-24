# Karin

An independent native Linux client for Yandex Music, built with Rust, GTK4,
Libadwaita and GStreamer. **v0.0 is a technical foundation, not a working music
player.** Karin is not affiliated with Yandex.

## What works

- Native window with overview, connection states and a diagnostics section.
- An explicit, anonymous availability request through a separate API adapter.
  An HTTP response does not establish login, subscription or playback support.
- Independent synthetic audio diagnostics, silently through `fakesink` or through
  a system output selected by GStreamer. The UI labels this as a test signal.
- D-Bus and Secret Service presence inspection, without reading credentials.
- Offline tests, headless GTK startup checks and a GitHub Actions workflow.

No tokens or passwords are requested, stored or logged. Authentication, library,
search UI, My Wave, streaming playback and MPRIS publication are not implemented.
The current cloud probe reports anonymous status with service availability false;
search/metadata probes returned HTTP 451. See [API feasibility](docs/API_FEASIBILITY.md).

## Build and run

Install stable Rust and the native development packages described in
[BUILDING.md](docs/BUILDING.md), then:

```sh
cargo build --workspace --locked
cargo run --locked
```

The primary manual acceptance platform is Fedora 44 / KDE Plasma / Wayland /
PipeWire. Cloud validation uses Debian 13 / Xvfb. In the prepared Codex Cloud
environment, prefix native build commands with `/workspace/karin-environment/run`.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
./target/debug/karin --audio-self-test
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- xvfb-run -a ./target/debug/karin --smoke-test
```

`--audio-self-test` is silent and requires no sound device. `--smoke-test` checks
the real window and navigation without contacting the API. The separate optional
`--probe-service` command makes a real anonymous network request and prints only
the resulting domain report. Network access is never required by automated tests.

## Project documentation

- [Architecture and concurrency](docs/ARCHITECTURE.md)
- [API feasibility and evidence](docs/API_FEASIBILITY.md)
- [Fedora, cloud and Flatpak build instructions](docs/BUILDING.md)
- [Validation and acceptance](docs/VALIDATION.md)
- [Known limitations](docs/LIMITATIONS.md)
- [Roadmap](docs/ROADMAP.md) and [ADRs](docs/adr/)

The initial Flatpak manifest and offline Cargo source preparation are in
`packaging/`. Full sandboxed Flatpak packaging remains unverified in this cloud
container because Bubblewrap namespace creation is denied.

## License

To be determined before the first public release. No third-party branding or
copyrighted audio is distributed by this repository.
