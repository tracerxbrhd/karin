<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Service-audio delivery investigation

Reviewed: 2026-10-29, for [Issue #9][issue9]. **Audible Yandex Music playback
is a required v0.2 acceptance criterion and is not yet satisfied.** The owner
has confirmed Karin's ordinary sign-in, Music account, liked-track list and
personal-playlist list on Fedora. Those results establish a usable metadata
session, not a delivery grant or audible service music.

The previous v0.1 source-only review is superseded by this investigation and
the opt-in owner-local delivery diagnostic. Audio work proceeds alongside
search and playlist repairs; it is not postponed until after the UI redesign.
No real account, token, delivery-info response or music was supplied to Cloud
or CI. The live delivery result below remains pending an owner-local run.

## Version-pinned chain, not just a playback window

The Cassette **v0.2.4** annotated tag object is
`fc6c6e689bd0c60fa662b03aa2aa3c05534b4811`; its source commit is
`9d34251d01bcd414ffbec2bef7931090d33c9124`. Source links below use the latter.
The inspected yamu main/0.1.2 revision is
`9803f319b9ea6f81befbc3117e4592132ed07d0e`, still its main head when checked.

| Stage | Cassette v0.2.4 implementation | Karin evidence / consequence |
| --- | --- | --- |
| User session | Published session-cookie → OAuth grant, followed by Music account inspection | Karin's existing Cassette-compatible sign-in and account validation passed on the owner's Fedora machine; this path is preserved |
| Track selection | [`Player.start_current_track`][cassette-player] selects the current queue item and asks `Cachier.get_track_uri` for a resource | Karin already has real track metadata and a bounded queue; neither supplies a playable resource |
| Cached versus remote source | [`Cachier.get_track_uri`][cassette-cache] returns a cached URI when present, otherwise calls the API's download-URI method on an audio worker | Karin does not import Cassette's media cache or enable offline downloads |
| Delivery offers | [`tracks_download_info`][cassette-offers] makes authenticated `GET /tracks/{id}/download-info`, deserializing `result` as an array | This bounded read is the first owner-local diagnostic, using the already validated Karin session and no new client identity |
| Legacy resolution | [`track_download_uri` / `form_download_uri`][cassette-resolution] selects a bitrate, fetches the returned metadata document, parses XML components and constructs a `get-mp3` URI with a fixed checksum prefix | A legacy descriptor is not itself an audio URI. This construction is separate from both the initial read and modern protected delivery; its permission/provenance is not established by the source license |
| Actual decoding | [`Player.start_current_track`][cassette-player] assigns the resulting resource to GStreamer `playbin` and calls `play`; its bus EOS callback advances the queue | Karin's existing GStreamer controller supports real local transport, but no eligible live service resource has yet reached it |
| Cache/prefetch | Cassette can start caching the current/next track | Not reused: metadata access and a GPL implementation do not establish Karin's right to store/distribute service music |

Cassette's [`DownloadInfo`][cassette-model] has `codec`, `bitrateInKbps`, `gain`,
`preview`, `downloadInfoUrl` and `direct`. **`direct: true` alone does not prove
that `downloadInfoUrl` is immediately playable:** the inspected Cassette method
always resolves that field as an XML document. Neither an available-track flag,
a codec nor the absence of an encryption-key field establishes entitlement.

## Legacy, streaming and protected mechanisms are distinct

| Assessed path | Observed contract | Decision / exact remaining step |
| --- | --- | --- |
| Legacy offer read, no signing query | Cassette calls `/tracks/{id}/download-info` using its existing authenticated API transport | Implemented as a bounded opt-in metadata diagnostic. The current Karin session's live HTTP/offer outcome must be checked locally |
| Legacy XML descriptor | Cassette and the independently inspected [yamuse model][yamuse-legacy] fetch the descriptor and derive a resource URI using XML components and fixed client-side checksum material | No claim that this is DRM or that its constant is proven extracted. Provenance and the usable delivery contract remain unresolved; Karin classifies the XML without reconstructing the URI or trying invalid signatures |
| Direct streaming/HLS variant of the same endpoint | [Community issue #566][streaming-discussion] describes `can_use_streaming=true`, `direct=true` and `container=hls`, but also documents a request signature reverse engineered from an Android client | Not imported or requested. A direct URL in that example does not justify reproducing the excluded signer |
| Modern yamu `/get-file-info` | [`download_info`][yamu-negotiation] computes a signature using a constant its own source explicitly identifies as extracted from an official client, and sends an additional official Web client identity | Excluded: no extracted constant, signing implementation or replacement client headers are added to Karin |
| Modern returned media | [`DownloadInfo`][yamu-model] contains URLs and an optional key; [`open_audio_stream`][yamu-negotiation] deliberately omits OAuth at the CDN; the downloader has separate key-dependent and ordinary paths | Not all responses are necessarily encrypted. A raw transport or missing key does not remove the preceding negotiation/identity issue |
| Complete, server-returned, authorized unprotected resource | No independently corroborated eligible resource has yet been observed with Karin's session | A server-signed URL is not automatically DRM. If obtained through a permitted contract, validate it, its expiry and redirect rules before a separate GStreamer handoff; do not invent such a response in production |

The historical legacy checksum path must not be conflated with the modern
extracted HMAC or encrypted transport. Inspection of the legacy implementation
and its public history did not establish an official protocol grant, the
prefix's provenance, or its classification as an extracted secret. This is an
**unknown**, not a demonstrated blanket prohibition or evidence that every
Yandex audio source requires DRM circumvention. Conversely, public source and
an entitled account are not by themselves public-distribution permission.

## Owner-local delivery diagnostic

The diagnostic is deliberately independent of normal search/library reads. It
uses the token from Karin's ordinary login, after the same Music account binding,
for one selected track. The owner explicitly opts in locally; no developer
credentials, manual token entry, token file/environment variable or Cloud
account access is introduced.

The first request is the authenticated legacy delivery-info read. At most one
returned descriptor is then fetched **without OAuth or cookies**, using a
separate client, strict reviewed origin/path policy, no redirects, a deadline
and a 64 KiB limit. The additional origin evidence is a [published Python
fixture][metadata-fixture] at `9ca132e5c18a72aab779c1fc68c169380a9357d0`,
identifying HTTPS `storage.mds.yandex.net` and the `get-download-info` path for
legacy metadata. That establishes a source-specific purpose, **not** an observed
current Karin response or a reason to trust arbitrary service/CDN hosts.

The XML parser forbids DTD/entity resolution, bounds nodes and input, rejects
ambiguous/malformed required fields, and recognizes only finite classifications.
A valid `download-info` document with the legacy components is reported as
`legacy-client-signature-required`: this identifies the next construction step
seen in the referenced clients, **not** DRM or a verified signing permission.
Explicit nonempty key/encryption/protection metadata is classified separately,
without inspecting or using its material. Unrecognized structures remain
unsupported; they are not converted into an invented direct resource.

Only finite categories cross the UI boundary: request stage, separate API and
optional metadata HTTP statuses, known error category, format/descriptor class
and whether protection requirements were explicit or **not established**.
Provider text, track/account IDs, metadata bodies, headers, URLs, cookies,
tokens, signing inputs and media bytes are not displayed or logged. Offers and
sensitive addresses stay in memory. No audio request, stream download, signature
construction or decryption is performed. Automated tests use loopback fixtures.

An offer establishes only that the delivery-info read succeeded. It cannot
establish that playback is allowed or audible. An XML descriptor, unverified
metadata destination, malformed response, unavailable track, HTTP refusal and
explicit protection indication remain distinguishable. Refusal does not justify
changing identity, region, TLS or access controls.

## Acceptance and next action

The exact currently unfinished step is **live delivery classification using the
working Karin session**, followed by an eligible resource-resolution contract.
It is not failed login, Music account HTTP 403, a password/VPN diagnosis, or a
proven claim that every assessed track is protected. The local probe provides
the missing evidence without transferring account credentials or media.

If the local result is only legacy negotiation metadata, XML/checksum resolution
and its permitted contract still need to be settled; it is not a playback PASS.
If the service refuses the read or explicitly requires protection, retain that
precise stage/category and do not retry through an excluded mechanism. If a
permitted complete unprotected source is confirmed, implement the validated,
short-lived `StreamResolver → PlaybackController → GStreamer` handoff immediately,
with independent credential/redirect isolation, cancellation and cleanup.

**v0.2 audible-player acceptance remains NOT MET** until an entitled owner on
Fedora 44 / KDE / Wayland / PipeWire selects a genuine liked, playlist and search
track, hears music, exercises pause/resume/seek/skip/queue, and closes Karin
without orphaned playback. Synthetic WAV decoding and metadata-only queue/player
UI do not satisfy this criterion. MPRIS/media-key playback claims require the
same real source. Public distribution approval, offline caching and persistent
sessions remain independent gates.

This document contains original analysis and source citations, not third-party
implementation or signing constants. Adapted code retains its applicable
copyright/license notices; GPL does not grant service or content rights.

[issue9]: https://github.com/tracerxbrhd/karin/issues/9
[cassette-offers]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/yam-client.vala#L1241
[cassette-resolution]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/yam-client.vala#L1221
[cassette-model]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/objects/download-info.vala#L24
[cassette-player]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/player/player.vala#L492
[cassette-cache]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier/cachier.vala#L186
[yamu-negotiation]: https://github.com/okhsunrog/yamu/blob/9803f319b9ea6f81befbc3117e4592132ed07d0e/src/client/downloads.rs#L16
[yamu-model]: https://github.com/okhsunrog/yamu/blob/9803f319b9ea6f81befbc3117e4592132ed07d0e/src/models/download.rs#L156
[yamuse-legacy]: https://github.com/flxxxxddd/yamuse/blob/368c74646520e8bcd30c44a9887e1bf99cb58172/src/models/track.rs#L323
[streaming-discussion]: https://github.com/MarshalX/yandex-music-api/issues/566
[metadata-fixture]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/tests/test_download_info.py
