<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin validation evidence

This report separates initial cloud checks, user-reported Fedora acceptance,
revised UI checks and compiler compatibility. A result for one environment or
revision is not evidence that another has passed.

## Authentication correction — current revision (2026-10-29)

The public GUI now has one `Войти` action and a native Adw.Dialog in every build
mode. Developer client/secret fields and Device Flow instructions are removed.
**Live Yandex sign-in and Music authorization remain unachieved.** The public
AuthProvider reports the local integration blocker without opening a meaningless
ID-only page, invoking another application's identity or inventing a new HTTP
refusal. The owner's own-client ID success / Music status+about 403 evidence
remains unchanged; the exact cause of that refusal is unknown.

The new AuthProvider boundary, single-use PKCE/callback transport and optional
WebKit component are exercised only with synthetic local fixtures. Their tests
are not evidence of a permitted Yandex provider. The existing MusicService,
read-only catalog/queue, independent GStreamer engine and owner PKCE diagnostic
are preserved; MusicService/OAuth/audio implementation files are unchanged by
this correction. [The pinned Cassette investigation](AUTHENTICATION_REVIEW.md)
and [ADR-0010](adr/0010-single-purpose-sign-in-and-provider-gate.md) distinguish
provider login, token identity, Music acceptance and public distribution rights.

### Executed compiler and runtime checks

Prepared Debian 13 retained GTK 4.18.6, Libadwaita 1.7.6 and GStreamer 1.26.2.
Optional WebKitGTK 6 is 2.54.0, with libsoup 3.6.5. Pinned `webkit6` 0.5.0 uses
the existing GTK 0.10 / GLib-GIO 0.21 family; 0.6's GTK 0.11 was not selected.
Default/Flatpak builds exclude both optional authentication components and
contain no configured live provider or bundled credentials.

| Check | Stable 1.99.1 | Exact Rust 1.98.1 |
| --- | --- | --- |
| Formatting; default and all-feature all-target Clippy, `-D warnings` | Passed | Passed |
| Public workspace tests | **106 passed**, 0 failed; 1 GTK test separately selected | Same |
| All-feature workspace tests | **114 passed**, 0 failed; 3 display tests separately assessed | Same |
| Explicit native Music/auth/HTTP fixture | **1 passed** | **1 passed** |
| Explicit ephemeral WebView construction/shutdown | **1 passed** | **1 passed** |
| Locked default/all-feature debug builds and GTK smoke | Passed | Passed |
| Original hidden-terminal cases and both silent audio/player diagnostics | Passed | Passed |

Totals include four binary tests: public has 102 library tests, all-feature has
110. Twenty auth protocol tests cover independent random state/verifier, S256,
strict raw origin/port/path and duplicate/encoded parameters, denial/expiry,
concurrent replay, non-consuming origin display, bounded loopback HTTP and
cancellation. Fixed-port retry tests retain old owners/readers while checking
that cancel/consumption releases the listener; the embedded expiry task holds
only a weak attempt reference across waits.

The expanded native fixture performs an automatic local callback and provider
exchange, keeps the dialog open through Music account validation, then reads
liked tracks and playlists independently. It checks account 403, cancel, retry,
a late validation and search response, independent liked403/playlist200, logout
and close. Delayed responses are released through channels after cancellation,
not scheduler-dependent sleeps. Synthetic data and browser launchers are test-only.
The public blocked dialog is also structurally checked at all three smoke sizes
in both themes. Errors use native message rows and clear on cancel/logout/retry.

The Ubuntu 24.04/GTK 4.14.5/Libadwaita 1.5.0 smoke exposed an early-close defect:
that Libadwaita version opens its sheet on map tick two, so immediate close can
leave an unopened sheet pending and retain the native window/audio worker.
Karin invalidates credentials immediately and defers native closure through
public GTK frame callbacks until presentation is ready. Native close attempts
and `closed` signals are guarded by dialog identity. Regression coverage cancels
through both the button and native close before the first main-loop yield,
reopens immediately, then checks the native dialog list and actual unparenting.
The reproduced Ubuntu smoke now passes focus and full window/worker cleanup;
no lifecycle assertion was removed or bypassed. A never-mapped dialog also has
weak-release coverage. This uses public APIs, not private widget-tree mutation.

Rust 1.98.1 public and all-feature `--release --offline --locked` builds passed,
with optimized GTK checks and public optimized terminal/audio/player checks.
Public artifacts were restored. No `--ignore-rust-version` or native/runtime
version change was used. The optional WebKit page-loading/isolation test needs
an isolated display and a functioning WebKit sandbox; its cloud/CI result is
recorded separately below rather than inferred from construction success.

