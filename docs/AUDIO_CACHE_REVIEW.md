<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Audio caching and offline access

Production Karin does not save service audio. The immediate target is transparent
persistent caching with **online revalidation before reuse**, not Downloads or an
offline mode. Two prerequisites remain unestablished: an applicable permission for
Karin to store and reuse catalog audio, and the provider's concrete rule for
checking current account/resource permission before reusing those bytes. Heard
online Music, a subscription, GPL source reuse and a working MP3 URL establish
neither. This does not turn an unfinished cache feature into a completed one.
Issue [#20](https://github.com/tracerxbrhd/karin/issues/20) remains open;
public authorization/API distribution rights remain separately tracked in
[#5](https://github.com/tracerxbrhd/karin/issues/5).

## Three distinct modes

| Mode | Current result | Additional condition |
| --- | --- | --- |
| Active streaming buffer | Transient pipeline memory; no saved media or cross-play reuse | Preserve online-resource admission |
| Reusable audio while online | Service persistence disabled, including hidden temporary files | Applicable storage/reuse grant and an established fresh account/resource admission check before each hit |
| Offline audio | Outside this initial cache scope; no offline switch or badge | A separate contract for permitted resources, applicable account/device scope, expiry, renewal, revocation and offline checks |

Automatic cleanup does not turn a disk recording into an in-memory stream.
An old local file cannot override an API refusal, lost rights or another account.
The resolver's sixty-second admission deadline and opaque descriptor `ts` are
transport properties, not offline leases or permitted retention periods. An
online-only cache need not invent an offline entitlement or demand an offline
feature before shipping; its own storage and online reuse conditions still need
to be established.

## Cassette source audit

Rechecked **2026-11-06** at Cassette v0.2.4, commit
[`9d34251`](https://github.com/Rirusha/Cassette/tree/9d34251d01bcd414ffbec2bef7931090d33c9124).
All six files below carry Vladimir Vaskov's copyright and GPL-3.0-or-later.
They are technical references; no cache implementation, obfuscation or
official-client material is copied into Karin's store.

| Pinned source | Observed mechanism and limit of the evidence |
| --- | --- |
| [cachier.vala](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier/cachier.vala#L140) | Reads complete audio bytes, selects temporary/permanent storage, tries local audio before resolving the network resource and starts a background save. This is persistence, not merely a streaming buffer. |
| [controller.vala](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier/controller.vala#L117) | Tracks loading and TEMP/PERM states; file location determines cached state. Presence alone does not establish current entitlement. |
| [job.vala](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier/job.vala#L145) | Album/playlist jobs reconcile constituent tracks. Cancellation stops further scheduling and waits for active saves before cleanup; the reviewed whole-file download has no per-chunk cancellation contract. |
| [storager.vala](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier/storager.vala#L614) | Separate XDG cache/data locations, local-first lookup, whole-file writes and decoded temporary playback files. Size reporting uses `du`; these six reviewed files do not establish an automatic quota/LRU policy. Reversible XOR is neither authenticated encryption nor a storage grant. |
| [info-db.vala](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier/info-db.vala#L23) | SQLite references retain tracks used by several saved collections. Those references are not account/device/track licenses. |
| [player.vala](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/player/player.vala#L492) | Passes the local/network URI to playbin and may prefetch the next track. Karin retains its existing player without adopting automatic service-audio saving. |

The reviewed path returns a local file before resolving a fresh network resource.
It establishes the legacy cache-selection/save chain, but not the current
permission for Karin to reuse that file. Nor does it establish an offline lease or
renewal/revocation protocol. This is an evidence gap, not a claim that Cassette
cannot technically cache audio or that every independent client is forbidden.

The reviewed v0.2.4 `save_audio` uses `FileUtils.set_data`, bound to
[`g_file_set_contents`](https://docs.gtk.org/glib/func.file_set_contents.html).
GLib supplies temporary-file/rename atomic replacement; it is incorrect to call
this an unprotected direct write. Its documented default does not guarantee
power-loss durability for a newly created/empty file. Cassette adds no digest or
complete-record validation around that operation. `load_audio` retries failed
reads indefinitely, writes a decoded temporary file and returns its URI; the
player unlinks that pathname after starting playback. Jobs cancel further
scheduling and wait for active work; the save path loads a whole resource and
exposes no per-chunk cancellation. No integrity index, orphan recovery or global
quota/LRU algorithm is established by these six files. Karin retains its bounded
writers, validated records and pinned playback FD.

### Newer revisions

The published [v0.2.5 release](https://github.com/Rirusha/Cassette/tree/b1c55e5fb71e60c9ff5421db42695f4dd5526a26)
(2026-10-12) changes the project version and the default visibility of stations;
[its comparison with v0.2.4](https://github.com/Rirusha/Cassette/compare/9d34251d01bcd414ffbec2bef7931090d33c9124...b1c55e5fb71e60c9ff5421db42695f4dd5526a26)
contains no cache/audio implementation change.

Current main was pinned at
[`bce825a`](https://github.com/Rirusha/Cassette/tree/bce825a13c1d1f6d0337dbea914c2d66b1ac07ba)
(2026-11-03). Its history diverges from the release line and uses `libtape`:

| Current source | Verified difference |
| --- | --- |
| [CacheManager](https://github.com/Rirusha/Cassette/blob/bce825a13c1d1f6d0337dbea914c2d66b1ac07ba/libtape/src/cache-manager/cache-manager.vala#L27) | Loads local artwork first, then the network, and saves when caching is enabled. Its exposed implementation has image loading and wipe, without the legacy audio scheduling chain. |
| [Storager](https://github.com/Rirusha/Cassette/blob/bce825a13c1d1f6d0337dbea914c2d66b1ac07ba/libtape/src/cache-manager/storager.vala#L620) | Retains permanent-before-temporary audio lookup, reversible encoding and whole-file save/load. Read retries are bounded to five attempts. Writes use GIO `replace_contents_async`; the reviewed code adds no explicit integrity/index transaction, repair or quota/LRU. |
| [InfoDB](https://github.com/Rirusha/Cassette/blob/bce825a13c1d1f6d0337dbea914c2d66b1ac07ba/libtape/src/cache-manager/info-db.vala#L50) | Retains SQLite content references; references do not authorize playback or retention. |
| [Player](https://github.com/Rirusha/Cassette/blob/bce825a13c1d1f6d0337dbea914c2d66b1ac07ba/libtape/src/player/player.vala#L26) and [GstPlayer](https://github.com/Rirusha/Cassette/blob/bce825a13c1d1f6d0337dbea914c2d66b1ac07ba/libtape/src/player/gst-player.vala#L164) | The public wrapper plays a file; its GStreamer adapter can consume file, HTTP and data sources. That capability alone does not connect catalog delivery to persistent audio. |

A caller search across all **45 Vala files** in that pinned main found audio
save/load use in storage tests and their definitions, without an application
caller connecting them to Music delivery. `controller.vala` and `job.vala` from
the legacy path are absent from this tree. Consequently the newer main is useful
for async storage/retry ideas, but is not evidence that its complete legacy audio
cache chain still runs or that a new provider permission exists. No Cassette
binary or live account was run in this source review.

The newer core cache/player files credit Vladimir Romanov and generally carry
GPL-3.0-or-later. Auxiliary `humanity-size.vala`, `location.vala` and `settings.vala`
carry GPL-3.0-only. Future reuse must retain each file's actual license/copyright;
Karin does not copy their implementation or relabel them under its own license.

## Current Karin audit

`cache`/`catalog_cache` persist reduced metadata and explicitly cacheable normalized
covers under [ADR-0019](adr/0019-bounded-account-cache.md) and
[ADR-0026](adr/0026-refusal-fences-and-queue-occurrence-identity.md). They store no
audio, signed resources or credentials, and cannot establish playback rights.
`stream`/`stream::resolver` admit a short-lived online source; `queue` and MPRIS
do not depend on media persistence.

`playback` uses one worker-owned playbin with `audio+soft-volume` flags. Both
DOWNLOAD and managed BUFFERING flags are absent. Its remote guard sets any
created queue2 to **2 MiB / 64 buffers / 2 seconds**, whichever limit is reached,
and clears `temp-template`; it rejects downloadbuffer and manifest paths.
[GStreamer queue2 documentation](https://gstreamer.freedesktop.org/documentation/coreelements/queue2.html)
confirms that a temporary-file template would instead spool the stream
independently of these memory limits.

Managed [playbin buffering](https://gstreamer.freedesktop.org/documentation/playback/playbin.html#buffering)
would require explicit BUFFERING-message handling, preserving user pause intent
and the actual Playing ledger. No measured starvation defect establishes a need
to change the accepted worker here. Its memory-only transport stays unchanged;
no buffer-size change is presented as reusable audio caching.

## Provider-controlled evidence

Music terms, application license and offline help were fetched successfully
again on **2026-11-06**. The fresh Plus request returned **HTTP429**; its clauses
below retain the complete **2026-11-05** review, not a claimed new full fetch.
These are mutable service statements, not a newly issued permission for Karin.

- [Music terms](https://yandex.ru/legal/music_termsofuse/ru/), §§2.1, 2.7–2.8:
  materials retain rightsholder protection; personal use is tied to the service's
  technical facilities and limits. The reviewed text supplies no applicable
  independent-client storage/reuse grant.
- [Music application license](https://yandex.ru/legal/music_mobile_agreement/ru/),
  §§1.1, 1.4, 4.1, 4.5 (published 2026-04-03): governs the described official
  mobile, Windows and macOS applications and their stored materials. It refers to
  a separate agreement for uses outside that license; it does not identify Karin
  as the licensed program or supply Karin's cache/reuse contract.
- [Offline help](https://yandex.ru/support/music/ru/listening-and-settings/app-settings):
  describes encrypted, device-bound files played by Yandex Music. Its mobile-only
  introduction coexists with desktop download/offline instructions; neither
  should be generalized into a blanket desktop ban or permission for Karin.
- [Subscription help](https://yandex.ru/support/music/ru/registration-and-login)
  and [Plus conditions](https://yandex.ru/legal/yandex_plus_conditions/ru/),
  §§1.5, 1.7: access varies by territory/content and device/concurrency limits.
  They do not provide Karin a per-track offline expiry/renewal/revocation protocol.

## Published API signals and their limits

The current published MarshalX model at commit
[`9ca132e`](https://github.com/MarshalX/yandex-music-api/tree/9ca132e5c18a72aab779c1fc68c169380a9357d0)
was rechecked alongside Cassette on **2026-11-06**. The current yamu and yamuse
heads cited below also remain unchanged; twelve re-fetched model/client/fixture
files match the earlier reviewed hashes. These are concrete schema candidates, not a
live Karin account response or an official permission for Karin:

- [`Status`](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/account/status.py)
  includes optional `cacheLimit`, described as the maximum downloaded-track count,
  and account `permissions`.
- [`Permissions`](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/account/permissions.py)
  has `until`, `values` and `default`. The reviewed schema does not define which
  value authorizes Karin to persist/reuse a specific saved resource. A count limit
  or account timestamp alone is not that admission rule.
- Legacy [`DownloadInfo`](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/download_info.py)
  describes codec, bitrate, gain, preview/direct hints and a descriptor URL. It
  establishes a transport mechanism, not a stored-resource reuse grant. Karin
  does not adopt a protected transport or decryption-key path to fill this gap.

These fields should be evaluated if the provider supplies an applicable contract;
they must not be renamed into invented entitlement or silently ignored as proof
that no account permission data exists.

The concrete route is authenticated `GET /account/status`:
[yamu's client](https://github.com/okhsunrog/yamu/blob/9803f319b9ea6f81befbc3117e4592132ed07d0e/src/client/account.rs)
requests it and preserves permissions as a free-form map.
[yamuse's model](https://github.com/flxxxxddd/yamuse/blob/368c74646520e8bcd30c44a9887e1bf99cb58172/src/models/account.rs)
independently names `permissions.until/values/default` and describes `cacheLimit`
as an offline track-count limit. The published MarshalX permission fixture contains
`landing-play`, `feed-play` and `mix-play`; it supplies no defined storage/reuse
value. None of these reviewed schemas makes `until` a saved track's retention
deadline or maps an arbitrary unknown string to permission to cache.

No real account response was collected for this review. Even a local confirmation
that these fields exist would supply wire evidence only; it would not create the
missing permission or define the provider's cache-hit authorization semantics.

## Production boundary and concrete external gate

[ADR-0029](adr/0029-synthetic-audio-cache-foundation.md) specifies the storage,
resolver and application boundary. Original generated audio and fixture-issued
permissions can verify the same code's integrity, publication, cancellation,
LRU, fallback and restart behavior. They cannot authorize service audio. Test
instructions and actual results are in [BUILDING](BUILDING.md#synthetic-audio-cache-checks)
and [VALIDATION](VALIDATION.md#v06-audio-cache-foundation).

Enabling transparent service caching requires these two concrete inputs:

1. An applicable provider/rightsholder permission covering Karin's local storage
   and later online reuse of the admitted resource/format, including any retention
   conditions. The reviewed service facilities and official-app license do not
   supply that permission for Karin.
2. A documented or specifically confirmed provider contract defining the fresh
   account/resource check that authorizes reuse, including refusal and rights-loss
   behavior. Current streaming account validation and an issued MP3 address do not
   themselves state that contract. No local TTL or invented entitlement substitutes
   for it.

A [prepared provider request](AUDIO_CACHE_PROVIDER_REQUEST.md) asks for these
specific inputs and their machine-checkable semantics. It has **not been sent**.
The owner can obtain that applicable permission and contract from the provider
or rightsholder; account tokens, signed URLs and successful synthetic tests are
not the missing evidence. There is no proven categorical ban on every independent
client, and a storage implementation defect is not evidence of such a ban.
Until the inputs are available, the real service must decline cache admission,
fall back to the established online path and leave PR #26/Issue #20 unfinished.

A later offline mode additionally needs its actual offline validation/lifetime
rules; it is not a prerequisite we invent for the initial online-only cache.
Do not import/decrypt official-app caches, persist signed URLs, infer permissions
from reversible encoding or bypass DRM/device binding.
