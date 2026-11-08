<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0029: Production audio-cache boundary and service admission

Status: Private HTTP caching with online resource revalidation; real-service acceptance pending
Date: 2026-11-05
Extends: ADR-0015, ADR-0019 and ADR-0026
Supersedes the test-only implementation boundary of the original PR #26 candidate

Resource-policy extension, 2026-11-06: [ADR-0030](0030-unified-cache-budget-and-preferences.md)
replaces the user-facing quota/defaults below with one shared budget and migrates
settings. Typed formats, admission, integrity and lifecycle boundaries remain.

Private HTTP policy revision, 2026-11-08: supersedes the earlier blanket missing-
provider-contract rejection. Playback access, HTTP storage eligibility, retained
representation validation and explicit content restrictions are separate checks.
Local HTTP admission does not assert a provider entitlement or legal license.

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
The absence of a special provider storage-grant endpoint does not itself reject
private HTTP caching of an already authorized, supported MP3 response. Explicit
HTTP/content restrictions still apply. Offline validation is a separate future
feature, not a prerequisite for online-revalidated reuse.

## Production boundary

Compile `audio_cache` into ordinary Native/Flatpak builds.
Keep account isolation, integrity validation, atomic publication, cancellation,
quota/LRU and restart recovery in this shared implementation. Generated-media
fixtures exercise the same store. The separate `LicensedResourceAuthority`
accepts trusted rightsholder declarations for exact CC0 resources, bound to
account/track, SHA-256, byte length and an admission revision. Untrusted API
license labels, cache directives and streaming success cannot create those
entries. Original generated fixture audio is expressly dedicated to CC0-1.0;
its Rust generator and application source remain GPL-3.0-or-later. Fake authorities
used by narrower unit tests remain test-only; there is no second synthetic store.
`StorageAuthority::authorize` runs on a worker; `check` examines its existing live
grant/revocation state without I/O. `StorageGrant` carries an opaque resource
identity and authority revision. The local namespace discriminator is a storage
format detail, not a claimed provider device identity.

The ordinary application's private HTTP policy replaces the
`MissingStorageContract` default. It is not a provider-grant adapter. The existing
online coordinator retains account/resource/request generations while the HTTP
policy checks the actual response and saved version. A subscription, account
success or metadata field does not become a fictitious provider entitlement.
Protected content and explicit storage restrictions cannot be admitted. Keep
local HTTP policy state distinct from independently licensed content and from
the online-attempt fence. The correction of the earlier unaccepted
`0709729..0f9257f` implementation remains historical evidence in
[PR26_REVISION_AUDIT](../PR26_REVISION_AUDIT.md).

A cache hit or publication must pass current account/resource access and
account/request generation checks. The source adapter carries opaque identity and
admission state in memory; it never serializes OAuth, cookies, personal models,
raw service responses or signed media addresses. Failed access or unverified
representation identity cannot be replaced by an arbitrary local TTL. Cache failure
must not turn an otherwise valid online playback attempt into a playback error.

Use `$XDG_CACHE_HOME/<app-id>/audio`, separate from metadata/artwork, with private
0700 directories/0600 files, SHA-256 integrity and atomic publication of complete
records. Hashes are integrity/opaque-name mechanisms, not encryption. The one
shared user budget and six choices follow ADR-0030, with 2 GiB by default. Hard
bounds are 1024 records, 64 MiB per resource and four pending requests. Bounded
staging counts against that shared budget; only complete records are published.
I/O belongs on workers. Clear, logout/account changes and cancellation fence pending writes
before cleanup. Incomplete/corrupt entries are not hits. Independent namespace
owners cannot create conflicting generations; stale requests/hits must not regain
admission after reopen. Normal close may retain admitted entries for revalidation
on a later online launch. Final-owner destruction explicitly unlocks its `flock`:
a fork/dup-retained open file description must not keep a dead owner's namespace
busy. Live strong owners still exclude a second owner; weak tickets cannot revive
the namespace. The destructor is owner-process guarded.

