<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Persistent audio and offline access review

Reviewed **2026-10-31** for v0.3, against merged baseline
`09f76355684873f32aaf83b8050e047df6077143` and the sources below.

**Decision: do not enable persistent service-audio caching or offline playback.**
The missing gate is an established permission and rights-lifecycle contract for
Karin's storage/reuse of catalog audio. This is separate from the working online
resolver and the owner's accepted actual Music/Flatpak playback. It is neither a
new HTTP failure nor a claim that every MP3 resource is DRM-protected.

The v0.3 metadata/cover cache remains independent and can be implemented fully.
Its entries never include service audio, signed media addresses, cookies or
OAuth values. No inactive offline switch or fabricated downloaded-track state
stands in for the missing audio capability.

## Retrieved service evidence

All sources in this table were retrieved over HTTPS on the review date with
HTTP 200. They are service-controlled moving documents; publication dates and
section references identify the reviewed scope rather than pinning future terms.
The review did not log in, send a real token, request a catalog recording or
read another application's files.

| Source | Observed scope | Consequence for Karin |
| --- | --- | --- |
| [Music terms of use][terms], Russia/Belarus edition published 25.08.2025, §§2.1, 2.7–2.8 | Recordings/materials retain their rightsholders' rights. Use is personal/noncommercial within the service's technical capabilities; copying/use restrictions and changeable service limits remain. | An authenticated delivery response does not supply an explicit third-party persistent-download grant. A GPL code license does not change content/service terms. |
| [Music application license][app-license], Russia/Belarus edition published 03.04.2026, §§1.1, 4.1, 4.5 | The licensed program is Yandex Music for mobile devices, Windows and macOS. Its restrictions cover manipulation/decryption of the program and other use of materials it stores without the stated permission. Limits may change. | These terms do not license Karin as that program or authorize importing/decrypting its private offline cache. Karin must not adopt an official-client identity, keys or stored media. |
| [Official application settings/offline help][offline-help] | Downloaded files are described as encrypted and playable only in Yandex Music on the device where they were downloaded. The page has iOS, Android and macOS/Windows instructions. | This describes an official-client/device-bound offline implementation. It does not document a transferable plaintext-MP3 cache format or a Linux third-party offline API. |
| [Registration and subscription help][registration] | Application listening requires the relevant Plus subscription; offline use is listed among subscription features. | A subscription is a prerequisite for official functionality, not sufficient evidence of a grant to implement Karin's separate persistent download mechanism. |
| [Plus conditions][plus], definitions, content-availability provisions and §1.7 | Access is time-bounded by the subscription and may vary by territory, particular content/rightsholder restrictions and device/concurrency limits. | Cached account/availability metadata cannot establish perpetual offline track rights. These terms supply no independently verifiable per-track offline lease to this client. |

The offline-help introduction says downloads are mobile-only while the same
retrieved page also supplies macOS/Windows instructions. This internal breadth
must not be turned into an asserted blanket prohibition on every desktop client.
The narrower supported observation is encrypted, device-bound playback in the
described official applications; Karin-specific authorization is **not
established**.

## Delivery, buffering and persistence are different contracts

The accepted online chain is documented in
[STREAM_DELIVERY_REVIEW.md](STREAM_DELIVERY_REVIEW.md) and
[ADR-0015](adr/0015-guarded-legacy-mp3-playback.md): an authenticated per-track
offer, a fresh HTTPS descriptor and a short-lived resource are handed to the
existing GStreamer worker. The owner's real audible-track acceptance establishes
that local prototype path, not a new persistent-download license.

The legacy public checksum is not automatically DRM. No conclusion here depends
on treating an ordinary MP3 delivery checksum as encryption or an extracted
official-client secret. Equally, successfully decoding that MP3 does not answer
whether it may be retained for a later session or used after a subscription,
territory, account or catalog right changes.

The current resolver's 60-second monotonic admission limit is a conservative
transport freshness check. The descriptor's opaque `ts` component is not a
subscription deadline or signed offline entitlement. A cached token, cached
`available` flag, prior successful playback or locally chosen file TTL cannot
replace a service-issued rights horizon. This review establishes no refresh,
revocation, offline-license or entitlement-renewal API.

During online playback, bounded in-memory buffering remains enabled only as
required by the existing pipeline. Remote-source policy keeps playbin download
caching disabled and prevents queue2/tempfile persistence. v0.3 must preserve
those guards. Writing a complete stream into `$TMPDIR`, an unlabelled XDG entry
or a hidden GStreamer download buffer would still be disk storage; it is not an
acceptable workaround.

## Technical feasibility and exact outstanding gates

| Gate | Result |
| --- | --- |
| Existing GStreamer controller can play an admitted local regular file | Implemented and covered by independent generated-media tests. This is a technical mechanism, not service-media permission. |
| Current authorized online Music source is audible in the accepted baseline | Owner-confirmed on Fedora/PipeWire, including Flatpak; changed revisions require regression acceptance. |
| An established service contract permits Karin to retain/reuse catalog audio | **Not established** by the reviewed terms/help or the accepted legacy delivery contract. |
| Account/track-specific offline rights, expiry, device binding, removal and renewal can be verified without bypass | **Not established**. No reviewed response supplies a usable offline lease or supported enforcement protocol for Karin. |
| Public Rust/Cassette media-cache implementations grant service/content rights | They do not. Source availability can explain mechanics but supplies no transferable content grant. |
| A full current Flatpak persistent-audio implementation has been exercised | Not applicable: the feature is intentionally absent. Existing native/Flatpak online playback is preserved. |

The two missing permission/rights-contract gates prevent the audio feature.
They do not block secure session storage, metadata/artwork persistence, settings,
playlist work or the native workspace. No second audio engine, daemon, hidden
recording job, official-cache import or offline-entitlement simulator is added.

## Conditions for a later lawful implementation

1. Obtain an applicable provider/rightsholder agreement or documented API grant
   covering this third-party client's persistent personal audio storage/reuse,
   rather than inferring it from OAuth scopes, Plus or a delivery URL.
2. Establish the account/device/track entitlement contract, including validity
   horizon, revocation/removal, catalog changes and any required periodic online
   check. Apply its actual rules; do not invent a grace period or circumvent
   encryption, DRM or device binding.
3. Confirm the permitted resource/format and storage lifecycle with one consenting
   owner's ordinary Karin session, locally. Keep tokens, addresses and recordings
   out of Cloud/CI/GitHub. Real encrypted/DRM media needs its authorized playback
   contract, not extracted keys or decryption adaptations.
4. Only then add a separate account-bound, limited and integrity-checked audio
   cache. Connect admitted files to the existing PlaybackController, preserve
   cancellation/cleanup/MPRIS, and test native and current Flatpak sandbox paths.
   Display offline availability only for a valid admitted cached resource.

Until these gates are met, cache settings describe **data and covers**. Clearing
that cache neither deletes protected session credentials nor stops current
online playback. Saved metadata is identified as saved/stale when necessary and
never used to assert authentication, current likes or offline playback rights.

[terms]: https://yandex.ru/legal/music_termsofuse/
[app-license]: https://yandex.ru/legal/music_mobile_agreement/
[offline-help]: https://yandex.ru/support/music/ru/listening-and-settings/app-settings
[registration]: https://yandex.ru/support/music/ru/registration-and-login
[plus]: https://yandex.ru/legal/yandex_plus_conditions/
