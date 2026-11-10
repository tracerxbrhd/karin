<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Audio caching and offline access

Karin implements a private HTTP cache for supported, already authorized full-MP3
playback, with **fresh online access and representation validation before reuse**.
The player receives its online source immediately; an independent bounded job
can save a complete supported response to protected staging. This is not a
GStreamer capture, a Playing/EOS prerequisite or a second audio engine.
Playback access, HTTP storage policy, version validation and applicable rights
remain separate: HTTP cacheability does not create a provider entitlement or
legal license. Protected formats, explicit restrictions and `no-store` reject
storage; unverifiable versions reject local reuse.

**The owner confirmed real first playback and complete background saving on
Fedora Native at `31d873d`; local replay and restart still failed with
`resource-changed`.** The revised logical-object revalidation and graphical
indicators need final Native/installed-Flatpak acceptance. The earlier `6ec2407`
playback and `f2050b0` saving failures remain historical results. Cloud fixtures
cannot establish a new real-CDN result. See
[VALIDATION](VALIDATION.md#owner-result-at-31d873d).
Downloads, mass prefetch and offline mode remain outside this scope.
Issue [#20](https://github.com/tracerxbrhd/karin/issues/20) remains open;
public authorization/API distribution rights remain separately tracked in
[#5](https://github.com/tracerxbrhd/karin/issues/5) and the
[release checklist #22](https://github.com/tracerxbrhd/karin/issues/22).

## Three distinct modes

| Mode | Current result | Additional condition |
| --- | --- | --- |
| Active streaming buffer | Transient pipeline memory; no saved media or cross-play reuse | Preserve online-resource admission |
| Reusable audio while online | Ordinary playback can save an eligible complete MP3 response and reuse a validated local record | Fresh account/resource access, acceptable HTTP/content policy and verified retained representation before each hit |
| Offline audio | Outside this initial cache scope; no offline switch or badge | A separate contract for permitted resources, applicable account/device scope, expiry, renewal, revocation and offline checks |

Automatic cleanup does not turn a disk recording into an in-memory stream.
An old local file cannot override an API refusal, lost rights or another account.
The resolver's sixty-second admission deadline and opaque descriptor `ts` are
transport properties, not offline leases or permitted retention periods. An
online-only cache need not invent an offline entitlement or demand an offline
feature before shipping. HTTP storage/revalidation, current playback access and
content/distribution rights remain separate decisions.

## Cassette source audit

Rechecked **2026-11-10** at Cassette v0.2.4, commit
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
permitted online playback available. A new private response must provide a
strong ETag, conservatively usable Last-Modified or full Repr-Digest; without
one, the worker declines its body/store as `validators-missing`. A stale source
can be refreshed through the existing account-bound resolver. A first CDN403/410
permits one fresh resolution and a retry only if the sealed URI changed; an
unchanged refusal, network/body error or a second refusal is not retried. No new
client identity or region is attempted. Tokens/cookies never reach CDN, and
signed addresses remain in memory.

A saved file is optional data, not playback entitlement. Every local replay
resolves current Music access and verifies the retained representation with a
bounded metadata request. A proven hit supplies its pinned read-only file to the
same GStreamer worker; slow/corrupt/unverifiable cache falls back online. Clear
can unlink the record while an admitted playback FD remains usable. Completed
records survive normal close/restart, subject to fresh validation on reuse.
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
`MissingStorageContract → NoGrant` decision for ordinary playback. Four checks
remain distinct:

| Check | Required evidence and behavior |
| --- | --- |
| Current playback access | The existing account-bound Music session and a fresh supported, non-preview Legacy MP3 delivery resolution. A refusal never becomes a local cache hit. |
| HTTP storage eligibility | Inspect the independent complete-download response. Require a usable version validator and bounded, complete `audio/mpeg` representation; allow identity encoding and `Vary: Accept-Encoding` with matching identity requests; reject other encodings/variant dimensions, `no-store`, partial transfer, malformed metadata and explicit storage restrictions such as HTTP451. |
| Retained version | Validate an existing record against the currently admitted source without another full media GET. A track ID, equal size or changed signed URL alone is insufficient. |
| Content constraints | Refuse protected/unsupported content and explicit restrictions. Private HTTP admission is a local engineering policy, not a fabricated provider entitlement or declaration of legal distribution rights. |

[RFC9111](https://www.rfc-editor.org/rfc/rfc9111.html) permits private response
caching subject to its rules. `no-cache` permits storage with revalidation;
`must-revalidate` cannot permit stale fallback. Karin requires a new successful
online access/version check before **every** local play, even within an indicated
freshness lifetime. A network failure therefore preserves a permitted online
playback attempt but does not authorize stale local reuse. If fresh validation
returns `no-store`, the completed record is removed before selecting the online
source; an existing active playback descriptor remains valid. HTTP itself does
not require a validator for every stored response, but Karin conservatively
declines a new private copy without usable ETag/Last-Modified/Repr-Digest evidence,
so an unreusable body is not downloaded solely for cache storage. Any older
record still needs successful current-version proof before a local hit.

A changed download address is not itself a new audio representation. The old
hash joined XML host/path and codec/bitrate; a legitimate placement change then
entered a destructive resource-changed/refill branch. The owner confirmed this
failure at `31d873d`, without exposing the actual changed component.

The revised opaque binding follows the **logical descriptor request object and
encoding** separately from physical delivery and saved bytes. It retains the
validated descriptor origin, full object path and semantic query. Only the
published signing fields `sign` and `ts` are excluded; unknown parameters remain
significant. Only documented routes with unambiguous, well-formed signing fields
receive that normalization; other inputs leave logical identity unavailable.
This is not guessed path trimming or Track-ID-only identity.

Primary source evidence from Yandex's public
[mediastorage-proxy `d7102c4`](https://github.com/yandex/mediastorage-proxy/tree/d7102c46252275e702ba5bbf62c73f581e1497d5):

| Source | Relevant contract |
| --- | --- |
| [Download-info handler](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/download_info.cpp#L171) and [object lookup](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/proxy.cpp#L838) | Resolve the descriptor request path to namespace/group/file. The namespace/file key names the stored object; query signing fields do not choose its physical location. |
| [Physical locator](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/lookup_result.cpp#L113), [XML generation](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/utils.cpp#L98) and [regional rewrite](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/proxy.cpp#L920) | Select replica host and physical file/offset/size paths; regional delivery can move a host component into the path. These addresses are not immutable object identity. |
| [ETag generation](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/get.cpp#L702) and [conditional response](https://github.com/yandex/mediastorage-proxy/blob/d7102c46252275e702ba5bbf62c73f581e1497d5/src/get.cpp#L831) | ETag derives from object timestamp/size, not audio bytes. It is usable as a scoped version validator, never a global MD5 content fingerprint. |

The published Music client's
[download-info fixture](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/tests/test_download_info.py#L20)
also separates logical file-download-info paths from `sign`/`ts`. The older MDS
source explains the protocol model; it does not establish that the owner's 2026
CDN deployment or every endpoint behaves identically. No real-account resource
or signing value is copied into diagnostics.

Strong validators must still describe a confirmed common logical object/variant
or exact resource scope. [RFC9110 §8.8.1](https://www.rfc-editor.org/rfc/rfc9110.html#section-8.8.1)
warns that equal ETags on different resources do not prove equal bytes, and bare
304/If-None-Match uses weak comparison. Same object and a matching strong tag plus
length can survive physical placement changes; a different logical object cannot
borrow that tag. Strong Last-Modified additionally needs the conservative RFC
Date gap of at least sixty seconds and the same resource/length checks.
Requests use identity encoding; unsupported Vary dimensions remain refused.

A trusted full SHA-256 in
[`Repr-Digest`, RFC9530 §3](https://www.rfc-editor.org/rfc/rfc9530.html#section-3)
can independently match the completed record's measured hash/length across
locators. Content-Digest on a HEAD message is not the full audio digest. A known
encoding mismatch cannot select the wrong representation. Older records without
separate binding metadata require sufficient existing proof or one complete
validated replacement before the new binding is available; the old copy remains
until successful atomic publication.

An unproved modern object/locator change, missing validator or temporary failure
uses online playback while retaining the correct local record and suppressing
another cache-body rewrite. Confirmed version/encoding changes, no-store,
explicit access/content refusals and corruption retain scoped retirement.
No stale local fallback is introduced. Finite diagnostic categories identify
signing, host, path, variant or proof outcomes, never their raw values.

Only bounded cache metadata, opaque identity, measured length/hash and validators
are retained beside the completed record. OAuth tokens, cookies, signed URLs,
raw responses and personal account fields are never serialized. The existing
shared budget accounts for storage; Clear, logout, account changes and
cancellation fence both payload and metadata. Partial or corrupt records are not
hits. A storage or validation failure must not break otherwise admitted network
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

The owner confirmed first real playback/background saving at `31d873d`, but
local replay and restart failed with resource-changed. **The revised replay and
presence behavior is not yet accepted on the real service.** The single final
[Native/Flatpak procedure](BUILDING.md#pr-26-corrected-cache-acceptance) obtains only
fixed cache/source/reason categories from the actual playback path. It never
requests tokens, signed addresses, identifiers, response bodies or music files.
PR #26/Issue #20 remain open until the requested live user scenario is accepted.
Offline listening, official-cache import/decryption, downloads and mass prefetch
are outside this implementation.
