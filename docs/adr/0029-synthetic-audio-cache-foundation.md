<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0029: Synthetic audio-cache foundation without service persistence

Status: Implementation under test; service persistence blocked on external rights contract
Date: 2026-11-05
Extends: ADR-0015, ADR-0019 and ADR-0026

## Context

The v0.6 task requests concrete storage work as well as a fresh source/rights
review. [AUDIO_CACHE_REVIEW](../AUDIO_CACHE_REVIEW.md) distinguishes Cassette's
working local-first cache from permission to reproduce that behavior for Karin's
service resources. No applicable storage/reuse grant or verifiable
account/device/track offline lifecycle has been established. Temporary whole-file
storage has the same missing prerequisite as persistent storage.

The accepted online resolver/player, Wave, metadata/artwork cache, MPRIS,
authentication and Flatpak permission boundaries remain intact.

## Decision

Implement a separate filesystem prototype compiled only under `cfg(test)`.
It has no MusicService, resolver-resource, HTTP, OAuth, cookie or arbitrary-media
input. Its sealed synthetic-audio constructor generates original WAV samples.
The ordinary application cannot instantiate or invoke the store; there is no
feature flag, hidden recording job, download UI, new settings or daemon.

A fake authority explicitly models allowed account/device/track scope, expiry,
renewal, revocation and online/offline conditions. It is a test oracle, not a
service license, and its times never become an invented production TTL.
Every read/publication checks current authority and account/generation.
A stored file or expired transport URL cannot grant permission.

Use private opaque account/entry paths, bounded records and bytes, integrity
validation, atomic publication and recoverable partial-file cleanup. Serialize
mutations and reject duplicate in-flight writes; cancellation, Clear, logout
and account replacement retire old operations before publication or read admission.
Maintain bounded LRU eviction and report storage failures honestly. One lifetime
namespace lock excludes independent owners with inconsistent generations; I/O
scans remove orphan partial records before accounting or fail visibly. The
prototype permits 64 records, four in-flight requests, 2 MiB per generated WAV
and a 32 MiB maximum budget; these are test bounds, not service policy. File I/O
belongs on workers if this component is later admitted into the application.
A cache failure must preserve ordinary online playback.

Original generated audio may pass through the existing local-file
PlaybackController/fakesink test path, including Clear during active playback;
there is no second audio engine. Tests use isolated private directories,
including XDG-style sandbox profiles, never a desktop keyring or service content.
Source-only sandbox simulation is not installed Flatpak acceptance.

## Consequences and admission gate

The result is executable storage/lifecycle evidence rather than a product offline
feature. Tests can prove miss/hit, partial/corrupt files, concurrency/deduplication,
cancel, quota/LRU, Clear, logout/account isolation, expired/revoked rights,
offline decisions, storage failures and restart recovery. They cannot establish
a right to store Yandex Music. Executed results belong in
[VALIDATION](../VALIDATION.md#v06-audio-cache-foundation).

Service caching requires explicit review once an applicable grant and actual
resource/format, account/device/track, expiry/renewal/revocation and offline-check
contracts exist. Production source selection must use that authority before
cache reuse, otherwise the existing StreamResolver, without stale-cache overrides
of refusals. Preserve one queue/player, bounded cancellable prefetch, logout
cleanup and narrow Flatpak grants. XOR, file hashing and synthetic permits are
not substitutes for that contract.
