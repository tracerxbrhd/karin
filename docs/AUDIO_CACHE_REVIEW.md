<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Audio caching and offline access

Karin implements a private HTTP cache for supported, already authorized full-MP3
playback, with **current Music access, matching selection, integrity and a valid
private HTTP reuse lease before local playback**. A fresh local hit is selected
before XML/CDN resolution. On a miss the player receives its online source
immediately; an independent bounded job can save a complete supported response
to protected staging. This is not a
GStreamer capture, a Playing/EOS prerequisite or a second audio engine.
Playback access, HTTP storage policy, version validation and applicable rights
remain separate: HTTP cacheability does not create a provider entitlement or
legal license. Protected formats, explicit restrictions and `no-store` reject
storage. Stale/unproved representations require validation or remain online.

**The owner confirmed real first playback and complete background saving on
Fedora Native. At `cac4d78`, replay still caused descriptor-path /
representation-changed downloads, without a confirmed local hit.** Corrected
local-first selection, response freshness and retained graphical indicators need
final Native/installed-Flatpak acceptance. Current real CDN freshness headers
have not been measured in Cloud. The earlier `6ec2407`
playback and `f2050b0` saving failures remain historical results. Cloud fixtures
cannot establish a new real-CDN result. See
[VALIDATION](VALIDATION.md#owner-result-at-cac4d78).
Downloads, mass prefetch and offline mode remain outside this scope.
Issue [#20](https://github.com/tracerxbrhd/karin/issues/20) remains open;
public authorization/API distribution rights remain separately tracked in
[#5](https://github.com/tracerxbrhd/karin/issues/5) and the
[release checklist #22](https://github.com/tracerxbrhd/karin/issues/22).

## Three distinct modes

| Mode | Current result | Additional condition |
| --- | --- | --- |
| Active streaming buffer | Transient pipeline memory; no saved media or cross-play reuse | Preserve online-resource admission |
| Reusable audio while online | Ordinary playback saves an eligible complete MP3 and selects fresh local data before XML/CDN | Current account/track access, matching authenticated API selection, integrity and fresh response lease; otherwise strict validation or online fallback |
| Offline audio | Outside this initial cache scope; no offline switch or badge | A separate contract for permitted resources, applicable account/device scope, expiry, renewal, revocation and offline checks |

Automatic cleanup does not turn a disk recording into an in-memory stream.
An old local file cannot override an API refusal, lost rights or another account.
The resolver's sixty-second admission deadline and opaque descriptor `ts` are
transport properties, not offline leases or permitted retention periods. An
online-only cache need not invent an offline entitlement or demand an offline
feature before shipping. HTTP storage/revalidation, current playback access and
content/distribution rights remain separate decisions.

## Cassette source audit

Rechecked **2026-11-11** at Cassette v0.2.4, commit
[`9d34251`](https://github.com/Rirusha/Cassette/tree/9d34251d01bcd414ffbec2bef7931090d33c9124).
All six files below carry Vladimir Vaskov's copyright and GPL-3.0-or-later.
They are technical references; no cache implementation, obfuscation or
official-client material is copied into Karin's store.
The reviewed cachier/storager/player files are byte-identical in v0.2.5
[`b1c55e5`](https://github.com/Rirusha/Cassette/tree/b1c55e5fb71e60c9ff5421db42695f4dd5526a26).
Cassette's audio key is only Track ID; quality is selected on a miss, with no
fresh account, version or HTTP-policy check on the local-hit path. Karin adopts
its local-before-network ordering, while retaining account/variant/integrity and
freshness safeguards.

| Pinned source | Observed mechanism and limit of the evidence |
| --- | --- |
| [cachier.vala](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier/cachier.vala#L140) | Reads complete audio bytes, selects temporary/permanent storage, tries local audio before resolving the network resource and starts a background save. This is persistence, not merely a streaming buffer. |
| [controller.vala](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier/controller.vala#L117) | Tracks loading and TEMP/PERM states; file location determines cached state. Presence alone does not establish current entitlement. |
| [job.vala](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier/job.vala#L145) | Album/playlist jobs reconcile constituent tracks. Cancellation stops further scheduling and waits for active saves before cleanup; the reviewed whole-file download has no per-chunk cancellation contract. |
| [storager.vala](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier/storager.vala#L614) | Separate XDG cache/data locations, local-first lookup, whole-file writes and decoded temporary playback files. Size reporting uses `du`; these six reviewed files do not establish an automatic quota/LRU policy. Reversible XOR is neither authenticated encryption nor a storage grant. |
| [info-db.vala](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier/info-db.vala#L23) | SQLite references retain tracks used by several saved collections. Those references are not account/device/track licenses. |
| [player.vala](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/player/player.vala#L492) | Passes the local/network URI to playbin and may prefetch the next track. Karin retains one player without adopting its next-track prefetch. |

The network side is concrete:
[`YaMTalker.get_download_uri/load_track`](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/talkers/yam-talker.vala#L300)
delegates to
[`Client.track_download_uri`](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/yam-client.vala#L1221)
and `get_content_of`. The client requests the track's `download-info`, chooses
an available bitrate, fetches the descriptor, forms the MP3 resource URI and
loads its complete response into bytes for the separate save operation. This
is a second resource transfer beside playbin's first network playback; it is
not a tee of bytes already consumed by the player. Those functions expose no
separate storage/reuse admission decision.

The reviewed path returns a local file before resolving a fresh network resource.
It establishes the legacy cache-selection/save chain, but not the current
permission for Karin to reuse that file. Nor does it establish an offline lease or
renewal/revocation protocol. This is an evidence gap, not a claim that Cassette
cannot technically cache audio or that every independent client is forbidden.

In both releases, the selected-track miss calls
[`download_audio_async.begin` before returning the network URI](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier/cachier.vala#L190).
The ordinary whole-file save runs on the audio pool with no player-generation,
Playing or EOS gate; Next does not revoke it. Collection-job cancellation is a
different layer. Saved cache/data files survive normal startup: the reviewed
explicit wipe is a Preferences action, while playback unlinks only its decoded
temporary file. Thus first Play can stream and save independently, replay finds
local audio, and restart retains the completed copy. Karin follows that lifetime
separation while requiring current access/version checks before its local hit.
The extra full GET costs bandwidth; streaming it to bounded staging avoids
Cassette's whole-response memory and Karin's former capture/EOS races.

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

The **2026-11-10** recheck resolved both release tags and current main again;
25 selected immutable blobs were checked against their Git hashes. The ten
reviewed legacy cache/player/API files are byte-identical between v0.2.4 and
v0.2.5. Current main remains the revision above. This establishes source behavior,
not a new runtime or provider-permission result.

The newer core cache/player files credit Vladimir Romanov and generally carry
GPL-3.0-or-later. Auxiliary `humanity-size.vala`, `location.vala` and `settings.vala`
carry GPL-3.0-only. Future reuse must retain each file's actual license/copyright;
Karin does not copy their implementation or relabel them under its own license.

## Current Karin implementation

The cache worker follows the released Cassette scheduling pattern while using
Karin's existing AudioCache, CacheBudget and StreamResolver. An explicit playback
intent receives a freshly admitted online source immediately. On a miss, one
independent bounded job downloads the selected MP3 through the existing HTTP
client; preparation without autoplay waits for Play. No unselected-track
prefetch, second audio engine or GStreamer download mode is introduced.

The job streams to protected staging instead of retaining a whole response in
memory. Bounded MP3 structure screening, declared/actual length, HTTP policy and
SHA-256 are checked before atomic publication. Screening checks leading tag
bounds and consecutive complete Layer III frames; it is not a full decoder or
proof that every frame will play. Bounds remain 64 MiB/resource, 1024 records, four small job
contexts and one staging writer, under the same enabled-by-default 2 GiB shared
quota, LRU, disk headroom and six Preferences choices. Native Soup buffering stays
at 2 MiB/64 buffers/2 seconds with no temporary-file template.

Source-pad capture, compressed-byte channels, capture overflow/offset bookkeeping,
Playing/EOS admission latches and the post-EOS completion downloader are removed.
The background request has its own one-use write operation: at most 30 minutes
queued, then a nonrenewable 30-second admission from worker start. Its HTTP
request timeout is 15 seconds; account/cache fences are checked around each
bounded read and before publication. Ordinary
Next/Previous, source replacement, manual Skip, Pause, Seek and Wave rollover do
not cancel it. Explicit Stop, application close, Disable/Clear, logout and account
change fence unfinished work. Seek never causes discontiguous playback bytes to
be joined into a saved file. Partial network responses are not published.

The downloaded response establishes its own storage policy and complete version;
no bytes from the player's response are reused. A complete newer representation
is not spliced onto an older prefix. No-store, unsupported/protected content,
access refusal, malformed metadata, timeout and incomplete transfer leave
permitted online playback available. A new private response needs a usable
response-freshness lease or scoped ETag/Last-Modified/Repr-Digest evidence;
otherwise the worker declines its body/store as `validators-missing`. A stale source
can be refreshed through the existing account-bound resolver. A first CDN403/410
permits one fresh resolution and a retry only if the sealed URI changed; an
unchanged refusal, network/body error or a second refusal is not retried. No new
client identity or region is attempted. Tokens/cookies never reach CDN, and
signed addresses remain in memory.

A saved file is optional data, not playback entitlement. Every local replay
checks current Music access and authenticated offer selection. Saved integrity
and fresh response policy can admit a local source before descriptor/CDN
resolution; otherwise a bounded representation check or online fallback applies.
The pinned read-only file goes to the same GStreamer worker. Clear can unlink
the record while its admitted playback FD remains usable. Completed records and
reuse policy survive normal close/restart; access is checked again on reuse.
The separate LicensedResourceAuthority exact-content policy is retained for
trusted CC0 declarations; it is not a mock Music grant in the ordinary app.

Fixed diagnostics distinguish download progress, atomic publication and actual
local reuse. Cloud tests use original permitted audio/local HTTP through the
normal application. They cannot prove compatibility with a real Music CDN or
Fedora/installed-Flatpak acceptance. See [VALIDATION](VALIDATION.md) and the
[single final procedure](BUILDING.md#pr-26-corrected-cache-acceptance).

## Provider-controlled evidence

Music terms, application license, offline help and Plus conditions were fetched
successfully again on **2026-11-06** during the PR #26 completion review. The
later Plus request returned **HTTP200**, replacing the earlier rate-limited
request; its relevant territory/device/concurrency clauses were rechecked.
These are mutable service statements, not a newly issued permission for Karin.

- [Music terms](https://yandex.ru/legal/music_termsofuse/ru/), §§2.1, 2.7–2.8:
  materials retain rightsholder protection; personal use is tied to the service's
  technical facilities and limits, with copying/reuse restricted without agreement.
  That affirmative boundary does not identify Karin's retained-file mechanism as
  an admitted facility. It also does not prove a categorical independent-client ban.
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

These fields remain account/content signals. They must not be renamed into a
provider storage entitlement or silently ignored as proof that no account
permission data exists. Their absence does not override ordinary HTTP policy.

The concrete route is authenticated `GET /account/status`:
[yamu's client](https://github.com/okhsunrog/yamu/blob/9803f319b9ea6f81befbc3117e4592132ed07d0e/src/client/account.rs)
requests it and preserves permissions as a free-form map.
[yamuse's model](https://github.com/flxxxxddd/yamuse/blob/368c74646520e8bcd30c44a9887e1bf99cb58172/src/models/account.rs)
independently names `permissions.until/values/default` and describes `cacheLimit`
as an offline track-count limit. The published MarshalX permission fixture contains
`landing-play`, `feed-play` and `mix-play`; it supplies no defined storage/reuse
value. None of these reviewed schemas makes `until` a saved track's retention
deadline or maps an arbitrary unknown string to permission to cache.

No real account response was collected for this review. A local confirmation of
these fields would supply wire evidence, not a legal license or an immutable
representation identity.

## Private HTTP admission and resource validation

The 2026-11-08 implementation revision supersedes the blanket
`MissingStorageContract → NoGrant` decision for ordinary playback. These checks
remain distinct:

| Check | Required evidence and behavior |
| --- | --- |
| Current playback access | The existing account-bound Music session and a current authenticated offer for supported, full Legacy MP3. An API refusal never becomes a local cache hit. |
| HTTP storage eligibility | Inspect the independent complete-download response. Require a bounded, complete `audio/mpeg` representation and usable freshness or validation evidence; allow identity encoding and supported Vary only; reject no-store, unsupported/protected types, partial transfers, malformed policy and explicit restrictions. |
| Retained selection and integrity | Compare schema-2 logical descriptor object plus MP3 codec/bitrate/gain/fullness from the API offer, independently of its temporary CDN locator. Recheck the account/token epoch, complete length and SHA-256; Track ID alone is insufficient. |
| Reuse window | Use an unexpired private response lease; otherwise require strict representation validation. A selection fingerprint is not an immutable byte-generation guarantee. |
| Content constraints | Refuse protected/unsupported content and explicit restrictions. Private HTTP admission is an engineering policy, not a fabricated provider entitlement or legal distribution license. |

The local-first order is current access → authenticated offer → local integrity
and freshness → local URI. XML descriptor/CDN resolution is reserved for a miss
or required validation. The offer's logical object and selected format identify
what was requested; the retained SHA-256 identifies the saved bytes. Neither
claims a provider-issued immutable content version. Public Track.storageDir,
fileSize, realId and descriptive version fields have no inspected byte-version
contract. A known logical selection/encoding change retires/refills the record;
a refreshed signed media URI alone is irrelevant to the fresh-hit path.

[RFC9111 §4.2](https://www.rfc-editor.org/rfc/rfc9111.html#section-4.2)
permits reuse of fresh stored responses. Calculate freshness from explicit valid
max-age or Expires, corrected Date/Age and persisted request/response clocks.
Without explicit expiration, §4.2.2 permits heuristic freshness for eligible
HTTP200 responses: Karin requires Date at least 60 seconds after Last-Modified
and uses ten percent of that interval, capped at one hour. This is a bounded heuristic, not an immutable-recording or
rights claim. Invalid/conflicting policy, clock rollback and old sidecars without
receipt metadata do not acquire this shortcut. RFC9111 computes apparent Age as
max(0, receipt time−origin Date): a future origin Date alone does not prove local
clock rollback. A replay clock before its saved local receipt remains uncertain.

`private` permits a private cache; `no-cache` always requires successful
validation; `must-revalidate` prevents reuse once stale; `no-store` forbids
storage. Current Music access is still checked on every playback, including
after restart. A stale or unproved file cannot bypass a refusal or network
failure. If validation returns no-store, remove the retained record while
preserving any already admitted playback descriptor. The presence of max-age
or a validator is not a copyright license. Actual deployed CDN freshness headers
remain unknown until the owner's service acceptance; absent explicit max-age is
not itself proof that standard freshness is impossible.

A changed download address is not itself a new audio representation. The old
aggregate hash invalidated host/path relocation. The `cac4d78` correction then
went too far in the opposite direction: it treated the same logical object as
a shared validator scope across physical replicas. The owner still observed
`descriptor-path-changed → representation-changed → downloading/write-confirmed`,
with no local hit. The finite log does not reveal which actual HTTP validator
changed. Code review establishes the invalid cross-scope comparison, without
inventing unavailable CDN headers or addresses.

The opaque logical binding retains the descriptor origin, full object path and
semantic query, excluding only documented `sign`/`ts` signing fields. It supplies
object provenance for the selected format, not an immutable byte version or
permission to compare independent CDN validators. Unknown query dimensions remain significant.

Primary evidence from Yandex's public
[mediastorage-proxy `d7102c4`](https://github.com/yandex/mediastorage-proxy/tree/d7102c46252275e702ba5bbf62c73f581e1497d5):

| Source | Established fact and limit |
| --- | --- |
| [Descriptor lookup](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/proxy.cpp#L838) and [physical location](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/lookup_result.cpp#L113) | Namespace/file lookup can select a replica and physical file/offset/size location. The logical key alone does not prove global HTTP validator semantics. |
| [ETag generation](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/get.cpp#L702) and [internal replica comparison](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/get.cpp#L210) | The exported ETag is MD5 of timestamp/size, not audio. Internal checksums exist, but that does not expose them to Karin or guarantee the deployed legacy CDN's cross-replica tags. |
| [XML fields](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/download_info.cpp#L230) and [signature timestamp](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/utils.cpp#L128) | This historical producer exports host/path/ts/s and generates signing expiry. It exports no full content checksum/version ID. Its signature protocol does not establish every deployed legacy get-mp3 field's semantics. |

Cassette and the pinned [Music client](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/download_info.py#L58)
copy XML `ts` into the legacy request. That field remains opaque in Karin; it is
not promoted to a version fingerprint or used to grant expiry/rights. No
independent provider body/version identifier has been confirmed in the consumed
API/XML contract. This does not forbid fresh local reuse: a matching current
selection and valid response lease avoid a new media URI entirely. Once
revalidation is required, a rotated URI without a full representation digest
may leave the retained candidate unproved; that strict path stays online without
redundant cache refills. Historical source is evidence about its
implementation, not a claim that the owner's 2026 CDN is identical.

[RFC9110 §8.8.1](https://www.rfc-editor.org/rfc/rfc9110.html#section-8.8.1)
limits strong-validator uniqueness to a particular resource and allows a tag to
change for representation metadata even when bytes are unchanged. Equal tags
across different resources do not prove equivalence. [RFC9111 §4](https://www.rfc-editor.org/rfc/rfc9111.html#section-4)
selects responses by method/target URI and Vary; [§4.3.4–4.3.5](https://www.rfc-editor.org/rfc/rfc9111.html#section-4.3.4)
limits 304/HEAD updates to responses eligible for that request.

Send old If-None-Match/If-Modified-Since only to the exact provider media URI,
with no known logical/encoding mismatch. Even if a change appears to be only a
signed wrapper, opaque legacy XML ts does not establish auth-only equivalence.
Different URIs/replicas' ETags or Last-Modified values, matching or not, cannot
prove byte sameness or difference merely through a logical key. Within the
exact compatible URI scope, a strong match and full length can validate reuse;
a validator mismatch invalidates that representation version, not necessarily
a cryptographically established byte change. Last-Modified additionally needs
the conservative Date gap. A 304 without a legitimate scoped precondition is
not new identity proof. Requests retain identity encoding, bounded full-range
206 handling and strict Vary/content policy.

A valid full SHA-256 in [Repr-Digest, RFC9530 §3](https://www.rfc-editor.org/rfc/rfc9530.html#section-3)
can independently match the retained hash and whole length across locators.
The existing HEAD asks for `Want-Repr-Digest: sha-256=1` under RFC9530 §4; a
server may omit it, so this neither adds a request nor assumes CDN support.
Content-Digest for a HEAD message or a partial response is not that proof.
A complete new length or full-content digest mismatch proves different bytes;
encoding changes, explicit restrictions and invalid records remain separate.

Required revalidation retains three distinct outcomes: Same admits a verified
local file after current access/final fences; Different follows retirement/update
policy; Unknown keeps a modern candidate and plays online without a new body save.
The fresh local-first path needs no new CDN validation request. Pre-schema-2
records and upgrades with unknown original response status first attempt strict
validation; missing clocks on an otherwise modern record do not classify it as
legacy or authorize repeated full downloads.
Positive Same may update the sidecar atomically without downloading audio. If
proof remains inconclusive, a current authenticated offer can admit a complete
response through the existing background writer to migrate that legacy record.
Only full validation and atomic publication replace the old bytes; old mtime,
opaque validators or an old file never become a fabricated fresh response.
Refusal/no-store prevents migration; successful schema-2 publication ends it.
An expired modern lease with incomparable replicas can admit one replacement
when current probe policy is fresh and the actual GET headers independently
permit fresh reuse. The latter gate runs before the body is read. Successful
publication renews the measured lease; no-cache/unproved freshness cannot enter
this path, so it does not redownload on every unchanged selection. Only a successful metadata rebase emits `binding-rebased`; a complete new
migration save emits normal publication events. No second track-state database
or stale local fallback is introduced. Saved continues to mean an actual complete file, not offline access.
Finite diagnostics distinguish content/length differences, scoped validator
invalidation, uncomparable replicas, missing proof, retention and verified local
use without exposing underlying values.

Only bounded cache metadata, opaque identity, measured length/hash and validators
are retained beside the completed record. OAuth tokens, cookies, signed URLs,
raw responses and personal account fields are never serialized. The existing
shared budget accounts for storage; Clear, logout, account changes and
cancellation fence both payload and metadata. Whole-response quota reservations
protect an existing target while counting staging. Atomic directory exchange
keeps the original through final fences and rolls back on failure; the pending
reservation stays live through rollback/cleanup against cross-store LRU. Partial
or corrupt records are not hits. A storage or validation failure must not break otherwise admitted network
playback.

## Rights, distribution and verification scope

The HTTP policy does not convert a subscription, transport URL, GPL code or
`Cache-Control` into a copyright license. Primary service terms and applicable
content rights remain separate; public registration/distribution questions stay
in #5/#22. Qualifying statutory personal copying is also distinct from transient
buffering or a provider-issued entitlement. No categorical independent-client
ban or universal permission is inferred here. The earlier
[prepared provider questions](AUDIO_CACHE_PROVIDER_REQUEST.md) remain unsent
historical context, not a mandatory new API prerequisite for private HTTP checks.

[ADR-0029](adr/0029-synthetic-audio-cache-foundation.md) records the revised
boundary. Automated fixtures use the ordinary MainWindow, persistence, shared
budget, queue and GStreamer with original audio and local HTTP responses. They
cover first save, replay/restart, validators, changed representations, refusal,
corruption, Clear and cancellation without a real account. Results belong in
[VALIDATION](VALIDATION.md); they do not demonstrate live CDN behavior.

The owner confirmed real playback/background saving, but at `cac4d78` repeated
representation invalidation still prevented a confirmed local hit. **Corrected
replay is not yet accepted on the real service.** The single final
[Native/Flatpak procedure](BUILDING.md#pr-26-corrected-cache-acceptance) obtains only
fixed cache/source/reason categories from the actual playback path. It never
requests tokens, signed addresses, identifiers, response bodies or music files.
PR #26/Issue #20 remain open until the requested live user scenario is accepted.
Offline listening, official-cache import/decryption, downloads and mass prefetch
are outside this implementation.
