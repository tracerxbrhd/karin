<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Persistent audio and offline access

Persistent service-audio caching and offline playback remain disabled. The
missing prerequisites are an established permission for Karin's storage/reuse
of catalog audio and a verifiable offline-rights lifecycle. Working online
playback and the owner's native/Flatpak acceptance do not establish those grants.
No offline switch, downloaded-track badge or hidden recording job substitutes
for them.

## Current decision

The accepted [online resolver](STREAM_DELIVERY_REVIEW.md) obtains a short-lived
resource for explicit playback. Its public legacy checksum is not automatically
DRM, but decoding an MP3 does not establish permission to retain it for later
sessions or after account, subscription, territory or catalog rights change.

The resolver's 60-second monotonic deadline limits admission of a transport
resource. Its opaque `ts` component, a cached token, metadata `available` flag,
prior successful playback or a locally chosen TTL is not an offline lease.
Refresh, revocation and offline-entitlement renewal contracts are not established.

Online buffering stays bounded in memory, with playbin download caching and
queue2 temporary-file storage disabled. Writing a complete stream into a temp
folder, hidden XDG entry or native download buffer still persists audio and is
outside this policy. Do not import or decrypt official applications' private
caches, copy protected keys or bypass DRM/device binding.

The account-isolated cache described in
[ADR-0019](adr/0019-bounded-account-cache.md) stores reduced metadata and explicitly
cacheable normalized covers only. It respects HTTP freshness/restrictions and
contains no audio, signed media URLs, cookies or OAuth values. Clearing it leaves
protected credentials and current online playback intact. Saved metadata must
not assert authentication, current mutable membership or offline entitlement.

## Service evidence

The reviewed service documents were retrieved on 2026-10-31; they are mutable
provider-controlled sources, not a new permission or current legal revalidation:

- [Music terms](https://yandex.ru/legal/music_termsofuse/) reserve material rights
  and restrict use to the service's stated technical/personal-use conditions.
- [Music application license](https://yandex.ru/legal/music_mobile_agreement/)
  governs the described official applications and their stored materials. It
  does not license Karin as that application.
- [Offline help](https://yandex.ru/support/music/ru/listening-and-settings/app-settings)
  describes encrypted, device-bound downloads playable in the official client.
  Its mobile-only introduction and desktop instructions must not be generalized
  into a blanket ban on every desktop implementation.
- [Subscription help](https://yandex.ru/support/music/ru/registration-and-login)
  and [Plus conditions](https://yandex.ru/legal/yandex_plus_conditions/) tie access
  to subscription, time, content, territory and device/concurrency restrictions.
  A subscription does not supply Karin a per-track persistent-storage grant.

## Conditions before implementation

1. Establish an applicable provider/rightsholder agreement or documented API
   grant for this client's persistent personal audio storage and reuse.
2. Verify account/device/track rights, expiry, removal, renewal and required online
   checks. Implement the actual service rules, without invented grace periods.
3. Confirm permitted resources and formats with a consenting local owner, keeping
   credentials, addresses and recordings out of Cloud/CI/GitHub.
4. Add only an account-bound, bounded, integrity-checked cache using the existing
   PlaybackController, cancellation and MPRIS lifecycle. Exercise native and
   Flatpak paths; show offline availability only for a valid admitted resource.

These gates concern persistent audio. Secure session storage, metadata/cover
caching and the accepted online player remain supported independently.
[LICENSING.md](LICENSING.md) keeps source and service/content rights separate.
