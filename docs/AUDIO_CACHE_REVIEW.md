<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Audio caching and offline access

Karin implements a private HTTP cache for supported, already authorized full-MP3
playback, with **fresh online access and representation validation before reuse**.
The policy considers the response actually delivered to the existing GStreamer
player. It does not invent a Yandex storage entitlement or call HTTP caching a
legal license. A missing bespoke provider-grant API no longer blocks analysis of
an otherwise eligible response. Explicit content restrictions, protected formats
and HTTP storage prohibitions still reject the cache copy.

**Real Music service behavior for this revision has not been verified.** The
production path is connected, but a local fixture cannot establish which
validators the actual CDN supplies or that a particular real track will produce
a local hit. Downloads, mass prefetch and offline mode remain outside this scope.
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

The PR #26 completion review on **2026-11-06** resolved the release tags again
and checked the current main head. They still identify the revisions above;
all thirteen re-fetched legacy/current cache and player source files match the
earlier reviewed bytes. No newer cache chain or provider storage/reuse contract
was established by that check.

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

For a policy-admitted resource, a source-pad probe copies the existing player's
compressed bytes into a bounded channel. The writer starts draining at playback
preparation, before Playing, so fast network/preroll data does not wait for a GTK
tick or a positive playback clock. The capture can retain at most one complete
**64 MiB** resource; at most four pending captures and one staging writer are
admitted. Bodies allocate on arrival, with a worst-case retained-body bound of
256 MiB across the four captures. This is separate from queue2's unchanged
limit, and creates no second
media GET or second player. Probe callbacks never wait for storage.

Capture completion means the exact HTTP body length has arrived with valid
sequence/policy checks; it does not wait for downstream or pipeline EOS. A
completed capture remains readable after the source pipeline is dropped. Actual
GStreamer Playing is latched separately and is still required before publication:
early draining is not permission to publish an unplayed or incomplete response.
HTTP200 or HTTP206 covering exactly bytes `0..total-1` can qualify. Incomplete
transfer, non-identity encoding, `no-store`, invalid byte sequence, premature
seek/flush/segment changes and overflow reject only the cache copy. The first
finite failure cause is retained instead of being overwritten by teardown.
The opt-in `captured` diagnostic denotes body completion, not publication;
`write-confirmed` remains a separate atomic-publication result.

A completed, played resource can seal its write-specific finalization admission
for at most **30 seconds** to finish hashing, metadata and atomic publication.
Normal Next, natural EOS, Wave continuation and a new online lookup do not revoke
that sealed write merely by replacing the playing track. Partial captures are
cancelled on replacement. Explicit Stop, account/logout, Clear and Disable still
cancel or fence pending writes immediately. The finalization pin cannot admit a
cache hit: every later play must obtain fresh access and version validation.

Disk integrity work overlaps StreamResolver's fresh access check. Selection
consumes only an already-ready candidate; a slow or busy cache leaves that play
online. A candidate additionally needs a bounded HTTP revalidation against the
currently admitted source. Final file-descriptor admission is memory-only and
LRU bookkeeping is deferred rather than delaying audio. The first play does not
start another full media request to fill the cache.

`LicensedResourceAuthority` is a production policy for trusted rightsholder
input, currently an exact **CC0-1.0** dedication bound to account/track, SHA-256,
length and an application admission revision. It checks those exact bytes before
publication and reuse. An HTTP `license` label, cache directive or API display
field cannot populate that registry. The original audio generated by the
ordinary-application process fixture is expressly dedicated to CC0; its Rust
source remains GPL-3.0-or-later. This separate policy is not the ordinary private
HTTP admission rule and does not manufacture a grant for Yandex catalog audio.

Managed [playbin buffering](https://gstreamer.freedesktop.org/documentation/playback/playbin.html#buffering)
would require explicit BUFFERING-message handling, preserving user pause intent
and the actual Playing ledger. No measured starvation defect establishes a need
to change the accepted worker here. Its memory-only transport stays unchanged;
no buffer-size change is presented as reusable audio caching.

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
| HTTP storage eligibility | Inspect the actual guarded GStreamer response. Accept only a bounded, complete `audio/mpeg` representation; allow identity encoding and `Vary: Accept-Encoding` with matching identity requests; reject other encodings/variant dimensions, `no-store`, partial transfer, malformed metadata and explicit storage restrictions such as HTTP451. |
| Retained version | Validate an existing record against the currently admitted source without another full media GET. A track ID, equal size or changed signed URL alone is insufficient. |
| Content constraints | Refuse protected/unsupported content and explicit restrictions. Private HTTP admission is a local engineering policy, not a fabricated provider entitlement or declaration of legal distribution rights. |

[RFC9111](https://www.rfc-editor.org/rfc/rfc9111.html) permits private response
caching subject to its rules. `no-cache` permits storage with revalidation;
`must-revalidate` cannot permit stale fallback. Karin requires a new successful
online access/version check before **every** local play, even within an indicated
freshness lifetime. A network failure therefore preserves a permitted online
playback attempt but does not authorize stale local reuse. If fresh validation
returns `no-store`, the completed record is removed before selecting the online
source; an existing active playback descriptor remains valid. A complete eligible
response may be stored without validators, but cannot become a local hit without
a later successful proof of its current version.

For URI-bound validation, a returned strong validator must describe the same
resource/variant. A bare `304` is not an exact-byte guarantee: `If-None-Match`
uses weak comparison. [RFC9110 §8.8.1](https://www.rfc-editor.org/rfc/rfc9110.html#section-8.8.1)
also warns that equal ETags on different resources do not prove equal bytes.
Karin retains opaque fingerprints, not signed URLs. The resolver additionally
binds the sealed Legacy descriptor's lowercase host, validated XML path, MP3
codec and selected bitrate to a file-locator fingerprint across renewed
signatures. Signing inputs `s` and `ts` are excluded. That binding still needs a
matching strong validator and length; it does not declare the path immutable.
Arbitrarily stripping URI signature or timestamp components is not a
representation proof. A strong Last-Modified validation can be used only with the conservative RFC Date gap of
at least sixty seconds and the required resource/length checks; an arbitrary date
is not a content version. Both player and validation requests explicitly use
`Accept-Encoding: identity`, allowing that one `Vary` dimension. Other nonempty
`Vary` dimensions are declined rather than compared under unknown request state.

A trusted full-representation SHA-256 in
[`Repr-Digest`, RFC9530 §3](https://www.rfc-editor.org/rfc/rfc9530.html#section-3)
can be compared with the completed record's measured hash and length even when a
signed URI changes. `Content-Digest` on a HEAD message is not the full audio
digest. A suitable standard validator can establish identity without a bespoke
Yandex SHA field, new entitlement endpoint or Offline Lease. If the actual server
offers none, that resource stays online with a fixed diagnostic reason; its
identity is not guessed.

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

**The real service has not been verified for this revision.** The single final
[Native/Flatpak procedure](BUILDING.md#pr-26-corrected-cache-acceptance) obtains only
fixed cache/source/reason categories from the actual playback path. It never
requests tokens, signed addresses, identifiers, response bodies or music files.
PR #26/Issue #20 remain open until the requested live user scenario is accepted.
Offline listening, official-cache import/decryption, downloads and mass prefetch
are outside this implementation.