### Packaging, visuals and target acceptance

The all-feature license inventory is **206 external crates / 177 Linux graph**.
Six new bindings are MIT-compatible. Four registry archives omit their MIT text;
an exact-commit, SHA256-checked original notice and provenance are preserved by
an explicit fallback. Unknown omissions/hash/source changes still fail. Registry
and vendor content is not rewritten. Stable/MSRV inventories match, and negative
fallback checks passed. See [LICENSING.md](LICENSING.md).

Fresh source preparation, a matching vendored license inventory and the
independent Rust 1.98.1 offline release build passed. All 28 Rust/Cargo files
matched the tested snapshot. Integer `GDK_SCALE=2` smoke (3840×2160 Xvfb), including
the new dialog, passed. All **62 fresh PNGs** were inspected: 50 public frames
and 12 native HTTP-fixture frames, evenly split between themes. All four sizes,
real EWMH maximize, dialog focus/close, native error placement and text checks
passed. All 14 recorded source/binary hashes match the rendered working tree.
[Selected screenshots and reproduction](ui/auth-correction/README.md) retain six
clean frames and metadata; previous galleries remain historical.

The actual local WebKit navigation/cookie-isolation/cancel test was attempted,
but its process terminated when Bubblewrap could not create a namespace and
launch the D-Bus proxy. This is **blocked**, not a passed page-load test; the
sandbox was not disabled. The separate construction/shutdown test did pass.
Hosted CI runs all three isolated GTK/WebKit tests in separate processes on its
normal runner, plus public/all-feature builds and Rust 1.98.1 offline releases.
Exact-head hosted outcomes and artifacts are recorded in PR #8's description;
local fixtures and screenshots never establish a live Yandex login.

