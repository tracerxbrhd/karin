<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0029: Unified cache and private online audio reuse

Status: Accepted; current production cache contract.

## Shared budget and storage

One CacheBudget coordinates typed metadata, artwork and AudioCache stores in private XDG
namespaces. Defaults are enabled / 2 GiB; choices are 500 MiB, 1/2/3/4 GiB and Unlimited.
Preserve measured usage, shared LRU, Clear, Retry and settings migration. Unlimited
removes only the user byte ceiling, not format/count bounds or disk headroom (1% capacity
clamped to 64–512 MiB plus reservations).

Every writer reserves complete staging/header size before I/O; reservations remain live
through publication/rollback. Account/policy/Clear/Disable generations fence late
admission. Bound scans to recognized regular records, never follow links or delete unknown
content. Use private 0700 directories/0600 files, integrity checks, atomic publication and
crash recovery. Slow copies hold reservations, not GTK's state mutex. Coalesce quota
dragging/trim after 200 ms; Clear/Disable fence at once. Clearing removes data without
credentials or current playback intent; an admitted read-only FD survives unlink. Disable
blocks reuse/new writes without pretending files were erased. Logout purges retired
personal data; normal close may retain it.

Metadata is reduced typed data, not raw responses. Mutable membership/pages remain
network-first; only eligible transport failure admits labelled saved data. Current
account/key refusal fences remain authoritative. Artwork persists normalized bounded
raster data only under its own freshness/no-store/no-cache/Vary policy; no metadata, image
or file can establish login, rights or mutation success.

## Selected complete audio, independent of playback

On Play, check current Music access and an authenticated supported full-MP3 offer. Try
matching retained audio before XML/CDN resolution. A local hit needs complete
length/SHA-256/structure integrity, matching schema-2 logical selection and a valid
private HTTP reuse lease or positive scoped validation. Track ID, file presence, equal
size, descriptor `ts` and logical selection alone prove neither bytes nor rights. A
rotated signed locator alone does not invalidate a fresh recording.

On a miss, give the admitted online source to the existing player immediately. One bounded
worker may independently stream a complete response to protected staging; no GStreamer
capture, seek-range splice, Playing/EOS latch, unselected prefetch or second player.
Preparation without autoplay waits for Play. Next/ Previous/source replacement/Wave
rollover/Pause/Seek do not cancel that write; Stop/close/Clear/Disable/logout/account
changes fence it. No partial becomes a hit. Keep 64 MiB/resource, 1,024-record,
four-job-context and single-writer bounds. One-use registration lasts at most 30 minutes,
worker admission 30 seconds and its complete HTTP request 15 seconds. Recheck
cancellation/account/token/cache permission around reads and publication. A first
CDN403/410 may resolve afresh once and retry only a changed sealed URI; body/network
errors, unchanged or second refusals do not retry. Never change identity/region or forward
tokens/cookies.

## HTTP reuse and conservative proof

Require complete supported identity-encoded `audio/mpeg`, bounded MP3 screening,
length/hash and admissible Vary/storage policy. Reject protected content, explicit
restrictions, partial/malformed transfers and `no-store`. MP3 screening is not a full
decoder. Storage failure cannot fail otherwise admitted online playback. HTTP policy is
not a copyright/license or fabricated provider storage grant. The separate exact-content
CC0 authority is not the ordinary Music permission.

Use corrected Date/Age and persisted local receipt clocks with valid max-age or Expires.
Without explicit expiry, an eligible HTTP200 Date/Last-Modified gap of at least 60 seconds
permits ten-percent heuristic freshness, capped at one hour. Clock rollback,
invalid/conflicting policy or missing receipt clocks grants no shortcut. `private` permits
private storage; `no-cache` always needs validation; `must-revalidate` forbids stale
reuse; `no-store` forbids storage and retires data. Current Music access is still
mandatory after restart: no offline mode is implied.

Logical selection binds descriptor origin/object/semantic query and MP3 codec/bitrate/gain/fullness, excluding
documented signing fields only; it is not an immutable byte version. ETag/Last-Modified
preconditions stay scoped to the exact compatible media URI, never shared between
replicas. Opaque `ts` does not prove auth-only relocation. A legitimate strong scoped
match plus full length, or a full SHA-256 Repr-Digest plus length across locators, may
prove Same. Bare 304, partial Content-Digest and cross-URI tag equality/mismatch are not
that proof. Keep Same/Different/Unknown distinct: Same may atomically rebase; Different
follows retirement/update policy; Unknown retains modern data and plays online without
redundant body refills.

Pre-schema-2/unknown-original-status records first attempt strict proof; if still
inconclusive, one current complete newly admitted response may migrate them. Never restamp
old bytes/mtime fresh. Successful publication ends migration; missing clocks on an
otherwise modern record do not repeatedly reclassify it as legacy. Expired modern
incomparable replicas permit replacement only if the current probe and actual GET headers
independently authorize fresh reuse before body download. No-cache/unproved freshness
cannot enter that replacement path. Protect old data through whole-copy reservation,
atomic directory exchange and rollback until final fences and cleanup complete. Store
bounded policy/identity/ length/hash/validators only, never credentials, signed URLs or
raw HTTP responses.

## Verification and limits

Indicators derive verified worker state, without per-row disk/network I/O: Stored means a
complete retained file, not offline access or a guaranteed next hit. Verify actual
local reuse with network counters and the same player, distinct from live CDN acceptance.
[RFC9111](https://www.rfc-editor.org/rfc/rfc9111.html), [RFC9110
§8.8](https://www.rfc-editor.org/rfc/rfc9110.html#section-8.8) and
[RFC9530](https://www.rfc-editor.org/rfc/rfc9530.html) define HTTP proof boundaries;
[LICENSING](../LICENSING.md) retains service/content rights separately.
