<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Building and running Karin

Karin is a single Rust package. Its minimum Rust series is 1.98
(`rust-version = "1.98"`); compatibility checks pin Rust 1.98.1, including the
Flatpak release profile. Current stable Rust remains the development default.
Native baselines are GTK >= 4.14, Libadwaita >= 1.5 and GStreamer >= 1.20.
`Cargo.lock` fixes Rust dependency versions. The primary manual acceptance target
is Fedora 44, KDE Plasma, Wayland and PipeWire; cloud checks use Debian 13.

## Fedora 44

Install native prerequisites:

```sh
sudo dnf install gcc gcc-c++ make pkgconf-pkg-config \
  gtk4-devel libadwaita-devel gstreamer1-devel gstreamer1-plugins-base-devel \
  gstreamer1 gstreamer1-plugins-base gstreamer1-plugins-good \
  openssl-devel dbus-devel dbus-tools libsecret-devel \
  blueprint-compiler flatpak flatpak-builder python3 \
  xorg-x11-server-Xvfb xorg-x11-xauth
```

Install Rust through the official [rustup instructions](https://rustup.rs/),
then select the required components. Do not disable TLS or checksum verification.

```sh
rustup toolchain install stable --profile minimal \
  --component rustfmt --component clippy --component rust-analyzer --component rust-src
rustup default stable
pkg-config --modversion gtk4 libadwaita-1 gstreamer-1.0 dbus-1
cargo build --workspace --locked
cargo run --locked
```

No Yandex account, token, OAuth client secret or subscription is required to build
and run the technical foundation. The explicit connection probe is anonymous.
Authentication and playback of Yandex Music streams are not implemented.

## Codex Cloud / Debian 13

The prepared cloud environment retains its setup assets outside the repository
at `/workspace/karin-environment`. The Rust toolchain lives in its `cargo/` and
`rustup/` directories; native dependencies live in a Docker image. Use the wrapper
so Cargo sees GTK and GStreamer:

```sh
/workspace/karin-environment/run cargo build --workspace --locked
/workspace/karin-environment/run cargo test --workspace --locked
/workspace/karin-environment/run sh
```

The wrapper's working directory is `/workspace/karin`. It mounts the current CA
bundle read-only and uses the platform proxy; neither verification nor the proxy
should be bypassed. The agent must follow the cloud environment startup
instructions if the Docker image needs restoring. Cloud tasks are already
isolated; use the existing checkout instead of creating another worktree unless
explicitly requested.

For the pinned compatibility toolchain, install it alongside stable and select it
per command; the wrapper does not change the default toolchain:

```sh
/workspace/karin-environment/run rustup toolchain install 1.98.1 --profile minimal \
  --component rustfmt --component clippy
/workspace/karin-environment/run cargo +1.98.1 build --workspace --locked
/workspace/karin-environment/run env RUSTUP_TOOLCHAIN=1.98.1 \
  python3 packaging/flatpak/prepare-source.py
```

`RUSTUP_TOOLCHAIN` also selects the Cargo subprocesses used by source preparation
and the dependency-license inventory.

## Verification

Run from the repository root (through the wrapper in the prepared cloud):

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
./target/debug/karin --audio-self-test
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a ./target/debug/karin --smoke-test
```

The audio self-test uses a synthetic signal and `fakesink`, not music or physical
audio output. The GTK smoke mode starts the actual application and exits after
checking its window lifecycle. Automated tests must not depend on the public
Yandex API or user credentials. CI runs these checks on Ubuntu 24.04 with its
native development libraries; a CI definition alone is not evidence that a
remote GitHub Actions run passed.

CI runs both current stable and the pinned minimum-version toolchain. To reproduce
the Rust 1.98.1 lane locally, install its Rustfmt and Clippy components, then run:

```sh
cargo +1.98.1 fmt --all -- --check
cargo +1.98.1 clippy --workspace --all-targets --locked -- -D warnings
cargo +1.98.1 test --workspace --locked
cargo +1.98.1 build --workspace --locked
cargo +1.98.1 fetch --locked
cargo +1.98.1 build --release --offline --locked
./target/release/karin --audio-self-test
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' ./target/release/karin --smoke-test
```

The release command matches the Cargo command in the Flatpak manifest. Fetching
populates the local Cargo cache first; a generated Flatpak source snapshot instead
uses its vendored sources. Neither check is a full SDK/sandbox package build.
Keep `Cargo.toml`'s minimum version and the pinned CI lane aligned, and verify code
and locked dependencies before raising that minimum. Never bypass Cargo's
version check with `--ignore-rust-version`.

The user has confirmed initial startup, anonymous API HTTP 200, audible test
output, D-Bus and Secret Service on Fedora 44/KDE/Wayland/PipeWire. The original
visual layout failed acceptance. Repeat visual acceptance of the revised layout
must check window/title controls, resize/maximize behavior, HiDPI, keyboard/focus,
accessibility and both themes under [the UI quality gate](UI_QUALITY_GATE.md).
A headless check does not establish those properties on Fedora.

For a substantial UI change, also run the screenshot matrix and review it under
[UI_QUALITY_GATE.md](UI_QUALITY_GATE.md). With the capture tools installed:

```sh
scripts/capture-ui.sh build/ui-captures
```

The prepared cloud visual image provides these tools through
`/workspace/karin-environment/run-visual`. The script uses its own Xvfb/Openbox
session and captures Karin only; it must not be pointed at a personal desktop.
Inspection of the images remains required even when the script exits successfully.

## Flatpak foundation

The initial manifest is
[`packaging/flatpak/io.github.tracerxbrhd.Karin.json`](../packaging/flatpak/io.github.tracerxbrhd.Karin.json).
It targets GNOME 50 and the matching Freedesktop 25.08 Rust SDK extension. This
keeps the GTK/Libadwaita runtime independent of the user's desktop environment.
It has no host filesystem permission or Secret Service access grant. Network and
audio permissions support only the currently explicit diagnostics.

During Fedora acceptance, the GNOME 50 SDK with
`org.freedesktop.Sdk.Extension.rust-stable//25.08` reported Rust 1.98.1. The earlier
Karin requirement of 1.99 rejected that compiler before compilation. The minimum
is now the 1.98 series, with exact 1.98.1 covered by CI; see the executed results in
[VALIDATION.md](VALIDATION.md). Updating the Runtime or native library baselines
is not needed for this metadata mismatch.

Install Flatpak Builder, the SDK/runtime and the Rust extension from the trusted
Flathub remote configured on your machine:

```sh
flatpak install --user flathub org.gnome.Platform//50 org.gnome.Sdk//50 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08
python3 packaging/flatpak/prepare-source.py
flatpak-builder --user --install --force-clean build/flatpak \
  packaging/flatpak/io.github.tracerxbrhd.Karin.json
flatpak run io.github.tracerxbrhd.Karin
```

The preparation script copies the working tree into ignored `build/flatpak-source`
and replaces that generated directory on each run. It excludes Git data, local
environment files and build outputs. It vendors only the versions fixed by
`Cargo.lock`; review the source snapshot before distribution. Cargo's generated
source configuration exists only in that copy. The manifest builds with
`--offline --locked`, with no dependency downloads inside the Flatpak build.
The preparation also runs `packaging/license_inventory.py` in the generated
snapshot and preserves supplied third-party license/notice files in
`third-party-notices/`. The manifest installs these notices, Karin's `LICENSE`
and `docs/LICENSING.md` under `/app/share/licenses/karin`. Review the inventory
and component obligations before distribution; generated notices are not a
complete review of the native runtime or final package.
The SDK's Rust must meet `Cargo.toml`'s minimum version. If a future SDK/compiler
combination falls below it, inspect the SDK's actual `rustc --version` and the
supported extension before changing the minimum or Runtime. Do not suppress the
version check. Karin's own code and original materials
use GPL-3.0-or-later. A distributable package must include the GPL text, preserve
third-party notices and meet corresponding-source and component-specific license
obligations; see [LICENSING.md](LICENSING.md). Source preparation and a successful
build alone do not establish complete distribution compliance or release
acceptance. GPL does not authorize Yandex Music API or content access.

Full Flatpak builds are **not verified in the cloud environment**: Bubblewrap
cannot create the required user namespace inside the nested container. Keep
sandboxing enabled and perform packaging acceptance on Fedora or suitable CI.

GNOME 50's Freedesktop SDK base is documented in the upstream
[gnome-build-meta junction](https://gitlab.gnome.org/GNOME/gnome-build-meta/-/blob/gnome-50/elements/freedesktop-sdk.bst).
