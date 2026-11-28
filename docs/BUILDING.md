<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Building and running Karin

Run commands from the repository root. Karin requires Rust 1.98+, GTK 4.14+,
Libadwaita 1.5+, GStreamer and WebKitGTK 6.0. CI checks Rust 1.98.1 and current stable
with `Cargo.lock`. Packaging helpers require Python 3.11+ and Git.

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

rustup toolchain install stable --profile minimal --component rustfmt --component clippy
rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy
```

Install [rustup](https://rustup.rs/) first. Generated MP3 fixtures also need `lamemp3enc`
and `xingmux`. Fedora KDE Plasma on Wayland with PipeWire is the reference desktop.
For native Wayland tray placement, install `gtk4-layer-shell-devel` (1.3+) before
building. The linked library is then required at runtime. A native build with
X11 support can also place Karin Mini on X11 or Xwayland, when available.

## Native build and launch

```sh
cargo +stable build --release --locked
./target/release/karin
```

For the packaged build, extract the archive from [GitHub Releases](https://github.com/tracerxbrhd/karin/releases/latest)
and launch `./run-karin`. Check the revision and package with `--version` or About.

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
cargo +1.98.1 build --release --offline --locked --no-default-features --features browser-session-auth
./target/release/karin --audio-self-test
./target/release/karin --playback-self-test
python3 -m unittest discover -s packaging/release -p 'test_*.py'
```

The self-tests use generated audio and `fakesink`. Packaging tests use temporary signing keys
and isolated repositories. The [CI workflow](../.github/workflows/ci.yml) defines the full matrix.
CI distributes the Ubuntu functional and HiDPI fixtures across three independent runners.
Debian visual and Lyrics fixtures use separate runners at 1× and 2×. High Contrast runs with
the 1× lane. All fixtures at their original scales are required for release.

### Native interaction fixtures

```sh
sudo dnf install xorg-x11-server-Xvfb xorg-x11-xauth dbus-x11 \
  openbox ImageMagick xdotool wmctrl xorg-x11-utils at-spi2-core python3-gobject playerctl

cargo +stable test --lib --all-features --locked --no-run --message-format=json > /tmp/karin-tests.jsonl
test_binary=$(python3 scripts/ci-native-tests.py artifact /tmp/karin-tests.jsonl)
python3 scripts/ci-native-tests.py verify "$test_binary"
python3 scripts/ci-native-tests.py run bus "$test_binary"
python3 scripts/ci-native-tests.py run webkit "$test_binary"
python3 scripts/ci-native-tests.py run functional "$test_binary"
python3 scripts/ci-native-tests.py run hidpi "$test_binary" --scale 2

cargo +stable build --locked
dbus-run-session -- xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  python3 scripts/test-application-fixture.py target/debug/karin \
  --theme light --out /tmp/karin-application-proof
```

Run the affected visual, lyrics and High Contrast cases with the settings in [CI](../.github/workflows/ci.yml).
Use a fresh output directory per run. These fixtures use original media and synthetic accounts.
Keep the WebKit sandbox enabled.

## Flatpak source build

Flathub supplies the GNOME runtime and SDK. Get Karin itself from its signed channel or build it locally.

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

Source preparation replaces `build/flatpak-source`, vendors locked dependencies and generates
license notices in that snapshot. Build offline with the manifest's compiler, feature flags
and sandbox permissions. [Architecture](ARCHITECTURE.md) covers portal storage and desktop integration.

### Flatpak acceptance build

Dispatch [Extended diagnostics and Flatpak acceptance](../.github/workflows/extended-native.yml)
with `flatpak-acceptance` on the candidate branch. It builds, reinstalls and smoke-tests the
bundle. Install `karin-flatpak-acceptance-<commit>` in a disposable desktop account with GNOME
Platform 50. The candidate uses a local origin and shares the stable App ID.

PR CI also uploads `karin-native-acceptance-<commit>`. For either package, confirm the tested
revision with `--version` and record live desktop or service checks separately from CI.

For the tray check, use a desktop with a StatusNotifier host such as KDE Plasma.
Enable closing to the tray in Karin's settings and close the main window. Click the tray icon to
open and hide Karin Mini. Verify its transport, seek and volume against the same playback session.
Use Open Karin to restore the main window and Quit from the tray menu to exit.

### Pinned release sources

For a pushed, reviewed commit, generate the standalone manifest and corresponding packages.

