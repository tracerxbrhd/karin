<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Validation

Owner acceptance, automated checks and performance observations have different
scopes. A fixture or screenshot proves only the recorded environment; ignored
test registration is not an executed PASS. Exact-head checks and measured CI
costs belong in the [PR #29 checks](https://github.com/tracerxbrhd/karin/pull/29/checks)
and its final review summary, rather than a repository log of every run.

## Current acceptance

In the final v0.8 task on **2026-11-22**, the owner reports completed Fedora
acceptance of the application's functionality, with three remaining changes:
remove the rejected experimental presentation, clean the repository and optimize
CI. The reported baseline is
[`42f811b`](https://github.com/tracerxbrhd/karin/commit/42f811be955247efde93bcc2ef6df1f700cc856a).
The owner had also explicitly confirmed synchronized karaoke at `d345fec`.

The accepted product includes System presentation, karaoke, Track Info/Lyrics/
Queue, global search, collection indexing/sorting, playlist/queue management,
MPRIS, My Wave, AudioCache and long-pause continuation, with Native and Flatpak
support. This supersedes the intermediate v0.8 pending/failed acceptance wording;
the historical failures remain identifiable below. No repeat of the full owner
acceptance is required for this cleanup.

The owner conditionally authorized an ordinary merge of **PR #29** after the
cleanup and required exact-head checks succeed. That authorization does not
publish a release or tag, accept the unavailable Karin design, establish public
service/content rights or resolve the independent cold-rendering investigation.
Do not claim a merge until GitHub confirms it.

## Required automated evidence

The [workflow](../.github/workflows/ci.yml) and
[build guide](BUILDING.md#development-checks) are the executable source of truth.
Coverage must retain:

- Formatting, strict Clippy, default/all/minimal feature tests, stable and exact
  Rust 1.98.1 compatibility, and a locked release in the Flatpak feature configuration.
- Account/session isolation, protected storage, cancellation and late-result fences;
  cache budgets, HTTP policy, complete publication, local reuse and restoration.
- Generated-audio GStreamer transport, seek/volume/EOS/cleanup, MPRIS and My Wave;
  exact queue occurrences, confirmed playlist/reaction writes and readback.
- Native GTK input, collection/global search, indexing/sorting, Lyrics consent and
  provider-to-LRC playback, pinned-panel behavior and System geometry/contrast.
- Targeted HiDPI/Reduced Motion and normal panel/Lyrics motion; the raw cold
  observer remains diagnostic evidence, with its unresolved result reported honestly.
- Navigating WebKit with its sandbox enabled; license/SPDX inventory, manifest
  checks, vendored Flatpak-source preparation and locked/offline compilation.
- Safe fallback of a removed saved style to System, preserving other settings
  through save/restart, and the unavailable Karin presentation boundary.

The [extended native workflow](../.github/workflows/extended-native.yml) runs the
current signed-out gallery manually; historical comparison sources remain in Git.
This does not remove underlying regression assertions from required coverage. CI evidence uses synthetic accounts, original art/text/audio and local
HTTP only; it contains no Music credentials, private content or signed resources.
A source build, installed Cloud Flatpak and heard Fedora Music are separate results.

## Remaining evidence limits

- Public OAuth/service/content distribution conditions remain unresolved in
  [#5](https://github.com/tracerxbrhd/karin/issues/5) and
  [#22](https://github.com/tracerxbrhd/karin/issues/22).
- Cloud cold Player Bar rendering remains unresolved in
  [#23](https://github.com/tracerxbrhd/karin/issues/23). Historical fresh-process
  Mesa/Xvfb observations contained roughly 0.77–1.12 s adjacent-frame gaps.
  Bounded profiling found substantial LLVM optimization/JIT work, without proving
  the whole cause or ordinary physical-GPU/Wayland relevance. Successful lifecycle,
  initialized motion or desktop acceptance does not convert this into PASS.
- General owner acceptance does not certify every optional keyring/portal backend,
  media key, refusal, network-failure or catalog-matching scenario. Synthetic
  failures protect those contracts without making live-service claims.

## Acceptance history

| Milestone | Durable evidence |
| --- | --- |
| v0.4 functionality, UI and separate Flatpak acceptance | [`97093ee` record](https://github.com/tracerxbrhd/karin/blob/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d/docs/VALIDATION.md); [PR #15](https://github.com/tracerxbrhd/karin/pull/15) |
| Stability and ordinary My Wave | [PR #24](https://github.com/tracerxbrhd/karin/pull/24), [PR #25](https://github.com/tracerxbrhd/karin/pull/25); [record at the v0.7 merge](https://github.com/tracerxbrhd/karin/blob/2ab6d280af9ee6c5a1d5cb0107a30149f7d87a8d/docs/VALIDATION.md) |
| Real Native/installed-Flatpak MP3 saving, local hits and restart-local playback | [`c1de869`](https://github.com/tracerxbrhd/karin/commit/c1de869361f36f5c749dec2b283ce56a356f06f2); [PR #26](https://github.com/tracerxbrhd/karin/pull/26), normal merge `1fd4028` |
| Confirmed Dislike→Next, ordinary queue and Wave | [PR #27](https://github.com/tracerxbrhd/karin/pull/27), merge `ddf93ea` |
| Restoration, real playlist edits and System appearance: 68/68 Fedora checks, Native 34/34 and installed Flatpak 34/34 | [`949d1f9`](https://github.com/tracerxbrhd/karin/commit/949d1f9315b210a118e02762a24d7747ff8e5321); [PR #28](https://github.com/tracerxbrhd/karin/pull/28), normal merge `2ab6d28` |

Downloading/Saved graphics were provisionally accepted with the cache milestone;
future designer materials remain separate. Full intermediate v0.8 failures,
repairs, source identities and performance receipts remain in the
[immutable pre-cleanup validation record](https://github.com/tracerxbrhd/karin/blob/42f811be955247efde93bcc2ef6df1f700cc856a/docs/VALIDATION.md)
and [PR #29](https://github.com/tracerxbrhd/karin/pull/29).
The superseded [PR #26 revision audit](https://github.com/tracerxbrhd/karin/blob/42f811be955247efde93bcc2ef6df1f700cc856a/docs/PR26_REVISION_AUDIT.md)
and [backlog audit](https://github.com/tracerxbrhd/karin/blob/42f811be955247efde93bcc2ef6df1f700cc856a/docs/ISSUES_AUDIT.md)
remain in Git. Earlier failures are historical outcomes, not the current status.

The accepted v0.4 [original visual evidence](https://github.com/tracerxbrhd/karin/tree/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d/docs/design/v0.4/evidence-final-refinement)
retains source/image hashes and the disclosed comparison-harness adaptation.
Full current captures/logs are CI artifacts with bounded retention; immutable
commits, PRs and essential facts above preserve history when artifacts expire.
