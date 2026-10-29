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
  gtk4-devel libadwaita-devel webkitgtk6.0-devel gstreamer1-devel gstreamer1-plugins-base-devel \
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
./target/debug/karin
```

No Yandex account, token, OAuth client secret or subscription is required to build
and run Karin, its automated tests or its local player. The ordinary connection
probe is anonymous. Gate B1's separate opt-in OAuth diagnostic is intended only
for a consented local desktop experiment; it is not application sign-in or
Yandex Music stream playback. The default sign-in provider is the Cassette-compatible
session prototype below; it requires no end-user OAuth configuration.

## Music sign-in and Fedora acceptance

<a id="experimental-music-session-and-fedora-acceptance"></a>

The default build now enables `cassette-auth` and WebKitGTK6. The normal
`Войти` action attempts real Yandex login using the published registration and
session grant used by Cassette. This is **not** Karin's own-registration PKCE
route, and it is not the previously blocked provider. No Client ID, Client secret,
app registration, token paste, device code or URL copy is required.

Use a trusted local Fedora desktop/terminal, not Cloud/CI, a terminal recorder,
remote support session or shared console. After installing the prerequisites
above, prepare one release build and run it:

```sh
rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy
cargo +1.98.1 build --release --locked
ulimit -c 0
./target/release/karin
```

For convenience, a successful matching MSRV CI run also uploads
`karin-fedora-prototype-1.98.1`, containing a native tar archive, `SHA256SUMS`,
build/source metadata, runtime instructions and licenses. Verify it with
`sha256sum -c SHA256SUMS` before extracting/running. This Ubuntu 24.04 x86_64
binary is a test artifact, not a Flatpak or production release; Fedora ABI/startup
is unverified. If unavailable or incompatible, use the canonical local source
build above. Select the matching successful PR revision, not an older artifact.

Run **without** `--ui-fixture`, `--smoke-test` or the separate
`--oauth-diagnostic`; those are not the real sign-in journey. No secrets belong
in command arguments, environment variables, files, chat or GitHub. The consent
page may name the upstream shared OAuth registration used by Cassette rather than
Karin's own registration. Inspect the actual Yandex origin shown in the dialog
and grant access only to your own account if you choose to proceed. Code licensing
does not establish service/identity distribution rights; see
[the scoped prototype decision](adr/0011-cassette-session-auth-prototype.md).

Perform this one substantive acceptance run on Fedora 44/KDE/Wayland/PipeWire:

1. Click `Войти`. Confirm the native dialog loads a real Yandex page and shows only
   the trusted origin, without exposing full URLs or developer fields. Sign in to
   your own normal account inside that page. Karin never reads its password form.
2. After consent, the dialog must move automatically through return/state,
   own-session cookie collection and OAuth exchange to **Music account validation**.
   Do not copy any code, token or URL. A Yandex login or issued token alone is not
   success. The dialog must close as authenticated only after Music validation.
3. Verify your actual liked tracks and personal playlists load; choose a playlist
   and verify its contents. Search for a familiar track, album and artist; inspect
   the corresponding returned results and details. Empty libraries are legitimate
   only if they match your account. Add metadata to the queue and confirm ordering;
   selecting it does not yet resolve or play a Music stream.
4. Sign out. Check that account/library/queue state and prior errors clear. Start
   another login, cancel it, retry and close the window; delayed results must not
   sign you back in. Restart should be signed out because sessions are memory-only.
5. Check approximately 420×720, 900×650, 1440×900 and maximized, both themes, HiDPI,
   HeaderBar/window controls, scrolling, keyboard focus/activation and Ctrl+Q.
   Stale floating Music 403 text must not remain; errors belong in native rows.
6. If a stage fails, share only the revision, `cassette-auth` build, failed stage,
   safe fixed category and HTTP status when supplied. Examples are trusted-return/
   state mismatch, missing session cookie, cookie-read failure, `/token` refusal
   or unsupported response, Music account 403, or library/search schema rejection.
   Do not publish cookies, tokens, full URLs, raw JSON/headers, account identifiers,
   private library screens or login screenshots. Do not change identity, region or
   OAuth mechanism during this acceptance run to hide a failed result.

Live sign-in, token exchange and actual library/search remain **pending until
this run succeeds**. Source review and cloud fixtures cannot prove them. The
selected return is exact `https://music.yandex.ru/` with matching fresh state;
if Yandex returns a different origin/path or omits state, the tool must report that
specific return-validation failure rather than accept a substring or fake success.
Cookie collection uses only this dialog's ephemeral session and the fixed official
OAuth token URI, not another browser/application's session. Cookies and tokens are
memory-only; cancellation clears browser data and stale work. Local logout does
not revoke provider consent; persistence, refresh, remote revocation and verified
Secret Service writes are not implemented. Music playback is a separate gate.

## WebKit authentication build and local fixture checks

WebKitGTK6 is a default prototype dependency, confined to the sign-in dialog.
`webkit6` 0.5 matches GTK Rust 0.10; 0.6 targets GTK Rust 0.11 and is not selected.
Fedora needs `webkitgtk6.0-devel` (included above); Debian/Ubuntu needs:

