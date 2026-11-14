<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Validation

This record separates owner acceptance, automated checks and unresolved limits.
A successful fixture, build or screenshot does not establish live service access
or desktop behavior outside its recorded environment.

## Owner acceptance

The owner's report, recorded on **2026-11-04** (Europe/Samara), accepts revision
[`97093ee71f8df7b47b5809ca8c1d4774a2c5b22d`](https://github.com/tracerxbrhd/karin/commit/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d):

| Scope | Reported result |
| --- | --- |
| Native Karin on Fedora 44 / KDE Plasma / Wayland / PipeWire | Functional acceptance |
| Current v0.4 UI/UX | Accepted |
| Flatpak Karin v0.4 | Separately accepted |

The report is an overall acceptance, not a supplied per-scenario test matrix.
Do not infer individual keyring/portal backend, hardware-media-key or failure-path
results that the owner did not report. No additional blocking functional defect
was reported. Acceptance does not establish that the cloud cold-rendering issue
below is fixed, nor authorize a merge or public service distribution.

Karin v0.4 is a development milestone. `Cargo.toml` still declares `0.0.0`; this
record does not announce a tagged or published release. PR #15 was merged into
`master` as [`562596b`](https://github.com/tracerxbrhd/karin/commit/562596b990b5cd87c763af41120bad7830fda551).
That merge does not automatically accept later stability changes on Fedora.

## Owner acceptance of PR #26 — 2026-11-11

The owner explicitly accepted [`c1de869`](https://github.com/tracerxbrhd/karin/commit/c1de869361f36f5c749dec2b283ce56a356f06f2)
on real Yandex Music: Native playback, complete background MP3 saving, local
cache hits and local playback after application restart; installed Flatpak
playback, audio caching and local hits after restart. Downloading/Saved graphics
are provisionally accepted; their later visual refinement belongs to the actual
new design. This does not infer unreported per-device/failure/backend checks.

The accepted HEAD had eight successful final-source CI jobs, no intervening
unreviewed changes, and a clean merge base. PR #26 was made ready and normally
merged at [`1fd4028`](https://github.com/tracerxbrhd/karin/commit/1fd40289771af5a627e05eaaa47d2e6381de4b89),
preserving both parents and all development commits. `master` contains the accepted
HEAD. Earlier FAIL/pending entries below remain revision-specific history; the
real cache replay/restart requirement is now owner-confirmed in both packages.

This technical acceptance does not establish public OAuth/service/content
redistribution permission, offline lifetimes or a release. #5/#22 stay separate.
The separate v0.7 acceptance is recorded below; it was not inferred from this
earlier cache report.

## Owner acceptance of PR #28 — 2026-11-14

In the v0.8 task, the owner explicitly accepted exact revision
[`949d1f9315b210a118e02762a24d7747ff8e5321`](https://github.com/tracerxbrhd/karin/commit/949d1f9315b210a118e02762a24d7747ff8e5321)
with **68/68 Fedora checks: Native 34/34 and installed Flatpak 34/34**.
The reported scope includes playback/position/queue restoration, real playlist
server mutations, appearance/scaling/adaptivity, authentication, MPRIS, Stop,
Logout and My Wave; no regressions were found in the checked scenarios. The
report was received on 2026-11-14 (Europe/Samara); no private account data or
per-action logs were requested or published.

All eight exact-head CI jobs had succeeded, and the current PR contained no
intervening unaccepted changes. The owner authorized a normal merge: #28 was
made ready and merged at
[`2ab6d28`](https://github.com/tracerxbrhd/karin/commit/2ab6d280af9ee6c5a1d5cb0107a30149f7d87a8d),
preserving both parents and development history. `master` contains the accepted
revision. Issues #19/#21 are technically complete. This accepts the reported
v0.7 workflows, not future v0.8 features, the unavailable Karin design, every
unreported failure/backend case, public distribution #5/#22 or the unresolved
Cloud first-render cause in #23. No release or tag was created.

## Automated coverage

The [CI workflow](../.github/workflows/ci.yml) tests stable Rust and exact
**Rust 1.98.1**, with default, all-feature and minimal configurations. It covers:

- Formatting, strict Clippy, unit/HTTP-fixture tests and locked builds.
- The ordinary `cassette-auth` release and the feature boundaries used by Flatpak.
- Account/session isolation, protected-store protocols, metadata/artwork cache,
  confirmed writes, catalog normalization and cancellation/late replies.
- Actual GTK mouse/keyboard input, collection filtering and continuation,
  duplicate occurrences, queue/transport, Shuffle/Repeat and MPRIS on private
  session buses.
- WebKit startup/cancel, isolated sessions and interactive navigation with the
  sandbox enabled. Local container namespace restrictions do not replace these
  hosted tests with a PASS.
- GStreamer transport, seeking, volume, EOS and cleanup using generated media;
  native layout, themes, HiDPI, screenshots and initialized panel motion.
- Dependency-license inventory, notices and Flatpak-style locked/offline release
  builds. Source preparation must also be exercised after packaging changes;
  neither check is a complete installed Flatpak test.

Ignored native tests are executed explicitly by CI, not counted as passed merely
because `cargo test` lists them as ignored. The accepted runtime registers **33
native selectors**: 30 functional cases and three modern visual/raw observers.
The Ubuntu lanes execute all 30 functional cases, with selected HiDPI repeats;
the modern GTK lanes run all three visual observers at both scales.

For `97093ee`, [push CI](https://github.com/tracerxbrhd/karin/actions/runs/41363877072)
and [PR CI](https://github.com/tracerxbrhd/karin/actions/runs/41363880855) completed
with **all eight jobs successful**. Each Ubuntu lane recorded all 30 functional
selectors, including both navigating WebKit fixtures. Each modern lane passed
six current-source executions followed by four archived-baseline executions.
The separate local stable 1.99.1 / 1.98.1 matrix and 56 interactive GTK executions
also passed for the same runtime source.

The squash baseline also passed [master CI](https://github.com/tracerxbrhd/karin/actions/runs/41780378135).
These remain revision-specific results, separate from the stability checks below.

## Stability verification

[PR #24](https://github.com/tracerxbrhd/karin/pull/24) starts from `562596b`.
Its runtime/test source at [`193cfa0`](https://github.com/tracerxbrhd/karin/commit/193cfa04ed05c852186bd31aab952e379da3d78e)
has the following executed synthetic checks on 2026-11-04:

- Both original defects reproduced against the unchanged baseline runtime:
  a real loopback HTTP 403 with held cache I/O returned `FreshCache` instead of
  the next Network response; deleting a row before a manual occurrence changed
  its actual MPRIS path `t6` → `t7` while the same GStreamer source remained.
- Stable **1.99.1** and exact **1.98.1**: fmt, strict Clippy/default/all/minimal,
  tests **399 / 407 / 385 library + 4 CLI** per compiler/configuration, and locked
  debug builds. The 20 new unit regressions cover refusal/storage races and
  stable occurrence identity; ignored native tests are not included as passes.
- Focused cache tests: **44 PASS per compiler**, including 15 new cases for
  Busy/removal failure, durable tombstones, old readers/writers, restart,
  account/logout/policy/disabled/Clear transitions and bounded fence saturation.
  Paused commits also prove state admission does not wait for disk I/O.
- MSRV ordinary and all-feature locked/offline release, hidden terminal fixtures,
  silent GStreamer, generated-WAV transport/seek/volume/EOS/cleanup, ordinary and
  all-feature GTK smoke, and exact dependency-license inventory comparison passed.
- Flatpak-source preparation and vendored offline metadata resolution passed:
  208 packages including Karin and 207 notice directories. SDK/runtime/permissions
  are unchanged; this is not an installed sandbox build.
- The extended native shared-player/MPRIS fixture passed: manual duplicates,
  before/after/current removal, retained worker and TrackId, retained SetPosition,
  rejected stale SetPosition after a real switch, Metadata signals and logout.
- Eleven additional explicit native executions passed on private session buses,
  including protected Secret Service/Portal, restore/forget/close, cache reply
  retirement, Shuffle/Repeat/EOS, queue races, collection filtering, pinned Info,
  WebKit construction/cancel and scale-2 shared MPRIS/cache presentation.

The native build was forced after an isolated baseline build shared its target;
the fresh compiler artifact and all 111 Rust/Cargo source hashes were checked
before execution. A cached executable was not substituted as new-source proof.
Current results for the exact final HEAD belong to
[PR #24 checks](https://github.com/tracerxbrhd/karin/pull/24/checks), not predecessor
runs. Required native coverage and the WebKit sandbox remain unchanged.

The [first stability PR run](https://github.com/tracerxbrhd/karin/actions/runs/41786819946)
at `ec220a7` failed the 2× Preferences clear-callback assertion. Its fixed 150 ms
input delay acknowledged neither focus scrolling nor event delivery. The fixture
now requires an actual GTK hit and two stable frames before one real click, then
waits for the callback and retains `== 1` under the existing eight-second bound.
Production Preferences is unchanged. Four corrected stable/MSRV 1×/2× cases
passed; the exact original event was not reproduced in nine local baseline runs.
That CI failure remains a FAIL, separate from subsequent final-head checks.

The owner accepted the scoped Fedora regression at `9d54834` and explicitly
authorized PR #24's squash merge on 2026-11-04. It merged as
[`920fd71`](https://github.com/tracerxbrhd/karin/commit/920fd7103d98d99a4fd6a89d8b4013b38d61237e),
with an identical Git tree and all eight accepted-head checks successful.
This does not assert a new installed Flatpak test beyond the previously reported
v0.4 acceptance.

<a id="v05-my-wave-experiment"></a>

## v0.5 My Wave

The initial branch from `920fd71` introduced an explicit `--experimental-wave`
local path with normal service access gated. The sections below retain that
progression; the current [ordinary integration](#ordinary-startup-integration)
is recorded separately. Cloud uses synthetic HTTP and generated media, without
a real account; the owner's real results remain separate.
The initial implementation at `b2bef76` used stable **1.99.1** and exact
**1.98.1**: formatting, strict Clippy/default/all/minimal, **417 / 425 / 403
library tests + 4 CLI tests** per compiler/configuration and locked debug builds.
The final exact-head CI remains a separate gate; ignored tests are not passes.

The explicit Wave native fixture passed at 1× and 2× with fatal GTK warnings,
loopback HTTP, generated MP3, the existing resolver/worker and private MPRIS bus.
It checks real Playing/Pause/Resume/seek/EOS, original duplicate/batch markers,
accepted feedback/continuation, bounded rolling history and visible Queue,
coalesced pending Next, later selection winning over a late batch, failed-session
mode restoration, MPRIS Stop, logout and uncertain writes without replay. Held
startup tests distinguish a later manual ordinary selection from automatic EOS.
These are synthetic service acknowledgments and silent playback, not live radio. A controlled pre-fix test also reproduced a cancelled-session flag
incorrectly replacing a pending HTTP 403 with Local/Capacity. The corrected
1×/2× native runs preserve the typed Feedback/Forbidden/403 result across a tick
and ordinary-source switch, without further POSTs.

Home presentation passed mouse/Enter/Space, focus, wrapping, structure and cleanup
at compact/medium/wide/maximized sizes in both themes and scales: **52 clean
captures**, with 18 originals visually inspected. Those state-only captures do
not establish a provider response or an audible track. The offline CLI startup
regression also passed ordinary/opt-in primary launch, forwarding to the running
instance and D-Bus cleanup on an isolated bus without wallet/portal activation.
CI explicitly registers **35 native selectors**, retaining all previous coverage
and adding both new Wave fixtures at both scales in both compiler lanes.

Locked ordinary/all-feature offline release, hidden terminal fixtures, silent
GStreamer, generated-WAV transport and ordinary/all-feature GTK smoke passed in
the local implementation matrix. Source preparation and vendored offline metadata resolution passed: 208 packages
including Karin, 207 notice directories and an unchanged license inventory.
These remain packaging checks, not installed GNOME 50/Flatpak acceptance.
The current draft PR/checks record final-head results and the Fedora prototype;
the subsequent production-integration revision has separate checks below.

### Owner's first real Wave test

On Fedora 44 / KDE Plasma / Wayland / PipeWire the owner confirmed an admitted
session (five tracks), normalization, heard real Music, accepted `radioStarted`,
repeated `trackStarted` and `skip`, and a further five-track batch. Later one
feedback POST returned HTTP 400. Its old diagnostic omitted the event and emitted
a second generic failure line. The supplied sequence does not establish which
event or action failed; subsequent catalog successes do not identify it either.
Explicit Finish worked locally but was rejected as unnecessary UX.

This first test confirmed a positive live chain while leaving the refusal
unidentified. Issue #16 stays open; Issues #5 and #23 are separate and unchanged.

<a id="production-integration-revision"></a>

### Production-integration revision (`8bd9317` / `a69dc79`)

The new Home Hero uses the shared player, has no active Finish control and limits
Cancel to startup. Pause preserves the session; ordinary source selection and
MPRIS Stop retire it internally. Logout/close fence queued and late work. Refused
feedback now has one fixed diagnostic with the submitted enum event, lifecycle
phase and origin/reason, without provider bodies, identifiers or resources.
At this revision the published protocol review and the first eventless owner
log had not established the refused event. The accepted `event.from`, original
markers, actual heard time and request policy remained unchanged. A synthetic
`radioFinished` refusal alone did not establish the live cause.

The then-added HTTP fixture reproduced an accepted cross-batch prefix and
independent 400s for each of the five then-existing local event kinds, checking
poisoning, redaction and no replayed POST. Native tests exercised the real shared
worker and Home controls; those synthetic refusals verified handling rather
than resolution of the owner's real 400.

The local stable **1.99.1** / exact **1.98.1** matrix passed formatting,
strict Clippy/default/all/minimal, **421 / 429 / 407 library + 4 CLI tests**
per compiler/configuration and all locked debug builds. The extended native
Wave fixture passed at both scales, including six event-specific HTTP 400 cases
(source-change and system Stop have separate retirement cases), actual Home
navigation/CTA, shared worker identity and startup cancellation.
An intermediate new mouse test failed after an immediate programmatic stack
remap: instrumentation showed no clicked signal. The final fixture follows
actual native navigation before its one click and retains the pointer-target,
callback, player-state and original deadline assertions; production timing and
the engine were unchanged. This is separate from the owner's service refusal.

MSRV ordinary/all-feature locked offline release, hidden terminal tests,
silent GStreamer/generated-WAV transport and both GTK smoke modes passed.
The new ordinary executable also passed isolated ordinary/opt-in primary launch,
single-instance forwarding and cleanup without an account/radio. Source
preparation and offline vendored metadata again passed: 208 packages, 207 notice
directories, unchanged license inventory and scoped Flatpak permissions. These
are not an installed GNOME 50 SDK/Flatpak test. Final-head hosted checks remain
recorded in [PR #25](https://github.com/tracerxbrhd/karin/pull/25/checks).

The first [push MSRV job at `8bd9317`](https://github.com/tracerxbrhd/karin/actions/runs/41870601613/job/126716292210)
failed an existing session/catalog test that still expected the removed Home
placeholder wording. Its failure is retained. The corrective test checks the
new subtitle/caption and still requires disabled service access and no Cancel
on ordinary startup; it changes no production behavior or deadline. The complete
native session/catalog/queue/logout fixture then passed locally on stable 1.99.1
and exact 1.98.1 (one executed, zero ignored each), with formatting and strict
all-feature Clippy.

At the prior `a69dc79` handoff seven hosted checks had passed and one remained
pending; that same attempt subsequently completed all eight successfully
([PR run](https://github.com/tracerxbrhd/karin/actions/runs/41872993299),
[push run](https://github.com/tracerxbrhd/karin/actions/runs/41872983694)). Those
results belong to that revision, not the feedback correction below.

The new Hero's state/structure/input matrix passed on Rust 1.98.1 at 1× and 2×,
with fatal GTK warnings, both themes, 420×720, 850×750, 1440×900, 1920×1080 and
maximized. Twenty fresh clean captures cover representative states; the two
[Home previews](screenshots/README.md#my-wave-home-preview) illustrate only
presentation, without a real account, network request or audible service music.

The existing native preferences fixture also passed at both scales: actual
Light/System/Dark changes, saved Dark restoration and the same continuously
Playing worker/current occurrence. Karin contains no setter for the legacy GTK
dark flag. The identical baseline executable emitted zero startup warnings with
a clean profile and exactly the reported warning with only an externally seeded
GTK4 legacy flag. Libadwaita reads it before Karin's appearance application;
published KDE code can write it. The owner's actual effective input/writer is
still unknown. No warning suppression or forced preference was added; use the
optional [read-only check](BUILDING.md#theme-input-check).

### Owner's `a69dc79` repeat and final feedback correction

The owner subsequently tested
[`a69dc79686ba066f194fb3b28b5deaa318016d3c`](https://github.com/tracerxbrhd/karin/commit/a69dc79686ba066f194fb3b28b5deaa318016d3c)
on Fedora 44 / KDE Plasma / Wayland / PipeWire and accepted the current Home
design. Real session creation, normalized tracks, heard Music, all four feedback
events (`radioStarted`, `trackStarted`, `skip`, `trackFinished`) and multiple
further batches succeeded with HTTP 200. During a switch to an ordinary source,
the safe diagnostics identified the previously unresolved retirement fault:

```text
stage=feedback category=accepted http=200 event=skip phase=retiring origin=retirement finish=source-change
stage=feedback category=http-rejected http=400 event=radioFinished phase=retiring origin=retirement finish=source-change
```

`WaveCoordinator::finish()` added the rejected `radioFinished` after the valid
track report. The correction removes it from the outgoing protocol and shared
retirement path. It preserves one eligible final `skip`, natural-EOS
`trackFinished`, original markers, actual Playing time, serialized feedback,
late-result fences and ordinary playback/mode restoration. It adds no new
authentication or UI path. An HTTP 400 for any remaining supported event still
stops continuation without replay.

The correction passed the local stable **1.99.1** / exact **1.98.1** matrix:
formatting, strict Clippy/default/all/minimal, **422 / 430 / 408 library + 4 CLI
tests** per compiler/configuration and locked debug builds. The 28 Wave API tests
include a retirement/recreation table, original markers, fractional actual time,
all four supported HTTP 400 refusals and no replay. The new native retirement
selector exercises eleven cases across all ten requested categories, including
separate logout/close, a never-started resolver, natural EOS, paused time,
in-flight feedback/batch, real MPRIS Stop and a fresh session. It and the preserved
full Wave fixture each ran one test with zero ignored at both scales on MSRV,
with fatal GTK warnings. The local retired identity is cancelled after draining
eligible reports, including an empty outbox; no path emits `radioFinished`.

MSRV ordinary/all-feature locked offline release, hidden terminal regressions,
generated-media GStreamer/transport, both GTK smoke modes and isolated ordinary/
opt-in startup/forwarding/cleanup passed. Flatpak-source preparation and offline
metadata passed with 208 packages, 207 external notice directories and unchanged
license inventory, SDK/runtime requirements and scoped permissions.

Final-head hosted results are recorded separately in
[PR #25 checks](https://github.com/tracerxbrhd/karin/pull/25/checks). The new CI
selector is additional to the prior coverage. CI now requires **36 compiled /
33 unique functional selectors and 52 explicit Ubuntu executions** per compiler lane.

The first [push MSRV job at `3302c87`](https://github.com/tracerxbrhd/karin/actions/runs/41883448638/job/126761563952)
failed the unchanged collection-filter fixture at scale 2: its global HTTP count
changed from 20 to 21 during typing. The original log did not identify that extra
request's route; the other seven jobs passed. A controlled local reproduction
proved that its busy-only drain could sample before the footer's map-deferred
membership read was admitted, producing the same count failure (21 to 22).
The test-only correction waits for the expected native Like control to be mapped
and confirmed before the baseline. A permanent deferred-read interleaving retains
the original exact global-count equality across real filter input. The full
fixture passed on stable and 1.98.1 at both scales (one executed, zero ignored
each), without changing production UI, deadlines, input or rendering policy.
This explains a reproduced fixture race class, not the unrecorded original route
or a Wave regression. The original hosted failure remains separate from final
checks of the correcting revision.

At that handoff no post-fix Fedora or installed-Flatpak acceptance had been
reported, so the runtime flag remained required. The owner's subsequent report
and the ordinary integration decision are recorded below. The theme warning
and public-distribution question remain separate.

### Ordinary startup integration

In the report received on **5 November 2026** (Europe/Samara), the owner states
that corrected candidate
[`2e4b978`](https://github.com/tracerxbrhd/karin/commit/2e4b9786bb475abd9fc7843503da5bf0d243e9ba)
was retested on Fedora 44 / KDE Plasma / Wayland / PipeWire without new reported
problems. That candidate's [push](https://github.com/tracerxbrhd/karin/actions/runs/41888186457)
and [PR](https://github.com/tracerxbrhd/karin/actions/runs/41888193857) CI completed
all eight jobs successfully. This supports ordinary integration after the
earlier confirmed real session/audio/four-event feedback/continuation chain;
it is not a supplied per-scenario matrix or a new installed v0.5 Flatpak result.

The current implementation exposes Wave on Home after ordinary Music-validated
sign-in. Only **Слушать** creates a session: application startup, restored
sign-in, Home navigation and GApplication forwarding do not start radio.
Session IDs, batch markers and signed resources remain memory-only. The accepted
Hero keeps its controls and removes permanent experiment notices. The legacy
`--experimental-wave` argument is a deprecated no-op for older launch commands,
not a service gate or separate instance.

Local verification of this integration passed on stable **1.99.1** and exact
**1.98.1**: formatting, strict Clippy/builds for three feature configurations,
**422 / 430 / 408** library tests plus **4** CLI tests per configuration,
locked offline release builds, hidden-input regressions, silent GStreamer,
generated-WAV transport and ordinary GTK smoke tests. Ten explicit native test
executions at 1×/2× passed: normal account/navigation/logout, protected-login
restoration without Wave requests, Home states in both themes, actual mouse
Listen into the synthetic session/player and the eleven-case retirement matrix.
Supported-event HTTP 400 tests remain intact. Ordinary and deprecated-alias
startup/secondary forwarding retained the same primary D-Bus owner/window and
clean shutdown, without starting radio. These are synthetic Cloud checks.

Flatpak source preparation passed with **208** packages and **207** third-party
notice directories; the license inventory and scoped GNOME 50 manifest are
unchanged. The Cloud environment does not have GNOME SDK 50 installed, so source
preparation and offline SDK-style builds do not establish an installed Flatpak
result. Final-head hosted checks, including sandboxed navigating WebKit, are
recorded in [PR #25 checks](https://github.com/tracerxbrhd/karin/pull/25/checks).
The final [Native/Flatpak procedure](BUILDING.md#my-wave-local-acceptance) is still
pending for ordinary startup and the installed v0.5 package. Issue #16 stays open
until that report; Issues #5 and #23 are unchanged and independent.

### Separate post-squash startup failure

The first [master run at `920fd71`](https://github.com/tracerxbrhd/karin/actions/runs/41814183326/attempts/1)
passed three jobs but failed the MSRV workspace fixture at its initial 1440×900
resize: the native window allocation remained **0×0** until the existing
eight-second deadline. This is separate from PR #24's eight successful
accepted-head checks and owner acceptance. Its exact cause is unestablished.
The original failure is retained; an unchanged failed-job rerun has its own
attempt in that run. No deadline, renderer policy, sandbox or production timing
was relaxed, and this is not a resolution of Issue #23.

## v0.6 track reactions

The Dislike direction starts independently from merged v0.5 master
[`860672b`](https://github.com/tracerxbrhd/karin/commit/860672b2f0973b86624e69d6f9e9d9176efba5a5).
[ADR-0028](adr/0028-confirmed-track-reactions.md) records the pinned published
contract: account Dislike and removal are distinct from Like/Unlike and from
recommendation-only Wave events. Paired reactions require both readbacks; when
Dislike is unavailable, fresh independent Like readback preserves the established
Like-only workflow and leaves Dislike unknown/hidden. No optimistic complement,
automatic POST replay or radio-protocol extension is permitted. The owner later
confirmed real server mark changes, Like/Dislike interaction and existing functions
at `1877a6c`. That live report identifies missing automatic Next as the remaining
behavior correction; it does not report installed Flatpak or revised-code results.

The correction delegates only a confirmed Dislike addition on the unchanged
current playback occurrence to existing Next. Opaque occurrence and playback-
intent guards reject late effects after duplicate IDs, replay, cancellation or
account replacement; removal/Like and errors do not advance. Wave uses its existing
Skip lifecycle, with no new feedback type. Revised execution results must be
recorded separately from the original candidate checks below.

The automatic-Next correction passed four new guard regressions on Rust 1.98.1
and four explicit native executions: confirmed reactions and the full shared-player
Wave lifecycle at both 1× and 2× (each one actual test, zero ignored). Native
fixtures cover exactly one POST/resolution/Next, removal/Like/refusal/ambiguous
outcomes without advance, duplicate occurrences, leaving/returning, held
confirmation during logout/close, and one original-batch Wave Skip. Fatal warnings
were enabled and the sandbox was not disabled. Thirty-six synthetic captures were
produced; the new 900×650 dark successor capture was inspected without overlap or
clipping. Source/vendor preparation retained 208 packages, 207 notices and the
existing GNOME 50 permissions; 46 Markdown files/207 links passed. Full compiler,
release and final-HEAD hosted results are reported in [PR #27](https://github.com/tracerxbrhd/karin/pull/27).
These checks do not establish revised Fedora or installed Flatpak acceptance.

Verification of the original `1877a6c` candidate passed on stable **1.99.1** and
exact **1.98.1**: fmt,
strict default/all/minimal Clippy/tests/builds, **434 / 442 / 420** library tests
plus **4** CLI tests per configuration, MSRV ordinary/all-feature locked offline
release, hidden terminal boundaries and GTK/GStreamer smoke tests. The twelve
new backend regressions cover both memberships/revisions, refusal/conflict,
ambiguous writes and account-epoch replacement without replay.

Fourteen explicit native executions passed at 1×/2×: actual reaction mouse/Enter/
Space input, pending indicators, Info isolation, opposite marks and detail-view
synchronization, HTTP403/lost-response playback preservation, GET-only recovery
and Like-only fallback; the design matrix in both themes; strict collection
filter counts; full Wave/eleven-case retirement; normal account/navigation/logout
and protected restoration. Original captures were inspected at compact/medium/
wide/maximized allocations, with separate 44px reactions and reachable transport.
These are synthetic Cloud checks, not a Music-account acceptance report.

The first candidate passed native reaction input at both scales and the 1×
design matrix, but its 2× design run timed out before any reaction-read result.
The constructor started a spinner before DEFAULT_IDLE-deferred request admission.
The correction keeps that initial state static until a request is admitted;
actual pending requests still animate. Explicit native assertions cover both
states, and the complete corrected 1×/2× matrix passed with unchanged deadlines,
renderer and geometry. GLib scheduling starvation is a plausible interpretation,
not a traced scheduler result. The original failure is retained separately from
the corrected evidence; it is not a resolution of cold-rendering Issue #23.

Final-head hosted native/MPRIS/WebKit checks belong to this PR's checks; source,
license/Flatpak preparation and new installed-Flatpak acceptance remain separate.

The owner-reported live `1877a6c` result is separate from Cloud fixtures. Revised
automatic Next and installed Flatpak acceptance have not been reported. The
[short Fedora procedure](BUILDING.md#track-reaction-acceptance) covers them;
Issue #17 retains unreported acceptance. Issue #20's audio-storage permission/
lifecycle and Issue #5's public-distribution conditions are independent. No Cloud
fixture establishes service recommendation effect or grants audio-storage rights.

<a id="v06-unified-cache"></a>

## v0.6 unified cache and backlog audit

The owner subsequently confirmed Dislike→Next, ordinary queue and My Wave on
Fedora Native and installed Flatpak after PR #27 merged at `ddf93ea`. The owner
also accepted builds, tests and available functions of cache revision `707539b`
in both environments. Issue #17 is closed using that reported scope; special
race/failure behavior remains supported by CI, not an invented desktop test.
The owner now identifies `0709729` as the last accepted PR #26 revision. This
does not establish service-audio saving, acceptance of later direct changes or
cold-first-frame smoothness in Issue #23.

The new revision uses one enabled-by-default 2 GiB policy, six discrete native
Scale choices and a shared coordinator across actual metadata/artwork/audio
stores. It replaces two user quotas rather than hiding one. Tests cover shared
reservations, cross-store eviction, the protected small-metadata floor, restart,
corruption accounting, staged files, disk headroom/Unlimited, cancellation,
disable→enable fencing and v1/v2 settings repair without resetting valid
remember-login/forget fields. The new ordinary Music authority still returns
NoGrant, so no service audio is saved or replayed from disk.

The first local cross-store run was refused by the new disk guard: the Cloud
root filesystem had about 330 MiB free, below its headroom plus a bounded audio
reservation. After two obsolete generated Flatpak source snapshots were removed,
the unchanged tests passed. This is a resource safeguard, not a disabled test.
The first native Preferences runs exposed a Generic accessible role on GtkScale
with this GTK configuration. Assigning the required Slider role explicitly
preserves the assertion; the original failures are retained with the test logs.

Both stable **1.99.1** and exact **1.98.1** passed strict Clippy in default/all/
minimal configurations, **485 / 493 / 471** library tests and **4 CLI tests** per
configuration, debug builds, ordinary locked/offline release builds, silent
GStreamer/generated-WAV transport, GTK smoke and isolated startup/cleanup. The
MSRV all-feature release and hidden-input regressions passed too. An initial
local launcher invocation omitted Xvfb and failed; the corrected isolated Xvfb
invocation passed. The original failure remains in its log.

Fresh all-feature test executables (`fresh=false` Cargo receipts) passed **12
actual native executions**: Preferences at 1×/2× with capture on/off and protected
session/settings persistence at both scales, on each compiler. There were no
ignored tests in those selected executions; ordinary library commands still
leave **36 / 36 / 33** native registrations ignored. The unchanged registration
set and complete hosted native/motion/WebKit coverage remain required.

The shorter unified section no longer clips the remember-login switch at
420×720. That separate clipping regression now establishes its original
precondition at 420×520; standard 420×720/900×650/1440×900 checks remain. Original
native failures are retained, with no deadline/hit/focus assertion relaxed.
**112 captures** supplement structural/input checks; [safe current images](design/cache-v06/README.md)
cover light/dark, Unlimited/default, compact/wide and HiDPI. No personal screenshot
or real account was used. Source/vendor/license preparation passed with unchanged
GNOME 50/scoped permissions and **208 packages / 207 notice directories**; it
is not an installed SDK/runtime build.

Separately, the ordinary MSRV release ran with fresh XDG directories, a private
session bus and Openbox under Xvfb. Both EWMH maximize atoms and actual 1920×1080
geometry were confirmed before/after real Ctrl+comma opened Preferences. Native
wheel input exposed the cache group, a framebuffer capture was inspected, and
Escape/window close exited cleanly. No account, authorization attempt or service
audio was used. This is actual maximization, separate from the twelve Rust fixture
executions and from Fedora/KDE acceptance.

Current-source results and final-head Actions/artifact links are recorded in
[PR #26](https://github.com/tracerxbrhd/karin/pull/26). Neither Cloud/generated
media nor source preparation establishes a new installed-Flatpak/Fedora PASS.
[The procedure](BUILDING.md#synthetic-audio-cache-checks) specifies the next owner
check. [ADR-0030](adr/0030-unified-cache-budget-and-preferences.md) records the
migration and resource policy; [the audit](ISSUES_AUDIT.md) records all twelve
Issues and the distinct remaining gates. [Provider questions](AUDIO_CACHE_PROVIDER_REQUEST.md)
were prepared, not sent. The primary-source research is recorded in [AUDIO_CACHE_REVIEW](AUDIO_CACHE_REVIEW.md);
the fresh completion review obtained HTTP200 for the previously rate-limited Plus page.

## v0.6 audio-cache foundation

### Original candidate and Fedora failure

The audio-cache branch started at v0.5 squash baseline `860672b`. Its original
candidate `0ce9ea0` compiled the store only under `cfg(test)`, with sealed generated
WAV input and a fake authority. No real Music audio, account, token, cache,
descriptor/CDN address or official-client cache was used. Ordinary builds kept
online-only service playback and unchanged Flatpak permissions.

That candidate's Cloud verification passed on stable **1.99.1** and exact **1.98.1**: formatting,
strict Clippy/tests/builds in default/all/minimal configurations, **438 / 446 /
424** library tests plus **4** CLI tests per configuration, locked offline
ordinary/all-feature release builds, hidden terminal boundaries and ordinary GTK
and generated-media GStreamer smoke tests. All **16** synthetic-cache tests ran
in each configuration; the restart case also executed a meaningful child test
in a separate process. Actual cached generated WAV playback exercised the
existing worker with Pause/Resume, Volume, Clear, Seek, EOS and cleanup. Review
found and corrected independent-owner generation and orphan-part quota gaps
before this matrix. No service cache adapter is inferred from that test.

The owner subsequently reported **15 PASS / 1 FAIL** on Fedora for both
`cargo +1.98.1 test --lib --all-features --locked audio_cache` and
`cargo +stable test --lib --locked audio_cache`. Reopening in
`one_namespace_owner_and_retired_tickets_survive_clear_and_reopen` returned `Busy`.
The original Cloud pass does not supersede that failure.

### Namespace lifetime correction

On 2026-11-05 the unchanged sixteen-test Cloud run passed again. A deterministic
regression then retained a duplicate of the lock's open file description, dropped
both strong cache owners and verified that the weak owner had expired. Reopen
failed with `Busy` before the fix. Linux `flock` belongs to that open file
description, so a dup/fork-retained descriptor can outlive the Rust owner even with
close-on-exec. This establishes a code defect; the exact retaining process/event
on the owner's Fedora machine was not traced.

Final-owner destruction now explicitly unlocks the namespace, guarded by owner
process identity. Acquisition remains exclusive. Live strong clones still exclude
another owner, old Hit/Lease/Request remain retired, and closing an old retained
descriptor cannot unlock the new owner. Before the production conversion, all
**17** focused tests passed on exact 1.98.1/all-features and stable/default. Five
sequential repetitions per compiler, each with eight test threads, yielded
**170 PASS / 0 FAIL / 0 ignored**. These results concern that focused correction,
not a claim that the later complete production revision was already validated.

### Production conversion and service gate

After PR #27 merged at `ddf93ea`, the audio branch incorporated master through
normal merge `142a364`. The store now compiles into ordinary builds, with a
worker-owned XDG namespace, account/generation authority, shared playback bridge,
separate audio quota/usage and combined Clear/logout cleanup. Generated fixtures
exercise these paths; only their fake authority/audio inputs remain test-only.
The real service authority declines storage/reuse admission. Consequently real
Music still uses the accepted online path without saved service audio.

The six pinned Cassette sources (`9d34251`) and primary service pages were
reviewed again on 2026-11-05. Music terms, application license and offline help
were fetched successfully; Plus uses the complete earlier same-day copy plus a
fresh bounded response, with a later HTTP429. Published account schemas expose
`permissions.until/values/default` and `cacheLimit` but do not define Karin's
storage/reuse permission or the online cache-hit authorization contract.
[AUDIO_CACHE_REVIEW](AUDIO_CACHE_REVIEW.md) separates that concrete evidence gap
from a supposed general third-party ban and from later offline requirements.

The production revision passed formatting, strict Clippy, default/all/minimal
tests and debug builds on stable **1.99.1** and exact **1.98.1**. Each compiler ran
**469 / 477 / 455** library tests and **4** CLI tests per configuration, with no
failures. The ordinary test commands leave **36 / 36 / 33** native tests ignored;
those are not counted as executed by the library matrix.

All **31** shared audio-cache tests execute in that matrix: the original sixteen,
the lock-lifetime regression, six production-store cases and eight bridge cases.
They cover fresh online resolution before a granted cached MP3 hit, bounded
background publication, restart/current-authority checks, corruption and online
fallback, cancellation, account isolation, quota/LRU and cleanup. Generated media
and a fake authority drive the real PlaybackController/GStreamer worker; this
does not establish live service storage permission or heard Music.

Separately, three native selectors ran at scales 1 and 2: the existing player
scenario with granted cache fill/reuse/retirement, Preferences and persistent
session/settings/forget/close. All **6 actual executions passed** on the final
runtime sources, without disabling the WebKit sandbox. **36 synthetic Preferences
captures** supplement their structural checks. Flatpak source/license preparation
passed with **208 packages / 207 notice directories**, unchanged license inventory,
GNOME 50 runtime and scoped permissions. The documentation check covered **47
Markdown files / 221 local and Git targets** without missing targets.

Both ordinary locked/offline release builds, the MSRV all-feature release,
hidden terminal regressions, silent GStreamer, generated-WAV transport, GTK smoke
and ordinary/legacy-alias single-instance startup/cleanup passed. The final 31
cache/bridge tests also passed three sequential repetitions per compiler:
**186 PASS / 0 FAIL / 0 ignored**, separately from the earlier lock-only repetitions.

The first hosted Ubuntu jobs at `09cfe2d` failed the Preferences fixture when its
second remember-login switch was clipped above the viewport. The internal
`GtkSwitch` rejected focus. A controlled clipped-target regression reproduced
the eight-second timeout. Focusing its owning `AdwSwitchRow` passed four local
stable/MSRV executions, but at `ec20d88` the PR stable job still timed out after
the row accepted focus: accepted focus did not establish a visible pointer target.

The final helper explicitly scrolls the native viewport adjustment to the target
and observes frames on the mapped window rather than the clipped child. A
permanent regression keeps the row focused while scrolling its switch out of view.
The actual native hit test, two stable frames, real mouse click, callback counts
and original deadline remain required. Production UI is unchanged. Full-feature
fixtures passed on stable and exact 1.98.1 at both scales, with strict Clippy.
A separate widget-only minimal build used verified Ubuntu GTK **4.14.5** and
Libadwaita **1.5.0** libraries in the existing container: the unchanged `ec20d88`
helper reproduced the timeout, and the correction passed at 1× and 2×. This
controlled widget test does not replace the full hosted WebKit/sandbox checks.

The no-capture MSRV jobs at `c76bef4` then exposed an unacknowledged error-focus
callback before the next focus assertion. Capturing a screenshot happened to drain
that idle callback; the test now explicitly observes the new inline error gaining
focus before setting up the clipped-switch scenario. The focus assertion remains.
Both compilers passed all eight full-feature combinations of scale 1×/2× and
capture on/off. Four additional minimal-profile runs with the older Ubuntu
libraries passed the same combinations. A one-off unchanged local no-capture run
did not reproduce that intermittent ordering failure; the original
[push](https://github.com/tracerxbrhd/karin/actions/runs/42180027292) and
[PR](https://github.com/tracerxbrhd/karin/actions/runs/42180033054) failures are retained.

The original [push](https://github.com/tracerxbrhd/karin/actions/runs/42072546505)
and [PR](https://github.com/tracerxbrhd/karin/actions/runs/42072557868) failures,
and the later [PR Preferences failure](https://github.com/tracerxbrhd/karin/actions/runs/42074992147),
remain separate evidence. The `ec20d88`
[push stable job](https://github.com/tracerxbrhd/karin/actions/runs/42074981666)
failed earlier in the unchanged workspace fixture with an initial **0×0**
allocation. That matches the already recorded startup-failure class, not the
Preferences failure. The unchanged workspace selector passed locally on both
compilers; the original hosted event's cause remains unestablished. No rendering,
geometry, timing, sandbox or production gate was relaxed to obtain these results.

Complete final-head hosted/native results are recorded in
[PR #26 and its checks](https://github.com/tracerxbrhd/karin/pull/26/checks). No new
Fedora/installed-Flatpak acceptance or real Music cache reuse is established here.
PR #26 remains Draft and Issue #20 stays open; #5 and #23 remain separate.
Synthetic expiry/revocation tests cannot establish external rights.

## PR26 post-acceptance correction

The following results describe `53eb05d`, before the single-transfer completion below.

The owner's new task identifies `0709729` as the accepted baseline and
`0f9257f` as the unverified direct-edit head. The
[commit-by-commit audit](PR26_REVISION_AUDIT.md) covers all eight intervening
commits. At `0f9257f` exact-head formatting and Clippy passed, but both compilers
failed the expired-source test's obsolete `fill.unwrap()`. The correction keeps
the early rejection and asserts that no copy is scheduled.

The production coordinator now requires independent storage permission and
fresh online admission. Its safe default delegates to `MissingStorageContract`.
A per-owner/session/budget context and one-use attempt prevent old or replayed
permission results from replacing current state. Failure before HTTP, current
resource refusal, cancellation/token expiry during permission lookup and aged
source admission cannot reuse an earlier grant. Permission lookup does not hold
the state mutex used by GTK. The shared quota, six choices, disk format,
authentication, single player/queue and ordinary network playback are preserved.

The new original-media fixtures exercise the actual production coordinator and
existing application/PlaybackController, not a second player. Service-source
availability still supplies no storage permission. The fresh Cassette audit and
primary pages did not establish an applicable Karin storage/reuse contract or
the provider's fresh cached-hit rule. Real-account caching remains incomplete;
no service tokens/content were used in Cloud.

The corrective runtime passed on stable **1.99.1** and exact **1.98.1**:

- Formatting and strict Clippy in default/all/minimal configurations.
- **499 / 507 / 485** library tests plus **4 CLI tests** per configuration,
  with no failures. All **47** audio-cache/coordinator/bridge tests executed;
  ordinary commands still leave **36 / 36 / 33** native tests ignored.
- Debug builds in all three configurations, locked/offline ordinary release
  builds with the Flatpak Cargo options, and the MSRV all-feature release.
  Silent GStreamer, generated-WAV transport, GTK smoke, hidden terminal tests
  and ordinary/legacy-alias startup/cleanup passed.
- **16 actual native executions** using newly compiled (`fresh=false`), hashed
  test binaries: player/cache, Preferences and persistent-session fixtures at
  1×/2×, plus MPRIS/playerctl and WebKit construction/shutdown on each compiler.
  All passed with fatal warnings and without disabling the WebKit sandbox.
  **112 generated Preferences captures** supplement structure/input checks;
  compact light and wide dark images were inspected. These are Xvfb checks.

The first navigating WebKit attempt in the local Docker container terminated
before page load: bubblewrap reported that it could not create a namespace and
its D-Bus proxy failed (exit133). `unshare -Ur` is also denied there. This is
recorded as a local environment limitation, not a successful navigation test or
a claimed product regression. The sandbox was not disabled. Complete navigating
WebKit/callback/cookie/cancellation and modern visual coverage remain mandatory
in the unchanged hosted workflow, using its exact-test-binary namespace policy.

Flatpak-source preparation passed with **208 packages / 207 notice directories**,
unchanged dependency-license inventory, GNOME50 runtime and scoped permissions.
The matching corrective Rust sources also built **offline/locked on 1.98.1**
from that vendored snapshot using a new empty `CARGO_HOME` and the manifest Cargo
flags. That build uses the container native libraries, not an installed GNOME SDK.

Local corrective-source checks are recorded here. Consult
[PR #26 and its checks](https://github.com/tracerxbrhd/karin/pull/26/checks) for
the final-head hosted CI status; earlier-head results do not validate this correction.
The [Fedora procedure](BUILDING.md#pr-26-corrected-cache-acceptance) separates
original-media production-path verification from owner Native/installed-Flatpak
and real-service acceptance. The Cloud has Flatpak/Builder but no installed
GNOME50 SDK/runtime; source/vendor validation is not a full installed Flatpak
result. PR #26 remains Draft and Issue #20 remains open. No merge is authorized.

## Historical afd3ce4 single-transfer application cache

This section records the preceding revision. Its ordinary `NoGrant` assembly is
superseded by the private HTTP integration below; its service-cache limitation
must not be read as the behavior of the new implementation.

The follow-up replaces the duplicate cache download with capture of the existing
guarded GStreamer source. Exact independently licensed SHA-256/length claims are
checked before publication/reuse; ordinary Yandex assembly still declines storage.
Permission/file verification overlaps source resolution and never delays it for a
slow cache. One writer streams without holding the read/Clear gate; staging,
cancellation, atomic commit, shared quota and deferred LRU remain bounded.

The normal-application fixture executes six separate OS processes through the
existing MainWindow, protected-session lifecycle, ordinary audio-store factory,
shared budget, queue and player. Its HTTP account/credentials are isolated, and
the original generated audio is expressly CC0. It does not install a test cache or
use `SyntheticAuthority`. Actual worker `source_kind`, inode, payload SHA/length,
HTTP counts and child cleanup are recorded in a machine-readable receipt:

| Completed scenario | Actual source | Cumulative media GETs | Cumulative media bytes |
| --- | --- | ---: | ---: |
| First play, then keyboard replay | Remote, then cached file | 1 | 24,546 |
| New application process and fresh account/resource check | Same cached inode | 1 | 24,546 |
| HTTP403 on fresh resource validation | Refused, without cached bypass | 1 | 24,546 |
| Deliberately corrupted cache | Online repair and complete publication | 2 | 49,092 |
| Native Clear while cached track is Playing | Existing FD continues Playing | 2 | 49,092 |
| Actual HTTP `no-store` response after Clear | Online, without a saved record | 3 | 73,638 |

These bytes describe the original fixture on this encoder; receipt expectations
use its measured length/hash, not a hardcoded media size. Each run requires
**6 account validations, 7 resource checks and 6 descriptors**. First/replay/restart
share one audio body; only corruption repair and the explicitly uncached play
request further bodies. Clear preserves the active generation and advancing
Playing clock. No Yandex account, service audio or live rights result is involved.

The final runtime passed stable **1.99.1** and exact **1.98.1** formatting, strict
Clippy and tests in default/all/minimal configurations: **522 / 530 / 508 library
PASS plus 4 CLI PASS** per configuration, with no failures. All **68** focused
audio-cache tests execute; ordinary commands leave **37 / 37 / 34** native tests
ignored, which are not counted as executed by that matrix. Debug and locked/offline
release builds, GTK smoke, GStreamer transport, hidden-input boundaries and
ordinary/legacy-alias startup cleanup passed on both compilers.

Fresh, hashed test ELFs passed **22 actual native selections** across both
compilers: player/cache, Preferences and persistent-session cases at 1×/2×,
MPRIS/playerctl, Wave, WebKit construction/shutdown and the six-process application
case at both scales. The latter accounts for **24 child application processes**;
all four receipts agree on the request/byte/source invariants above. **112 generated
Preferences captures** supplement structural/input checks. This is Cloud/Xvfb,
not Fedora/KDE acceptance. Local navigating WebKit retains the previously recorded
container namespace limitation; complete sandboxed navigation remains required in
hosted CI, without disabling that sandbox or weakening existing native gates.

Flatpak-source preparation retained **208 packages / 207 notice directories**,
the unchanged license inventory and scoped permissions. With an empty Cargo home,
the vendored snapshot passed locked/offline metadata resolution and the exact
MSRV release feature set (`--no-default-features --features cassette-auth`). Its
actual ELF passed GStreamer diagnostics, playback transport and GTK smoke. These
use host libraries: GNOME50 SDK/runtime is not installed in this Cloud and no new
installed-Flatpak or Fedora result is claimed.

The existing CI now registers and executes the new process-level selector in
both Rust lanes and retains its safe receipt and child logs. Current-head hosted
status and downloadable Native build are published in
[PR #26](https://github.com/tracerxbrhd/karin/pull/26/checks). Earlier-head checks do
not validate this runtime. The single final owner procedure is in
[BUILDING](BUILDING.md#pr-26-corrected-cache-acceptance).

The first hosted attempt at `1dcb2b7` failed waiting for the new fixture's Clear
operation on GTK4.14; allocation-only pointer coordinates did not establish a
native button hit. The fixture now reuses Preferences' viewport reveal and
two-frame native hit checks, and asserts exactly one delivered mouse click.
No wait limit or product behavior was relaxed; the full matrix is repeated for
the corrected test sources. The earlier failed run remains failure evidence.

Real Yandex Music saving/reuse is **not completed or accepted**: the exact local
stop is `MissingStorageContract::authorize → NoGrant`. The affirmative service
terms and reviewed wire contracts establish neither Karin's applicable storage
permission nor a confirmed mapping from a fresh signed resource to the retained
representation. This is not a measured CDN failure or proof of a categorical
third-party ban. [AUDIO_CACHE_REVIEW](AUDIO_CACHE_REVIEW.md) separates those facts
from HTTP cacheability and the genuine CC0 rightsholder declaration used here.
PR #26 stays Draft and #20 stays open. No repeated owner test is requested to
rediscover a gate that remains closed.

## PR26 private HTTP integration — 2026-11-08

The following integration/fixture record describes
[`f8a85b8`](https://github.com/tracerxbrhd/karin/commit/f8a85b8775b8176f008023e500c8888c2e293c52),
before the owner's Fedora failure and the lifecycle correction below. Its
automated results are not live Music acceptance.

At that head the ordinary application selects `PrivateHttpCachePolicy` rather than the
blanket missing-provider-contract gate. Its typed candidate is not a provider
entitlement or copyright license: direct ordinary writes/reads cannot bypass
HTTP response checks and current representation validation. Playback access,
response storage policy, retained byte version and explicit content restrictions
remain separate. The existing single GStreamer source supplies the cache bytes.

Complete media and bounded checksummed HTTP metadata publish in one transaction,
under the existing shared budget. Each local hit first refreshes Music delivery
and validates a strong representation proof through the existing HTTP client.
The sealed Legacy file-locator/encoding binding can survive a renewed signature;
it is not an immutability assumption. Changed validators/locators, refusals,
unsupported variants and failed checks fall back online. No-store on validation
retires the old record; actual streamed no-store cannot publish its replacement.
Missing validators may permit storage, but never an unverified local hit.

The existing ordinary-application fixture now uses the production HTTP policy,
without injecting a CC0 or mock storage authority. Its original generated media
remains CC0 for provenance. Eleven separate processes cover first/replay/restart,
rotating signed wrappers, refusal, corruption, changed audio/ETag, HEAD no-store,
restoration, GET-only no-store, restoration, active Clear and missing validators.
It checks the actual player source, completed bytes/hash, conditional requests,
whole-media GET counts and the restricted diagnostic grammar. These local HTTP
results are not a live Yandex Music or Fedora result.

The controlled first/replay/restart sequence needs one whole-media GET in total.
The complete scenario's cumulative GET counts are
`1 / 1 / 1 / 2 / 3 / 4 / 5 / 6 / 7 / 7 / 9`: later GETs deliberately exercise
repair, changed representations, no-store and unverifiable versions. Fresh
account/delivery checks are not counted as media downloads.

Final-head compiler, native, packaging and CI results belong to
[PR #26 checks](https://github.com/tracerxbrhd/karin/pull/26/checks); an earlier
head does not validate new runtime code. The one final owner procedure is
[BUILDING](BUILDING.md#pr-26-corrected-cache-acceptance). It uses only fixed
`cache`, `source` and `reason` diagnostics, without sharing user credentials,
identifiers, signed URLs, response bodies or music files.

**Cloud did not verify the real service. The later owner result was FAIL**, as
recorded below. Public-distribution/content questions remain separate in #5/#22.
Installed Flatpak and Fedora acceptance are not inferred from fixtures, source
preparation or Xvfb. PR #26 remains Draft.

## PR26 completed-capture lifecycle correction — f2050b0

This section records the previous correction, whose later real-service result
is **FAIL** below. Its controlled complete-body regressions do not establish
successful capture of the whole response during real sequential playback.

### Owner evidence at f8a85b8

The owner built `f8a85b8775b8176f008023e500c8888c2e293c52` on Fedora 44,
KDE Plasma, Wayland and PipeWire with Rust 1.98.1. Ordinary sign-in and audible
Music worked. Real Liked and My Wave playback repeatedly reported:

```text
cache=miss source=remote reason=record-absent
cache=not-cacheable source=remote reason=cancelled
```

No `write-confirmed` or `hit source=local` was observed. This is **service-cache
FAIL**, irrespective of the earlier 8/8 CI result. The generic cancellation pair
does not identify the first rejected event, body size, Playing transition,
pipeline drop or writer/commit ordering on that machine. The exact Fedora event
must not be inferred from a local fixture or described as an HTTP/storage refusal
without the corresponding evidence.

### Revised lifecycle contract

Code inspection identified independently actionable cancellation paths: the
writer waited for a GTK-observed positive Playing clock while the source could
fill its 2 MiB channel; full byte receipt still depended on an EOS event for
completion; normal track teardown could cancel pending publication; and the
next online check retired a previous `current` admission still used by the
writer. The new contract separates those events:

- Drain during preparation, before Playing. Latch actual GStreamer Playing
  separately and require it before publication. Never publish an unplayed body;
  pipeline retirement before Playing also releases a complete preroll reader.
- Complete capture at the exact validated HTTP byte count, without waiting for
  pipeline EOS. Keep complete queued bytes readable after source teardown.
- Bound retained media to 64 MiB per capture, four pending captures and one
  staging writer. Allocation follows incoming bytes, with a worst-case retained
  body bound of 256 MiB; the GStreamer queue2 transport limit remains 2 MiB.
- A completed, played write can seal its own admission for at most 30 seconds of
  finalization. Natural EOS, automatic Next, Wave rollover or a new online check
  do not revoke that finalization merely by replacing the current track. This
  does not admit any later playback without fresh access/version checks.
- Partial replacement, seek/discontinuity, overflow and failed capture cannot
  publish. Explicit Stop, account/logout, Clear and Disable still cancel/fence
  writes. Capture, actual Playing, final hash/metadata and atomic commit remain
  distinct gates; HTTP no-store and validator rules are unchanged.
- Preserve the first finite failure cause. `cache=captured` reports full-body
  progress, including allowed EOS/replacement finalization, rather than a saved
  record. Only `cache=write-confirmed` reports successful publication.

These are lifecycle defects identified in the code and correction invariants,
not a claim that the owner's ambiguous `cancelled` line proves one particular race.

### Verification and remaining service acceptance

The `f2050b0` ordinary-application regressions exercised complete capture through
automatic Next/EOS and a held publication boundary, fast source before Playing,
partial Skip/Seek, consecutive Wave tracks, admission replacement, Clear/logout
and fresh refusal/changed-resource checks. Those controlled complete-body
scenarios did not establish behavior when decoder EOS precedes complete HTTP
receipt on a real source.

At `f2050b0`, stable 1.99.1 and exact 1.98.1 passed the default/all/minimal
matrix (563/571/549 library tests plus four CLI tests per configuration),
22 native selections and both CI workflows: [push](https://github.com/tracerxbrhd/karin/actions/runs/42481898361)
and [PR](https://github.com/tracerxbrhd/karin/actions/runs/42481904320), eight jobs
in total. These executed results remain valid for that immutable source, but
do not override the later real-service failure below. No installed-Flatpak
acceptance for this correction was reported.

## PR26 incomplete-source completion correction

This section records the historical design tested through `6ec2407`. The
independent-download correction below supersedes its capture/EOS contract;
its original executions and owner failures remain evidence for those revisions.

### Owner evidence at f2050b0

The owner repeated Native acceptance at
`f2050b06c99b28ca1bdb4a839ee6d2b1e4c24e1b` on Fedora 44, KDE Plasma,
Wayland and PipeWire. Sign-in and audible real Music remained working. Liked
and My Wave produced the following pair at least six times:

```text
cache=miss source=remote reason=record-absent
cache=not-cacheable source=remote reason=track-replaced-incomplete
```

No `captured`, `write-confirmed` or `hit source=local` appeared. This remains
**service-cache FAIL**; the earlier complete-body lifecycle fix and 8/8 CI do
not override the observation. The category establishes incomplete capture at
retirement, but does not identify whether the native source had finished, why
the track changed or which HTTP/body state prevented completion. Those missing
observations must not be invented from a Cloud fixture.

### Completion contract and evidence boundary

The revised path carries the actual queue/Wave transition origin and observes
native source/pipeline EOS separately. Complete capture retains the one-transfer
fast path. Only confirmed native pipeline EOS plus actual Playing can keep an
incomplete source's bounded completion operation alive while the next track
starts. A worker can then discard partial staging, check the live account/token
scope, resolve fresh delivery and request one complete replacement response; no partial ranges are
joined and no new audio engine is created. The sealed resource binding, known
original version evidence, response policy, size/hash and publication fences
still apply. Missing original headers require an independently complete fresh
response; they do not allow an old prefix to be guessed or joined. Manual incomplete Skip/Seek does not admit that request; Stop, Clear,
Disable and account/logout close pending publication immediately.

New fixed categories expose probe attachment, response status/length, body
progress buckets, source/pipeline EOS, Playing and transition origin. They do
not include exact private response values, URLs, identifiers or credentials.
`captured` remains distinct from `write-confirmed` and `hit source=local`.

Final-head compiler/native/packaging/CI results must be recorded from actual
executions at [PR #26 checks](https://github.com/tracerxbrhd/karin/pull/26/checks).
Earlier-head results do not validate this correction. Original local audio
and HTTP fixtures must exercise incomplete natural completion, auto-Next/Wave,
fresh full GET, changed/expired sources, refusals and cancellation through the
ordinary application, then prove local replay and restart without another media
GET. The held-tail fixture explicitly identifies injected native source/pipeline
EOS while the HTTP tail is gated: it controls event order rather than claiming
that the local decoder or real CDN naturally produced that order. A separate
`native-byte-segment-eos-with-partial-http-response` case bounds the actual Soup
source's byte segment at the MP3 prefix. It obtains a real 206/Content-Range and
native pipeline EOS without injecting EOS, then requires a complete conditional
GET and validated local replay. This source constraint is test-controlled; it
is not evidence that the real CDN caused the same event. Another case closes
the replacement response after 8192 bytes despite a full declared length and
requires no partial or published file. These remain Cloud evidence rather than
a real Music result. One
[final Native/installed-Flatpak procedure](BUILDING.md#pr-26-corrected-cache-acceptance)
is reserved for the finished revision; no intermediate owner runs are requested.

At `64019ca`, both compiler matrices, 22 Native executions/140 child processes
and both offline source releases passed locally. Its first push CI nevertheless
failed: the new Wave fixture rejected a later original batch outside its three
intended completion cases. Its fourth response was fully served and cleanup
released HTTP gates before stopping playback. The correction holds that fourth
response and settles/stops the player before releasing gates, retaining the
three exact payloads, original `1,1,2` markers, request counts and deadlines.
The same push also missed a distinct intermediate animation phase in the
existing HiDPI motion gate (66,668 µs native frame gap); the paired PR job passed.
That observation is retained without changing motion, renderer or acceptance
thresholds. Neither failure is erased by later source checks or service claims.

## PR26 independent background cache and playback correction

### Owner evidence and established defect

The owner reported **Native playback FAIL at `6ec2407`** on Fedora: real tracks
no longer played and resolution reported `media-origin-unverified`, with
HTTP200 for offers and the descriptor. Cache then reported
`playback-access-unavailable`. My Wave displayed a response-processing error;
no specific Wave response/stage was supplied, so it is not evidence of a shared
cause. Earlier playback acceptance is historical, not acceptance of this source.

The media-origin category is emitted when the XML descriptor's host fails the
playback host policy. The old `s` + limited digits + exactly three letters rule
also exists in earlier working revisions; it was not introduced by `6ec2407`.
Commit `3692f39` replaces this naming convention with one canonical DNS label
under the existing dedicated `.storage.yandex.net` zone. Exact zone, HTTPS,
port, path, redirect, credential and expiry restrictions remain. No actual
rejected owner hostname is available in Cloud and no newly observed domain is
claimed. Tests exercise old and newly supported label shapes, normalization
and hostile origins; they do not reproduce the owner's actual CDN response.

### Independent-download correction

The selected source starts on the existing player immediately; one bounded job
can stream its full MP3 response into the existing private AudioCache. Explicit
Play/autoplay intent starts the job, while preparation without autoplay waits
for Play. There is no source-pad capture, compressed-body queue, physical-Playing
permission, EOS finalizer or partial-byte reconstruction. Ordinary Next/Previous,
manual Skip, Seek, Pause, source replacement and Wave rollover preserve that
job. Stop/close, Clear/Disable and logout/account changes fence unfinished work.
Current access, HTTP restrictions, whole-response size/hash/format and atomic
publication remain necessary; each local replay still requires fresh access and
representation validation. See [ADR-0029](adr/0029-synthetic-audio-cache-foundation.md).

The application fixtures use original permitted audio/local HTTP through the
ordinary account, resolver, store, player and queue path. They distinguish a
complete saved file, the actual GStreamer local-file source, restart/revalidation
and media-body request counts. They also exercise full downloads across track
changes, refused/truncated/slow responses, corruption, Clear and scope fences.
Any executed results recorded for this correction must name its actual immutable
source; earlier-head CI and these test descriptions are not a new PASS.
Final compiler/native/packaging/CI results belong with the final PR #26 revision
and [checks](https://github.com/tracerxbrhd/karin/pull/26/checks).

At the initial handoff, real playback/cache acceptance of this correction was
unconfirmed. The subsequent `31d873d` owner result below confirms first playback
and saving, while local replay/restart failed. Local fixtures did not establish
that CDN result or installed-Flatpak operation. The current final
[Native/installed-Flatpak procedure](BUILDING.md#pr-26-corrected-cache-acceptance)
applies to the revised replay/indicator build. PR #26 remains Draft and unmerged.

## PR26 locator revalidation and cache indicators — 2026-11-10

### Owner result at 31d873d

The owner tested real Yandex Music on Fedora Native at
`31d873dc1b7f2349d5900dd895c4afc4be479678` and confirmed:

| Scenario | Owner result |
| --- | --- |
| First real MP3 playback | PASS |
| Independent background save | PASS: miss → queued → downloading → downloaded → write-confirmed; measured cache usage increased |
| Same-track local replay | FAIL: stale/resource-changed, followed by another download/write; no local hit |
| Local replay after application restart | FAIL: the same resource-changed/refill sequence; no local hit |
| New graphical cache indicators | Not yet accepted |
| This revision installed as Flatpak, or individual Wave/MPRIS/source combinations | Not established by this report |

This supersedes the earlier unconfirmed first-save result for that revision; it
does not relabel the `6ec2407` playback failure or earlier incomplete-cache
failures. The [31d873d push](https://github.com/tracerxbrhd/karin/actions/runs/42772868403)
and [PR](https://github.com/tracerxbrhd/karin/actions/runs/42772875846) Cloud checks
passed, but their permitted fixtures did not establish the real CDN's local hit.

### Correction and acceptance boundary

The old aggregate resource hash combined descriptor host/path with codec/bitrate.
A changed field entered the same resource-changed branch, deleted a valid record
and scheduled a replacement without establishing that its MP3 bytes changed.
The owner's finite log establishes that aggregate mismatch, not which address or
encoding component changed. No host, path or signed resource is requested here.

At `cac4d78`, the attempted correction separated logical object/variant from
physical placement, but incorrectly treated logical equality as a common
validator scope across CDN replicas. Different replica ETags/modification dates
could therefore enter the destructive representation-changed branch even for
equivalent audio. This was an unsupported inference from historical MDS source,
not confirmed current CDN byte-version semantics.

The current correction requires the exact compatible provider media URI before
using old conditionals or comparing opaque validators. A changed signed URI,
including an opaque legacy XML ts, is not presumed equivalent. Logical keys are
provenance, not global byte/version IDs. A trusted full representation digest can prove the
saved payload independently; complete-length/full-digest differences and
same-scope representation invalidation remain distinct. Same, Different and
Unknown have separate outcomes; Unknown retains the valid file and streams
online without another body save. Legacy metadata rebases only after positive
Same proof, without a download just to add binding fields. Actual version/
encoding changes, refusals, no-store and corrupt records keep their fences.

Native indicators derive Missing/Downloading/Stored from the account-scoped
AudioCache and its complete records, with worker-only integrity/file work and
no per-row HTTP. A Stored icon describes retained local data; it does not prove
that the next playback is local or available offline. Clear/Disable, LRU,
corruption and account changes must remove obsolete indications; normal restart
must recover them from actual records.

### Owner result at cac4d78

The owner reports that Fedora Native still downloads and atomically saves real
audio. Repeating the same tracks produced
`descriptor-path-changed → representation-changed → queued/downloading → write-confirmed`.
A binding-upgrade/resource-identity-unverified sequence was also observed.
**No cache=hit/source=local is confirmed.** These fixed categories do not expose
the actual CDN addresses or validator values, so their exact current behavior
must not be invented. Installed-Flatpak replay and individual indicator/Wave/MPRIS
combinations are not established by this report.

Automated correction coverage must include identical original MP3 bytes on
different replicas with different ETags or Last-Modified dates, equal tags on
unrelated scopes, signed-wrapper renewal, full Repr-Digest, actual audio/encoding
changes, repeated Unknown without another body write, proof-based legacy binding
migration and process restart. Preserve the original refusal/no-store/Clear/
logout/integrity tests and all worker-derived indicator checks. Fixtures use the
ordinary application and GStreamer with permitted original audio/local HTTP;
they do not demonstrate the real service.

The successful `cac4d78` Cloud checks remain historical evidence for what those
fixtures asserted; their logical-object/304 assumption was not a valid test of
independent replicas. Current immutable-head compiler/native/packaging/CI results
belong with PR #26. **Current real local replay remains unconfirmed** until the
single final [Native/installed-Flatpak procedure](BUILDING.md#pr-26-corrected-cache-acceptance).


## PR26 local-first selection and freshness revision — 2026-11-11

The owner confirms complete real MP3 background saving, but repeat playback at
`126b5a2` still selected the network source with `descriptor-path-changed` /
`cdn-validators-incomparable`; retained records did not establish a local hit.
This is a repeat-playback FAIL, not a demonstrated Music access refusal or byte
change. Previous compiler/fixture passes remain evidence only for their source.

The production correction checks current Music access and authenticated full-MP3
offers, then saved selection/integrity and private response freshness before
XML/CDN resolution. Selection schema 2 includes logical object and codec/bitrate/
gain/fullness; it is not an immutable provider content version. Explicit
max-age/Expires or conservative RFC9111 Date−Last-Modified freshness can admit a
local file. No-cache, stale or unclocked records retain strict validation.
Pre-schema-2 records and upgrades with unknown original response status can
migrate after inconclusive proof only through a complete
newly admitted response and atomic publication, never by renewing old bytes;
modern unknown/unclocked records do not repeatedly migrate. An expired modern lease can
renew through one complete response only when both probe and actual GET policy
permit fresh reuse. Protected quota reservation and atomic exchange retain the
old record until final publication fences, with rollback on failure. Account/
token epochs and storage/cancellation fences remain. Known selection changes retire/refill; a renewed CDN locator alone cannot retire
a fresh record.

Runtime [`2dc03b9`](https://github.com/tracerxbrhd/karin/commit/2dc03b9372a4a860be185de85bf7e40c1772ea1a)
passed the complete local matrix on stable **1.99.1** and exact **1.98.1**:
formatting, strict Clippy and **630 / 638 / 616 library tests plus 4 CLI tests**
per default/all/minimal configuration, locked debug builds, offline ordinary
release builds and the MSRV all-feature release. GTK/GStreamer/playback smokes,
four ordinary/legacy-alias Wave launch cases and fifteen hidden-input terminal
cases passed. Ignored tests are not included in those library PASS counts.

Fresh, source-hashed native test ELFs passed **22 explicit executions**, including
the ordinary application at 1×/2× on both compilers: **296 distinct child
processes and 24 receipts**. The original 55 cases remain, with 19 local-first
cases per application run. Independent audits checked actual `CachedFile`
Playing, source transitions, file identity/full SHA-256, HTTP counts, MPRIS and
cleanup. **168 synthetic PNGs** cover the retained Preferences and cache-indicator
geometry, themes and HiDPI checks; Cloud/Xvfb is not Fedora desktop acceptance.

For two original permitted MP3 resources, each **24,546 bytes**, all four
application runs recorded the following deltas:

| Operation | Actual source | XML | CDN HEAD | Full media GETs | Media bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| First selection/save of both resources, including local replays | Remote, then CachedFile | 2 | 0 | 4: 2 native + 2 independent saves | 98,184 |
| Distinct application process; MPRIS Pause/Play/Seek/Volume/Next/Previous | CachedFile | 0 | 0 | 0 | 0 |
| Further process with rotated signed wrapper/CDN host/path | CachedFile | 0 | 0 | 0 | 0 |

The latter processes retain the same payload SHA-256 and file identities after
current authenticated offer checks. Expired-policy refresh publishes one newly
admitted complete response and then returns to local playback. Tests also cover
logical/quality/gain changes, preview/protection/access refusals, missing timing,
origin clock skew versus local clock rollback, stale/no-cache/304 policy,
partial/corrupt files, account changes, Clear and quota/concurrency failures.
The ordinary UI test confirms a rejected local decoder triggers exactly one
online resolution while preserving the queue occurrence and Wave cycle.

Both vendored Flatpak-source lanes passed locked/offline release builds from
empty Cargo homes and smoke tests of their actual pinned ELFs. All **229 tracked
source files**, the manifest, **208 packages / 207 notice directories**, SPDX and
Markdown links passed independent checks. These builds use host libraries;
GNOME50 SDK execution and installed Flatpak are separate checks. The final
documentation-only commit changes no compiled source, workflow or packaging;
exact current-head hosted results and the Native build are available in
[PR #26 checks](https://github.com/tracerxbrhd/karin/pull/26/checks).

Original local HTTP/MP3 fixtures do not reproduce current Yandex CDN headers or
prove audible service local replay. No new real Music local-hit or installed
Flatpak acceptance is claimed; use the single final
[Native/installed-Flatpak procedure](BUILDING.md#pr-26-corrected-cache-acceptance).

## Known unresolved checks

### Cold Player Bar rendering in Cloud

The fresh-process Xvfb/NGL observer recorded these maximum adjacent native-frame
gaps during the first actual fixture Play:

| Execution | Scale 1 | Scale 2 |
| --- | --- | --- |
| Local final runtime source | 900,018 µs | 850,017 µs |
| Hosted `97093ee` MSRV capture | 800,016 µs | 766,682 µs |
| Earlier CI of the same Rust source, `708a23b` | 1,116,689 µs | 1,083,355 µs |

Each first appearance retained only one intermediate footer frame. Functional
Play/Pause/Stop, released hidden-footer size requests and monotonic height do
not establish smooth cold appearance. The cause and meaning of the timing
variation are not fully explained. The owner's UI acceptance does not resolve these
cloud observations or establish a driver diagnosis.

The bounded stability investigation adds a same-process real row-click repeat
only after the unchanged first Play and completed Stop. On source `193cfa0`,
Rust 1.98.1/all features, GTK 4.18.6/Libadwaita 1.7.6 and scale-1 Mesa/Xvfb
with normal damage and animations, the retained diagnostic scripts recorded:

| Run | First-open maximum gap | Repeated Play maximum gap |
| --- | ---: | ---: |
| Uninstrumented control | 916,685 µs | 33,334 µs |
| Fixed-stage timing | 1,033,354 µs | 50,001 µs |

Both four-transition lifecycle runs passed, not cold smoothness. The instrumented
cold GTK iteration took 1,015,964 µs wall / 1,009,686 µs thread CPU, containing 64
non-overlapping LLVM optimization/JIT spans totalling 655,224 µs. Repeat opening
contained three spans totalling 21,498 µs; an earlier matched run contained none.
This establishes a substantial first-render compilation
contributor, not the complete cause, exact shader/layout trigger or behavior on
a physical GPU/Wayland. A separate companion GLX context reported llvmpipe;
that is not a direct query of the application's GL context. The reveal call itself and observed worker GStreamer
startup do not account for the near-second gap; PNG exports happen afterwards.
No production fix is justified by these measurements. Followup
[#23](https://github.com/tracerxbrhd/karin/issues/23) calls for a bounded GTK-thread
stack profile and ordinary Wayland comparison. No prewarm, duration stretch,
global renderer change or weakened test/sandbox guard was introduced.

For a synthetic rerun, build the ignored native test executable using the
workflow's Cargo artifact selection and record its exact source/binary hashes.
Run these CPU-exclusively with different new private output directories:

```sh
scripts/observe-cold-playerbar.sh control /absolute/native-tests /tmp/karin-cold-control-new
scripts/observe-cold-playerbar.sh trace /absolute/native-tests /tmp/karin-cold-trace-new
```

The [runner](../scripts/observe-cold-playerbar.sh) executes only the exact fixture
on isolated X11/Xvfb. The opt-in [timing shim](../scripts/cold-playerbar-timing.c)
records fixed numeric stages, never shader contents, addresses or user fields;
it is not loaded by normal Karin. It perturbs execution, does not cover every
render path, and marks absent LLVM coverage unobserved. The tested fallback is
LLVM 19.1; unsupported symbols fail explicitly. Logs/images remain generated
local artifacts, not a repository archive or a Fedora performance PASS.

Initialized panel motion uses the unchanged 240 ms animation after separately
recorded graphics initialization, with test-only `LP_NUM_THREADS=2`. Static
captures use full redraw; motion/cold captures use ordinary GTK damage. Neither
initialized motion nor a complete scene-node image proves cold responsiveness,
ordinary framebuffer-damage correctness or performance on a physical GPU.
There is no production prewarming or renderer/timing override.

### Environment and service boundaries

The earlier cloud check did not build or install the complete current Flatpak:
GNOME SDK 50/runtime was absent there. The owner's separate Fedora Flatpak
acceptance is recorded above; it does not identify every portal/keyring backend.
Real accounts, tokens and Music content were not used in Cloud/CI. Generated
WAV/MP3 playback is an engine regression test, not audible Yandex Music proof.

Public OAuth-identity distribution conditions, unsupported service features and
persistent audio remain documented in [limitations](LIMITATIONS.md),
[authentication](AUTHENTICATION_REVIEW.md) and [licensing](LICENSING.md).

## Durable history

Detailed intermediate reports and redundant images were removed from the active
documentation tree during finalization, without rewriting Git history. Important
results remain identifiable in immutable source snapshots:

- `a9eed45` passed its cloud checks but failed the owner's Fedora UI/UX review.
  It is a historical FAIL, not acceptance of that design; see the
  [earlier validation record](https://github.com/tracerxbrhd/karin/blob/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d/docs/VALIDATION.md).
- Hosted `144492f` missed the 75%-openness frame in two strict 2× panel jobs.
  The [failure records](https://github.com/tracerxbrhd/karin/tree/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d/docs/design/v0.4/evidence-refinement/failures)
  retain those outcomes. Later scoped passes do not erase them.
- The intermediate C12 sweep had **48 PASS / 8 FAIL**, including a real hidden
  footer minimum-width defect and stale fixture assumptions. Its
  [summary and logs](https://github.com/tracerxbrhd/karin/tree/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d/docs/design/v0.4/evidence-final-refinement/metadata/history/candidate-12)
  remain in Git. They are not relabeled as the later passing runtime.
- The [accepted-source evidence](https://github.com/tracerxbrhd/karin/tree/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d/docs/design/v0.4/evidence-final-refinement)
  retains original source/image hashes, raw local cold observations and the
  disclosed 850×750-only adaptation to the `2e3ae41` comparison harness.

CI visual artifacts were retained for seven days and native prototypes for
fourteen days. They may expire. Git source, the regression fixtures, required
licenses/provenance and the essential facts above are the durable record;
temporary artifact downloads are supplementary evidence.

## Repeating the checks

Follow [BUILDING.md](BUILDING.md) for native/Flatpak prerequisites, build/run
commands and the current Cargo checks. Repeat them with Rust 1.98.1 as well as
stable; fetch the locked dependencies before the offline release build.

Use synthetic loopback fixtures for automated authentication/catalog tests and
an isolated session bus/display for native tests. Keep WebKit sandboxing, TLS,
callback/state checks, bounded requests and strict motion assertions enabled.
Exercise real-account Music and desktop integration only on the consenting
owner's local installation, without copying credentials or personal captures
into Cloud/CI/GitHub. Use [UI_QUALITY_GATE.md](UI_QUALITY_GATE.md) for substantial
UI changes, and retain the exact source revision and environment with results.

## v0.7 combined continuation, playlist and appearance change

[PR #28](https://github.com/tracerxbrhd/karin/pull/28), branch
`feature/v0.7-playback-playlists-appearance`, started at the accepted normal
PR #26 merge `1fd4028` and normally merged at `2ab6d28`. The owner separately
accepted `949d1f9` with 68/68 Native/installed-Flatpak checks, as recorded above.
The following Cloud fixtures remain distinct evidence of their individual
synthetic scenarios; acceptance was not inherited from the earlier audio/cache path.

The changes add account-bound IDs-only ordinary playback snapshots, exact
occurrence/Shuffle continuation without autoplay, fresh batch normalization and
explicit cache/resolver Play with saved seek. Stop/forget/logout have durable
suppression; radio is never serialized. Confirmed playlist Insert/Remove/Move,
Rename/Visibility/Delete use ownership, complete positions/revision, one POST,
independent readback and GET-only uncertainty recovery. Metadata endpoints have
no confirmed server revision CAS. System presentation and color scheme are
independent; schema 3 migrates old preferences, while Karin Style remains visibly
unavailable. See ADR-0031/0032/0033 and [the implementation boundary](ARCHITECTURE.md).

The native registration now includes 38 selectors: the original 37 plus
`native_v07_playback_restore_playlist_appearance_process_restart`. Its real OS
subprocesses use the ordinary MainWindow factory, a synthetic protected-store
backend, current account HTTP validation, production cache/resolver policy,
original CC0 audio and actual GStreamer/MPRIS. Counted requests distinguish local
CachedFile replay from a new media GET; fresh offer validation still occurs.
The same retained profile exercises restart, exact duplicate/current/order/mode/
position, missing/deleted sources, unavailable/wrong-account/corrupt/version
records, Stop/logout and honest partial views. Held credential-save fixtures
cover manual sign-in and fast-close/restart; a held batch reply tests logout
before late restoration can publish. A separate HTTP 503 phase seeds an inherited
storage retry and invokes ordinary periodic saving without a forced flush: the
valid record must remain byte-identical, then native Retry must recover its
queue/position without an audio request. Real native editor clicks while
cached audio plays cover each mutation, Cancel-default deletion and stale-row
callbacks. Appearance changes must retain that exact queue and player worker.
New dialog captures supplement measured GTK layout at compact/medium/wide,
both themes and both scales; Preferences additionally checks synthetic desktop
scheme/high contrast, native focus and unavailable-style presentation.

Ignored listings are not passes. Final executed results and immutable source/
artifact identities belong to [PR #28 checks](https://github.com/tracerxbrhd/karin/pull/28/checks)
and its handoff report. Existing timing/geometry, navigating WebKit sandbox,
Wave, MPRIS, native cache-process and archived visual-baseline gates remain.
Cloud original-audio/HTTP fixtures establish no real service mutation, physical
PipeWire output, KDE portal behavior, installed Flatpak or future Karin design
acceptance. Use the [single final procedure](BUILDING.md#v07-final-fedora-acceptance).
