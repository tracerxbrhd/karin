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

### Stability acceptance

The owner accepted the scoped native Fedora stability fixes at `9d54834`;
PR #24 was squash-merged as `920fd71`. This does not add an installed-Flatpak
acceptance claim. For later changes, build the exact revision and repeat the
relevant regression checks below. Keep HTTP/WebKit/GStreamer tracing disabled.

1. Launch ordinary Karin. Check protected session restoration, or complete normal
   sign-in if storage is unavailable. Open library, search and a playlist.
2. Play a real entitled track through PipeWire. Check Pause/Resume, Seek, Volume,
   Next/Previous and EOS; no synthetic signal substitutes for heard service music.
3. Append the same track twice manually. Play the second occurrence, inspect
   another track in Track Info, then remove a row before the playing occurrence
   and a row after it. Audio and the inspected selection must remain unchanged;
   KDE's media widget must not report a track switch. Previous must select the
   other duplicate; removing the current occurrence must select its successor.
4. Check Shuffle, Repeat Queue/One and history with native controls and KDE media
   keys/widget. Optional tools: `playerctl -p io.github.tracerxbrhd.Karin status`,
   `playerctl -p io.github.tracerxbrhd.Karin shuffle On`, and
   `playerctl -p io.github.tracerxbrhd.Karin loop Track`.
5. Clear metadata/artwork cache in Preferences while playing. Audio, queue and
   live session must survive. Disable/re-enable caching, restart normally and
   check restoration. Cached metadata never establishes a valid login.
6. Check both themes, ordinary/HiDPI sizing and pinned Info/Queue for obvious UI
   regressions. Record first Player Bar appearance separately from later reveals.
   Cloud cold gaps remain unresolved; this checklist does not presume a fix.
7. Confirm logout cancellation is harmless; confirm deliberate logout clears
   personal views, queue and system metadata. Repeat the relevant steps in the
   current installed Flatpak without adding filesystem or session-bus grants.

Report only revision, native/Flatpak environment and PASS/FAIL/not-tested per
step. Unreported individual/backend cases remain unconfirmed; storage failure,
concurrency and rejected-record behavior also use synthetic automated tests.
My Wave needs its separate opt-in acceptance below.


## My Wave local acceptance

The owner has heard real Wave audio and confirmed accepted session, feedback and
continuation. A later feedback HTTP 400 remains unresolved: the old log omitted
its event. Keep the labelled preview opt-in until the repeat below succeeds.
Quit any running Karin instance so an older binary cannot handle
the launch, build this PR revision, and use the normal desktop session:

```sh
cargo +1.98.1 build --release --locked
./target/release/karin --experimental-wave
```

For a locally built/installed Flatpak from the same revision, use
`flatpak run io.github.tracerxbrhd.Karin --experimental-wave`. This needs no new
permissions; source preparation alone does not prove an installed Flatpak run.
Use normal protected restoration or ordinary **Войти**. No token, cookie,
Client ID/secret, alternative account identity or tracing option is required.
The flag is not saved; quit and launch without it to return to ordinary Home.

1. Open **Главная → Моя волна → Слушать**.
   Require an actual admitted session and normalized real tracks; a loading
   screen or visible title alone is insufficient.
2. Hear a real entitled track through PipeWire. Check Hero **Пауза/Продолжить**,
   return to Home without restarting the session, and change Light/Dark/System
   while audio continues. Check Seek, Volume
   and KDE media controls against the same Player Bar/current track. Cloud's
   generated-media tests cannot substitute for heard Music.
3. Use Next, and Previous within retained history. Continue deliberately toward
   the admitted batch's end until a further batch is accepted. Shuffle/Repeat
   are off in Wave; no old session/history is reconstructed beyond the bound.
   Include a natural track ending, not just skips.
4. Confirm the experiment's safe `Karin wave:` terminal lines: `stage=session` and
   `stage=normalization` with `category=accepted`, feedback with
   `stage=feedback category=accepted http=200` and a fixed `event`, and a later
   `stage=tracks category=accepted`. Require feedback acknowledgment for
   `trackStarted` and a terminal `skip` or `trackFinished`, not only
   `radioStarted`. `count`/`omitted` are bounded track counts, not identifiers.
   Hearing alone does not prove feedback or continuation.
5. Play a track from **Плейлисты** or **Нравится**: Wave leaves automatically,
   ordinary playback continues and previous Shuffle/Repeat return. There is no
   Finish control; **Отмена** appears only during startup. In another deliberate
   Wave session test KDE/MPRIS Stop and its safe feedback result. Test logout/close during
   pending work: personal views and queue clear, late results cannot revive
   them, and no queued feedback starts with the retired session. An already
   dispatched request cannot be recalled. Restart without the flag: no radio
   session should resume automatically.

If feedback fails, share only its safe `Karin wave:` line containing fixed
`stage/category/http/event/phase/origin/finish` classifications and the preceding
action (for example Next, natural end, source change or system Stop).
For other failures report fixed stage/category/HTTP and PASS/FAIL/not-tested.
No event is inferred from a neighboring success line. An unknown
POST outcome is never automatically replayed; **Начать новую волну** is a new
session, not confirmation or replay of that event. Do not share full terminal
output, HTTP/WebKit/GStreamer traces, titles/IDs, session/batch markers, URLs,
credentials, screenshots of a personal library or audio recordings.

Record revision, native/Flatpak environment, heard sound, feedback acceptance,
further batch, transport/MPRIS and cleanup separately. Until this whole chain is
confirmed again without the rejected sequence, My Wave remains opt-in and
Issue #16 stays open. Public-distribution Issue #5 remains independent. See
[ADR-0027](adr/0027-experimental-wave-lifecycle.md).

## Theme input check

Karin applies Light/Dark/System through `AdwStyleManager`; it does not set
`gtk-application-prefer-dark-theme`. In a controlled comparison the unchanged
baseline binary emitted zero startup warnings with a clean profile, and one
reported warning with an externally seeded GTK4 legacy flag. Libadwaita checks it during initialization, before Karin applies
its saved appearance. KDE's published
[`kde-gtk-config`](https://github.com/KDE/kde-gtk-config/blob/c400d9b44fb2616ac8fe48c5e4cd91d83c103afe/kded/gtkconfig.cpp#L291)
can write this key, but the
writer/effective input on the owner's Fedora installation is not established.

If the warning persists, this optional read-only check prints only bounded fixed
origin labels and that one boolean, without paths or other configuration:

```sh
python3 scripts/check-gtk4-theme-key.py
```

The output reports file declarations, not effective precedence or which process
wrote them. It changes no preferences/files; do not delete GTK/KDE settings or
force a theme to hide the warning. Continue to verify all three appearance modes
and uninterrupted playback in the normal desktop session.
