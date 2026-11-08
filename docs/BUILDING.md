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
After validation, **Главная → Моя волна → Слушать** starts personal radio in the
shared player. Startup, restored sign-in and opening Home do not start Wave.
The old `--experimental-wave` argument is a deprecated no-op compatibility alias
for earlier launch commands; neither native nor Flatpak use needs it.

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

<a id="synthetic-audio-cache-checks"></a>

## Unified cache checks and private HTTP policy

The production stores share one coordinator, enabled by default with a 2 GiB
allowance. Preferences → **Кеширование** offers exactly 500 МБ (500 MiB),
1/2/3/4 ГБ (GiB), and **Без ограничений**, one measured total and **Очистить кеш**.
Mouse and keyboard changes snap to those six choices. Settings/trim coalesce
for 200 ms; Disable and Clear fence old work immediately. Clear preserves the
live account, queue and already admitted playback.

```sh
cargo +1.98.1 test --lib --all-features --locked audio_cache
cargo +stable test --lib --locked audio_cache
cargo +1.98.1 test --lib --all-features --locked cache_budget
cargo +stable test --lib --locked settings
```

Tests use private isolated directories, original generated audio and local HTTP
responses, not your application cache or account. They exercise the ordinary
private HTTP policy and shared store/player, including validators, changed
representations, refusals and restart. This does not establish live Music results.
The disk guard requires free space for the reserved write plus 1% of filesystem
capacity, clamped to 64–512 MiB; tests must have that headroom too. Unlimited
removes the user byte quota, not this guard or format/work bounds.

Existing v1 settings migrate without resetting sign-in: the two old default
256 MiB limits become 2 GiB; custom valid limits are summed and rounded up to
the next offered choice, with sums above 4 GiB becoming Unlimited. Corrupt cache
values recover independently of valid session/forget/appearance fields. Existing
records without usable HTTP validation metadata are refreshed online rather
than trusted as private HTTP hits.

A resource is stored only when its actual supported MP3 response allows it.
Every replay obtains fresh account/resource access and verifies the retained
representation. `no-store` and explicit restrictions decline storage; absent or
unusable version evidence declines local reuse. Storage failure leaves admitted
network playback available. HTTP cache admission is not a provider entitlement,
legal license, Offline Mode or permission to distribute content. See
[AUDIO_CACHE_REVIEW](AUDIO_CACHE_REVIEW.md) and
[ADR-0030](adr/0030-unified-cache-budget-and-preferences.md).

## PR #26 corrected-cache acceptance

**The real service has not been verified for this revision.** After the final
Cloud checks, perform this single sequence on the exact PR HEAD in Native and
installed Flatpak. Local HTTP fixtures establish application behavior; only this
local service check can establish what a real CDN resource supports.

Build Native as described above:

```sh
cargo +1.98.1 build --release --locked --no-default-features --features cassette-auth
ulimit -c 0
KARIN_AUDIO_CACHE_DIAGNOSTICS=1 ./target/release/karin 2>&1 |
  grep --line-buffered -E '^cache=(miss|write-confirmed|hit|stale|not-cacheable) source=(local|remote) reason=[a-z0-9-]+$'
```

Build/install the same source with the Flatpak instructions below, then run:

```sh
ulimit -c 0
flatpak run --env=KARIN_AUDIO_CACHE_DIAGNOSTICS=1 io.github.tracerxbrhd.Karin 2>&1 |
  grep --line-buffered -E '^cache=(miss|write-confirmed|hit|stale|not-cacheable) source=(local|remote) reason=[a-z0-9-]+$'
```

The opt-in diagnostic reports only fixed `cache`, `source` and `reason`
categories. It does not enable HTTP, WebKit or GStreamer tracing. Keep those
traces off; do not share URLs, tokens, IDs, HTTP bodies, music files or personal
screenshots. No developer credentials or altered client identity are required.

1. Use ordinary sign-in or protected restoration. Keep caching enabled and choose
   one ordinary track you can play. Confirm audible sound and normal
   Pause/Resume/Seek. First playback should select `source=remote`; eligible
   complete responses can produce `cache=write-confirmed` without a second full
   media download. Wait for that confirmation before checking a replay.
