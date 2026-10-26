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
and run Karin, its automated tests or its local player. The ordinary connection
probe is anonymous. Gate B1's separate opt-in OAuth diagnostic is intended only
for a consented local desktop experiment; it is not application sign-in or
Yandex Music stream playback.

## Local OAuth diagnostic (Gate B1)

Run this experiment **on the owner's local Fedora desktop**, from a trusted,
interactive terminal. Do not run it in Codex Cloud, CI, a remote support session,
a terminal recorder or a shared/logged console. Build the binary first:

```sh
cargo build --workspace --locked
ulimit -c 0
./target/debug/karin --oauth-diagnostic
```

The command takes no credential arguments and reads no credential environment
variables or configuration files. Supply only Karin's own **Client ID** when
prompted locally; input is hidden. Never supply a Client secret, account password,
OAuth access/refresh token or credentials of another application. The four scopes
requested are `music:api-public`, `music:major-public-api`, `login:info` and
`login:avatar`; they must be assigned to this exact user-authorization app.
Its registered test redirect must be exactly
`https://oauth.yandex.ru/verification_code`.

The tool creates a fresh cryptographically random verifier and state, uses S256,
and opens the authorization request through GIO's Linux default-browser handler.
It does not print the authorization URL or embed a browser. Check that Yandex's
consent screen names **Karin** and the expected permissions. Complete consent in
the system browser, then enter **only the seven-digit one-time authorization
code** displayed by Yandex into the local hidden prompt. A pasted token or implicit-flow URL is
not an alternative. Blank input, Escape, Ctrl-C or Ctrl-D cancels; each hidden
prompt has a five-minute input deadline. If the browser
fails to launch, rerun after correcting the desktop default browser rather than
copying credentials into another tool. An interactive controlling terminal is
required; stdin, including any pipe or redirection, is never read for credentials.
Without a controlling terminal it refuses before browser/network activity. Handled
cancellation restores terminal settings. External termination/suspension can
interrupt cleanup; recover the terminal with `stty sane` if needed.

The manual verification page does not document a returned `state` value in its
code-only presentation. The tool sends state, but cannot claim to have validated
a returned state from that single code. It explicitly reports this limitation;
S256 PKCE binds the exchanged code to this process's verifier. This experiment
must not be promoted into a production callback/login flow. A production
redirect must return and validate state and needs its own provider-supported
Linux callback design. Code expiry and failed/denied consent require a fresh
run, not a different OAuth identity or bypass.

After a secretless initial code exchange, the tool performs these separate checks:

1. `GET https://login.yandex.ru/info?format=json`: require the returned `client_id`
   to match the locally entered Client ID and observe whether identity is present.
   Profile fields and avatar are neither displayed nor fetched separately.
2. `GET https://api.music.yandex.net/account/status`: observe authenticated-account
   presence and `serviceAvailable` independently of ID success.
3. `GET /search?text=Beethoven&type=track&page=0&nocorrect=true` on the same
   Music host, only after prior checks permit it. It reports HTTP status and a
   recognized result-object shape without printing catalog or account content.
   `serviceAvailable=false` remains visible; it does not prevent this read-only
   catalog diagnostic once an authenticated Music account is present.

Only sanitized HTTP statuses, boolean/unknown facts and fixed error categories
are printed. A missing, restricted or malformed result remains visible; the tool
does not retry with another client, region or endpoint. A token's optional `scope`
response can reveal a smaller grant, but its omission is not an independent scope
introspection result. No stream URL, audio bytes or protected content is requested.

Tokens are kept only in process memory until the diagnostic ends. Owned sensitive
buffers are zeroized on release; this does not guarantee erasure of transient
HTTP/TLS-library or operating-system copies. `ulimit -c 0` disables core dumps
for the invocation; it does not disable swapping or external process inspection.
There is no
refresh, token file, keyring write, automatic account login or GUI session.
Secret Service presence detection alone is not a verified credential lifecycle.
Closing the process drops local token state; it does not revoke Yandex's server
consent. If appropriate, remove the grant through the account's own Yandex
application-permission settings. The browser has its own history/session storage;
do not publish its URL, screenshots, code page or account data.

