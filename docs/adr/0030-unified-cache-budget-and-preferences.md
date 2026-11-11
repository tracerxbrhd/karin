<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0030: One cache policy and shared disk budget

Status: Implemented; final verification and desktop acceptance recorded separately
Date: 2026-11-06
Extends: ADR-0018, ADR-0019, ADR-0026 and ADR-0029
Supersedes their separate user quotas/defaults, not their data or admission rules

## Context

Metadata, permitted artwork and authority-admitted audio have different codecs,
freshness and access requirements. The earlier two independent quotas made
Preferences confusing and could not enforce one requested total. Merely hiding
one limit would retain that defect. Existing credentials, cache files, account
partitions and accepted playback must survive this policy change.

## Decision

Keep typed stores and their private XDG namespaces; use one shared `CacheBudget`
instance in the ordinary application's metadata/artwork and audio stores. The
coordinator owns aggregate admission, accounting and eviction. Store-local byte
policies remain available for standalone callers/tests; the application's shared
mode does not apply them as two additional quotas.

The user policy is one enabled switch (default **on**) and one `CacheLimit`:
**500 MiB, 1 GiB, 2 GiB, 3 GiB, 4 GiB or Unlimited**. Default **2 GiB**.
Preferences expose a native horizontal integer Scale at positions 0–5 with a
textual value and accessible name/value. Unlimited is `None`, not an invented
large integer. There is one measured usage, Clear, busy/error state and Retry.
A short explanation identifies measured cached data without promising offline
playback. Audio row indicators derive complete-record presence independently of
the next online access/version decision under ADR-0029.

### Shared storage accounting

Every writer reserves a bounded upper size, including its record header, before
creating staging content. Concurrent writes see completed records, pending
reservations and recognized staging overhead in one budget. A slow media copy
holds a reservation, not the shared I/O mutex. Publication rechecks the current
policy generation; completion/cancellation releases its reservation. A disable,
Clear, account change or replaced generation cannot admit a late publication.

The coordinator scans only bounded, recognized private record layouts under the
two Karin cache roots; it never follows an unexpected link or recursively deletes
unknown content. Existing record names/formats are retained. Record reads still
validate their own identity, schema, bounds and digest: aggregate accounting is
not integrity validation or permission to use data. Usage reports measured
regular-file contents and known staging, not filesystem allocation estimates,
credentials, pending reservation estimates or music that was never saved.

Eviction uses shared persisted access ordering, with a small metadata allowance
inside the same total: audio admission preserves up to `min(8 MiB, quota / 8)`
for small metadata. Metadata above that allowance remains evictable. Small
metadata can reclaim large audio before artwork/other metadata. This protects
useful small records without giving each category an independent quota.

A reduced limit fences old admissions immediately and schedules bounded worker
trim. Cache-limit saves/maintenance are coalesced after **200 ms** of inactivity;
intermediate pointer movements cause no synchronous disk scan on GTK. Disabling
and Clear fence operations immediately rather than waiting for that timer.

Unlimited removes the user byte ceiling. It retains format/count/size limits and
requires available disk headroom before a new write: **1% of filesystem capacity,
clamped to 64–512 MiB**, plus pending reservations. The same disk-space guard
applies to finite limits. Allocation failure still aborts publication and leaves
valid online playback usable. These are local resource safeguards, not a service
retention period or an assurance that external processes cannot fill the disk.

### Settings migration

Write a version-2 preferences envelope containing the enum. Accept version 1's
`cache_enabled`, `cache_limit_bytes` and optional `audio_cache_limit_bytes`:

- Preserve enabled/disabled state, appearance, remember-login, forget barrier
  and Track Info intent. A missing legacy audio limit uses its old 256 MiB default.
- The former default pair, 256 MiB + 256 MiB, becomes the new 2 GiB default.
- Sum valid custom legacy limits and round **up** to the nearest offered finite
  position. A sum above 4 GiB becomes Unlimited, avoiding a silent custom-budget
  reduction. Normal quota enforcement may later evict records; migration itself
  neither deletes files nor resets authorization.
- Invalid legacy quota numbers or a malformed version-2 quota recover that choice
  to 2 GiB while retaining other valid fields. Unsupported/malformed envelopes
  retain the existing bounded corrupt-file recovery behavior. Never erase a
  separate protected credential as a quota-repair side effect.

### Preserved boundaries

Store/account leases, refusal fences, metadata/artwork TTL, current cache-hit
permission, integrity, cancellation and atomic complete-record publication remain.
Clear keeps active memory/session state and an already admitted playback FD;
logout purges the retired account. There is still one PlaybackController and no
additional daemon or wider Flatpak permission.

The later private HTTP policy in ADR-0029 supersedes the original blanket
MissingStorageContract refusal for ordinary supported audio. Shared quota,
Unlimited, a subscription or HTTP200 still supplies no content license. Actual
response policy, fresh playback access and matching selection with a valid reuse
lease or scoped representation proof remain required; [AUDIO_CACHE_REVIEW](../AUDIO_CACHE_REVIEW.md) records that boundary.
The prepared provider questions are historical context, not an invented cache-hit
API prerequisite. Audio presence indicators read the same store and disappear on
shared LRU eviction; they neither reserve a second quota nor authorize local use.
Issue #20 remains open until the complete real user scenario is accepted.

## Verification and consequences

Test cross-store competition and bounded reservations, LRU/metadata fairness,
restart, finite/Unlimited disk exhaustion, unsafe paths, quota migration and
round trips. Preserve the original store regressions and cache failure/online
fallback. Native tests must drive the actual slider by pointer and keyboard,
check all six values and capture compact/wide, themes and HiDPI without relaxing
focus/hit-test deadlines. Verify Clear, cancellation, logout, normal music, Wave
and MPRIS on both Rust compilers. Record executed results in
[VALIDATION](../VALIDATION.md); owner acceptance of the previous revision does
not establish acceptance of this new policy/UI. PR #26 stays Draft until its
remaining service-storage and acceptance criteria are met.