```sh
karin_release_revision=$(git rev-parse HEAD)
python3 packaging/release/manifest.py --revision "$karin_release_revision" \
  --output build/release/io.github.tracerxbrhd.Karin.json

rustup component add rust-src --toolchain 1.98.1
RUSTUP_TOOLCHAIN=1.98.1 python3 packaging/flatpak/prepare-source.py
(
  cd build/flatpak-source
  CARGO_TARGET_DIR=../../target cargo +1.98.1 build --release --offline --locked \
    --no-default-features --features browser-session-auth
)
RUSTUP_TOOLCHAIN=1.98.1 python3 packaging/release/artifacts.py native --revision "$karin_release_revision" \
  --binary target/release/karin --source-dir build/flatpak-source --output build/release
python3 packaging/release/artifacts.py flatpak --revision "$karin_release_revision" \
  --manifest build/release/io.github.tracerxbrhd.Karin.json \
  --source-dir build/flatpak-source --output build/release
```

To test the pinned manifest directly, run `flatpak-builder --user --install --force-clean build/flatpak-release build/release/io.github.tracerxbrhd.Karin.json`.
Keep the locked `.crate` files for source verification and use the binary's exact compiler and
`rust-src` for Rust runtime notices. See [licensing and corresponding-source obligations](LICENSING.md).
For local checksums, run `python3 packaging/release/artifacts.py checksums --output build/release/SHA256SUMS FILE...`.

