<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Building and running Karin

Run commands from the repository root. Karin requires Rust 1.98 or newer,
GTK4 4.14+, Libadwaita 1.5+, GStreamer and WebKitGTK 6.0. Development and CI
use current stable Rust and the pinned compatibility compiler **1.98.1**.
`Cargo.lock` fixes the Rust dependencies. The reference desktop is Fedora 44
with KDE Plasma, Wayland and PipeWire.

## Fedora prerequisites

```sh
sudo dnf install git gcc gcc-c++ make pkgconf-pkg-config python3 \
  gtk4-devel libadwaita-devel webkitgtk6.0-devel \
  gstreamer1-devel gstreamer1-plugins-base-devel \
  gstreamer1 gstreamer1-plugins-base gstreamer1-plugins-good \
  openssl-devel dbus-devel dbus-tools libsecret-devel

pkg-config --modversion gtk4 libadwaita-1 gstreamer-1.0 webkitgtk-6.0 dbus-1
gst-inspect-1.0 souphttpsrc
gst-inspect-1.0 mpg123audiodec
```

The GStreamer checks verify the HTTPS source and MP3 decoder used by playback.
Synthetic MP3 tests also need `lamemp3enc` and `xingmux`, supplied by the normal
GStreamer plugins. Blueprint Compiler is optional for development; the current
UI is constructed in Rust.