2. Select the same track again. A verified saved representation must report
   `cache=hit source=local`; hearing audio alone or seeing larger total usage is
   insufficient proof. If the resource reports `stale` or `not-cacheable`, retain
   only its fixed reason: the app must continue permitted online playback without
   guessing identity or overriding a response restriction.
3. Close normally without logging out, reopen with the same diagnostic command
   and play that track again. Protected sign-in and fresh access/version checks
   must precede any local hit. A renewed signature alone must neither prove a new
   audio version nor authorize reuse without a valid resource binding/validator.
4. While audio is playing, use Preferences → **Очистить кеш**. Existing playback
   and sign-in must remain active; later play starts from a cache miss. Check
   all six limit choices, one measured total, persisted choice and any reported
   Retry. Exercise Next/Previous, queue, Wave and media keys/MPRIS briefly.
5. Report the exact revision, Native/installed-Flatpak installation and the fixed
   category lines with PASS/FAIL/not-assessed for save, replay, restart and Clear.
   A CDN without a safe validator is an explicit per-resource limitation, not a
   confirmed cache hit. Do not weaken checks or send private data to diagnose it.

No offline behavior is promised. Public client distribution remains a separate
question in #5/#22. This procedure is one final service acceptance, not a request
to repeat a local test after each internal change.

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
PipeWire desktop audio uses the existing PulseAudio-compatible socket; Wave
adds no filesystem, session-bus or other sandbox permission. Source preparation
and offline Cargo checks are not a complete installed GNOME 50 Flatpak test.
The owner has accepted the ordinary Native and installed Flatpak baseline;
use the procedure below for regression checks of a changed revision.

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
Use the focused My Wave Native/Flatpak acceptance below for the v0.5 integration.


## My Wave local acceptance

The owner retested corrected candidate `2e4b978` on Fedora without reporting new
problems, following earlier real audio, four-event feedback and continuation.
The owner later accepted ordinary Native and installed Flatpak use; Issue #16
is closed. The sequence below remains a revision-specific regression procedure.
Quit any running Karin instance so an older binary cannot handle activation.
From the current PR checkout, build and run as the normal desktop user:

```sh
cargo +1.98.1 build --release --locked
./target/release/karin
```

1. Start without arguments while signed out, then use ordinary **Войти** or
   restore a permitted saved login. Home must offer **Слушать** after Music
   validation, with no Wave requests or audio before that explicit action.
2. Press **Слушать** and hear real Music through PipeWire. Check Pause/Resume,
   Next, natural EOS and a further batch. Browse other tabs and return Home:
   the shared Player Bar, Info/Queue and same radio session must continue.
   Confirm accepted `radioStarted`, `trackStarted`, `skip` and `trackFinished`;
   a title, HTTP 200 alone or Cloud generated media does not prove heard Music.
3. Select an ordinary track and a playlist, including after Pause/EOS. The new
   music must play and Shuffle/Repeat return. Also test KDE/MPRIS Stop during
   Playing/Pause, then **Слушать** for a fresh Wave. Retirement sends no
   `radioFinished` or duplicate skip for an already finished cycle. A subsequent
   actually started track is a separate cycle. Cancel exists only during startup.
4. Check deliberate logout, application close and restart. Personal state and
   system metadata clear on logout; late work cannot revive it. Permitted saved
   sign-in may restore after normal close, but radio must never auto-resume.

