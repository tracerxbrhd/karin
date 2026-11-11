<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0029: Production audio-cache boundary and service admission

Status: Owner-accepted Native/installed-Flatpak cache at c1de869; normally merged in PR #26

Date: 2026-11-05; independent-download/replay revisions 2026-11-10; local-first revision 2026-11-11

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

A user playback intent checks current Music access and selects an authenticated
full-MP3 API offer. It tries the saved recording before fetching XML or resolving
its signed CDN URI. A matching selection with verified integrity and a fresh
private HTTP reuse lease supplies its pinned read-only file to the existing
PlaybackController. Otherwise ordinary source resolution supplies the network
source immediately and a bounded worker can save one complete response. Preparation without autoplay does not
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
Whole-response quota reservations protect a valid replacement target while
counting the complete staged copy as well. Atomic directory exchange keeps the
original until final fences succeed; late failure rolls back instead of deleting
it. The reservation remains live through cleanup, protecting this transaction
from another store's LRU. Neither GTK nor the audio worker waits for storage or a background download.

## Local replay and failure

Every replay obtains current Music access and an authenticated supported API
offer, independently of a signed CDN locator. Schema-2 selection metadata binds
the validated logical descriptor object, MP3 codec/bitrate/gain and full/preview
classification. Account/token epochs, cancellation and integrity checks remain
separate. Known selection changes retire/refill the selected record; a new signed
CDN URL alone does not invalidate a fresh saved representation.

The selection is not an immutable provider content version. Public models expose
storage directory/size and canonical Track ID, but no inspected contract makes
them a byte-generation identifier. Karin combines selection equality with a
bounded private HTTP reuse lease and the measured saved SHA-256. It does not
copy Cassette's Track-ID-only admission or share validators between replicas.

Use explicit valid max-age/Expires with corrected response Age and persisted
receipt clocks. Without explicit expiration, a valid Date/Last-Modified pair can
provide conservative RFC9111 heuristic freshness: ten percent of their interval,
capped at one hour. Clock rollback, malformed/conflicting directives and missing
receipt metadata cannot grant freshness. `private` permits private storage;
`must-revalidate` forbids reuse once stale; `no-cache` always requires validation;
`no-store` forbids storage. These rules do not create content/distribution rights.

When the lease is stale or validation is required, retain bounded credential-free
HTTP validation. Opaque ETag/Last-Modified preconditions remain scoped to the exact
compatible media URI; a valid full Repr-Digest and length can prove bytes across
locators. Same, Different and Unknown remain distinct: positive proof can rebase
metadata atomically, confirmed changes follow retirement policy, and Unknown
retains modern records while playing online without redundant refills. Records
with pre-schema-2 selection or an upgraded unknown original response status
first use this strict path. If proof remains inconclusive after a current
authenticated offer, the
existing background writer can migrate them with one complete newly admitted
response. Atomic replacement occurs only after full validation/publication; the
old file/mtime is never relabeled fresh. Successful schema-2 publication ends
this migration, and refusals/no-store cannot trigger it. An otherwise modern
record with no receipt clock stays strict; that absence alone cannot repeatedly
trigger migration. An expired modern lease
with incomparable replicas may seed a fresh replacement only when both the
current probe and the actual GET response authorize fresh reuse; the GET gate
runs before its body is read. No-cache or unproved freshness stays online. This
renews measured policy without asserting that the old bytes changed. A bare 304, Track ID,
equal size or file presence alone cannot admit local playback. Cache failure
must not turn an admitted stream into a playback error.

GTK displays a bounded, worker-derived AudioCache presence view in existing
track rows and current-track surfaces. Missing hides the indicator; actual
background work shows Downloading; only a verified complete record shows Stored.
Restart rebuilds that view from the store, and Clear/Disable, eviction, corruption
and account changes remove obsolete status. GTK performs no per-row file or HTTP
checks. This adds neither a second track database nor offline availability.
HTTP storage, current playback access and provider/content rights remain separate.

Only completed, admitted local data reaches the single GStreamer engine through
its pinned file descriptor. Clear may unlink a record without interrupting that
already admitted descriptor. Normal close retains completed records and their
reuse leases; restart still checks current access, selection and integrity before
local playback. Account switches fence the old scope and logout
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
On 2026-11-11 the owner accepted `c1de869` real Native and installed-Flatpak
playback, complete saving, local hits and local replay after restart; graphics
are provisionally accepted. PR #26 normally merged at `1fd4028`, preserving its
history. Actual CDN headers remain unmeasured in Cloud. The
[Native/installed-Flatpak check](../BUILDING.md#pr-26-corrected-cache-acceptance)
remains a regression procedure. Technical acceptance does not establish public
client/content distribution or offline lifetimes.