A successful local run must distinguish code issuance, token issuance/client
binding, actual Music account access and catalog access. Registration alone or
Yandex ID success does not establish the latter two. Share only the sanitized
report, never codes, tokens, verifier, Client secret, private JSON or screenshots.
No real Fedora OAuth run has yet been recorded; see
[API feasibility](API_FEASIBILITY.md) and [validation](VALIDATION.md).

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
./target/debug/karin --playback-self-test
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a ./target/debug/karin --smoke-test
```

`--audio-self-test` checks a finite synthetic pipeline through `fakesink`.
`--playback-self-test` instead generates a temporary WAV and exercises the real
`playbin` controller, including transport, volume, seeking, EOS and cleanup.
Both run without music, physical audio output, accounts or network access.
The GTK smoke mode starts the actual application and checks its lifecycle and
responsive layout. CI runs the checks on Ubuntu 24.04 with its native development
libraries; a CI definition alone is not evidence that a remote run passed. Read
the executed results in [VALIDATION.md](VALIDATION.md).

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
./target/release/karin --playback-self-test
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' ./target/release/karin --smoke-test
```

The release command matches the Cargo command in the Flatpak manifest. Fetching
populates the local Cargo cache first; a generated Flatpak source snapshot instead
uses its vendored sources. Neither check is a full SDK/sandbox package build.
Keep `Cargo.toml`'s minimum version and the pinned CI lane aligned, and verify code
and locked dependencies before raising that minimum. Never bypass Cargo's
version check with `--ignore-rust-version`.

The user has confirmed the v0.0 functional checks on Fedora 44/KDE/Wayland/PipeWire,
and Issue #3 reports native and Flatpak prototype startup with diagnostic audio.
This is baseline evidence, not acceptance of Gate A's new player. Repeat manual
checks must cover real system-output playback, pause/resume, seeking, volume,
EOS/error recovery and closing during playback. Visual acceptance must check
window/title controls, resize/maximize behavior, HiDPI, keyboard/focus,
accessibility and both themes under [the UI quality gate](UI_QUALITY_GATE.md).
A headless check does not establish those properties on Fedora.

## Developer playback diagnostics

Open the developer playback section on the Diagnostics page. It creates a quiet
synthetic WAV in a temporary file and uses the same controller as the playback
self-test. Choose silent `fakesink` output for device-independent checks, or
explicitly choose system output when testing audibility on a desktop. Prepare
the source, then exercise play, pause/resume, stop, volume and seeking when the
source reports it is seekable. The controls do not select or resolve Yandex Music
content, and the temporary media is not shipped in the repository.

Inspect asynchronous state, position, duration and errors as well as listening.
`fakesink` success proves decoding and lifecycle behavior for the generated WAV;
it does not prove PipeWire output, support for other codecs or service streams.
The Music authentication blocker and next authorized action are documented in
[API_FEASIBILITY.md](API_FEASIBILITY.md).

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
It has no host filesystem permission or Secret Service access grant. The existing
network and audio permissions support the explicit diagnostics; Gate A adds no
sandbox grants. Generated media is created within the application's own temporary
directory, so the player requires no access to the user's music files.

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
cannot create the required user namespace inside the nested container. The user
reports that the v0.0 Flatpak prototype launches on Fedora; Gate A still needs a
fresh package/build/install and audio/UI acceptance there. Keep sandboxing enabled
and perform packaging acceptance on Fedora or suitable CI.

GNOME 50's Freedesktop SDK base is documented in the upstream
[gnome-build-meta junction](https://gitlab.gnome.org/GNOME/gnome-build-meta/-/blob/gnome-50/elements/freedesktop-sdk.bst).
