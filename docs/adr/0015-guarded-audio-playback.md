<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0015: Guarded audio resolution and one playback engine

Status: Accepted; current source/transport contract.

## Resolution and transport

An explicit Play uses the current Music session and a bounded worker. Read one
`/tracks/{id}/download-info` response (128 KiB, 64 offers); admit an explicit full,
unprotected `preview:false` MP3, not metadata availability or a `direct` hint. Reject
protected, contradictory, ambiguous or unsupported offers.

Fetch at most one credential-free descriptor: exact HTTPS `storage.mds.yandex.net:443`, a
validated nonempty suffix under `/get-download-info/`, `/download-info/` or
`/file-download-info/`. Reject redirects, userinfo, fragments, traversal and ambiguous
paths. Parse at most 64 KiB/64 XML nodes without DTD/entities; require unique bounded
components. The reviewed legacy checksum uses public framing and fresh server components;
never import extracted modern signers, DRM/decryption keys or identity fallbacks.

Only canonical HTTPS `/get-mp3/` on one DNS label below `.storage.yandex.net` is admitted.
A label has 1–63 lowercase ASCII letters/digits/internal hyphens; nested labels, lookalike
zones, IPs, query, userinfo and fragments are refused. This is not a general Yandex/CDN
allowlist. New origins need verified evidence. XML `ts` is opaque, not a version or
entitlement. A monotonic 60-second admission starts before the offer request and is not
reset by descriptor latency.

Return an opaque resolver-issued source after final account/token/cancellation checks.
Preserve independent local regular-file sources. Remote playbin uses the guarded Soup
source: TLS, no redirects/native retries/cookies/OAuth/extra headers, no secondary URI
handlers, manifests or unexpected codec caps. Buffer only bounded transient memory (2
MiB/64 buffers/2 seconds), with download caching and tempfile templates disabled. Signed
addresses stay in memory, excluded from Debug, snapshots, logs, disk and D-Bus. Suppress
URI-bearing native diagnostics; UI receives fixed categories and trusted numeric HTTP
status only.

## Playback and renewal

Keep one PlaybackController with a worker-owned audio playbin, a bounded 32-command queue
and actual bus/state/position/duration/seek/volume snapshots. Playing requires GStreamer
confirmation. Next/Previous/EOS use the same queue; Stop/logout/close retire intents and
release transport off GTK. One running/ latest-pending resolution plus
account/source/intent fences reject late results. An admission expiry does not interrupt
an already playing track or fabricate a rights failure while paused.

Explicit Resume of an expired source may renew once through current access and the
existing [local-first cache/resolver](0029-unified-cache-and-audio-reuse.md). Retain exact
occurrence, trusted position, modes/volume, logical generation and Wave's actual-Playing
ledger. Coalesce repeated Play; Seek supplies the latest target. The same worker retires
old transport, prerolls and restores position before autoplay under a separate source
revision. Pause/Stop/replacement/logout/ close fence that ticket. Do not refresh idle
Pause periodically or extend leases. Failure to restore a nonzero position remains Paused
with an explanation; internal restoration is neither a user Seeked nor a new Wave
track/session. Ordinary access/network errors remain visible, without renewal loops or
identity retries.

## Verification and provenance

`--delivery-diagnostic` stays metadata-only, stopping before resource/media construction.
Synthetic success is not heard service audio or rights. Preserve [pinned legacy
references/notices](../../packaging/third-party-licenses/cassette-auth/PROVENANCE.md) and
the separate [licensing boundary](../LICENSING.md).
