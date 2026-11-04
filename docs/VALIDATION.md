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

## v0.5 My Wave experiment

The branch from `920fd71` adds an explicit `--experimental-wave` local path.
Normal startup remains gated. Cloud uses synthetic HTTP and generated media,
without a real account; the owner's subsequent real results are separate below.
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

This confirms a positive live chain with an unresolved refusal, not complete Wave
acceptance. Issue #16 stays open; Issues #5 and #23 are separate and unchanged.

### Production-integration revision

The new Home Hero uses the shared player, has no active Finish control and limits
Cancel to startup. Pause preserves the session; ordinary source selection and
MPRIS Stop retire it internally. Logout/close fence queued and late work. Refused
feedback now has one fixed diagnostic with the submitted enum event, lifecycle
phase and origin/reason, without provider bodies, identifiers or resources.
No proven wire violation was found in the refreshed published protocol review:
accepted `event.from`, original markers, actual heard time and request policy
remain unchanged. In particular, a synthetic `radioFinished` refusal does not
prove actual service support or explain the owner's event.

The added HTTP fixture reproduces an accepted cross-batch prefix and independent
400s for each of the five local event kinds, checking poisoning, redaction and no
replayed POST. Native tests exercise the real shared worker and Home controls;
these synthetic refusals verify handling, not resolution of the real 400.

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

Repeat [the single Fedora procedure](BUILDING.md#my-wave-local-acceptance),
including natural EOS, skips, source replacement, Stop and safe refused-event
classification. This revision is not newly accepted on Fedora or in an installed
Flatpak. Default Wave enablement remains blocked until the refused sequence is
identified/corrected and its repeat accepted.

### Separate post-squash startup failure

The first [master run at `920fd71`](https://github.com/tracerxbrhd/karin/actions/runs/41814183326/attempts/1)
passed three jobs but failed the MSRV workspace fixture at its initial 1440×900
resize: the native window allocation remained **0×0** until the existing
eight-second deadline. This is separate from PR #24's eight successful
accepted-head checks and owner acceptance. Its exact cause is unestablished.
The original failure is retained; an unchanged failed-job rerun has its own
attempt in that run. No deadline, renderer policy, sandbox or production timing
was relaxed, and this is not a resolution of Issue #23.

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
