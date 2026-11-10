<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0029: Production audio-cache boundary and service admission

Status: Private HTTP first save owner-confirmed; revised local replay and indicators await acceptance

Date: 2026-11-05; independent-download and replay/presence revisions 2026-11-10

Extends: ADR-0015, ADR-0019, ADR-0026 and ADR-0030

## Decision

Use a bounded whole-response download job, independent of the player's lifetime,
for the selected, authorized MP3. This replaces the 2026-11-08 source-pad capture
and 2026-11-09 native-EOS completion designs. Their code and failed service
acceptance remain available in immutable PR #26 history and [VALIDATION](../VALIDATION.md).
The released Cassette v0.2.4/v0.2.5 chain supplies the architectural reference:
local audio lookup, immediate online URI on a miss, and a separate complete-file
save. Karin implements this independently in Rust and retains its stronger
storage, cancellation and revalidation checks; see [AUDIO_CACHE_REVIEW](../AUDIO_CACHE_REVIEW.md).

A user playback intent resolves the current account-bound MP3 source. A verified
cache hit supplies its pinned read-only file to the existing PlaybackController.
Otherwise the player receives the network source immediately and a bounded
worker can save one complete response. Preparation without autoplay does not
start a cache download until Play. There is no unselected-track prefetch,
GStreamer source probe, compressed-body channel, native-Playing admission latch,
or EOS-triggered retry downloader.

Next, Previous, ordinary source changes, manual Skip, Pause and Seek do not
invalidate that independent job. It may finish a complete file while a newer
composition plays. Explicit Stop, application close, Clear, Disable, logout and
account change cancel or fence unfinished work before cleanup. No partial file
becomes a cache hit. A whole-file job does not join playback seek ranges.

## Admission and storage

Playback access, actual HTTP storage eligibility, retained representation
validation and content restrictions are separate checks. A missing proprietary
storage-grant API is not a blanket refusal. Conversely, an account/subscription,
HTTP 200, metadata license label or GPL source does not create a provider
entitlement or copyright license. Reject protected/unsupported media, explicit
content refusals and `no-store`; no DRM, extracted official-client key or imported
service cache is used.

The ordinary app uses the private HTTP policy. The separate
LicensedResourceAuthority remains available for trusted exact-content CC0
resources, bound to account/track, SHA-256 and length. It is not a fake Music
provider permission. Cloud fixtures use original CC0 audio through the ordinary
application's HTTP policy.

Register a one-use write operation only after current source admission. Its
bounded worker lifetime is separate from latest-playback admission; starting the
next track must not revoke an eligible old write or make an old cache hit current.
Every read/publication checkpoint rechecks its live account/cache generation,
permission, token and cancellation. Registration lasts at most 30 minutes; worker activation starts a 30-second
nonrenewable admission. A complete-body HTTP request times out after 15 seconds. No token, cookie, signed URL or grant is persisted or sent to CDN.

Reuse AudioCache's protected XDG directory, 0700 directories/0600 files,
streamed staging, bounded MP3 screening, length/SHA-256 validation, atomic publication and crash
recovery. Keep the existing 64 MiB resource, 1024-record, four-job-context and
single-writer bounds; remove complete-body capture buffers. CacheBudget retains
one enabled-by-default 2 GiB quota, six choices, shared metadata/artwork/audio
LRU, disk headroom, reservations and Clear/Retry under [ADR-0030](0030-unified-cache-budget-and-preferences.md).
Neither GTK nor the audio worker waits for storage or a background download.

## Local replay and failure

Every replay resolves fresh Music access and verifies the saved representation
through a bounded credential-free HTTP check. Keep the logical descriptor
request/encoding as provenance separately from physical delivery and byte version.
A logical MDS key does not establish a shared ETag/Last-Modified namespace across
replicas. The earlier `cac4d78` model overextended that assumption; published
MDS timestamp/size tags are not global body hashes. See the
[primary-source analysis](../AUDIO_CACHE_REVIEW.md#private-http-admission-and-resource-validation).

Only the exact provider media URI, without a known logical/encoding mismatch,
permits opaque conditional strong-validator/length validation. Legacy XML ts is
opaque, so even a changed signed wrapper cannot borrow the former URI's tag or
date. A valid full Repr-Digest plus whole length can independently prove equal
bytes across locators. Complete-length/full-digest differences, same-scope representation
version invalidation and a different encoding must be distinguished from mere
uncomparable replica validators. A bare 304, Track ID, equal size, logical key
or file presence alone cannot establish a hit.

Model Same, Different and Unknown separately. Same admits only after current
access and final fences; Different follows the applicable update policy; Unknown
retains the valid record online without a repeated body refill. Legacy binding
metadata is rebased only after positive Same proof, atomically, without another
full media GET. Missing binding or uncomparable replica metadata cannot authorize
blind replacement. Confirmed representation/variant changes,
no-store, explicit access/content refusals and corruption retain their scoped
retirement rules. `no-cache` and `must-revalidate` never permit stale local
fallback. Cache failure must not turn an admitted stream into a playback error.
New private copies without useful version evidence are still declined before
body storage. Existing records are not discarded just for missing new optional
binding metadata; they need sufficient current proof before reuse.

GTK displays a bounded, worker-derived AudioCache presence view in existing
track rows and current-track surfaces. Missing hides the indicator; actual
background work shows Downloading; only a verified complete record shows Stored.
Restart rebuilds that view from the store, and Clear/Disable, eviction, corruption
and account changes remove obsolete status. GTK performs no per-row file or HTTP
checks. This adds neither a second track database nor offline availability.
HTTP storage, current playback access and provider/content rights remain separate.

Only completed, admitted local data reaches the single GStreamer engine through
its pinned file descriptor. Clear may unlink a record without interrupting that
already admitted descriptor. Normal close can retain completed records for fresh
online validation after restart; account switches fence the old scope and logout
purges it. Queue, Wave's actual-playing ledger and MPRIS remain playback concerns.

## Verification and remaining acceptance

Verify the ordinary application with original permitted MP3 audio/local HTTP:
first playback and background save; Next/Wave/Skip/Seek; actual local replay and
restart; refusal/version change/no-store; timeout/truncated response; Clear,
logout, corruption, quota/LRU and disk headroom. Record actual HTTP counters,
GStreamer source, full hash and file identity. These tests do not establish real
CDN behavior. At `31d873d` the owner confirmed first playback and complete
background saving on Fedora Native. At `cac4d78`, descriptor-path /
representation-changed still caused repeated downloads; no local hit was
confirmed. Earlier failures remain historical results.
The revised proof/presence behavior needs the single final
[Native/installed-Flatpak check](../BUILDING.md#pr-26-corrected-cache-acceptance).
PR #26 stays Draft; offline listening and public client/content distribution are
separate matters.