```sh
sudo apt-get install libwebkitgtk-6.0-dev
```

`cassette-auth` enables `auth-webkit`; `--all-features` additionally includes the
library-only legacy Device Flow experiment without exposing it in the GUI.
`cargo build --no-default-features` is an intentional compatibility build without
the real provider, not the artifact for Fedora authentication acceptance.
GNOME 50 already supplies WebKitGTK6; the existing Flatpak manifest enables
`cassette-auth` without changing runtime/SDK versions or permissions.

Display-dependent WebKit tests are ignored by the ordinary test command. Run each
in a separate display/D-Bus process with the WebKit sandbox enabled. They use only
synthetic/local pages and credentials, never a Yandex account. The full test needs
Openbox and `wmctrl` with a 1920×1080 Xvfb screen for actual maximization; optional
captures also need `xdotool` and ImageMagick's `import`. Install these test tools
separately from normal application runtime prerequisites.

```sh
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- xvfb-run -a \
  cargo test --lib --locked --all-features \
  webkit_auth_construction_and_shutdown_without_navigation -- --ignored --test-threads=1
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- \
  xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' sh -eu <<'SH'
openbox --sm-disable >/tmp/karin-webkit-openbox.log 2>&1 &
wm_pid=$!
trap 'kill "$wm_pid" 2>/dev/null || true; wait "$wm_pid" 2>/dev/null || true' EXIT
attempt=0
until wmctrl -m >/dev/null 2>&1; do
  attempt=$((attempt + 1))
  test "$attempt" -lt 50
  sleep 0.1
done
cargo test --lib --locked --all-features \
  webkit_auth_local_startup_cookie_isolation_and_cancel -- --ignored --test-threads=1
SH
```

`--smoke-test` and the capture helper's explicit `--ui-fixture` mode inject an
offline provider. They exercise dialog/layout/lifecycle without contacting Yandex;
they are not evidence of real sign-in. Never use `--ui-fixture` for the owner
acceptance above. Hosted sandbox checks and remaining namespace restrictions are
recorded in [VALIDATION.md](VALIDATION.md).

## Local OAuth diagnostic (Gate B1)

This existing developer diagnostic is unchanged and is never part of normal
GUI sign-in. Its local manual URL transfer does not configure the new automatic
Cassette session path or change its Music validation result.

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
the system browser. Once it reaches the registered verification page, copy the
**complete URL from the browser's address bar directly into Karin's hidden local
redirect prompt**. Keep it out of the shell, chat, support logs and files: it
contains the one-time code and returned state. The diagnostic expects the exact
`https://oauth.yandex.ru/verification_code` origin/path, extracts the opaque code
and validates `state` against this attempt before exchanging it. The observed
provider-added `cid` is accepted without assigning it a meaning. Missing or
duplicate response fields, unexpected parameters, fragments, wrong origin/path
and invalid state are rejected locally. A code-only paste, OAuth token or
implicit-flow URL is not an alternative. Blank input, Escape, Ctrl-C or Ctrl-D
cancels; each hidden prompt has a five-minute input deadline. If the browser
fails to launch, rerun after correcting the desktop default browser rather than
copying credentials into another tool. An interactive controlling terminal is
required; stdin, including any pipe or redirection, is never read for credentials.
Without a controlling terminal it refuses before browser/network activity. Handled
cancellation restores terminal settings. External termination/suspension can
interrupt cleanup; recover the terminal with `stty sane` if needed.

The owner observed an alphanumeric code and echoed `state` in this browser
redirect on Fedora. Authorization codes are bounded, nonempty opaque values;
there is no digit-count or numeric-format assumption, and the tool does not trim
or normalize a decoded code. The full pasted URL is limited to 4096 bytes and
the decoded code to 2048 bytes; control characters are rejected. Successful
returned-state validation is reported as `Returned state: ValidatedRedirect`.
S256 PKCE also binds the exchanged code to this process's verifier.

This is **manual URL transfer**, not an automatic callback listener, production
application sign-in or persistent account session. A production Linux callback
still needs a separately supported design and acceptance. The initial tool's
seven-digit validation rejected the owner's code before any token exchange;
discard that previous/exposed/expired code and start a fresh local run with the
corrected binary. Code expiry, state mismatch and failed/denied consent require
a new flow, not a different OAuth identity or bypass.

After a secretless initial code exchange, the tool performs these separate checks:

1. `GET https://login.yandex.ru/info?format=json`: require the returned `client_id`
   to match the locally entered Client ID and observe whether identity is present.
   Profile fields and avatar are neither displayed nor fetched separately.
2. `GET https://api.music.yandex.net/account/status`: observe authenticated-account
   presence and `serviceAvailable` independently of ID success.
