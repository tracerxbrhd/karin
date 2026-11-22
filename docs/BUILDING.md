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

`0.8.0-rc.1` is an engineering candidate. Check `--version` and About for the
source commit, clean/modified state, package and profile. Preparing a candidate
does not publish a tag or release, establish Yandex integration rights, or
authorize Flathub submission.

Builds/tests need no Yandex account. Ordinary **Войти** uses the supported
Cassette-compatible sign-in flow, without technical credentials, registration,
pasted tokens or diagnostic flags. Music validation precedes library access.
Unlock the desktop keyring for protected persistence; memory-only login remains
available. **Главная → Моя волна → Слушать** starts radio explicitly. The old
`--experimental-wave` flag is a no-op compatibility alias.

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
path. `--ui-fixture` is likewise a capture mode, not ordinary sign-in. See the
[UI quality gate](UI_QUALITY_GATE.md) for inspection requirements.

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

### Pinned release-candidate sources

The development manifest above is not a standalone publication manifest. For an
existing pushed, reviewed commit, generate a separate source-based manifest:

```sh
python3 packaging/release/manifest.py --revision "$(git rev-parse HEAD)" \
  --output build/release/io.github.tracerxbrhd.Karin.json
flatpak-builder --user --install --force-clean build/flatpak-rc \
  build/release/io.github.tracerxbrhd.Karin.json
```

For the complete candidate set, prepare matching sources, build Native once,
then use the artifact commands (the `flatpak` command replaces the direct
`flatpak-builder` invocation above and also exports a bundle):

```sh
karin_release_revision=$(git rev-parse HEAD)
RUSTUP_TOOLCHAIN=1.98.1 python3 packaging/flatpak/prepare-source.py
(
  cd build/flatpak-source
  CARGO_TARGET_DIR=../../target cargo +1.98.1 build --release --offline --locked \
    --no-default-features --features cassette-auth
)
python3 packaging/release/artifacts.py native --revision "$karin_release_revision" \
  --binary target/release/karin --source-dir build/flatpak-source --output build/release
python3 packaging/release/artifacts.py flatpak --revision "$karin_release_revision" \
  --manifest build/release/io.github.tracerxbrhd.Karin.json \
  --source-dir build/flatpak-source --output build/release
```

The Native tar contains `run-karin`, desktop/icon/metainfo assets and licenses;
use that launcher so the private `share` directory is visible. It bundles no
system libraries; `NATIVE-LIBRARIES.json` records the packaging host's dependencies.
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

## Desktop regression checks

The owner has accepted v0.8 functionality on Fedora; this is a reusable guide
for future relevant regressions, not a request to repeat that acceptance.
Close older instances and verify `--version`, About and any `BUILD-INFO.txt`/
`SHA256SUMS` against the intended commit/package. Use normal Native and installed
Flatpak with the same source when the changed behavior affects both packages.

1. Verify ordinary sign-in/restoration and library navigation. Play entitled
   Music and hear sound; exercise Pause/Resume, Seek, Volume, Next/Previous/EOS,
   Shuffle/Repeat and KDE/MPRIS. Check long-pause Resume keeps the occurrence and
   confirmed position without a second player or renewed radio cycle.
2. Check exact duplicate queue edits and ordinary close/restart without autoplay.
   Stop/forget/logout remove continuation; Wave never restores. For writes, use
   a disposable owned playlist or deliberate reaction, require independent
   readback and use GET-only Refresh after an uncertain outcome.
3. Exercise collection/global search, sort and index continuation, then Lyrics
   TEXT/LRC, consent/revocation, Follow/manual scroll and stale selection. Repeating
   the footer Lyrics action and Back return to Track Info in the open panel.
4. Check System in light/dark/system colors, compact/wide/maximized, 1×/2×,
   keyboard/focus, High Contrast and Reduced Motion. Karin remains unavailable.
   The footer retains a 24-logical-pixel inner bottom margin and zero hidden height.
5. Check actual saving/local-source cues and replay/restart under current online
   access. A Stored icon alone proves no hit or offline right. Clear must remove
   saved indications while preserving admitted audio/live sign-in. My Wave starts
   only from Listen and uses the existing four-event/no-replay lifecycle.

Report exact revision/package, environment and PASS/FAIL/not-tested for checks
actually performed. Keep account content, credentials, lyrics and private captures
local. Headless generated media cannot substitute for heard service audio.
[VALIDATION](VALIDATION.md) records current acceptance;
[LIMITATIONS](LIMITATIONS.md) retains independent cold-render/distribution bounds.

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
