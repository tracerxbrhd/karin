<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Online audio delivery

Karin's online player uses the current Music session, `StreamResolver` and one
GStreamer worker. Owner-reported native/Flatpak acceptance of `97093ee` is scoped
in [VALIDATION.md](VALIDATION.md). Cloud checks use generated fixtures.

## Resolver boundary

An explicit Play resolves the selected validated track on a bounded background
job. [`src/stream.rs`](../src/stream.rs) and
[`src/stream/resolver.rs`](../src/stream/resolver.rs) enforce four stages:

1. Read one authenticated `/tracks/{id}/download-info` response, at most 128 KiB
   and 64 offers. Admit an explicit, unprotected `preview:false` MP3 offer;
   reject protected or contradictory MP3 alternatives and unsupported responses.
   Metadata availability and `direct` alone are insufficient.
2. Read at most one descriptor without OAuth, cookies, Origin or Referer. Permit
   only HTTPS `storage.mds.yandex.net:443` below `/get-download-info/`,
   `/download-info/` or `/file-download-info/`, with a nonempty validated suffix.
   Reject redirects, userinfo, fragments, traversal/ambiguous paths and other
   origins/ports. Any signed query remains private in memory.
3. Parse at most 64 KiB/64 XML nodes, with no DTD/external entities. Require unique
   bounded host/path/ts/server-input components and reject protection/preview
   indications. The historical checksum uses public framing plus fresh server
   components, without modern extracted signing or decryption material.
4. Admit only canonical HTTPS `/get-mp3/` resources on one canonical DNS label
   below the existing dedicated `.storage.yandex.net` CDN zone. Labels contain
   1–63 lowercase ASCII letters, digits or internal hyphens; nested labels,
   lookalike zones, IP addresses and malformed names remain refused. The earlier
   `s` + 1–5 digits + exactly 3 letters rule inferred trust from a historical
   server naming convention, which Cassette's descriptor contract does not impose.
   No arbitrary Yandex subdomain, query, fragment or caller-supplied URI is allowed.
   Check account/token validity and cancellation again before returning the source.

The owner reported `media-origin-unverified` at `6ec2407` with successful offer
and descriptor HTTP responses. That category is emitted specifically when the
descriptor host fails this policy; expiry, XML and path failures have other
categories. The old naming rule also exists in earlier working revisions, so
this is not evidence that `6ec2407` introduced it. The actual rejected host is
not available in Cloud. The correction preserves the existing trusted zone and
does not invent a newly observed CDN domain or authorize arbitrary HTTPS origins.
The subsequent owner run at `31d873d` confirmed real first playback and background
saving on Fedora Native. At `cac4d78`, repeated representation invalidation
still prevented a confirmed local hit; [AUDIO_CACHE_REVIEW](AUDIO_CACHE_REVIEW.md)
separates that cache validator-scope correction from the working stream path.

The monotonic 60-second admission deadline begins before the offer request and
is not reset by descriptor latency. XML `ts` is opaque, not an entitlement or
expiry grant. An already playing track may continue beyond admission freshness;
new connections, resume or seek after expiry fail visibly and explicit retry
resolves a fresh source. Do not expand host/path policy without verified evidence
or turn a refusal into automatic identity/region/restriction retries.

## Player boundary

Remote playback accepts only an opaque resolver-issued `RemotePlaybackSource`.
Its private URI is zeroized on drop and excluded from Debug, snapshots, UI,
MPRIS and logs. Local regular-file playback remains independent.

The guarded Soup HTTPS source retains strict TLS, disables redirects/native
retries, and supplies no OAuth, cookies, user credentials or extra headers to
CDNs. Reject manifests, secondary URI handlers and unexpected codec caps.
Download caching is disabled; bounded queue2 buffering stays in memory without
a tempfile template. Suppress URI-bearing native debug before handoff. Errors
cross the boundary as fixed categories or trusted numeric HTTP status.

Worker snapshots drive the existing controls, queue and MPRIS. One-running/
latest-pending resolution and track/session/intent generations retire stale work.
Stop, replacement, logout and close cancel intent and release its worker. Bounded
in-flight HTTP can settle until its deadline without blocking GTK. Optional
[audio caching](AUDIO_CACHE_REVIEW.md) saves a complete selected resource in an
independent bounded worker; ordinary source changes do not cancel that job.
Cache failure cannot block the admitted online player.

`--delivery-diagnostic` is a separate owner-local metadata-only probe. It reads
offers and at most one descriptor, reports safe stage/category/HTTP results, then
stops before resource construction/media. A successful probe or generated MP3
must not be reported as hearing a real service track.

## Source evidence and rights

The pinned legacy references are:

- [Cassette v0.2.4 offers/XML resolution](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/yam-client.vala#L1221);
  the retained provenance also pins its playbin/EOS handoff.
- [lifeart's delivery investigation](https://github.com/lifeart/ya-namp/blob/49cfee7002ebf816f4144860e67d6fa3be5f5708/docs/yandex-api.md#L36),
  which corroborates the exact `/download-info/` metadata family.
- [MarshalX DownloadInfo](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/download_info.py#L58)
  and its [metadata fixture](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/tests/test_download_info.py),
  supporting canonical legacy construction and `/file-download-info/`.

The ordinary legacy checksum is not automatically DRM. The reviewed modern
yamu signer explicitly uses extracted official-client material and is excluded.
Keep the [Cassette copyright/provenance](../packaging/third-party-licenses/cassette-auth/PROVENANCE.md)
and [ADR-0015](adr/0015-guarded-legacy-mp3-playback.md). Technical playback and GPL
reuse do not establish public registration/service-distribution or content rights.
[Private audio caching](AUDIO_CACHE_REVIEW.md) separately checks current access,
HTTP storage policy, retained version and content restrictions. Its HTTP policy
is not a service entitlement or a content/distribution license. Real-token/resource/recording checks belong only on
the consenting owner's local desktop, never in Cloud/CI/GitHub.