[Flathub's generative AI policy](https://docs.flathub.org/docs/for-app-authors/requirements#generative-ai-policy)
prohibits AI-assisted manifests and automated submission interactions, and requires disclosure of
other known generated material. Karin's standalone release manifests were prepared with AI
assistance. A Flathub submission requires a manifest that meets Flathub's policy.

### Stable Flatpak channel

The [stable workflow](../.github/workflows/stable-update.yml) builds both formats from one reviewed
commit. See [how Karin verifies updates](ARCHITECTURE.md#application-updates).

| Release | Assets uploaded to GitHub Releases |
| --- | --- |
| 0.8.2 | Native archive, signed Flatpak bundle and `KARIN-FLATPAK-UPDATE.json` |
| 0.8.3 and later | `karin-<version>-linux-x86_64.tar.gz` and `karin-<version>-x86_64.flatpak` |

GitHub adds its standard tagged source downloads. The complete matching source archive is
published separately in [karin-source](https://github.com/tracerxbrhd/karin-source/releases).

#### Owner publication procedure

1. Confirm the stable release, all eight successful CI jobs on the final `master` commit and its
   matching version tag. Dispatch **Prepare stable Flatpak update** on `master` with the full
   `revision` and `deploy_pages=true`. Keep `master` at that commit through deployment. Signing
   and Pages use the protected `karin-stable-signing` and `github-pages` environments. Keep the
   production key aligned with the [pinned fingerprint](../packaging/release/flatpak-key-fingerprint)
   and retain the environment reviewers.
2. Download `karin-corresponding-source-<commit>` and `karin-release-verification-<commit>`.
   Publish `karin-<version>-source.tar.gz` in the matching `v<version>` release of
   `tracerxbrhd/karin-source`. Verify its anonymous download and digest from the reviewed checkout.

   ```sh
   python3 packaging/release/publication.py verify-source /path/to/publication.json
   ```

3. Publish the main stable Latest release using exactly `karin-release-assets-<commit>`.
   Include the verified source archive link and the hidden Pages-record digest from the receipt
   as the last line of the notes.
4. Approve Pages in the same run after both releases are public. If the job already failed
   awaiting them, rerun only `publish`. Confirm the public record, signed ref and installer.

Pages retains the current signed OSTree repository and deployment record within a 250 MiB budget.
Retain earlier binaries and complete corresponding sources in their release repositories.

#### Move an existing bundle installation to the channel

Install the release bundle with Discover, GNOME Software or `flatpak install --user path/to/karin.flatpak`.
The embedded remote is usually `karin-origin`. Older `.flatpakref` installations may use `karin`.
Both should point to `https://tracerxbrhd.github.io/karin/flatpak/`.

```sh
flatpak info --user --show-origin io.github.tracerxbrhd.Karin
flatpak remotes --user --columns=name,url
flatpak update --user io.github.tracerxbrhd.Karin
```

Use `--system` for a system installation. Keep the App ID, installation scope and user data.
For an older bundle without the signed origin, follow the [0.8.1 migration instructions](https://github.com/tracerxbrhd/karin/releases/tag/v0.8.1).
Karin 0.8.1 reads the record attached to 0.8.2. For 0.8.2 → 0.8.3, use the system manager or
Flatpak CLI to confirm the new read-only Discord IPC permissions. For 0.8.5 → 0.8.6, use the same
route to grant Karin access to the desktop's status notifier watcher and see-only access to the
Plasma shell's bus name. Update through your system manager if a later built-in update cannot
confirm or install the deployment.

### Bounded cold-render diagnostic

Use the extended workflow's `cold-profile` mode for software GL and Cairo control runs.
For one targeted desktop check, record the build identity and use the default renderer.

```sh
cargo +1.98.1 test --lib --all-features --locked --no-run --message-format=json > /tmp/karin-cold-build.jsonl
karin_cold_binary=$(python3 scripts/ci-native-tests.py artifact /tmp/karin-cold-build.jsonl)
karin_cold_output=$(mktemp -d "${TMPDIR:-/tmp}/karin-cold.XXXXXXXX")
{
  git rev-parse HEAD
  git diff --binary | sha256sum
  rustc +1.98.1 --version --verbose
  sha256sum "$karin_cold_binary" | cut -d ' ' -f 1
} > "$karin_cold_output/build-identity.txt"
scripts/observe-cold-playerbar.sh trace "$karin_cold_binary" \
  "$karin_cold_output/run" default desktop
```

Keep `summary.json` with the build identity and renderer. CI results only describe the tested backend.
The software-renderer measurements and their scope are recorded in [#23](https://github.com/tracerxbrhd/karin/issues/23).

## UI verification

- Exercise the affected views at relevant sizes, 1×/2×, light/dark, High Contrast and Reduced Motion.
  Inspect actual allocations, clipping, hit targets, AT-SPI roles and keyboard focus.
- Check mouse/keyboard input, panel motion, Lyrics timing and affected playback/session behavior.
  Preserve zero hidden-footer height and 24 px active inner bottom spacing.
- Keep renderer, geometry and timing checks intact. Record the tested build, environment and results.
  Use synthetic data in shared captures unless publication of a private capture is authorized.

## Troubleshooting

- Missing library or plugin: launch `./run-karin` from a terminal and check `pkg-config` or `gst-inspect-1.0`.
  Use Flatpak on a system with incompatible library versions. `NATIVE-LIBRARIES.json` lists the build host's libraries.
- Missing Native icon in Plasma: register the extracted launcher and icons under the user's XDG paths.
- No sound: check Karin's volume and the PipeWire output assigned to the application.
- Missing offline dependency: fetch with `cargo +1.98.1 fetch --locked` and repeat source preparation.
- Protected storage unavailable: unlock the desktop keyring or continue with a memory-only sign-in.
- WebKit startup fails: check supported versions and user namespaces. Keep sandboxing and TLS enabled.
- Missing covers: restart with `./run-karin --artwork-diagnostic` or `flatpak run io.github.tracerxbrhd.Karin --artwork-diagnostic`.
  Open the affected view and include only the `Karin artwork: event=` lines in a bug report.
- Discord Rich Presence unavailable: allow a few seconds for Karin to reconnect after Discord starts or restarts.
  For a recurring error, launch Native with `KARIN_DISCORD_DIAGNOSTICS=1` or pass
  `--env=KARIN_DISCORD_DIAGNOSTICS=1` to `flatpak run`. Share only the
  `Karin Discord: event=` lines. They contain event names, error categories,
  numeric codes and elapsed time.

To register Native in the application menu, install `desktop-file-utils` and run this from the extracted directory:

```sh
karin_desktop_data=${XDG_DATA_HOME:-"$HOME/.local/share"}
mkdir -p "$karin_desktop_data/applications" "$karin_desktop_data/icons"
cp -R share/icons/hicolor "$karin_desktop_data/icons/"
desktop-file-install --dir="$karin_desktop_data/applications" \
  --set-key=Exec --set-value="\"$(pwd -P)/run-karin\"" \
  share/applications/io.github.tracerxbrhd.Karin.desktop
```

For owner-local auth tests, `--oauth-diagnostic` is the manual OAuth diagnostic.
`--independent-auth login [--remember]`, `restore` and `logout` work before GTK through either
launcher. Use an authorized Karin registration and exact registered loopback port, and share only
redacted result categories.
`--delivery-diagnostic` checks metadata, and `KARIN_AUDIO_CACHE_DIAGNOSTICS=1` reports fixed cache categories.

## Theme input check

An external `gtk-application-prefer-dark-theme` setting can trigger an Adwaita startup warning.
This read-only diagnostic reports the setting's origin and value.

```sh
python3 scripts/check-gtk4-theme-key.py
```