For the installed Flatpak, first follow the SDK prerequisites in
[Flatpak source build](#flatpak-source-build), then prepare/install this same checkout:

```sh
RUSTUP_TOOLCHAIN=1.98.1 python3 packaging/flatpak/prepare-source.py
flatpak-builder --user --install --force-clean build/flatpak \
  packaging/flatpak/io.github.tracerxbrhd.Karin.json
flatpak run io.github.tracerxbrhd.Karin
```

Repeat the four steps inside that installed build, including protected sign-in
restoration through the available Secret Portal, actual sound, KDE controls and
exit/restart. Report unavailable storage or memory-only login explicitly. No
token input, extra permission or experimental flag is needed. Record native and
Flatpak results separately; prior v0.4 acceptance does not accept this package.

If feedback fails, share only its safe `Karin wave:` line containing fixed
`stage/category/http/event/phase/origin/finish` classifications and the preceding
action. Only `radioStarted`, `trackStarted`, `trackFinished` and `skip` belong to
the outgoing contract; a refusal of any of them remains an error. An unknown
POST outcome is never automatically replayed; **Начать новую волну** creates a
new session. Report other failures as fixed stage/category/HTTP and
PASS/FAIL/not-tested. Do not share full terminal output, HTTP/WebKit/GStreamer
traces, IDs, session/batch markers, URLs, credentials, personal screenshots or recordings.

Record revision and PASS/FAIL/not-tested per step, including environment,
heard sound, feedback, transport, restoration and cleanup. Issue #16 stays open
until final ordinary-native and installed-Flatpak acceptance. Public-distribution
Issue #5 remains independent. See [ADR-0027](adr/0027-experimental-wave-lifecycle.md).

## Track reaction acceptance

Use the current Dislike PR's native build with ordinary `./target/release/karin`,
then repeat in its installed Flatpak using the [source build](#flatpak-source-build).
Quit an older running instance before testing. Sign in normally; no diagnostic
credentials or account data are needed in terminal arguments.

1. Play a real track whose reaction you consent to change. Wait for the current
   Like/Dislike reads. Dislike must appear only after a valid account-bound
   result; a refused/unknown response must not look like an unmarked track.
   A Dislike-read failure must preserve Like/Unlike if a fresh independent Like
   read succeeds, without claiming a Dislike state. Record the initial known
   marks locally, without sharing the track or account ID.
2. With an ordinary queue containing a next item, activate **«Не нравится»** by
   mouse, then exercise its keyboard focus on another deliberate change. Pending
   feedback must be visible. Only independent confirmation of Dislike and Like
   permits one Next; the queue context is preserved and the new current track,
   footer and KDE/MPRIS agree. Adding Dislike is expected to clear Like. There
   must be no optimistic advance, duplicate write or Track Info opening.
3. Revisit the track, or restart and select it again, to read the state anew.
   Remove Dislike: it clears without adding Like or advancing. Add/remove Like
   explicitly and confirm it separately without advancing; adding Like is
   expected to clear Dislike. Restore original marks only through deliberate,
   independently confirmed actions.
4. Check compact/medium/wide layouts, both themes, scale 1/2, Tab/Enter/Space and
   independent metadata Info/reaction targets. Change track during a pending
   reaction, including to another occurrence of the same Track ID: a late reply
   must not advance the new occurrence or apply one track's state to another.
   Logout and cancellation
   retire playback effects. Synthetic fixtures cover forced timing races; do
   not manipulate a real account or network to manufacture them.
5. During ordinary Wave, add Dislike to a consenting current track. Confirmation
   invokes the existing Next and applicable listening Skip once; it must not
   create another radio session or a new negative Wave event. Removal/Like and
   refused or ambiguous writes do not advance. Transport, feedback, Info/Queue
   and KDE/MPRIS keep working. No immediate recommendation effect is claimed.

If a change is rejected, ambiguous or unconfirmed, use the UI's state refresh
(GET only). Do not repeatedly click to replay an uncertain POST. The error must
remain understandable and audio must remain controllable. Authorization loss
still follows normal session teardown. Synthetic tests cover forced timeout,
schema/conflict and concurrent-account cases; do not manufacture those failures
with a real account.

Report revision, Native/installed-Flatpak environment, initial/readback agreement
and PASS/FAIL/not-tested per step. Share only fixed operation/category/HTTP values
if needed, never tokens, identifiers, responses, headers, URLs, personal screenshots
or recordings. A published helper or Cloud fixture is not live confirmation.
The owner confirmed real server reactions and their interaction at `1877a6c`.
That report does not establish the revised automatic Next or installed Flatpak
acceptance. Record these separately in
[Issue #17](https://github.com/tracerxbrhd/karin/issues/17); service/public-distribution
Issue #5 remains separate.

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