Install Rust using the official [rustup instructions](https://rustup.rs/), then:

```sh
rustup toolchain install stable --profile minimal \
  --component rustfmt --component clippy --component rust-analyzer
rustup toolchain install 1.98.1 --profile minimal \
  --component rustfmt --component clippy
```

## Native build and launch

```sh
cargo +stable build --release --locked
./target/release/karin
```

Builds and automated tests need no Yandex account. To use Music, run Karin as
your normal desktop user, click **Войти** and complete the Yandex page. Normal
sign-in needs no Client ID, client secret, pasted token or developer flag.
Music account validation precedes library access. Keep the desktop keyring
unlocked if you want protected session persistence; unavailable storage still
allows memory-only sign-in. See [authentication](AUTHENTICATION_REVIEW.md).

## Development checks

```sh
for toolchain in stable 1.98.1; do
  cargo +"$toolchain" fmt --all -- --check
  cargo +"$toolchain" clippy --workspace --all-targets --all-features --locked -- -D warnings
  cargo +"$toolchain" test --workspace --locked
  cargo +"$toolchain" test --workspace --all-features --locked
  cargo +"$toolchain" test --workspace --no-default-features --locked
  cargo +"$toolchain" build --workspace --locked
done

cargo +1.98.1 fetch --locked
cargo +1.98.1 build --release --offline --locked --no-default-features --features cassette-auth
./target/release/karin --audio-self-test
./target/release/karin --playback-self-test
```

The final build uses the same Cargo options as the Flatpak manifest. The two
self-tests use a synthetic signal or generated WAV through `fakesink`; they
check controller operation and cleanup, not audible Music playback.

For an isolated GTK smoke test, install the display-test tools and run:

```sh
sudo dnf install xorg-x11-server-Xvfb xorg-x11-xauth dbus-x11 \
  openbox ImageMagick xdotool wmctrl xorg-x11-utils
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' ./target/release/karin --smoke-test
```

Regular `cargo test` leaves native interaction fixtures ignored. CI executes
them separately on private session buses and Xvfb displays, including sandboxed
WebKit navigation. The [workflow](../.github/workflows/ci.yml) contains the full
commands and current selector registration. See the [UI quality gate](UI_QUALITY_GATE.md)
for visual checks, themes, HiDPI and the separate cold-start measurement.
`--ui-fixture` is an offline capture mode, not ordinary sign-in; never use real
accounts in Cloud/CI or point capture tools at a personal desktop.

## Flatpak source build

Karin is not published on Flathub. The Flathub remote below supplies only the
GNOME runtime, SDK and Rust extension needed for a local source build.

```sh
sudo dnf install flatpak flatpak-builder
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.gnome.Platform//50 org.gnome.Sdk//50 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08
RUSTUP_TOOLCHAIN=1.98.1 python3 packaging/flatpak/prepare-source.py
flatpak-builder --user --install --force-clean build/flatpak \
  packaging/flatpak/io.github.tracerxbrhd.Karin.json
flatpak run io.github.tracerxbrhd.Karin
```

[Source preparation](../packaging/flatpak/prepare-source.py) replaces only the
generated `build/flatpak-source` directory. It copies the working tree without
Git data, environment files or build outputs, vendors the locked dependencies
and generates the license inventory and third-party notices. Review the snapshot
before distribution. Cargo configuration is written inside that copy, not into
the checkout. Dependency retrieval happens during preparation; the SDK build
uses `--offline --locked --no-default-features --features cassette-auth`.

The [manifest](../packaging/flatpak/io.github.tracerxbrhd.Karin.json) retains
GNOME 50 and the 25.08 Rust extension. Its compiler must satisfy `rust-version`;
do not use `--ignore-rust-version`. No host filesystem or broad session-bus
permission is needed. MPRIS uses the exact application own-name grant. Protected
session storage uses the available Secret Portal/backend. Distribution must
preserve [Karin and third-party licensing obligations](LICENSING.md).

## Troubleshooting

- **A native library is missing:** check the `pkg-config` output and install the
  matching `-devel` package. Default builds include WebKitGTK 6.0; a
  `--no-default-features` build is a compatibility check, not the normal client.
- **No sound:** check the two GStreamer plugins above, PipeWire output and KDE's
  application volume. A passing `fakesink` test does not prove physical output.
- **Session cannot be saved:** use the normal desktop session and unlock its
  keyring. Memory-only login remains available; never copy tokens between
  applications or introduce plaintext storage.
- **Offline build lacks dependencies:** run `cargo fetch --locked` or repeat
  source preparation with network access. Do not alter the lockfile to hide a
  missing cache or lower the compiler requirement without compatibility proof.
- **WebKit cannot start:** check the installed WebKitGTK version and supported
  user namespaces. Do not disable its sandbox or TLS checks. Some nested cloud
  containers cannot run navigating WebKit tests; hosted CI and a real desktop
  remain distinct verification environments.

For a local image-loading investigation, `./target/release/karin --artwork-diagnostic`
emits bounded fixed categories without URLs or personal fields. The separate
`--delivery-diagnostic` performs a metadata-only delivery probe; see
[stream delivery](STREAM_DELIVERY_REVIEW.md). The opt-in `--oauth-diagnostic`
remains an independent local PKCE experiment, not the application's sign-in path.
Run each diagnostic flag alone. Never share HTTP/WebKit/GStreamer tracing,
credentials, cookies, signed addresses or private cache contents.

<a id="v04-native-music-acceptance-on-fedora"></a>

## Desktop verification

The owner accepted native functionality, UI/UX and Flatpak at `97093ee` on
Fedora 44/KDE/Wayland/PipeWire. [Validation](VALIDATION.md) records that scope and
known limitations; it does not infer individual results the owner did not report.

For a later runtime change, check ordinary sign-in/restoration, library and
playlist navigation, collection filtering, actual audible Music, transport/seek/
volume/queue/modes and KDE media controls. Exercise light/dark, HiDPI, compact and
maximized windows, keyboard focus and the pinned Info/Queue panel. Repeat the
relevant checks in Flatpak. Use the ordinary renderer and separate first cold
appearance from repeated openings; cloud frame gaps remain unresolved.

Report the exact revision and native/Flatpak environment with anonymized
PASS/FAIL/not-tested outcomes. No account data, audio, secret values or personal
screenshots are needed. Source preparation, a headless smoke test and a full
Flatpak desktop run are different checks.
