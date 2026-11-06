<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0029: Production audio-cache boundary and service admission

Status: Production infrastructure; real service storage/reuse remains blocked
Date: 2026-11-05
Extends: ADR-0015, ADR-0019 and ADR-0026
Supersedes the test-only implementation boundary of the original PR #26 candidate

Resource-policy extension, 2026-11-06: [ADR-0030](0030-unified-cache-budget-and-preferences.md)
replaces the user-facing quota/defaults below with one shared budget and migrates
settings. Typed formats, admission, integrity and lifecycle boundaries remain.

## Scope and evidence

The first candidate at `0ce9ea0` compiled its store only under `cfg(test)`. That
proved storage mechanics but supplied no reusable audio cache to the ordinary
application. The owner also reported its namespace-reopen test failing with
`Busy` on Fedora under both compilers. Both limitations must be addressed; a
storage bug is independent of service permissions.

The immediate product target is transparent persistent caching of listened tracks
with an online check before reuse. Downloads, mass prefetch and offline playback
are outside this iteration. [AUDIO_CACHE_REVIEW](../AUDIO_CACHE_REVIEW.md) records
Cassette's working background save/local-first path and the primary service terms.
No Karin-applicable storage/reuse permission or concrete provider contract for
fresh account/resource admission of cached bytes has been established. A full
offline lease is a separate requirement for a later offline feature, not an
invented prerequisite for the online-only cache.

## Production boundary

Compile `audio_cache` into ordinary Native/Flatpak builds.
Keep account isolation, integrity validation, atomic publication, cancellation,
quota/LRU and restart recovery in this shared implementation. Generated-media
fixtures exercise the same store; the fake permission authority and original WAV
constructor belong only to tests. Do not keep a second unused synthetic store.
`StorageAuthority::authorize` runs on a worker; `check` examines its existing live
grant/revocation state without I/O. `StorageGrant` carries an opaque resource
identity and authority revision. The local namespace discriminator is a storage
format detail, not a claimed provider device identity.

Cache storage/reuse permission is distinct from streaming-source admission. The
`MissingStorageContract` cannot issue a positive cache grant from a subscription,
Music account success, track metadata or an MP3 URL. Until a concrete grant/check
contract exists, it returns an explicit unsupported-admission result and preserves
the established online StreamResolver path. This decision does not claim that
all third-party storage is categorically forbidden or that the cache feature is
finished.

A cache hit or publication must pass current account/resource permission and
account/request generation checks. The source adapter carries opaque identity and
permission state in memory; it never serializes OAuth, cookies, personal models,
raw service responses or signed media addresses. Unknown, expired, revoked or
refused permission cannot be replaced by an arbitrary local TTL. Cache failure
must not turn an otherwise valid online playback attempt into a playback error.

Use `$XDG_CACHE_HOME/<app-id>/audio`, separate from metadata/artwork, with private
0700 directories/0600 files, SHA-256 integrity and atomic publication of complete
records. Hashes are integrity/opaque-name mechanisms, not encryption. The default
completed-record budget is 256 MiB; Preferences allow 16–4096 MiB. Hard bounds are
1024 records, 64 MiB per resource and four pending requests. One serialized writer
can temporarily add one bounded staging file, at most 64 MiB, before publication
and eviction. I/O belongs on workers. Clear, logout/account changes and cancellation fence pending writes
before cleanup. Incomplete/corrupt entries are not hits. Independent namespace
owners cannot create conflicting generations; stale requests/hits must not regain
admission after reopen. Normal close may retain admitted entries for revalidation
on a later online launch. Final-owner destruction explicitly unlocks its `flock`:
a fork/dup-retained open file description must not keep a dead owner's namespace
busy. Live strong owners still exclude a second owner; weak tickets cannot revive
the namespace. The destructor is owner-process guarded.

The existing StreamResolver performs fresh online admission before cache selection.
Only an authority-admitted hit supplies the verified read-only file descriptor to
the single PlaybackController. Playback uses that pinned file, not a path reopened
after validation. A cache miss retains the existing remote source; an admitted
write starts only after the actual Playing acknowledgment. A decoder failure
before any actual Playing gets one guarded fallback to the already admitted
online source, preserving pending transport intent in the same worker. After
actual Playing, a failure remains the normal player error; there is no seamless
mid-track retry or reset of Wave's measured-playing ledger. There is no parallel
audio engine or repeated resolution loop.

An independently granted fill makes one bounded whole-resource GET with the
existing credential-free resolver client. It requires HTTP 200 and one
`audio/mpeg` content type, rejecting redirects, `no-store`, Content-Encoding and
Content-Range. The existing request deadline and 64 MiB streaming limit apply.
No OAuth token or cookie is forwarded to the media host; no service grant means
no fill request or media write.

No second GStreamer engine, daemon, DRM mechanism,
new OAuth identity or broad Flatpak permission follows. Cache maintenance and
settings report measured storage state and a separate audio quota. Combined Clear
removes metadata, artwork and audio records while preserving the active validated
session and an already admitted playback file descriptor. Logout purges its audio
scope; account switches fence the old scope. No saved/offline Music badge follows.
A disabled service admission path is an explicit remaining blocker.

## Validation and remaining acceptance

Preserve all original storage scenarios and add regressions for namespace lifetime,
reopen/restart, production hit/miss/write/reuse, current permission checks,
integrity/fallback, cancellation/concurrency, quota/LRU, cleanup and account changes.
Exercise native settings/lifecycle and shared-player behavior without real account
data in Cloud. Recorded results belong in
[VALIDATION](../VALIDATION.md#v06-audio-cache-foundation).

The owner-visible criterion remains incomplete until an applicable storage/reuse
grant and concrete online validation contract can be wired into the service:
first network playback publishes a complete entry; a later online-authorized hit
uses it; restart preserves it; refusal/account change/Clear prevents stale reuse;
storage failures leave valid network playback working. Then verify this on Fedora
Native and installed Flatpak. Until then PR #26 stays Draft and Issue #20 remains
open. Public-distribution Issue #5 is independent.