Disk hashing overlaps the existing StreamResolver's fresh online admission on a
bounded worker. Selection consumes only an already-ready candidate; a slow or
busy cache leaves that play online. Reuse additionally requires a bounded HTTP
version check against the currently admitted source, with no full audio GET.
File-descriptor wrapping and final admission are memory-only, while LRU work is
deferred. Start the selection's opaque check before network resolution. Failure,
cancellation or replacement retires that admission; an older completion cannot
replace a newer account/request/policy generation, including a return to the same
account. This local freshness fence does not grant legal rights, delete a
valid payload or invalidate the file descriptor of already admitted playback.
Only a policy-admitted hit supplies the verified read-only file descriptor to
the single PlaybackController. Playback uses that pinned file, not a path reopened
after validation. A cache miss retains the existing remote source; an admitted
write starts only after the actual Playing acknowledgment. A decoder failure
before any actual Playing gets one guarded fallback to the already admitted
online source, preserving pending transport intent in the same worker. After
actual Playing, a failure remains the normal player error; there is no seamless
mid-track retry or reset of Wave's measured-playing ledger. There is no parallel
audio engine or repeated resolution loop.

A policy-admitted fill captures the existing guarded `souphttpsrc`
compressed bytes without changing buffers/events, starting another request or
creating another GStreamer pipeline. Its nonblocking 2 MiB memory queue abandons
the cache copy on overflow instead of delaying audio. The existing writer starts
only after actual Playing. HTTP headers must establish `audio/mpeg`, a bounded
complete length and either HTTP200 or HTTP206 covering exactly `0..total-1`.
`no-store`, non-identity encoding, partial ranges, seek/discontinuity, incomplete
transfer, network failure and cancellation prevent publication while leaving the online
player path available. A private HTTP record retains bounded response policy,
validators, opaque URI identity and the measured payload SHA-256/length alongside
atomic publication. No OAuth token, cookie or signed URL is persisted or forwarded
to the media host. A separately licensed record must also match its independent
exact-content claim.

`no-cache` and `must-revalidate` never authorize stale replay: every local play
requires new successful online access and version validation. Same track ID or
equal file size is insufficient. Strong validators require the same resource and
variant; a fingerprint of the sealed Legacy descriptor's lowercase host,
validated XML path, MP3 codec and selected bitrate retains that file-locator
binding across renewed signatures. Auth-wrapper inputs `s` and `ts` are excluded;
paths are not declared immutable. A bare 304 with weak comparison cannot prove
exact bytes. A trusted full `Repr-Digest` can establish representation identity
across signed-URI changes. Last-Modified is usable only with the conservative RFC
sixty-second Date gap and the required resource/length checks. An unverifiable
or changed candidate selects the ordinary network source; unsupported
validators cannot become guessed guarantees. No new offline lease is invented.
Identity Content-Encoding is allowed. `Vary: Accept-Encoding` is supported by
explicit identity requests in both player and validation; other encodings or
nonempty variant dimensions decline caching. A complete eligible response can
be stored without validators, but reuse still needs proof. A fresh `no-store`
result removes a completed record
before online fallback, preserving any active playback descriptor. HTTP451 is
reported as a fixed content restriction rather than a generic missing grant.

No second GStreamer engine, daemon, DRM mechanism,
new OAuth identity or broad Flatpak permission follows. Cache maintenance and
settings report measured total storage under the shared quota. Combined Clear
removes metadata, artwork and audio records while preserving the active validated
session and an already admitted playback file descriptor. Logout purges its audio
scope; account switches fence the old scope. No saved/offline Music badge follows.
HTTP cacheability does not settle public client distribution or content rights;
those questions remain separate in #5/#22.

## Validation and remaining acceptance

Preserve all original storage scenarios and add regressions for namespace lifetime,
reopen/restart, production hit/miss/write/reuse, current permission checks,
integrity/fallback, cancellation/concurrency, quota/LRU, cleanup and account changes.
Exercise native settings/lifecycle and shared-player behavior without real account
data in Cloud. The ordinary-application process fixture uses MainWindow,
persistence, the shared budget, queue and GStreamer with original CC0 audio and
the ordinary production HTTP policy. Record completed cache files, actual
media GET/byte counts, local replay/restart identity, refusal, corruption repair,
active Clear and `no-store` behavior. These are permitted-resource scenarios,
not live-service evidence. Recorded results belong in
[VALIDATION](../VALIDATION.md#v06-audio-cache-foundation).

The real Yandex Music criterion remains unverified until the final Native and
installed-Flatpak check observes the connected production path:
first eligible network playback publishes a complete entry; a later online-authorized hit
uses it; restart preserves it; refusal/account change/Clear prevents stale reuse;
storage failures leave valid network playback working. A resource without safe
version validation stays online with a fixed reason instead of being called a
cache hit. Until that real-service check is accepted, PR #26 stays Draft and
Issue #20 remains open. Public-distribution Issue #5 is independent.
