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
record does not announce a tagged or published release.

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

These are revision-specific results. For a later repository-cleanup commit,
check the [current PR checks](https://github.com/tracerxbrhd/karin/pull/15/checks)
for that exact HEAD; the runs and packages above do not become new-tree results.

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
variation remain unknown. The owner's UI acceptance does not resolve these
cloud observations or establish a driver diagnosis.

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
