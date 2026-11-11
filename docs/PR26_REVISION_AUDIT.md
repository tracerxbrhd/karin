<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# PR #26: Audit of the unaccepted online-cache revisions

Date: 2026-11-06

## Reviewed boundary

The owner identifies `0709729` as the last accepted revision. This audit covers
all eight subsequent commits through
[`0f9257fc3b627e05734021c5500e95cff118e32e`](https://github.com/tracerxbrhd/karin/commit/0f9257fc3b627e05734021c5500e95cff118e32e).
The range changes only `src/audio_cache.rs`, `src/audio_cache/bridge.rs`, its
tests and `src/ui/music/persistence.rs`. It does not change the shared
`CacheBudget`, settings migration, quota choices, GStreamer engine, authentication
or Flatpak permissions. This document records the reviewed baseline and the
required correction; final implementation/test evidence belongs in
[VALIDATION](VALIDATION.md#v06-audio-cache-foundation).

Review used the immutable commit diffs, production call sites, existing storage
and playback contracts, [AGENTS](../AGENTS.md),
[ADR-0029](adr/0029-synthetic-audio-cache-foundation.md) and
[ADR-0030](adr/0030-unified-cache-budget-and-preferences.md). Code inspection and
Cloud fixtures do not establish a service storage permission or Fedora acceptance.
The Cassette/service evidence is recorded separately in
[AUDIO_CACHE_REVIEW](AUDIO_CACHE_REVIEW.md).

## Every commit in the range

| Commit | Actual change | Review disposition |
| --- | --- | --- |
| [`73aed76`](https://github.com/tracerxbrhd/karin/commit/73aed76fac54fc725111689d93ec03a406d7e76f) | Adds `OnlineStreamAuthority`, a resolver observation hook and a single current in-memory `StorageGrant`. Every successful observation creates a grant. | Preserve the reusable online-admission boundary, but replace permission creation with composition over a separate storage-permission authority. Add attempt/session fencing. |
| [`282ece1`](https://github.com/tracerxbrhd/karin/commit/282ece172644a1e9a34b54446acba0f851542b34) | Observes fresh resolution before cache selection and rejects expired sources early. | Retain expiry and fallback guards. Start a freshness ticket before network work, so failed or late attempts cannot leave/revive a previous admission. Update the expiry regression for the early rejection. |
| [`a0e18a9`](https://github.com/tracerxbrhd/karin/commit/a0e18a93458e264b651e61f29522570afc1da059) | Replaces `MissingStorageContract` with default `OnlineStreamAuthority` in ordinary Native and Flatpak store construction. | Unsafe permission inference under the reviewed authority semantics. Ordinary construction must remain closed for real service writes until an applicable storage contract is provided. A default wrapper may stay only if its independent delegate refuses permission. |
| [`545a9f1`](https://github.com/tracerxbrhd/karin/commit/545a9f149b4dc9bd9a15f0dbcde859fc301fc3e9) | Adds local HTTP coverage for fill, repeat hit, reopening and account mismatch. | Retain useful production-path coverage with an explicit fixture storage grant. Extend it for failed resolution, stale completion and the actual Playing-triggered player path; it cannot justify a real-service storage grant. |
| [`b5424ae`](https://github.com/tracerxbrhd/karin/commit/b5424aec19e4804b9a0a5aee5939c76c70b0a17e) | Wraps the new regression-test import. | Formatting-only; no runtime correction. |
| [`c98a7ea`](https://github.com/tracerxbrhd/karin/commit/c98a7ea161ec364eb8015e3dc19c0c675dbdb288) | Adjusts that import and wraps a restart assertion. | Formatting-only; no runtime correction. |
| [`9f3d409`](https://github.com/tracerxbrhd/karin/commit/9f3d409242c669b1b40ba123aa65b1ef3663df71) | Reformats the observation method signature. | Formatting-only; no runtime correction. |
| [`0f9257f`](https://github.com/tracerxbrhd/karin/commit/0f9257fc3b627e05734021c5500e95cff118e32e) | Wraps ordinary shared-budget store construction. | Formatting-only. Exact-head CI passed formatting; remaining failures are described below. |

## Confirmed defects and architectural consequences

### Streaming success creates a storage grant

At the reviewed head,
[`OnlineStreamAuthority::observe_verified_online`](https://github.com/tracerxbrhd/karin/blob/0f9257fc3b627e05734021c5500e95cff118e32e/src/audio_cache.rs#L168)
increments a local revision and creates `StorageGrant` without consulting an
independent permission authority. The ordinary application's
[`open_audio_store`](https://github.com/tracerxbrhd/karin/blob/0f9257fc3b627e05734021c5500e95cff118e32e/src/ui/music/persistence.rs#L309)
instantiates that authority. Consequently a fresh playable source enables media
copying under the ordinary build despite the absence of a Karin-applicable
storage/reuse contract. A comment saying this is not a license does not preserve
the actual permission gate. Existing documentation and Preferences also continue
to describe service audio storage as unavailable.

Correction: keep online account/resource admission separate from storage
permission. `OnlineStreamAuthority::with_storage_permission` must compose the
fresh-resolution check with an independently supplied `StorageAuthority`; its
default delegate remains `MissingStorageContract`. Preserve and recheck the
delegate's opaque grant/revision. An online attempt number is a separate local
freshness fence, not a replacement for that revision. No user switch, synthetic
authority or successful stream response may enable real Yandex service writes.

### Late observation can revoke a newer session's valid work

The reviewed
[`AudioCache::observe_verified_online`](https://github.com/tracerxbrhd/karin/blob/0f9257fc3b627e05734021c5500e95cff118e32e/src/audio_cache.rs#L531)
checks a lease, mutates the independent authority state, then checks the lease
again. `bind` and `fence` can run between those operations. An old worker can pass
the first check, be superseded by a new session/selection, overwrite that newer
grant and finally return `Cancelled`. The newer pending Hit/Fill then sees
`Revoked`. Account/identity checks still prevent the old lease from accessing the
new account; this finding concerns stale-state mutation and lost valid work,
not a demonstrated cross-account data disclosure.

The one current observation also survives a later failed resolution because
observation happens only after resolution success. It does not by itself enforce
the claim that every selection must acquire a new admission.

Correction: begin an opaque online-check ticket before network resolution,
retiring previous admission for that selection. Bind it to the cache owner,
account/resource, request order and current session/policy generations. Admit
completion only while those bindings remain current, with ordering that prevents
a retired completion from replacing a newer admission. Failed, cancelled and late
resolutions must not revive earlier tickets. Preserve already admitted playback
FDs and valid payloads; losing an in-memory admission is not evidence that their
bytes became corrupt. Test these interleavings deterministically.

### The expired-source test still expects work that is now rejected early

The new
[`select` guard](https://github.com/tracerxbrhd/karin/blob/0f9257fc3b627e05734021c5500e95cff118e32e/src/audio_cache/bridge.rs#L71)
returns `fill: None` for an expired remote source. The existing
[`changed_disk_bytes_and_expired_remote_cannot_be_used_as_a_hit_or_fill`](https://github.com/tracerxbrhd/karin/blob/0f9257fc3b627e05734021c5500e95cff118e32e/src/audio_cache/bridge/tests.rs#L572)
still unwraps a Fill and expects cancellation when running it. This is a
deterministic test-contract regression, observed on both compilers in CI.

Correction: retain the early expiry guard and assert no cached source, no Fill,
no additional media request and no published record. Do not remove expiry checks
or relax storage/network bounds to satisfy the old assertion.

## Useful behavior retained, with limits

The new fixture shows a real production bridge fill followed by a local hit,
reopening with a fresh online resolution, and rejection of an account mismatch.
It uses generated content and local HTTP. Its first fill calls `Fill::run`
directly, so this test alone does not establish automatic background copying
after actual GStreamer Playing, normal source-change cleanup or Fedora playback.
Those require the native player integration fixtures and owner acceptance.

The disk record header stores opaque identity, size and payload integrity, not
the in-memory `StorageGrant` revision. A new online revision therefore does not
inherently invalidate an otherwise valid disk file. Restart/revalidation is a
viable technical path; the defects above must not be described as a nonexistent
disk-revision format mismatch.

Retain the existing credential-free bounded media GET, response restrictions,
atomic complete-record publication, integrity checks, pinned read-only playback
FD and one guarded pre-Playing online fallback. Keep one shared quota and the
existing six slider positions unchanged. No new audio engine, broad Flatpak
permission, stored signed address or DRM mechanism is part of this correction.

## Implemented correction

The corrective revision retains the production store, resolver bridge, early
expiry guard and actual-Playing background worker. Its online authority now
composes an independent storage permit, with default `MissingStorageContract`.
The provider's permission revision remains separate from the local attempt fence.
A random owner/session/budget admission context is memory-only; disk keys and the
record format are unchanged, so existing healthy records can be revalidated.

An opaque one-use check starts before transport. Permission preparation happens
outside GTK's state lock; a memory-only commit rechecks the current lease,
attempt, cancellation, token and source freshness. Clear/account/policy changes,
late work and replay cannot replace a newer admission. Cleared/expired sessions
retire their own admission even before HTTP; a positively different account
cannot retire the current account's grant. Cached-source construction and final
GTK/worker admission also reject an aged source without interrupting an already
playing file descriptor.

The stale expired-source assertion is corrected. Existing application fixtures
now use the production coordinator with independent permissions for original
generated media, shared quota, Ready→Playing gating, background save, local
repeat playback, controller/store recreation and fresh account validation,
HTTP403 refusal/retry, source cancellation and logout. Deterministic core tests
cover blocked permission lookup, cancellation, late completion, revision changes
and one-use tickets. These tests grant no service permission. Executed results
and pending desktop checks are recorded in [VALIDATION](VALIDATION.md#pr26-post-acceptance-correction).

## Exact-head CI evidence and remaining acceptance

The completed Rust jobs of
[run 42218704949](https://github.com/tracerxbrhd/karin/actions/runs/42218704949)
at `0f9257f` report the following on stable and exact Rust 1.98.1:

- Formatting: passed.
- Clippy and experimental-feature Clippy: passed.
- Unit/local integration tests: **485 passed, 1 failed, 36 ignored**. The failure
  is the expired-source `Option::unwrap()` described above.
- Later functional tests and release packaging in those jobs: skipped after the
  unit-test failure. The missing functional-evidence upload in the stable job is
  a secondary consequence, not proof of a separate audio failure.

The duplicate exact-head run
[42218696978](https://github.com/tracerxbrhd/karin/actions/runs/42218696978)
shows the same Rust-test failure. At the time of inspection the separate modern
Native UI jobs had not completed; this audit does not claim their result.
Formatting failures from earlier revisions do not describe the final reviewed
head. These results do not validate the subsequent corrective working tree.

Final corrective-head stable/MSRV, native GTK/WebKit/GStreamer, Flatpak-source
and CI results must be recorded separately after execution. Fedora Native and
installed-Flatpak acceptance of the earlier revision does not accept this range.
The service first-play/save/reuse/restart criterion remains unconfirmed while an
applicable storage/reuse contract is absent. Infrastructure fixtures must not be
reported as successful caching of a real Yandex track. PR #26 remains Draft;
correction uses ordinary commits without history rewriting or automatic merge.
