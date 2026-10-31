<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0019: Bounded account-scoped metadata and cover cache

Status: Accepted for v0.3; implementation checks and owner acceptance are recorded separately
Date: 2026-10-31
Supersedes: ADR-0013's memory-only artwork policy for explicitly cacheable covers

## Context

The accepted music client repeatedly loads the same metadata and cover images.
A persistent cache should reduce unnecessary reads and preserve clearly labelled
saved metadata during network failures, without becoming an authorization,
playlist-revision or favorite-membership authority. Audio is a separate content-
rights and offline-entitlement problem; a playable HTTPS resource alone grants
no persistent music-cache permission.

## Decision

Add a GTK-independent cache store and typed metadata codec, reusing existing
Serde, cryptography and bounded workers. No database, extra daemon or audio
engine is needed for the bounded records. Use Karin's private XDG cache area,
with private regular files/directories, atomic writes and corruption recovery.
Credential envelopes and preferences live outside this cache.

Bind a cache lease only to a validated Music account. Derive account/entry
filenames from bounded validated identifiers; raw names, queries, OAuth values
and resource addresses do not become paths. Hashing a filename is not encryption
or an anonymity guarantee. Keep account partitions distinct and validate record
kind, identity, page, bounds, schema and integrity again when reading. Persist
only reduced metadata and normalized raster data, never raw provider responses,
headers, tokens, cookies, signed media URLs or audio.

Use one configured byte budget for metadata and artwork, including record
framing, across retained cache records. The default is 256 MiB and the supported
user range is 16 MiB–4 GiB. Use bounded record/count/scope limits, LRU-like
access ordering and eviction; do not scan or accept unbounded attacker-chosen
files. Settings expose measured record-file usage, not an invented estimate or
a claim about every filesystem's physical allocation overhead.

Metadata has explicit freshness and per-reply provenance. `CachedCatalog` keeps
stable track/album fresh hits and a bounded same-key miss gate separate from
mutable liked/playlist/artist pages, which are network-first. Only Network/Timeout
may fall back to a saved/stale record, re-read after failure under its current
lease. Authorization/policy/rate/schema refusals and other HTTP errors remove the
record and never fall back. UI labels saved/potentially stale data and rechecks
cache-derived leases; fresh network replies do not depend on a cache lease.
Explicit refresh bypasses fresh hits. Cached values never establish a session, entitlement, current favorite
state, current playlist revision or success of a write. Membership, account
validation and read-after-write confirmation always use the live service.
Invalidate affected data after every write outcome, including ambiguity, so an
old favorite/playlist snapshot cannot confirm a failed or unknown mutation.

Normalize bounded decoded covers before persistence and keep graphical materials
separate from Karin's GPL-licensed source/assets. Respect explicit HTTP no-store,
no-cache, private/Vary restrictions and server freshness bounds; an image may
still be shown through the existing bounded RAM presentation without being
persisted. No original embedded metadata or provider address is written into the
raster record. Preserve the existing separate artwork-worker admission limits
and main-thread-only GTK textures.

Stabilization clarification: `must-revalidate` with explicit positive remaining
`max-age` permits fresh reuse, but never stale-on-network-error fallback. Retain
that fact in the normalized raster's policy flag across restarts (bit 0 is alpha,
bit 1 forbids stale reuse; reject unknown bits). Existing 0/1 raster flags remain
compatible. This is not permission to store raw HTTP headers or original image
metadata. `no-store`, `no-cache` and the other restrictions above still prevent
persistence.

A lease contains account and policy generations. Binding another account,
disabling persistence or clearing invalidates obsolete jobs. Late readers and
writers must not populate the new account, revive disabled data or repopulate a
cleared store. Serialize filesystem mutations and use safe temporary files plus
atomic rename; malformed, partial, oversized and incompatible records become
misses/removal, not fabricated data or application crashes.

Disabling caching prevents persistent content reads and new writes, but does
not implicitly erase existing files. Usage/explicit clearing remain available;
explain this distinction in Storage preferences. Clear removes cache records
without deleting credentials, changing playback context or stopping the current
source. Already displayed metadata/textures are presentation state, not evidence
that a cleared record remains on disk. Errors and in-progress clearing remain
visible; never report zero usage before it is measured/confirmed.

Explicit confirmed logout is different from ordinary cache clearing or closing
the window: invalidate account leases/RAM first, stop audio, then purge Karin's
persistent metadata and cover records across this single app profile while
forgetting the saved credential. Keep replacement login blocked during active
cleanup I/O and expose failures with Retry. Settled failure may allow memory-only
login, but persistent cache/writes remain paused behind the forget barrier.
Normal window close retains the
permitted cache and remembered session. Invalid restored credentials use the
durable forget barrier before removal; stale cleanup cannot act on a newer login.

## Audio boundary

Do not write service audio to this cache or temporary directories as a hidden
fallback. Persistent audio/offline requires a separately permitted API contract,
account/subscription/rights-expiry lifecycle and a non-bypassing way to enforce
it. [AUDIO_CACHE_REVIEW.md](../AUDIO_CACHE_REVIEW.md) records that independent
evidence. If those conditions are not established, no offline switch, download
tile or offline-availability badge is exposed. The existing GStreamer network
path continues to use its bounded transient memory buffering.

## Validation and consequences

Exercise cache hit/miss/stale, identity/account isolation, malformed/truncated
records, byte accounting/eviction, atomic interruption, concurrent clear/write,
disable/re-enable, policy changes and obsolete leases with synthetic files and
loopback sources. Verify real reuse by counting network calls, rather than only
checking that a file exists. Explicit refresh and mutation confirmation must not
be satisfied from cache. Artwork fixtures check no-store and freshness policy.

Run integration with settings, session restore/logout, current playback and
MPRIS. Test native and Flatpak XDG path isolation on the actual target desktop;
cloud source preparation and vendored compilation are not that sandbox test.
User acceptance must distinguish saved metadata and cached cover presentation
from audible offline music. No real account/cache payload or service recording
belongs in automated artifacts.
