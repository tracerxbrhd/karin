<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Service-audio delivery contract and implementation

Updated: 2026-10-30, for [Issue #9][issue9] and its [P0 follow-up][p0].
**Audible Yandex Music is mandatory for v0.2, and that acceptance is still NOT
MET.** The guarded resolver, remote GStreamer source and player controls are now
implemented; they are not another source-only plan or a claim that the owner
has heard music. No real account, token, private response or music entered Cloud/CI.

## Observed owner evidence

The owner confirmed ordinary Karin sign-in, Music account/library access, opening
a real track with confirmed liked state, and album search/details on Fedora.
Track search still failed at `99a177e`; the parent PR's subsequent optional-credit
correction is a separate workflow and cannot block Library-track audio work.

The actual delivery diagnostic returned only:

```text
stage=delivery-info category=metadata-origin-unverified http=200 metadata-http=none formats=mp3 resource=https-descriptor protection=not-established direct-hint=false preview-hint=false
```

This proves an offer read, **not** a descriptor GET, media GET, entitlement for
every track, lack of DRM or audible playback. The exact returned URL was neither
provided nor requested. Its rejected path is not asserted to be the new variant.

## Version-pinned protocol evidence

| Reference | Observed contract | Use in Karin |
| --- | --- | --- |
| Cassette v0.2.4 source `9d34251d01bcd414ffbec2bef7931090d33c9124` ([offers/resolution][cassette-resolution], [playbin][cassette-player]) | Authenticated per-track offers → XML components → historical checksum/get-mp3 resource → playbin/EOS | Original Rust resolver and existing worker follow this narrow legacy chain; no Cassette media cache or UI resource is imported |
| lifeart/ya-namp `49cfee7002ebf816f4144860e67d6fa3be5f5708`, [2026-07-16 investigation][lifeart] | Explicit `preview:false`, MP3 offer at exact HTTPS `storage.mds.yandex.net/download-info/`; XML/checksum → canonical resource | Corroborates the missing metadata-path alternative; not evidence of the owner's exact URL or of a service permission grant |
| MarshalX `9ca132e5c18a72aab779c1fc68c169380a9357d0`, [`DownloadInfo`][marshal-download] | Canonical `/get-mp3/<checksum>/<ts><leading-slash-path>`; about one minute from delivery information, then HTTP 410 | Canonical construction and conservative local freshness; `ts` stays opaque rather than being interpreted as a grant/expiry timestamp |
| [Published legacy XML example][legacy-host] | Historical host `s268myt.storage.yandex.net` and hexadecimal-like `ts` | Narrow generated-host policy and bounded hexadecimal component parsing; not an exhaustive observed current CDN inventory |
| yamu `9803f319b9ea6f81befbc3117e4592132ed07d0e`, [`get-file-info` negotiation][yamu-negotiation] | Modern source explicitly uses an extracted official-client HMAC constant and another Web identity; returned media may have a key | Excluded: no modern signer, copied identity headers, key retrieval or decryption |

The legacy public checksum framing is assessed separately from encryption,
DRM and the explicitly extracted modern signer. The reviewed evidence does not
establish that this historical prefix is an extracted official secret or that
all Yandex media is protected. It also does not establish Yandex's approval of
Karin or public-distribution rights. A source license is not a service/content
license. Existing Cassette attribution and original upstream notices remain.

## Implemented resolver boundary

An explicit user **Play** action sends the existing validated Music session and
selected track ID to `StreamResolver` on a worker. The normal authentication
mechanism and MusicService account binding are unchanged. No token/URL entry,
new app registration, region change or unrelated client identity is involved.

1. Fetch one bounded `/tracks/{id}/download-info` response with OAuth attached
   only to that Music API request. Reject errors mixed with usable-looking
   results, contradictory offers, protection/key/encryption markers, unsupported
   codecs and missing/full-track preview permission. Select an explicit
   `preview:false` MP3 offer; neither `direct` nor availability metadata alone is
   sufficient.
2. Fetch at most one bounded descriptor without OAuth, cookies, Origin or Referer.
   Accept only exact HTTPS `storage.mds.yandex.net:443` and a nonempty path below
   `/get-download-info/`, `/download-info/` or `/file-download-info/`. Reject userinfo (including empty
   userinfo), fragments, ambiguous/traversal path forms, other hosts/ports and all
   redirects. Unknown origin and known-origin unsupported path have separate
   fixed categories. A signed query, if present, stays private in memory.
3. Parse at most 64 KiB / 64 XML nodes, with no DTD or external entities. Required
   host/path/ts/server-input components must be unique and bounded. Protection
   and preview indications, duplicate/ambiguous components and unsupported shapes
   fail before construction. The historical ordinary MD5/checksum uses only its
   public framing and the fresh server-described components; no modern official
   signing/decryption material is used.
4. Admit only canonical HTTPS `/get-mp3/` resources under the restricted single
   label `s` + 1–5 decimal digits + 3 lowercase letters + `.storage.yandex.net`.
   No arbitrary `*.yandex.net` or caller-supplied resource is accepted. The
   monotonic 60-second deadline starts **before the offer request** and is not
   reset after a slow descriptor. Token validity, account binding and cancellation
   are checked again before returning the opaque source.

The previous document's [Python-fixture citation][metadata-fixture] actually
uses `/file-download-info/`; it did not establish the old `/get-download-info/`
claim. The existing path policy is retained, lifeart corroborates
`/download-info/`, and the pinned Python fixture corroborates the third exact
`/file-download-info/` family. No arbitrary storage path or origin is admitted.

## GStreamer and user controls

`PlaybackSource::local_file` remains intact. The separate remote constructor
accepts only an opaque resolver-issued `RemotePlaybackSource`, whose private
URI is zeroized on drop and absent from Debug, snapshots and displayable errors.
It is not a general URI entry point. The worker checks policy/freshness again
before a new connection, resume or seek. An already playing track may outlive
the admission deadline; expiry does not cut off a four-minute song after one
minute. An explicit retry gets a fresh source and may restore the measured
position when supported. No automatic identity/restriction retry loop exists.

Remote playbin uses the required Soup HTTPS source with strict TLS, redirects
and native retries disabled, no OAuth/cookies/user credentials or extra headers.
Its guard rejects manifests, adaptive/secondary URI handlers and unexpected
codec caps. Download buffering is disabled; bounded queue2 storage stays in
memory with no tempfile template. GStreamer URI debug output is disabled before
handoff, and bus errors cross the boundary only as fixed categories or the
structured numeric Soup HTTP status. Signed resources and provider text are
never formatted into logs. Production proxy configuration is preserved.

The existing native player bar exposes actual Play/pause/resume/Stop,
position/duration, seekability, volume, Next/Previous, queue selection and EOS
advance. Loading/resolving is distinct from Playing. A track/session/intent
binding and one-running/latest-pending resolver lane reject stale replies.
Removing/replacing a playing selection, Stop, logout and close cancel the intent
and retire the worker; older results cannot restart audio. In-flight bounded
HTTP can finish until its deadline, without blocking GTK. Local invalidation is
not remote OAuth revocation. The old generated-WAV developer tool stays separate.

## Metadata diagnostic remains separate

`--delivery-diagnostic` adds **Проверить доставку аудио без загрузки** to track
details. It performs offers and at most one XML read, reports separate HTTP
statuses and finite categories, then stops. It does not fetch media or invoke
checksum construction/playback. `legacy-client-signature-required` describes
that diagnostic stopping point; the normal explicit Play path invokes the
implemented resolver. Neither a diagnostic result nor a synthetic MP3 is a
substitute for hearing the selected service track.

## Acceptance and remaining evidence

Automated checks use only original runtime-generated media and loopback HTTP/TLS
fixtures. They cover source construction, expiry, credential separation,
protection/preview refusal, hostile origins/redirects, actual native transport,
controls/EOS/cleanup and URI-log privacy. Their executed results and exact CI
artifacts belong in [VALIDATION.md](VALIDATION.md) and the audio PR; they must not
be inferred from this implementation description.

Use [one combined Fedora run](BUILDING.md#v02-catalog-and-delivery-info-acceptance-on-fedora)
with the audio child build: normal sign-in → a real Library track → explicit Play
→ **hear it** → pause/resume/seek/volume/queue/EOS/Stop/close. Then exercise a
playlist and corrected track search in the same build. If any stage fails,
report only its fixed stage/category/numeric HTTP and anonymized outcome. No
private URL, raw response, title/ID, token, cookie or audio is needed in Cloud,
CI or GitHub. A refusal is not a reason to change identity, region, TLS or access
controls.

The current **live unfinished step is the descriptor/resource/media path after
the owner's successful offer**, not login or the known independent own-client
Music 403. Until actual sound and cleanup are accepted, v0.2 is not complete.
MPRIS/media-key claims wait for truthful working service playback; public OAuth
identity/distribution approval, persistent sessions and offline caching remain
separate gates.

[issue9]: https://github.com/tracerxbrhd/karin/issues/9
[p0]: https://github.com/tracerxbrhd/karin/issues/9#issuecomment-6524404858
[cassette-resolution]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/yam-client.vala#L1221
[cassette-player]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/player/player.vala#L492
[lifeart]: https://github.com/lifeart/ya-namp/blob/49cfee7002ebf816f4144860e67d6fa3be5f5708/docs/yandex-api.md#L36
[marshal-download]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/download_info.py#L58
[metadata-fixture]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/tests/test_download_info.py
[legacy-host]: https://github.com/MarshalX/yandex-music-api/issues/25#issuecomment-532221323
[yamu-negotiation]: https://github.com/okhsunrog/yamu/blob/9803f319b9ea6f81befbc3117e4592132ed07d0e/src/client/downloads.rs#L16
