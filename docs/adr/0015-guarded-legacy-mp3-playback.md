<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0015: Guarded legacy MP3 resolution and remote playback

Status: Accepted; owner Fedora core-audio PASS at `8f1b09d` (2026-10-31)
Date: 2026-10-30
MPRIS deferral superseded by [ADR-0017](0017-playback-context-unified-workspace-and-mpris.md); the guarded resolver/engine remains unchanged.
Supersedes: ADR-0007's local-only source restriction and ADR-0012's metadata-only
implementation boundary for the explicit Play path

## Context

Issue #9 makes actual audible Yandex Music mandatory for v0.2. The owner has
verified normal sign-in, a Music library, real track details/liked state and album
search on Fedora. Track search still failed at `99a177e` and is corrected
separately in PR #12. A consenting per-track delivery probe returned HTTP 200
and an MP3 descriptor offer, but the old metadata allowlist stopped before any
descriptor/media GET. At that decision date, no live XML, playable resource or
heard track was confirmed.

Acceptance update, 2026-10-31: the owner's
[PR #13 conclusion](https://github.com/tracerxbrhd/karin/pull/13#issuecomment-6539311586)
confirms a heard entitled Yandex Music track, working controls/queue and correct
logout/close on Fedora at `8f1b09d`. This supersedes the preceding dated audio
uncertainty. Preserve the implementation while correcting search/UX; it does
not settle current Flatpak or public-registration/service-distribution approval.

Cassette v0.2.4, the published 2026-07-16 lifeart investigation and pinned
MarshalX/yamuse describe the legacy offer → XML → ordinary checksum/get-mp3
chain. It is distinct from the newer yamu signer whose source explicitly
identifies extracted official-client material. The legacy checksum is not
automatically DRM or proven extracted signing material. These protocol facts
also do not grant public OAuth-registration, service or content rights.
Immutable references and the owner's exact safe result are in
[STREAM_DELIVERY_REVIEW.md](../STREAM_DELIVERY_REVIEW.md).

## Decision

Keep the current AuthProvider, MusicService, native UI and GStreamer worker.
Add `stream::StreamResolver` on a bounded background job. An explicit user Play
uses the existing validated Music session to select an explicit non-preview MP3
offer; it rejects protected, contradictory, ambiguous or unsupported offers.
OAuth is attached only to the fixed Music API call.

Descriptor requests have no OAuth/cookies and accept only exact HTTPS
`storage.mds.yandex.net:443` with a nonempty path below one of the three reviewed
families: `/get-download-info/`, `/download-info/`, `/file-download-info/`.
The latter two have distinct pinned published references; the first retains the
previous policy requested by the owner. This is not permission for arbitrary
storage paths, Yandex subdomains or redirects. Reject userinfo/fragment,
ambiguous/traversal paths, unsupported response combinations and oversized work.

Parse bounded XML without DTD/entities; require unique bounded legacy components
and reject explicit protection/preview indications. Use the public legacy
checksum framing and fresh server components to construct the canonical resource.
Do not import the modern extracted signer, encryption keys, decryption or any
restriction/identity fallback.

Return an opaque `RemotePlaybackSource`, never a displayable address. Its narrow
media policy is HTTPS 443, canonical `/get-mp3/`, no userinfo/query/fragment, and
the restricted generated `s` + 1–5 digits + 3 lowercase letters single-label
family under `storage.yandex.net`. This conservative policy is not an exhaustive
current CDN inventory. Its 60-second monotonic admission deadline starts before
the offer request and is not reset by descriptor latency. XML `ts` is an opaque
hexadecimal-like path component, not an expiry/entitlement grant.

Preserve local regular-file sources. Remote playbin requires the guarded Soup
source with strict TLS, no redirects/retries/cookies/OAuth or extra headers.
Reject manifests, secondary URI handlers and unexpected codec caps. Disable
download caching, bound memory queues, and suppress native URI debug output
before handoff. Only finite errors and trusted structured numeric HTTP status
cross to UI; signed addresses never enter Debug, snapshots, logs or disk.
Production proxy configuration is retained. Test-only loopback transports and
local CA trust cannot be selected by normal application input.

Connect the existing player bar to actual controller snapshots. An explicit Play
starts resolution; Playing requires actual GStreamer state. Support pause/resume,
Stop, progress/seek when reported, volume, queue/Next/Previous and EOS advance.
One running/latest-pending intent plus track/session generations discard stale
results. Stop/logout/close cancel intents, clear source binding and retire the
worker. An active track may outlive admission freshness; new connections, resume
or seek after expiry fail visibly and an explicit Play resolves afresh. No retry
loop changes identity or circumvents a refusal.

`--delivery-diagnostic` remains a separate metadata-only action. It neither
constructs a playback resource nor requests media. MPRIS, media keys, My Wave,
offline caching and persistent sessions are outside this focused slice.

## Acceptance and consequences

Run stable and exact Rust 1.98.1, fmt, Clippy, tests, GTK/WebKit/GStreamer lifecycle,
offline release, license and CI checks. Synthetic loopback fixtures generate
original MP3 data at runtime, exercise HTTP/TLS, policy/error/expiry boundaries,
actual transport/seek/EOS/cleanup and URI-log privacy. Native fixtures exercise
the resolver, queue and controls together. Record executed results separately;
fixture success is not provider acceptance.

The final combined Fedora build must use normal login, a real entitled Library
or playlist/search track, explicit Play and **heard music through PipeWire**,
followed by pause/seek/skip/Stop/logout/close. The owner has now accepted this core
path at `8f1b09d`; each changed build still needs playback regression coverage.
The full v0.2 release remains blocked by corrected live search/UX and separate
packaging/distribution gates. A failure identifies the exact fixed stage/category/HTTP, without raw
responses, identifiers, addresses, credentials or music entering Cloud/CI/GitHub.

A technically working local prototype is separate from public distribution
approval for the shared OAuth registration and service/content use. Existing GPL
attribution/notices remain and no approval, blanket service ban or redistribution
right is inferred from open source, a token or an offer.