The initial hosted run at `40ec106` also hit Ubuntu 24.04's AppArmor uid-map
restriction. CI now loads a temporary profile attached only to the exact compiled
library-test executable, granting the `userns` permission required by WebKit's
sandbox. The global `apparmor_restrict_unprivileged_userns=1` guard is asserted
unchanged before/after, WebKit's sandbox stays enabled, tests run as the ordinary
runner user, and the profile is unloaded immediately afterward. There is no
global bwrap allowance or host sysctl relaxation. This follows Ubuntu's shipped
[Epiphany profile](https://git.launchpad.net/ubuntu/+source/apparmor/plain/profiles/apparmor.d/epiphany?h=ubuntu/noble).
Its actual test outcome is recorded in the final hosted run, not inferred from
the profile's presence. The managed cloud's namespace restriction is unchanged.

Full sandboxed Flatpak and real WebKit page loading remain subject to namespace
support; no sandbox-disable or privilege workaround is permitted. Existing GNOME
50 already supplies WebKitGTK6 runtime/development components, so no new Flatpak
runtime/module or filesystem/keyring permission was added. The public manifest
still builds `--no-default-features`.

Fedora 44/KDE/Wayland acceptance is pending. Follow the single default build/start
and expected blocked-dialog procedure in [BUILDING.md](BUILDING.md#music-sign-in-and-fedora-acceptance).
Actual account/library acceptance requires a permitted Music identity/grant and
supported automatic callback first; no end-user credentials/configuration fields
are a substitute. Service audio remains a separate unconfirmed delivery gate.

## Historical v0.1 music vertical slice — `7509884`

Issue #7 adds the identity-neutral read-only MusicService, native library/search/
detail views, a metadata queue and local experimental Device Flow. Both the build
feature and runtime flag are required; no application credentials are embedded,
no real account/token is supplied to Cloud/CI and no session is persisted.
Own-client PKCE remains independent and unchanged. No permitted service-stream
source has been established, so service playback and MPRIS remain unavailable.

### Executed cloud checks

The prepared Debian 13 environment retained GTK 4.18.6, Libadwaita 1.7.6,
GStreamer 1.26.2 and the approved SDK/runtime selection. No crate was added and
`Cargo.lock` is unchanged.

| Check | Stable 1.99.1 | Exact Rust 1.98.1 |
| --- | --- | --- |
| Formatting; default and all-feature all-target Clippy with `-D warnings` | Passed, no warnings | Passed, no warnings |
| Public workspace tests | **86 passed**, 0 failed; 1 GTK test separately selected | **86 passed**, 0 failed; 1 GTK test separately selected |
| Experimental workspace tests | **94 passed**, 0 failed; 1 GTK test separately selected | **94 passed**, 0 failed; 1 GTK test separately selected |
| Explicit native Music HTTP-fixture test under Xvfb | **1 passed**, 0 failed | **1 passed**, 0 failed |
| Locked public and experimental debug builds | Passed | Passed |
| Public/experimental GTK layout, navigation and cleanup smoke | Passed | Passed |
| Public hidden-terminal and both silent GStreamer diagnostics | Passed | Passed |

The public totals are 82 library + 4 binary tests, and experimental totals are
90 library + 4 binary tests. The native test is normally marked ignored because
it needs an isolated display; it was explicitly run and passed on both toolchains.
It exercises native application-credential cancellation without a provider call,
Music account validation, liked-track pagination, playlists, search, track/album
details, 50-row album pages, queue editing and an in-flight result after logout.
Every HTTP endpoint remains loopback, and all account/credential/media data is
synthetic. Test fixtures/capture hooks are under `cfg(test)`, not release content.

The 11 Music protocol groups include deterministic account-cache regressions:
logout during validation, overlapping old/new accounts and an invalidated queued
worker. Cache epochs and shared local token invalidation prevent stale account
facts from restoring or displacing a new session. Eight Device Flow protocol
groups check pending/slowdown, expiry/denial, cancellation including an in-flight
grant, bounded responses, trusted verification URLs and secret-safe errors.
The unchanged own-client PKCE suite and 14 PTY cases plus missing-TTY rejection
also passed; no real browser, token or provider account was used.

Both public and opted-in Rust 1.98.1 release builds passed with `--offline
--locked`; optimized GTK checks passed for both, and the public binary passed
terminal and silent audio/player checks. The public artifact was restored after
the experimental build. Default builds reject `--experimental-music`. Integer
HiDPI (`GDK_SCALE=2`, Xvfb 3840×2160) passed the public GTK smoke separately.
Structural checks wait a bounded time for resize/breakpoint allocation to settle
and retain every clipping/navigation/resource assertion.

The initial hosted run exposed a real GTK 4.14 layout regression: a horizontally
homogeneous `GtkStack` measured hidden wrapped pages at their minimum height,
forcing a 746 px content minimum into a 420 px window. Measuring the visible page
fixes this without changing native baselines or weakening geometry checks. The
Ubuntu 24.04 reproduction (GTK 4.14.5, Libadwaita 1.5.0, GStreamer 1.24.2) passed
Rust 1.98.1 all-feature Clippy/tests, the explicit native HTTP-fixture test and
both public and experimental GTK smokes after the fix. The total-timeout fixture
also no longer assumes a client can finish writing before its deadline; it holds
the accepted connection without responding and still checks the fixed timeout
category.

The dependency-license inventory was regenerated and byte-compared: **200 external
crates / 171 Linux graph**, unchanged. Public Cargo/Flatpak feature and permission
policy checks passed. `prepare-source.py` vendored the locked dependencies and
preserved notices; its snapshot matched all current Rust sources and Cargo files.
An independent native `cargo +1.98.1 build --release --offline --locked
--no-default-features` from that vendored snapshot passed, including a repeat
after the compatibility fix. This is a source/build check, **not a full sandboxed
Flatpak build**: Flatpak Builder 1.4.4
is available, but Bubblewrap reported that namespace creation is not permitted.
No privilege or sandbox workaround was attempted.

### Visual evidence and hosted checks

All **50 real GTK PNGs** were inspected: 42 public application frames and eight
explicitly synthetic native HTTP-fixture frames, split evenly between light and
dark themes. The application matrix covers library/search/queue/diagnostics at
420×720, 900×650, 1440×900 and actual WM maximize 1920×1080, with geometry/EWMH
records, keyboard focus/activation and diagnostic playback states. Header/text/
spacing/scrolling checks found no blocking defect. The persistent player's
intentional one-line metadata ellipsis requires a full tooltip/accessibility
label; viewport scrolling is distinguished from text clipping.

[Selected screenshots and reproduction](ui/v0.1-music/README.md) retain six clean
frames and the source/binary hashes. Capture metadata identifies the reviewed
working tree over `90a9799`, including the GTK compatibility fix, rather than
mislabeling that base as the rendered code.
Hosted CI captures the exact PR head again and uploads the complete matrix;
exact-revision results are linked in the stacked PR description. Earlier matrices
below remain evidence only for their stated revisions. Xvfb/Openbox and integer
GDK scaling do not establish Fedora/KDE/Wayland or fractional-HiDPI acceptance.

### Owner facts received on 2026-10-28

- [Own-client status/about comparison](https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6470853664):
  state/token/ID succeeded; both Music endpoints returned generic HTTP 403 with
  TUN on and off. No conclusion about the exact refusal cause or subscription.
- [Unmodified yamu experiment](https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6471162750):
  Device Flow, account, search, track and album reads worked on Fedora. This is
  not a new Karin UI/adapter acceptance result or a stream test.
- [Same-token header A/B](https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6471631991):
  status succeeded with and without the Android client header. The result is
  limited to that token/endpoint and grants no redistribution permission.

Pending owner acceptance includes the new in-memory Device Flow, actual Karin
library/playlist/search/details and paging, cancel/logout/late-response behavior,
keyboard/HiDPI/themes and the exact public Flatpak artifact. The owner must share
only coarse outcomes and safe categories; no codes, credentials, personal rows,
raw responses, URLs with parameters or screenshots of account/auth pages.
See [the checklist](BUILDING.md#experimental-music-session-and-fedora-acceptance),
[ADR-0009](adr/0009-music-vertical-slice.md) and
[delivery evidence](STREAM_DELIVERY_REVIEW.md).

## Historical v0.1 Gate B1 (2026-10-26)


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

### Independent status/about diagnostic (revision `5dbb554`)

The diagnostic now plans separate same-token `GET /account/status` and
`GET /account/about` requests after successful ID-client/account verification.
About runs even after status HTTP 403, network/schema failure or absent account;
it has its own HTTP/error result and cannot supply status facts or unlock the
catalog. About failure does not suppress catalog access after valid authenticated
status. HTTP 200 about accepts only an object `result` without a top-level error;
all fields are discarded. Both methods retain bounded bodies/deadlines, the same
closed error-code allowlist and no retries/redirects.

The prepared Debian 13 environment (GTK 4.18.6, Libadwaita 1.7.6, GStreamer
1.26.2) passed the following checks. Native versions, SDK/runtime and permissions
are unchanged.

| Check | Stable 1.99.1 | Rust 1.98.1 |
| --- | --- | --- |
| Formatting and all-target Clippy with `-D warnings` | Passed, no warnings | Passed, no warnings |
| All workspace tests | **71 passed, 0 failed, 0 ignored** (67 library + 4 binary) | **71 passed, 0 failed, 0 ignored** (67 library + 4 binary) |
| Locked debug build | Passed | Passed |
| Hidden terminal input | 14 PTY cases plus missing-TTY rejection passed | Same checks passed |
| Silent GStreamer and generated-WAV player diagnostics | Passed | Passed |
| GTK smoke: both themes/pages, resize, local playback and cleanup | Passed | Passed |

The 29 OAuth protocol tests include five new groups for independent status/about
outcomes, same-token/no-borrowed-identity requests, fail-closed envelope handling,
private-field redaction, status network failure/timeout and bounded about reads.
Malformed, oversized, incomplete or timed-out Music 403 bodies retain HTTP 403
and generic `Forbidden`. All requests, including unexpected paths, stay on
loopback. New fixture connection waits are bounded so regressions fail promptly.

Exact Rust 1.98.1 `cargo build --release --offline --locked` passed, followed by
optimized terminal, audio/player and GTK smoke checks. The dependency-license
inventory was regenerated and byte-compared: 200 external / 171 Linux graph,
unchanged. Hosted results for the final revision are linked from
[PR #6's checks](https://github.com/tracerxbrhd/karin/pull/6/checks). Historical
matrices below remain evidence for their stated revisions only.

At this revision, no live `/account/about` result had been supplied. The later
owner-reported HTTP 403 is recorded above; the exact refusal cause and Music
service acceptance remain unverified. Local fixture success cannot establish any of those outcomes. No real
OAuth inputs or account response were supplied to Cloud/CI, and no API-client
dependency was added. Public library sources were inspected without executing
them; see the [Rust-client review](RUST_API_CLIENT_REVIEW.md). OAuth/PKCE, routes,
region and application identity remain unchanged.

### Owner-reported DIRECT repeat and anonymous control

The owner's [26 October DIRECT-path report](https://github.com/tracerxbrhd/karin/pull/6#issuecomment-6424867774)
supersedes the earlier uncertainty about initial token exchange and ID-client
binding. A fresh local flow returned `ValidatedRedirect`, token HTTP 200 and
Yandex ID HTTP 200, with a matching Karin client and an existing ID account.
Authenticated Music `/account/status` again returned HTTP 403. Music account
presence and `serviceAvailable` were unknown; the catalog query was correctly
skipped. These are owner-reported Fedora results, not a cloud account test.

The route lookup selected an ordinary Ethernet/LAN path, and conventional
uppercase/lowercase HTTP(S)/ALL proxy variables were absent in that run. This
weakens the proxy/TUN hypothesis; it does not establish public egress or rule out
policy routing and other host/network effects. The owner's separate [anonymous
curl control](https://github.com/tracerxbrhd/karin/pull/6#issuecomment-6424928210)
returned HTTP 200. Anonymous reachability and authenticated HTTP 403 are distinct
contexts; curl and reqwest do not independently prove identical egress. Neither
result establishes subscriptions, catalog access or playable content.

The diagnostic now interprets only an allowlisted Music error identifier from
the already-returned HTTP 403 response, preserving the response-size/deadline
bounds and generic `Forbidden` fallback. Unknown remains unknown: no raw JSON,
messages/descriptions, personal fields, request IDs, headers, URLs or secrets are
printed. The original classifier change added no request; the current independent
about probe is described above and is not a fallback for the denied status.
No region/route/identity change or retry is part of either change. The actual
Fedora 403 identifier remains unobserved; no root cause is claimed from community code or fixture results.

### Music HTTP 403 classifier verification (revision `d6363b2`)

The Music classifier was checked on 2026-10-26 in the prepared Debian 13
environment, with GTK 4.18.6, Libadwaita 1.7.6 and GStreamer 1.26.2. Rust/native
dependency versions, SDK/runtime selection and Flatpak permissions are unchanged.

| Check | Stable 1.99.1 | Rust 1.98.1 |
| --- | --- | --- |
| Formatting and all-target Clippy with `-D warnings` | Passed, no warnings | Passed, no warnings |
| All workspace tests | **66 passed, 0 failed, 0 ignored** (62 library + 4 binary) | **66 passed, 0 failed, 0 ignored** (62 library + 4 binary) |
| Locked debug build | Passed | Passed |
| Hidden input, audio and GTK diagnostics | 14 PTY cases plus missing-TTY rejection, both silent GStreamer paths and GTK smoke passed | Same checks passed |

The 24 OAuth protocol tests include five new classifier tests: exact allowlisted
identifiers; unknown/malformed/ambiguous envelopes; unchanged ID/OAuth and other
HTTP-status behavior; catalog errors after a known Music account; and bounded,
unreadable or timed-out 403 bodies. They assert redaction, generic fallback,
unknown account/service facts after forbidden status and skipped catalog access.
All HTTP fixtures remain on loopback; the terminal suite isolates browser
handling and traps any network attempt. No real account or provider request was
used by these checks.

Rust 1.98.1 `cargo build --release --offline --locked` also passed. The optimized
binary passed the same terminal suite, both silent audio diagnostics and GTK
smoke, including both themes/pages, resize, actual local playback and window/
worker cleanup. The reviewed license inventory was regenerated and byte-compared:
**200 external crates**, **171** in the Linux target graph.

Exact-revision hosted CI outcomes are linked from [PR #6's checks](https://github.com/tracerxbrhd/karin/pull/6/checks).
The earlier redirect-correction and initial Gate B1 matrices below describe their
stated revisions. Neither those results nor these fixtures establish Music access
or identify the error code in the owner's actual 403.

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

### Executed redirect-correction checks (revision `9f29e2b`)

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

Hosted GitHub Actions for the corrected implementation `9f29e2b` passed both
stable and Rust 1.98.1, including offline release, terminal, audio and GTK checks:
[PR run](https://github.com/tracerxbrhd/karin/actions/runs/40339573169) and
[push run](https://github.com/tracerxbrhd/karin/actions/runs/40339561523).
These are results for that revision; use PR #6's checks for later heads.
At that revision token exchange and Music API acceptance still awaited a local
run. The subsequent owner-reported token/ID success and Music 403 are recorded
above; none of these cloud fixtures supplies real-provider evidence.

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

### Gate B1 remaining Music API and desktop acceptance (historical scope)

Karin's code/state return, initial secretless token exchange and ID-client
binding are now owner-reported successes. The effective Music rights, Music
account/service availability and read-only catalog response remain unestablished
after HTTP 403. Do not mark
Music authorization working from ID success, selected scope names, fixtures or
an HTTP 200 without authenticated Music account evidence. Do not publish browser
screenshots, raw responses or credentials. No streams, DRM bypass or subscription
playback test is part of Gate B1.

Do not repeat account login solely to recheck VPN, or change routes, region,
identity or OAuth mechanism to bypass this denial. Any separately consented
future local test must keep credentials on the owner's machine and share only
the sanitized category/status/unknown facts. The full-URL transfer is not an
automatic production callback. Public-client refresh/revocation,
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