3. `GET https://api.music.yandex.net/account/about`: make one independent read-only
   request with the same Karin token after successful ID-client/account checks,
   even if status returned HTTP 403, failed on the network or had an unsupported
   response. Report its HTTP status and fixed error category separately as
   `Music /account/about`. HTTP 200 is accepted only for a bounded JSON object
   containing an object `result`, without a top-level `error`; no personal fields
   are retained or interpreted. This is a minimal diagnostic envelope check,
   not a verified about schema or evidence of an authenticated Music account.
4. `GET /search?text=Beethoven&type=track&page=0&nocorrect=true` on the same
   Music host, only after status establishes an authenticated Music account.
   About success cannot unlock the catalog after failed status; about failure
   does not suppress the catalog after valid status. The query reports HTTP
   status and a recognized result-object shape without printing catalog or
   account content. `serviceAvailable=false` remains visible; it does not
   prevent this read-only catalog diagnostic once an account is present.

The status line is labeled `Music /account/status`. Neither Music method runs
if ID binding/account checks fail. Each method keeps its own outcome; about never
changes the account/service facts obtained from status. The overall diagnostic
passes only when every planned stage, including about, succeeds. Even that result
would not establish production sign-in or stream rights.

Only sanitized HTTP statuses, boolean/unknown facts and fixed error categories
are printed. A missing, restricted or malformed result remains visible; the tool
does not retry with another client, region or endpoint. A token's optional `scope`
response can reveal a smaller grant, but its omission is not an independent scope
introspection result. No stream URL, audio bytes or protected content is requested.

For Music HTTP 403, only the exact allowlisted identifier `missing-required-scopes`
in a top-level string `error` or nested `error.name` selects the fixed Music
scope-error category. The tool does not classify free-form messages such as
`API access restricted`, even when they resemble a known condition. Unknown or
malformed responses remain `Forbidden`;
the tool never prints raw JSON, error messages/descriptions, personal fields,
request IDs, headers or URLs. Forbidden Music status leaves account presence and
service availability `unknown` and skips the catalog query, while the independent
about probe still runs. Both Music methods and the catalog share this safe error
classification. Classification uses each response already received and adds no
retry or fallback request; about is a separately planned probe, not a way around
status refusal. Both methods retain the five-second connect limit, fifteen-second
total deadline and 1 MiB body ceiling; redirects and retries remain disabled.

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
The owner has since confirmed fresh state validation, token HTTP 200 and Yandex
ID HTTP 200 with a matching Karin client on Fedora. Music status still returned
HTTP 403 in the DIRECT-path repeat; a separate anonymous curl control returned
HTTP 200. These are distinct request contexts, not proof of identical egress or
Music authorization. The owner subsequently observed generic HTTP 403 for both
status and about with TUN on and off. The actual cause remains unknown; share
only the separate sanitized results, never bodies or credentials.
Do not repeat OAuth solely to recheck VPN, change routes, region or application
identity, or publish raw responses. Safe classification
does not bypass the denial. See
[API feasibility](API_FEASIBILITY.md) and [validation](VALIDATION.md).

## Codex Cloud / Debian 13

The prepared cloud environment retains its setup assets outside the repository
at `/workspace/karin-environment`. The Rust toolchain lives in its `cargo/` and
`rustup/` directories; native dependencies live in a Docker image. Use the wrapper
so Cargo sees GTK and GStreamer:

```sh
/workspace/karin-environment/run-auth cargo build --workspace --locked
/workspace/karin-environment/run-auth cargo test --workspace --locked
/workspace/karin-environment/run-auth sh
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
/workspace/karin-environment/run-auth rustup toolchain install 1.98.1 --profile minimal \
  --component rustfmt --component clippy
/workspace/karin-environment/run-auth cargo +1.98.1 build --workspace --locked
/workspace/karin-environment/run-auth env RUSTUP_TOOLCHAIN=1.98.1 \
  python3 packaging/flatpak/prepare-source.py
```

`RUSTUP_TOOLCHAIN` also selects the Cargo subprocesses used by source preparation
and the dependency-license inventory.

## Verification

Run from the repository root (through the wrapper in the prepared cloud).
Default and all-feature checks require WebKitGTK6 as described above:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --all-features --locked
cargo build --workspace --locked --all-features
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
cargo +1.98.1 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.98.1 test --workspace --locked
cargo +1.98.1 test --workspace --all-features --locked
cargo +1.98.1 build --workspace --all-features --locked
cargo +1.98.1 build --workspace --locked
cargo +1.98.1 fetch --locked
cargo +1.98.1 build --release --offline --locked --no-default-features --features cassette-auth
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
The Music authentication evidence and pending local acceptance are documented in
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
network and audio permissions support diagnostics and future authorized reads;
the music slice adds no sandbox grants. The manifest explicitly passes
`--no-default-features --features cassette-auth`, including WebKit session auth
and excluding the legacy library-only Device Flow module from this artifact. Generated media is created within the
application's own temporary directory, so the player requires no access to the
user's music files.

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
`--offline --locked --no-default-features --features cassette-auth`, with no
dependency downloads inside the Flatpak build. The GNOME 50 runtime supplies the
native WebKit engine.
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
