<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin

An independent native Linux client for Yandex Music, built with Rust, GTK4,
Libadwaita and GStreamer. **Gate B1 adds an opt-in local OAuth experiment;
Yandex Music authorization and playback remain unverified.** Karin is not
affiliated with Yandex.

## What works

- Native window with overview, connection states and a diagnostics section.
- An explicit, anonymous availability request through a separate API adapter.
  An HTTP response does not establish login, subscription or playback support.
- Independent synthetic audio diagnostics, silently through `fakesink` or through
  a system output selected by GStreamer. The UI labels this as a test signal.
- A reusable GStreamer `playbin` controller with prepare, play, pause/resume,
  stop, volume, supported seeking, EOS and error handling. The developer playback
  section exercises it with a generated local WAV, separately from service access.
- D-Bus and Secret Service presence inspection, without reading credentials.
- A separate `--oauth-diagnostic` for locally testing Karin's own OAuth client
  with authorization code + S256 PKCE, in-memory credentials and read-only probes.
  It is not the application's sign-in feature and has not been accepted on Fedora.
- Offline tests, headless GTK startup checks and a GitHub Actions workflow.

Normal application operation requests no credentials. The opt-in diagnostic
accepts a locally entered Client ID and full browser redirect URL through hidden
terminal prompts, validates returned `state` and extracts the opaque one-time
authorization code; it never requests a password, Client secret or pasted OAuth
token, and never persists
credentials. Yandex Music sign-in, library, search UI, My Wave, service stream
resolution and MPRIS publication are not implemented. The owner confirmed a
Karin-owned user-authorization registration with two Music scopes on 2026-10-26;
this supersedes the earlier registration uncertainty. The owner subsequently
observed Karin consent and code/state return on Fedora, but the initial tool
rejected the code before exchange. Token issuance and Music API access remain
unverified. The application keeps its unavailable capability.
See [API feasibility](docs/API_FEASIBILITY.md), the
[Gate B1 decision](docs/adr/0008-local-oauth-pkce-diagnostic.md) and
[local Fedora instructions](docs/BUILDING.md#local-oauth-diagnostic-gate-b1).

The developer controls operate on temporary, quiet synthetic media, not a song,
playlist or Yandex Music stream. Silent output is the default; physical output
requires an explicit choice. This validates a player engine rather than the full
v0.1 music-client experience.

## Build and run

Install stable Rust (minimum supported series: 1.98; compatibility checks use
1.98.1) and the native development packages described in
[BUILDING.md](docs/BUILDING.md), then:

```sh
cargo build --workspace --locked
cargo run --locked
```

The primary manual acceptance platform is Fedora 44 / KDE Plasma / Wayland /
PipeWire. [Issue #3](https://github.com/tracerxbrhd/karin/issues/3) reports that
the v0.0 native and Flatpak prototypes launch and diagnostic audio works there.
Gate A's playback controls and revised layout require fresh manual acceptance;
startup alone is not visual signoff. Cloud validation uses Debian 13 / Xvfb.
In the prepared Codex Cloud environment, prefix native build commands with
`/workspace/karin-environment/run`.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
./target/debug/karin --audio-self-test
./target/debug/karin --playback-self-test
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- xvfb-run -a ./target/debug/karin --smoke-test
```

CI also checks Rust 1.98.1, including an offline release build matching Flatpak's
Cargo command. See [BUILDING.md](docs/BUILDING.md) for the pinned-toolchain commands.

Both audio self-tests are silent and require no sound device.
`--playback-self-test` decodes generated local media through the real controller;
it is separate from the finite pipeline check in `--audio-self-test`.
`--smoke-test` checks the real window and navigation without contacting the API.
The separate optional `--probe-service` command makes a real anonymous network
request and prints only the resulting domain report. Network access is never
required by automated tests. Executed results and pending manual checks are in
[VALIDATION.md](docs/VALIDATION.md).

## Project documentation

- [Architecture and concurrency](docs/ARCHITECTURE.md)
- [API feasibility and evidence](docs/API_FEASIBILITY.md)
- [Fedora, cloud and Flatpak build instructions](docs/BUILDING.md)
- [Validation and acceptance](docs/VALIDATION.md)
- [UI quality gate](docs/UI_QUALITY_GATE.md)
- [Inspected cloud UI screenshots](docs/ui/README.md)
- [Known limitations](docs/LIMITATIONS.md)
- [License scope and dependency review](docs/LICENSING.md)
- [Roadmap](docs/ROADMAP.md) and [ADRs](docs/adr/)

The Flatpak manifest and offline Cargo source preparation are in `packaging/`.
GNOME 50 and the Freedesktop 25.08 Rust extension are retained, with no additional
sandbox permissions for Gate A. Full sandboxed builds remain unavailable in this
cloud container because Bubblewrap namespace creation is denied; the new revision
must be packaged and accepted on Fedora independently of the reported v0.0 result.

## License

Karin's own code and original materials are licensed under
**GNU GPL-3.0-or-later**, unless a file states otherwise. See [LICENSE](LICENSE)
for the standard GPLv3 text and [the licensing decision](docs/adr/0005-licensing.md)
for scope and distribution considerations. Third-party components retain their
own licenses and notices.

GPL does not grant permission to use Yandex Music, its API, trademarks or content,
and does not replace the service's terms. No copyrighted audio is distributed by
this repository.
