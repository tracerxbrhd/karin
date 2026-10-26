<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin validation evidence

This report separates initial cloud checks, user-reported Fedora acceptance,
revised UI checks and compiler compatibility. A result for one environment or
revision is not evidence that another has passed.

## v0.1 Gate B1 (2026-10-26)

The owner-confirmed Karin user-authorization registration and four selected
scopes are recorded in [Issue #5](https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6411056234).
This is registration/consent-preview evidence supplied by the owner. Registration
alone is not a token exchange, ID-client binding result, Music authenticated
status or catalog response. The subsequent browser observation below is a
separate evidence stage.

Gate B1 adds a separate opt-in local OAuth diagnostic. Automated checks use only
local HTTP fixtures with synthetic credentials; no real Client ID, authorization
code, verifier, access/refresh token, private account JSON or OAuth session was
supplied to Codex Cloud, CI or GitHub. No real authorization or token endpoint was
exercised by the cloud/automated checks. The existing
GUI login capability remains unavailable and the playback engine is independent.

### Owner-observed Fedora browser flow and redirect correction

On 2026-10-26 the owner [reported in PR #6](https://github.com/tracerxbrhd/karin/pull/6#issuecomment-6417990070)
that the actual Yandex browser flow displayed Karin's consent screen and reached
the registered verification page with `state`, `code` and an additional `cid`.
The code was opaque alphanumeric text, longer than seven digits. The initial
diagnostic rejected it as invalid local input; **no token POST occurred**.
This is a Karin input-validation failure, not an OAuth-provider or Music API
rejection. No private screenshot, URL, Client ID, code or state is published.

The correction accepts a full browser redirect URL through hidden local input,
validates origin/path and returned state before exchange, and treats the code
as bounded opaque text. The observed `cid` is uninterpreted. There is no code-only
fallback. A valid transfer reports `Returned state: ValidatedRedirect`; this is
manual URL transfer, not an automatic production callback.

### Executed redirect-correction checks

The corrected parser and hidden full-URL path were checked on 2026-10-26 in the
same prepared Debian 13 environment (GTK 4.18.6, Libadwaita 1.7.6, GStreamer
1.26.2). Rust/native dependency versions, SDK selection and Flatpak permissions
are unchanged. These results supersede the initial 57-test matrix below.

| Check | Stable 1.99.1 | Rust 1.98.1 |
| --- | --- | --- |
| Formatting and all-target Clippy with `-D warnings` | Passed, no warnings | Passed, no warnings |
| All workspace tests | **61 passed, 0 failed, 0 ignored** (57 library + 4 binary) | **61 passed, 0 failed, 0 ignored** (57 library + 4 binary) |
| Locked debug build | Passed | Passed |
| Hidden terminal checks | 14 PTY cases and missing-TTY rejection passed | 14 PTY cases and missing-TTY rejection passed |
| Finite and generated-WAV GStreamer diagnostics | Passed through silent `fakesink` paths | Passed through silent `fakesink` paths |
| GTK smoke | Passed: both themes/pages, resizing, actual local playback and window/worker cleanup | Passed: both themes/pages, resizing, actual local playback and window/worker cleanup |

Protocol fixtures exercise opaque alphanumeric codes and safely encoded reserved
characters, successful full-URL/state validation, absent/mismatched/duplicate
code/state, extra response parameters including the tolerated `cid`, expiry,
provider denial and secretless PKCE exchange. A valid synthetic return reports
`Returned state: ValidatedRedirect`. All endpoints remain loopback, including
unexpected fallback paths. The oversized-response fixture tolerates the expected
client disconnect while still requiring the exact `ResponseTooLarge` result.

The terminal suite adds hidden redirect-prompt checks for wrong/missing state,
wrong origin, code-only paste rejection, cancellation, control data and overflow.
It verifies no echo, exact terminal restoration and no HTTP request. Its harmless
local URI handler and loopback proxy trap prevent real browser/provider access
even if a validation regression occurs.

The Rust 1.98.1 offline optimized release build passed with `--offline --locked`.
The optimized binary passed the same terminal suite, both silent audio
diagnostics and GTK smoke. The license inventory was regenerated and
byte-compared: **200 external crates**, **171** in the Linux target graph.
An initial headless GTK allocation check returned zero width; a focused repeat
and the final complete matrix passed with unchanged UI sources.

Hosted GitHub Actions for the correction remain pending until the updated
revision runs. Token exchange and Music API acceptance remain pending the next
fresh local Fedora flow; none of these cloud fixtures supplies that evidence.

### Initial Gate B1 cloud checks (superseded revision `9f6a914`)

The following initial Gate B1 checks passed on 2026-10-26 in the prepared Debian 13
x86_64 environment, with GTK 4.18.6, Libadwaita 1.7.6 and GStreamer 1.26.2.
The native library baselines, GNOME 50 / Freedesktop 25.08 SDK selection and
Flatpak permissions are unchanged. Compiler checks used both current stable
**Rust/Cargo 1.99.1** and exact minimum compatibility toolchain **1.98.1**.

| Check | Stable 1.99.1 | Rust 1.98.1 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | Passed | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed, no warnings | Passed, no warnings |
| `cargo test --workspace --locked` | **57 passed, 0 failed, 0 ignored** (54 library + 3 binary) | **57 passed, 0 failed, 0 ignored** (54 library + 3 binary) |
| `cargo build --workspace --locked` | Passed | Passed |
| `python3 scripts/test-oauth-terminal.py` | Seven terminal-input cases and missing-controlling-TTY rejection passed | Seven terminal-input cases and missing-controlling-TTY rejection passed |
| `--audio-self-test` | Passed, silent finite `fakesink` pipeline | Passed, silent finite `fakesink` pipeline |
| `--playback-self-test` | Passed, real generated-WAV `playbin`/`fakesink` lifecycle | Passed, real generated-WAV `playbin`/`fakesink` lifecycle |
| GTK smoke under Xvfb | Passed: both pages/themes, resize, actual local playback and window/worker cleanup | Passed: both pages/themes, resize, actual local playback and window/worker cleanup |

OAuth tests cover the RFC 7636 S256 vector; fresh verifier/state; secretless form
exchange; verifier mismatch, missing/mismatched returned state and provider denial;
local/code/token expiry; missing scopes; exact ID client binding; anonymous Music
accounts; and a sequential read-only catalog probe. Fixture requests also verify
that redirects are not followed, credentials are not forwarded, automatic retries
are disabled, errors are redacted, malformed/oversized bodies fail, and the total
deadline stops continuously streaming responses. These are loopback fixtures,
not observations of Yandex accepting Karin credentials.

Terminal tests exercise Ctrl-C, Ctrl-D, Escape, empty input, invalid control data,
bounded overflow and invalid Client ID, checking no echo, exact terminal-setting
restoration and sanitized outcomes. A separate missing-TTY case exits before
credential input. These checks use synthetic invalid input and stop before browser
launch or provider requests. Their isolated XDG configuration installs a harmless
local URI handler, so even a parser regression cannot open a real browser. An
independent offline GIO check verified that guard without contacting a provider.

The regenerated dependency-license inventory was byte-compared with the updated
reviewed inventory: **200 external crates**, **171** in the Linux target graph.
This is dependency evidence, not a final-package license or service-access grant.
The GUI layout and playback implementation are unchanged; existing Gate A visual
evidence remains scoped to that layout, with Fedora manual acceptance separate.

Rust 1.98.1 `cargo build --release --offline --locked` passed after fetching the
locked dependencies. The optimized binary also passed the terminal checks, both
silent GStreamer diagnostics and GTK smoke. Hosted GitHub Actions outcomes must
be read from this PR's checks; a workflow definition alone is not a passed run.
Flatpak source preparation passed with the same Rust sources, lockfile and
terminal script, the reviewed inventory and the new dependencies' original
notices. The manifest still selects GNOME 50 and the existing Rust extension.
Full Flatpak SDK/sandbox
packaging remains a separate Fedora check because the cloud container denies
Bubblewrap user namespaces; no bypass is used.

### Pending local Fedora acceptance

Run `karin --oauth-diagnostic` only on the consenting owner's Fedora desktop with
Karin's own Client ID. Record only the tool's sanitized result. Independently
verify code issuance; initial secretless S256 token exchange; matching ID
`client_id` and identity presence; effective-scope metadata; Music account
identity/service availability; and the one read-only catalog result. Do not mark
Music authorization working from ID success, selected scope names, fixtures or
an HTTP 200 without authenticated Music account evidence. Do not publish browser
screenshots, raw responses or credentials. No streams, DRM bypass or subscription
playback test is part of Gate B1.

Discard the previous/exposed/expired code and begin a new browser flow with the
corrected binary. Paste the full returned URL only into its hidden local prompt
and check the sanitized `ValidatedRedirect` outcome. The earlier code-only
limitation is superseded by this full-URL transfer; an automatic production
callback still requires separate acceptance. Public-client refresh/revocation,
Secret Service lifecycle and GUI sign-in remain future acceptance gates. The
executed cloud checks above do not establish these local/provider results.
Historical Gate A results do not imply a Gate B1 OAuth pass.

## Historical v0.1 Gate A (2026-10-25)

Gate A was developed on `feature/v0.1-auth-playback-feasibility` from current
`master` (`8b060aa`, merged PR #2). Issue #3 reports that the v0.0 native and
Flatpak prototypes launch on Fedora 44/KDE/Wayland/PipeWire and diagnostic audio
works there. That user-reported baseline does not validate the new player,
controls, visual layout or Gate A Flatpak artifact.

### Music integration decision

The [official-document review and scoped live observations](API_FEASIBILITY.md)
record eleven official document GETs and their URLs/dates. General Yandex ID
authorization-code/S256 PKCE is documented, including a secretless initial code
exchange. Music-specific permissions, integration/distribution permission and
permitted stream delivery for Karin's own public Linux client remain
**Blocked / not established**. Public-client callback and token lifecycle details
also remain unverified. No client registration, account login, OAuth/device/token
request or service stream retrieval was performed.

The application exposes `AuthenticationUnavailable / MusicGrantNotEstablished`.
No ineffective sign-in button, token input or signed-in/catalog view is enabled.
The future resolver is a typed boundary with no live implementation or
constructible service stream. See [ADR-0006](adr/0006-music-authorization-feasibility.md).

Five anonymous rechecks from the same cloud egress on 2026-10-25 returned HTTP
200 for status (no identity, `serviceAvailable=false`) and HTTP 451 for the same
search/track/album/artist routes used in the baseline. No authorization, cookies,
redirect following, retry, alternate identity/region or bypass was used. These
responses do not determine the restriction's exact cause or authenticated/Fedora
metadata behavior. No account payloads, tokens, stream URLs or service audio are
published.

### Executed cloud checks

Debian 13 x86_64; Rust/Cargo **1.98.1**, Rustfmt and Clippy from that toolchain;
GTK **4.18.6**, Libadwaita **1.7.6**, GStreamer **1.26.2**. `Cargo.toml` remains at
minimum Rust 1.98; GNOME 50 / Freedesktop Rust extension 25.08 and Flatpak
permissions are unchanged. No dependency was added or updated; `Cargo.lock`
retains SHA256 `dea4fadcad8782fae898905c9993d48864e809f9293ba08d4d0ba6b89da43462`.

| Rust 1.98.1 check | Result |
| --- | --- |
| Formatting | `cargo fmt --all -- --check` passed |
| Clippy, workspace/all targets/locked | Passed with `-D warnings`, no warnings |
| Unit and local integration tests | **38 passed, 0 failed, 0 ignored**; binary/doc targets have 0 tests |
| Debug build | `cargo build --workspace --locked` passed |
| Finite GStreamer diagnostic | Passed: 40 synthetic buffers through `fakesink` |
| New `--playback-self-test` | Passed through actual generated-WAV → `playbin` → synchronized `fakesink`: prepare, play, pause/resume, volume, seek, stop/restart, EOS and worker cleanup |
| GTK smoke test | Passed: both pages/themes and three sizes; header/text/control bounds, adaptive navigation, focus, resize selection; actual GTK preparation/play action and close while playing; widgets and worker released |
| Locked dependency-license inventory | Regenerated and compared identically with the reviewed inventory |
| Flatpak source preparation and manifest parsing | Passed, including identical lockfile/inventory and original third-party notices |
| Vendored release build | `cargo build --release --offline --locked` passed inside `build/flatpak-source`, with no dependency downloads |
| Release finite audio/player self-tests and GTK smoke | All passed with the optimized binary |

The nine new playback tests use real GStreamer decoding and resources. They
check clocked progression and stable pause, seek/EOS, actual volume property,
invalid commands/volume, sanitized decoder failure and retry, preservation of
fatal errors during rapid commands, stop/restart/source replacement, queue
backpressure, final-handle shutdown, weak-reference release, private temporary
file ownership and an expired transition's cleanup. Test-only expired-deadline
injection exercises a real `playbin` without waiting five seconds. Two added
wire tests preserve HTTP 451 without retries/alternate routes and prevent
identity/permission fields in HTTP 200 from implying Music authentication.
Existing redirect tests now use the production client policy.

### Visual and keyboard review

Fresh [Gate A screenshots](ui/gate-a/README.md) cover both pages/themes at
420×720, 900×650, 1440×900 and real WM-maximized 1920×1080. The 26 raw frames
also show native keyboard focus, actual Playing/Paused/Stopped states, and the
lower compact diagnostics. Captures use isolated Xvfb/Openbox, Cairo/full-redraw
and generated local WAV through `fakesink`; no public API or physical audio is
used. Source/binary/image fingerprints and dimensions accompany the gallery.

An initial visual run caught compact horizontal overflow in the output selector
row. It was corrected; the smoke test now checks meaningful content/control
bounds against the client width, including after playback starts and resizes.
Window shadows and decorative scrollbar paint are excluded from that content
check. The final structural run has no width warning. Header/title/window
controls, text wrapping, contrast, navigation and the actual playback states
were inspected in the PNGs. Keyboard transport and focus are checked separately
from scrolling to expose the final read-only native-library information.

The first hosted run exposed retained GTK content on Ubuntu 24.04
(GTK 4.14.5 / Libadwaita 1.5.0), despite successful player cleanup. The same
failure was reproduced locally with those libraries. A native reference trace
identified GTK's pending focus-move reference being overwritten when adaptive
layout repeatedly hides the focused root. Clearing window focus when the owned
ToolbarView unmaps fixes the reproduced failure. The full widget/ancestor and
worker cleanup assertions remain enabled, with detailed failure diagnostics.
The previously focused control is restored after remapping when it is still
mapped/sensitive and no new focus was selected. Full Rust 1.98.1 checks were
repeated on both Debian and a locally reproduced Ubuntu 24.04 environment
(GTK 4.14.5, Libadwaita 1.5.0, GStreamer 1.24.2): Clippy, all 38 tests, debug
build, finite/player diagnostics and the GTK cleanup test passed. The updated
vendored release binary also passed both audio paths and GTK smoke on Debian.
This describes the observed GTK/Libadwaita combination; it does not claim that
every newer GTK version changed that native function. The temporary tracer and
logs remain outside the repository.

CI checks both stable and exact 1.98.1, including the new player diagnostic;
the MSRV lane also checks the optimized player and GTK lifecycle. The exact
hosted result must be read from the Gate A PR checks, not inferred from local
success. A successful full SDK/sandbox Flatpak build remains unavailable in the
nested cloud environment; no namespace/sandbox bypass was attempted.

### Remaining target-desktop acceptance

Repeat Gate A native and Flatpak build/install/launch on Fedora 44/KDE/Wayland/
PipeWire. Exercise explicit system output with the generated WAV, pause/resume,
volume, seek, stop/EOS/restart, rapid actions and closing during preparation or
playback. Check both themes, all sizes/maximize/restore, keyboard/focus, top-edge
window controls, HiDPI and accessibility. Cloud `fakesink` and Xvfb do not pass
those manual gates or prove Yandex Music playback.

The Music blocker requires official owner/provider clarification, followed only
if permitted by an approved Karin registration and separately consented account,
Music entitlement and stream-delivery tests. It is not resolved by the local
player, GPL licensing or a generic Yandex ID token.

## Rust 1.98.1 / Flatpak compiler compatibility

During subsequent Fedora acceptance, the user reported that GNOME SDK 50 with
`org.freedesktop.Sdk.Extension.rust-stable//25.08` provided Rust 1.98.1. Karin's
previous `rust-version = "1.99"` prevented compilation. This exact rejection was
reproduced in the cloud with Rust 1.98.1 before changing the package metadata.
The minimum is now `1.98`; the compatibility check uses exact patch version 1.98.1.
No compiler-version check is suppressed.

The compatibility run uses Debian 13 x86_64, rustc 1.98.1
(`48a229cea`, 2026-09-01), Cargo 1.98.1 (`797e8a9bc`), Rustfmt 1.9.0-stable and
Clippy 0.1.98. Native libraries remain GTK 4.18.6, Libadwaita 1.7.6 and
GStreamer 1.26.2. Current stable remains the development default; the pinned
toolchain is installed alongside it in the prepared cloud environment.

The source audit found no use requiring Rust 1.99. Locked metadata covers 197
external packages; none declares a Rust minimum above 1.98 (the highest declared
minimum is 1.96.0 for `kstring` 2.0.5). Some dependencies do not declare an MSRV;
the actual Linux compilation and tests, rather than metadata alone, provide the
compatibility evidence. Other target platforms were not compiled.

`Cargo.lock` is unchanged, SHA256
`dea4fadcad8782fae898905c9993d48864e809f9293ba08d4d0ba6b89da43462`.
The Flatpak source snapshot contains the identical lockfile. Dependency versions,
GTK/Libadwaita/GStreamer baselines and the GNOME 50 / Freedesktop 25.08 selection
are unchanged.

The following checks completed on 2026-10-25 with `RUSTUP_TOOLCHAIN=1.98.1`.
The release command ran inside `build/flatpak-source`, using its generated Cargo
source replacement configuration and vendored dependencies. It is the exact
Cargo build command used by the existing Flatpak manifest.

| Check on Rust 1.98.1 | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed, exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed, no warnings, exit 0 |
| `cargo test --workspace --locked` | **23 passed, 0 failed, 0 ignored**; binary and documentation targets have 0 tests |
| `cargo build --workspace --locked` | Passed, exit 0 |
| Debug GStreamer diagnostic | Passed: 40 synthetic buffers through `fakesink`, exit 0 |
| Debug GTK smoke test under Xvfb | Passed: both themes/pages, three sizes, header/text allocations, focus, navigation and widget cleanup, exit 0 |
| Locked dependency-license inventory regeneration/comparison | Passed; identical to the reviewed inventory |
| Flatpak source preparation, lockfile/inventory comparison and manifest parsing | Passed; source preparation selected Cargo 1.98.1 and preserved third-party notices |
| Vendored `cargo build --release --offline --locked` | Passed, optimized release binary built without dependency downloads, exit 0 |
| Release GStreamer diagnostic | Passed: 40 synthetic buffers through `fakesink`, exit 0 |
| Release GTK smoke test under Xvfb | Passed: responsive layouts, themes/pages, header/text allocations, focus, navigation and widget cleanup, exit 0 |

The fix changes package metadata and build verification/documentation; Rust
application sources and UI layout are unchanged. The existing inspected UI
captures remain evidence for that layout, rather than a new Fedora acceptance.

An independent push-triggered CI run exposed a capture-driver startup race:
`xdotool windowactivate` ran before Openbox published EWMH activation support.
The capture helper now waits for those capabilities with bounded failure
diagnostics. A real cloud rerun with deliberately delayed Openbox startup passed
and produced all 22 frames with exact size/maximize-state checks. This changes
test orchestration, not application rendering or layout.

CI now has separate current-stable and pinned 1.98.1 lanes. Both run formatting,
Clippy, tests, build, license inventory, silent audio and GTK checks; 1.98.1 also
builds the release profile offline and checks that binary's audio and GTK paths.
A metadata guard prevents the pinned MSRV lane from silently drifting away from
`Cargo.toml`. Inspect [PR #2 checks](https://github.com/tracerxbrhd/karin/pull/2/checks)
for the hosted result for each commit.

These native builds do not establish a complete GNOME SDK/sandbox Flatpak build.
Bubblewrap namespace creation remains denied in this cloud container. Full
Flatpak package/build/install acceptance must be repeated on Fedora after this
update; the revised UI's manual Fedora acceptance also remains pending.

## User-reported Fedora acceptance of the original foundation

The user reported the following results on Fedora 44 / KDE Plasma / Wayland /
PipeWire while reviewing PR #2. These are manual observations supplied by the
user, not a repeat Fedora session performed by the cloud agent.

| Observation | Reported result |
| --- | --- |
| GTK4 application startup | Passed |
| Anonymous API diagnostic | HTTP 200; no authorization established |
| Silent GStreamer diagnostic | Passed |
| Physical synthetic test signal | Audible |
| D-Bus and Secret Service | Detected |
| Original visual layout | **Failed manual acceptance; sent for correction** |

The supplied screenshots are review evidence only. They include desktop elements
and have not been added to the public repository. They do not constitute approval
of the revised layout.

## Revised UI acceptance status

Repeat manual visual acceptance on Fedora 44 / KDE Plasma / Wayland is
**pending**. Do not mark it passed from cloud screenshots. The required size,
theme, keyboard, focus and clipping checks are defined in
[UI_QUALITY_GATE.md](UI_QUALITY_GATE.md).

### Revised cloud structural checks

The revised GTK smoke test has passed both under raw Xvfb and under managed
Xvfb/Openbox. It exercises 420×720, 900×650 and 1440×900 allocations in both themes
and both pages, checks HeaderBar height (at least 48), header child bounds,
horizontal overflow, text ellipsis/height, adaptive navigation visibility, page
selection and preservation during resize, focus and weak-reference cleanup.
These are structural results, not
automatic approval of visual quality or of KDE window decorations.

The capture helper is `scripts/capture-ui.sh build/ui-captures`, invoked through
`/workspace/karin-environment/run-visual` in the prepared cloud image. It uses
Xvfb 21.1.16 and Openbox 3.6.1 on a 1920×1080 virtual screen, ImageMagick 7.1.1.43,
xdotool 3.20160805.1 and the native SVG loader. Normal capture sizes are exactly
420×720, 900×650 and 1440×900; the actual WM-maximized allocation is 1920×1080.
The screenshots use the clean application rectangle, with no user account or
desktop data. Binary fingerprints and geometry accompany the generated captures.
The helper forces a full Cairo redraw during capture to avoid stale X11 partial
updates after resizing or changing pages; it does not edit the captured pixels
or change the renderer used by a normal application launch.

The final clean PNGs were inspected and published in [docs/ui](ui/README.md):
22 matrix/keyboard frames and one separate About/Legal image. No user-supplied
desktop screenshots were copied into the repository. The
[capture manifest](ui/capture-manifest.json) records actual dimensions and
source/binary/image fingerprints for the matrix.

| Cloud visual case | Light and dark overview/diagnostics result |
| --- | --- |
| Compact, 420×720 | Inspected: bottom navigation, readable single-column content and fixed header; lower diagnostics reachable by scrolling |
| Medium, 900×650 | Inspected: header navigation, bounded components and readable two-column content |
| Wide, 1440×900 | Inspected: top-aligned hierarchy, bounded widths, intact title/controls and no stretched empty-state layout |
| WM-maximized, 1920×1080 | Inspected: actual window-manager maximize atoms verified; header, controls and content remain intact |

Tab and Shift+Tab produced visible focus. The sequence exercised Enter and Space;
Enter started the real silent GStreamer diagnostic and produced a visible EOS
success result. Six Tab steps in
compact diagnostics exposed the lower native-library card. The audio result
survived resizing from 900×650 to 420×720; Ctrl+Q closed the process with exit
code 0. Final images were checked for text, hierarchy, header/control clipping
and clean content rather than treating a successful capture script as signoff.
These cloud observations do not pass the pending Fedora visual gate.

### Revised source verification

The revised application source passed the following reruns in the cloud. These
are separate executions from the initial foundation baseline below.

| Check | Revised-source result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed, exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed, no warnings, exit 0 |
| `cargo test --workspace --locked` | **23 passed, 0 failed, 0 ignored** |
| `cargo build --workspace --locked` | Passed, exit 0 |
| `./target/debug/karin --audio-self-test` | Passed: synthetic GStreamer pipeline reached EOS through `fakesink`, exit 0 |
| Revised GTK smoke test | Passed: both pages/themes at three sizes, header/title/menu bounds, wrapping/ellipsis/overflow, focus and weak-reference cleanup, exit 0 |
| Dependency-license inventory regeneration and comparison | Passed: regenerated reviewed inventory matches, exit 0; this is not final-package legal clearance |
| Flatpak manifest parsing | Passed, exit 0; full sandboxed package build remains blocked |
| Final Flatpak source preparation | Passed, exit 0, including locked source preparation and notice generation; not a package build |
| GitHub Actions for the revised source | The authoritative per-commit result is in [PR #2 checks](https://github.com/tracerxbrhd/karin/pull/2/checks); local results above do not imply a hosted-runner outcome |

The native [About → Legal screenshot](ui/about-license.png) was inspected and
shows **GNU General Public License, version 3 or later**. This agrees with GTK's
configured license type and the approved source/metadata notices.

## Initial cloud baseline

The following baseline checks were executed on 2026-10-24 at foundation commit
`dc82909`, using `/workspace/karin-environment/run` from the repository root.

### Environment

Debian 13 x86_64; Rust/Cargo 1.99.1 stable; GTK 4.18.6; Libadwaita 1.7.6;
GStreamer 1.26.2; GCC 14.2.0; pkg-config 1.8.1. Development/test profiles omit
debug information to reduce the cost of compiling gtk-rs in cloud runners.
Release builds use Cargo's normal release profile.

### Executed checks

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed, no warnings |
| `cargo test --workspace --locked` | **23 passed, 0 failed, 0 ignored**; binary and documentation targets contain 0 tests |
| `cargo build --workspace --locked` | Passed |
| `./target/debug/karin --audio-self-test` | Passed: 40 synthetic buffers through `fakesink`, EOS observed, pipeline reset to `Null` |
| `GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- xvfb-run -a ./target/debug/karin --smoke-test` | Passed: real window realized, diagnostics navigation selected, window/key widgets released after close |
| `./target/debug/karin --probe-service` | Real anonymous HTTP 200, `Anonymous`, `service_available: Some(false)`; this is not login or playback |
| `python3 packaging/flatpak/prepare-source.py` | Passed: locked Cargo source snapshot prepared outside tracked source in ignored `build/flatpak-source` |
| `flatpak-builder --show-manifest packaging/flatpak/io.github.tracerxbrhd.Karin.json` | Passed: manifest parsed; no package build implied |
| Bubblewrap namespace capability check | Blocked: kernel/container denied namespace creation; no sandbox bypass attempted |

Tests cover domain transitions and retry/duplicate behavior, application-service
boundaries, actual local HTTP responses (401/403/451/503, redirects, malformed and
oversized bodies, disconnect, timeout and a slow-drip total deadline), UI state
presentation, application identifiers, and GStreamer EOS/error/timeout cleanup.
HTTP fixtures are test-only and use loopback; no real API, account credentials or
sound device is required by the automated tests.

The GTK smoke check explicitly observes weak references after closing the real
window, guarding against permanent widget reference cycles in signal handlers.
It does not replace target-desktop accessibility or rendering acceptance.

### Acceptance mapping

| Criterion | Evidence |
| --- | --- |
| Real Rust project and modular boundaries | Single Cargo package, committed lockfile, modules described in [architecture](ARCHITECTURE.md) |
| Native GTK4/Libadwaita shell builds and runs | Successful build and Xvfb smoke check |
| Isolated API adapter and real vertical slice | UI action → `ConnectionService` → `MusicApi`/`YandexApi` → `ConnectionReport` → UI; same service exercised by the live CLI probe |
| Independent audio scenario | UI worker action and CLI self-test use the same finite GStreamer diagnostic |
| Loading, unauthorized/unavailable and error states | Typed state transitions and presentation tests; UI permits retry |
| Honest service capabilities | [Feasibility matrix](API_FEASIBILITY.md) distinguishes observation, documentation and blockers |
| Reproducible dependency/build workflow | Cargo.lock, `--locked` commands, CI definition and vendored offline Flatpak source preparation |
| CI | GitHub Actions workflow provided; hosted-runner outcome must be read from the PR checks, not inferred from local results |

## Unrun/manual acceptance

Full Flatpak package build/install, repeat visual acceptance of the revised UI on
Fedora 44/KDE/Wayland, HiDPI and accessibility acceptance remain unverified. The
user-reported functional and audible-output observations above remain distinct
from those pending checks. No OAuth flow,
subscribed account, music stream, personal library, My Wave, Secret Service
credential lifecycle or MPRIS playback integration was exercised. See
[limitations](LIMITATIONS.md) and [build instructions](BUILDING.md).
