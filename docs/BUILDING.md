<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Building and running Karin

Run commands from the repository root. Requirements: Rust 1.98+, GTK 4.14+,
Libadwaita 1.5+, GStreamer and WebKitGTK 6.0. CI checks current stable and exact
**Rust 1.98.1** with the committed `Cargo.lock`. The reference desktop is Fedora
44, KDE Plasma, Wayland and PipeWire.
Release helpers additionally require Python 3.11+ and Git.

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

The plugins provide HTTPS and MP3 decoding. Generated MP3 fixtures additionally
need `lamemp3enc` and `xingmux`. Install Rust through [rustup](https://rustup.rs/):

```sh
rustup toolchain install stable --profile minimal --component rustfmt --component clippy
rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy
```

## Native build and launch

```sh
cargo +stable build --release --locked
./target/release/karin
```

`0.8.2` is prepared for owner acceptance. Install the current stable version
from [GitHub Releases](https://github.com/tracerxbrhd/karin/releases/latest).
Check `--version` and About for the
source commit, clean/modified state, package and profile. Any source-tree change
requires newly built artifacts and checksums; old files with the same version
label do not prove the new commit. Preparing artifacts
does not publish a tag or release, establish Yandex integration rights, or
authorize Flathub submission.

Builds/tests need no Yandex account. Ordinary **Войти через Яндекс** uses the supported
Cassette-compatible sign-in flow, without technical credentials, registration,
pasted tokens or diagnostic flags. Music validation precedes library access.
Unlock the desktop keyring for protected persistence; memory-only login remains
available. **Главная → Моя волна → Слушать** starts radio explicitly.

## Development checks

For a full compatibility check:

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

The release options match the Flatpak manifest. Self-tests use generated media
and `fakesink`; they prove controller operation/cleanup rather than audible Music.
The [workflow](../.github/workflows/ci.yml) assigns checks to the required lanes;
it avoids repeating the full native visual matrix on each compiler.

### Native interaction fixtures

Install display/input tools alongside the native runtime:

```sh
sudo dnf install xorg-x11-server-Xvfb xorg-x11-xauth dbus-x11 \
  openbox ImageMagick xdotool wmctrl xorg-x11-utils at-spi2-core python3-gobject playerctl
```

Regular `cargo test` ignores native fixtures. Compile their all-feature test
executable once and run registered groups through the shared driver:

```sh
cargo +stable test --lib --all-features --locked --no-run --message-format=json > /tmp/karin-tests.jsonl
test_binary=$(python3 scripts/ci-native-tests.py artifact /tmp/karin-tests.jsonl)
python3 scripts/ci-native-tests.py verify "$test_binary"
python3 scripts/ci-native-tests.py run bus "$test_binary"
python3 scripts/ci-native-tests.py run webkit "$test_binary"
python3 scripts/ci-native-tests.py run functional "$test_binary"
python3 scripts/ci-native-tests.py run hidpi "$test_binary" --scale 2
```

Each selector keeps a separate process, session bus and, where needed, Xvfb
screen. The registry rejects missing/unregistered ignored fixtures. Per-selector
logs/results are written to `build/native-tests`; use a fresh `--output` directory
for repeated cold observations. Modern GTK visual environments also run `visual`
at scales 1/2, `lyrics` for ordinary-damage playback motion and `high-contrast`.
`compatibility` provides a small compiled-runtime cross-check. The workflow
records exact renderer/environment settings. The manual
[extended workflow](../.github/workflows/extended-native.yml) captures the current
signed-out gallery; historical comparison sources remain in immutable Git history. Never disable WebKit's sandbox or relax motion,
geometry, fatal-warning or cold-observer assertions to obtain a pass.

The ordinary executable has a separate provider/input regression:

```sh
cargo +stable build --locked
dbus-run-session -- xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  python3 scripts/test-application-fixture.py target/debug/karin \
  --theme light --out /tmp/karin-application-proof
```

Its explicit `--application-fixture` uses only built-in synthetic identity,
loopback HTTP and original generated audio. It cannot accept an account/token or
read/save a real system login. Real input drives TEXT, disclosed LRCLIB consent,
LRC playback, search and restart preferences through the ordinary application.
Repeat the relevant dark/2× cases using the workflow settings and a fresh output
path. `--ui-fixture` is likewise a capture mode, not ordinary sign-in. Follow
the [UI verification](#ui-verification) requirements for changed presentation.

## Flatpak source build

Karin is not published on Flathub; that remote supplies the runtime/SDK only.

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

[Source preparation](../packaging/flatpak/prepare-source.py) replaces only generated
`build/flatpak-source`. It snapshots the working tree without Git/environment/
build outputs, vendors locked dependencies and generates license inventory/notices.
Review it before distribution. Cargo configuration stays inside that copy; the
SDK build uses `--offline --locked --no-default-features --features cassette-auth`.

The [manifest](../packaging/flatpak/io.github.tracerxbrhd.Karin.json) retains GNOME
50 and Rust extension 25.08. Its compiler must satisfy `rust-version` without
`--ignore-rust-version`. Keep exact MPRIS own-name permission, Secret Portal storage
and PulseAudio-compatible PipeWire audio. No broad session-bus/host-filesystem
grant is required. Preserve [licensing](LICENSING.md). Source preparation/offline
Cargo and a full installed Flatpak runtime check are distinct results.

### Pinned release sources

The development manifest above is not a standalone publication manifest. For an
existing pushed, reviewed commit, generate a separate source-based manifest:

```sh
python3 packaging/release/manifest.py --revision "$(git rev-parse HEAD)" \
  --output build/release/io.github.tracerxbrhd.Karin.json
flatpak-builder --user --install --force-clean build/flatpak-release \
  build/release/io.github.tracerxbrhd.Karin.json
```

For the complete release set, prepare matching sources, build Native once,
then use the artifact commands (the `flatpak` command replaces the direct
`flatpak-builder` invocation above and also exports a bundle):

```sh
karin_release_revision=$(git rev-parse HEAD)
rustup component add rust-src --toolchain 1.98.1
RUSTUP_TOOLCHAIN=1.98.1 python3 packaging/flatpak/prepare-source.py
(
  cd build/flatpak-source
  CARGO_TARGET_DIR=../../target cargo +1.98.1 build --release --offline --locked \
    --no-default-features --features cassette-auth
)
RUSTUP_TOOLCHAIN=1.98.1 python3 packaging/release/artifacts.py native --revision "$karin_release_revision" \
  --binary target/release/karin --source-dir build/flatpak-source --output build/release
python3 packaging/release/artifacts.py flatpak --revision "$karin_release_revision" \
  --manifest build/release/io.github.tracerxbrhd.Karin.json \
  --source-dir build/flatpak-source --output build/release
```

The Native tar contains `run-karin`, desktop/icon/metainfo assets and licenses;
use that launcher so the private `share` directory is visible. It bundles no
shared system libraries; `NATIVE-LIBRARIES.json` records the packaging host's dependencies.
Static Rust std/compiler-builtins notices are collected from the matching
compiler's supplied library notices and `rust-src`; a compiler mismatch is rejected.
For a stable-compiler package, install its `rust-src` component and set
`RUSTUP_TOOLCHAIN=stable` when packing that binary. The SDK uses its own compiler
and source component. These static-runtime notices are separate from Cargo crate
notices and from non-bundled desktop/runtime libraries.
The matching source tar includes locked vendor, notices and build markers.
`artifacts.py checksums --output build/release/SHA256SUMS FILE...` takes an explicit
list of delivered archives, bundle, manifest and BUILD-INFO files; verify with
`sha256sum --check SHA256SUMS` from their directory. The source validator requires
the original `.crate` cache from locked Cargo fetch to authenticate vendor bytes.

The generator pins the project's Git archive and its SHA-256, registry crate
archives from `Cargo.lock`, and offline Cargo configuration. It does not package
local build outputs or require a sibling generated development source directory.
Use the release artifact tooling for the matching corresponding source, notices,
BUILD-INFO and checksum manifest. These are source-repeatable and
checksum-verifiable artifacts; independent byte-for-byte reproducibility is not
claimed.

Public distribution still depends on the recorded integration/release gates.
Separately, the [current Flathub requirements](https://docs.flathub.org/docs/for-app-authors/requirements#generative-ai-policy)
prohibit AI-generated or AI-assisted Flathub manifests, require disclosure of other
known generated material and prohibit automated submission interactions. These
AI-assisted standalone manifests are not represented as submission-eligible;
disclosure or review does not override that manifest restriction. No Flathub
submission, public release or tag is performed by these commands.

### Stable Flatpak channel

The [update ADR](adr/0036-signed-flatpak-updates.md) describes the trust boundary.
The build commands above prepare a local bundle. The
[stable update workflow](../.github/workflows/stable-update.yml) additionally
prepares distribution packages with a signed HTTPS origin. The source's
`packaging/release/flatpak-key-fingerprint` is empty until a production key is
approved and pinned in a reviewed source commit.

For an authorized publication:

1. Keep the primary GnuPG signing key offline. Pin its uppercase 40-hex
   fingerprint in source, and store the base64 export of a dedicated unattended
   signing subkey with that primary certificate in the
   `karin-stable-signing` environment's `KARIN_FLATPAK_SIGNING_SUBKEY` secret.
   Restrict that environment to `master` with required reviewers. Enable GitHub
   Actions as the Pages source and protect the separate `github-pages`
   environment with reviewers and the same branch restriction.
2. Dispatch **Prepare stable Flatpak update** on the exact reviewed `master`
   commit after its push CI and authorized stable tag. Set `deploy_pages=true`
   to request the separate Pages approval in this run. The signing job verifies
   the packaged binary and signs the tested deployment and repository metadata.
3. Review the `karin-stable-release-<commit>` artifact, verify `SHA256SUMS`, and
   attach the packages, corresponding source, build receipts and exact
   `KARIN-FLATPAK-UPDATE.json` from this run to the stable GitHub release. Publish
   the release only with owner authorization. Approve the Pages job after
   publication. It rechecks the remote tag, current `master` and published asset
   digest and deploys the signed site artifact from that same workflow run.
   Keep the upload and approval in the same run so signatures and the advertised
   deployment describe the same prepared bytes.

The snapshot has a 250 MiB budget under GitHub Pages' 1 GB site limit. GitHub
documents a soft 100 GB/month bandwidth limit and a ten-minute deployment
timeout. Earlier package versions remain in their GitHub Releases. A fresh
snapshot drops older refs and static deltas, so the site stays bounded. Revisit
retention and hosting before adding architectures or exceeding the limits.
[GitHub Pages limits](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits).

#### Move an existing bundle installation to the channel

Once the owner has published the channel, new users can open the verified
`.flatpakref` in Discover. The signed standalone bundle also carries the remote
URL. For an existing **user** installation of 0.8.0 with a bundle-only origin,
quit Karin, verify the downloaded `karin.flatpakrepo` against the stable release
checksums, then switch the existing ref to the signed remote:

```sh
flatpak info --user --show-origin io.github.tracerxbrhd.Karin
flatpak remote-add --user --from karin ./karin.flatpakrepo
flatpak install --user --reinstall --no-deps --no-related karin io.github.tracerxbrhd.Karin//master
flatpak info --user --show-origin io.github.tracerxbrhd.Karin
flatpak remotes --user --columns=name,url
```

The origin should be `karin` with
`https://tracerxbrhd.github.io/karin/flatpak/`. If the `karin` remote already exists,
first verify its URL and key from the published installer before using it. For
an existing system installation, apply the same change in the system scope via
the package manager. The 0.8.0 and 0.8.1 packages use the same installed runtime.
Keep the same install scope and app ID to retain the application data. Subsequent approved upgrades can use **Проверить обновления**
in Karin when the host Portal reports the signed deployment.

Run the signing regression with real OSTree and GnuPG installed:

```sh
python3 -m unittest discover -s packaging/release -p test_flatpak_channel.py
```

This uses temporary signing keys and an isolated repository. It verifies
repository signature enforcement. A separate installed-package test is required
to confirm Portal update and restart.

### Bounded cold-render diagnostic

The existing native cold fixture remains a required regression; a passing fixture
does not certify cold latency. Detailed interposition is optional and runs outside
ordinary app sessions. The manual extended workflow's `cold-profile` mode collects
two fresh control/trace pairs for software GL and Cairo, with source/ELF identities.
Raw traces and PNGs are artifacts, not repository assets. Hook spans overlap and
missing hooks do not prove zero cost.

For one targeted Fedora check, compile the current fixture and run it on the
existing desktop without forcing a renderer or exporting screenshots:

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

This synthetic test uses a private profile and session bus, its own generated
audio and native GTK row activation; the Cloud/Xvfb path retains real XTest input.
It measures first Play → Stop → repeat Play → Stop with the same 240 ms reveal.
It neither logs into Yandex nor reads the real account. Retain `summary.json` and
the build identity locally; no private desktop capture or full Fedora acceptance
repeat is needed. The reported actual GDK/GSK/GL identity determines its platform
scope; Xvfb/llvmpipe results cannot certify physical Wayland/GPU behavior.

## UI verification

For substantial UI changes inspect actual mapped widgets and native input,
not only a successful build or static screenshot. Use original synthetic data;
private desktop/account captures require publication permission.

- Check compact, medium, wide and maximized windows, both schemes, relevant
  1×/2× cases, High Contrast and Reduced Motion. Measure actual logical
  allocations, density, clipping, contrast and hit targets rather than requested
  outer geometry alone. Preserve zero hidden-footer height and its active
  24-logical-pixel inner bottom spacing.
- Exercise mouse, Tab, Enter/Space and keyboard alternatives; inspect focus and
  AT-SPI roles/names. Verify search/dismissal, pinned panels, duplicate occurrences,
  queue/transport/MPRIS, consent and cancellation where affected.
- Check real paired Lyrics glyph changes/Follow and panel motion in native frames,
  retaining source/ELF, timing, backend/theme/scale and actual state identity.
  Initialized motion does not prove first-use latency. Never add prewarm, force
  renderer changes or weaken geometry/frame-gap assertions to hide failures.
- Test relevant session/cache/playlist/Wave lifecycle effects without restarting
  working playback. Report executed PASS/FAIL/not-tested and environment limits;
  local HTTP/generated audio is not a live-service or physical-desktop acceptance.

The owner accepted v0.8 Fedora Native/Flatpak functionality; repeat only checks
relevant to a later functional change. Documentation cleanup alone does not
require full owner reacceptance. Current limits are in [LIMITATIONS](LIMITATIONS.md).

## Troubleshooting

- Missing library/plugin: check `pkg-config`/`gst-inspect` and matching development
  packages. Minimal features test compatibility; they are not the ordinary client.
- No sound: inspect PipeWire output and application volume. `fakesink` proves no
  physical output.
- Protected storage unavailable: unlock the normal desktop keyring or use
  memory-only login. Never introduce plaintext storage or import other sessions.
- Offline dependency miss: fetch the locked dependencies or repeat preparation;
  keep the lockfile and compiler requirement intact.
- WebKit startup: check supported versions/user namespaces. Some nested Cloud
  containers cannot navigate with sandboxing; report that limit without disabling
  sandbox/TLS or calling it PASS.

Run diagnostic flags separately. `--artwork-diagnostic` emits fixed categories;
`--delivery-diagnostic` is metadata-only; `--oauth-diagnostic` is the independent
PKCE experiment. Optional `KARIN_AUDIO_CACHE_DIAGNOSTICS=1` emits fixed cache/source/
reason classifications only. Do not share HTTP/WebKit/GStreamer traces, signed
addresses, identifiers, cookies, tokens or private cache contents.

## Theme input check

Karin applies color through `AdwStyleManager`, without setting the legacy
`gtk-application-prefer-dark-theme` flag. An external setting can trigger an
Adwaita initialization warning before saved appearance is applied. The writer
on the owner's desktop is unestablished. This optional read-only command prints
bounded origin/boolean labels, not effective precedence or other configuration:

```sh
python3 scripts/check-gtk4-theme-key.py
```

It changes no settings. Do not delete KDE/GTK preferences or force a theme to
hide the warning.
