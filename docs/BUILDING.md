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

Install the current stable version from [GitHub Releases](https://github.com/tracerxbrhd/karin/releases/latest).
Use `--version` or About to check the source commit, source state, package and
profile. Rebuild after a source-tree change to obtain artifacts and checksums
for that revision.

Builds/tests need no Yandex account. Ordinary **Войти через Яндекс** uses the supported
Cassette-compatible sign-in flow, without technical credentials, registration,
pasted tokens or diagnostic flags. Music validation precedes library access.
Unlock the desktop keyring for protected persistence; memory-only login remains
available. **Главная → Моя волна → Слушать** starts radio explicitly.

## Development checks

Use a branch and pull request. The maintainer approves merges and releases.
Start with the [architecture overview](ARCHITECTURE.md) and record significant
changes to component contracts in an [ADR](adr/). Use synthetic fixtures for
service tests. Keep credentials, private account data and signed media URLs
out of commits, logs and screenshots. Preserve the [required notices](LICENSING.md).

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

### Flatpak acceptance build

Run **Extended diagnostics and Flatpak acceptance** in `flatpak-acceptance`
mode on the candidate branch. The job builds the pinned source, reinstalls the
exported bundle on its fresh runner, checks its commit and runs the GTK smoke
test. Download `karin-flatpak-acceptance-<commit>` from that run.

Use the acceptance bundle in a disposable desktop account with GNOME Platform
50. Its App ID matches the stable installation, so the separate desktop account
isolates both the installed app and its data. The acceptance bundle has a local
origin. Release signing and channel publication use the stable workflow below.

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

The Native tar contains `run-karin`, desktop/icon/metainfo assets and licenses.
Run the launcher from the extracted directory so the private `share` directory
is visible to Karin. Pull request CI uploads the same Native package layout and
its corresponding source as `karin-native-acceptance-<commit>`. Extract the
Native tar into its own directory and run `./run-karin --version` to confirm
the tested commit.

For a Fedora menu or Plasma taskbar check, extract the tar to a permanent
location in a test account. With `desktop-file-utils` installed, run from the
extracted directory to register the launcher and icons for that account:

```sh
karin_desktop_data=${XDG_DATA_HOME:-"$HOME/.local/share"}
mkdir -p "$karin_desktop_data/applications" "$karin_desktop_data/icons"
cp -R share/icons/hicolor "$karin_desktop_data/icons/"
desktop-file-install --dir="$karin_desktop_data/applications" \
  --set-key=Exec --set-value="\"$(pwd -P)/run-karin\"" \
  share/applications/io.github.tracerxbrhd.Karin.desktop
```

Launch Karin from the application menu. Plasma resolves the Wayland App ID
against this desktop entry and the session's icon theme. An unpacked binary or
launcher started from a terminal does not register files with Plasma, which may
still see the icon from an installed version using the same App ID. Flatpak
exports its desktop entry and icons during installation.

The Native package uses system shared libraries. If the loader reports a
missing `.so` or library version, run `./run-karin` in a terminal to see the
missing dependency. Use the [Fedora prerequisites](#fedora-prerequisites) to
identify the supported GTK, WebKitGTK and GStreamer packages. Use the Flatpak
installer on systems with incompatible library versions. `NATIVE-LIBRARIES.json`
records the packaging host's dependencies.
Static Rust std/compiler-builtins notices are collected from the matching
compiler's supplied library notices and `rust-src`; a compiler mismatch is rejected.
For a stable-compiler package, install its `rust-src` component and set
`RUSTUP_TOOLCHAIN=stable` when packing that binary. The SDK uses its own compiler
and source component. These static-runtime notices are separate from Cargo crate
notices and from non-bundled desktop/runtime libraries.
The matching source tar includes locked vendor, notices and build markers.
The source validator requires the original `.crate` cache from locked Cargo
fetch to authenticate vendor bytes. Maintainers can generate internal checksums
with `artifacts.py checksums --output build/release/SHA256SUMS FILE...`. The
publication pipeline verifies its own build receipt and GitHub asset digests.

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

The [stable update workflow](../.github/workflows/stable-update.yml) builds both
formats from one reviewed source commit. See the [update ADR](adr/0036-signed-flatpak-updates.md)
for how GitHub release notes, the Pages record and the signed repository match.

| Release | Assets uploaded to the main GitHub Release |
| --- | --- |
| 0.8.2 | `karin-0.8.2-x86_64.flatpak`, `karin-0.8.2-linux-x86_64.tar.gz`, `KARIN-FLATPAK-UPDATE.json` |
| 0.8.3 and later | `karin-<version>-x86_64.flatpak`, `karin-<version>-linux-x86_64.tar.gz` |

GitHub adds its two tagged source downloads. The signed bundle embeds the
existing HTTPS Karin origin, its public key and the Flathub runtime repository.
The Pages snapshot retains the signed OSTree repository, installer descriptors
and the schema 1 record. BUILD-INFO, the pinned manifest, internal checksums and
publication receipt are available to maintainers through workflow artifacts.

#### Owner publication procedure

1. Use the public `tracerxbrhd/karin-source` repository and retain its immutable,
   versioned releases. Keep the current production fingerprint, signing secret
   and reviewer protection on `karin-stable-signing` and `github-pages`.
2. After acceptance and separate release authorization, merge the reviewed
   changes, verify the four jobs for the final `master` push and create its
   matching stable version tag. Dispatch **Prepare stable Flatpak update** on
   `master` with the full commit and `deploy_pages=true`. Approve the signing job
   if GitHub requests approval. Keep `master` at the release commit until Pages
   deployment completes.
3. Download `karin-corresponding-source-<commit>` and
   `karin-release-verification-<commit>` from that run. Publish
   `karin-<version>-source.tar.gz` in the public source repository's release for
   `v<version>`. Before publishing binaries, verify the anonymous download from
   the reviewed checkout.

   ```sh
   python3 packaging/release/publication.py verify-source /path/to/publication.json
   ```

   The archive must match the build receipt. The verifier prints the versioned
   source link and hidden digest line for the notes. Pause publication if the
   check fails. Keep each source archive indefinitely.
4. Use exactly the files in `karin-release-assets-<commit>` for the main stable
   Latest release. Include the verified source archive link and append the exact
   hidden Pages-record digest from the receipt as the last line of the notes.
5. After the main release is public, approve the Pages job in the same run if
   GitHub requests approval. The job verifies the source download, release notes,
   asset set and digests, and matches `master` and the tag to the reviewed run.
   If the Pages job already stopped because the release or archive was missing,
   rerun only `publish` in the same workflow after both are public. Check the
   public record, signed Flatpak ref and installer after deployment.

The Pages snapshot has a 250 MiB budget. It retains the current release with
loose objects and drops earlier refs and static deltas. Earlier binaries remain
in their GitHub Releases, and the complete matching source archives are
retained separately. [GitHub Pages limits](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits)

#### Move an existing bundle installation to the channel

Open the downloaded `.flatpak` with Discover or GNOME Software, or pass the file
path to `flatpak install --user`. The embedded remote usually appears as
`karin-origin`. Installations from an earlier `.flatpakref` may use `karin`.
Both use `https://tracerxbrhd.github.io/karin/flatpak/`.

Karin 0.8.1 reads the update record attached to Release 0.8.2. For later
releases, use the signed remote through the system application manager or the
terminal when the installed updater cannot confirm or start the transaction.

For the 0.8.2 to 0.8.3 upgrade, use the system application manager or Flatpak
CLI to confirm the new read-only Discord IPC permissions
`xdg-run/discord-ipc-0:ro` and `xdg-run/app/com.discordapp.Discord:ro`.

```sh
flatpak info --user --show-origin io.github.tracerxbrhd.Karin
flatpak remotes --user --columns=name,url
flatpak update --user io.github.tracerxbrhd.Karin
```

Use the system scope for a system installation. An older bundle-only
installation without a Karin URL needs a one-time move to the signed origin.
Keep the same AppID and scope and retain the application data.
[Existing 0.8.1 release instructions](https://github.com/tracerxbrhd/karin/releases/tag/v0.8.1)

The packaging test suite uses temporary signing keys and isolated repositories.

```sh
python3 -m unittest discover -s packaging/release -p 'test_*.py'
```

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
