<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Known limitations

The owner accepted native functionality and the v0.4 UI/UX at `97093ee` on
Fedora 44, KDE Plasma, Wayland and PipeWire, and separately reported successful
Flatpak acceptance. No official public release or per-backend/per-scenario acceptance matrix is
implied. [VALIDATION.md](VALIDATION.md) separates owner and automated evidence.

## Service and distribution

The Yandex Music API is unofficial and can change. A valid Music session, a
subscription or previously cached metadata does not authorize every resource.
Unknown schemas, origins, protection or delivery formats fail explicitly; Karin
does not bypass DRM or import extracted official-client keys.

GPL licensing and the provenance of the Cassette-compatible authentication code
are separate from OAuth-registration, service/content and public-distribution
rights. Public service/distribution conditions remain unresolved. See [LICENSING.md](LICENSING.md),
[AUTHENTICATION_REVIEW.md](AUTHENTICATION_REVIEW.md) and
[STREAM_DELIVERY_REVIEW.md](STREAM_DELIVERY_REVIEW.md).

Normal startup keeps My Wave explicitly unavailable. `--experimental-wave`
enables an unverified local integration with the existing Music session/player;
published protocol and synthetic fixtures do not establish live radio access,
accepted feedback or batch continuation. See the
[local acceptance procedure](BUILDING.md#my-wave-local-acceptance). No account
secrets may enter Cloud/CI. An already admitted HTTP request cannot be unsent;
logout/cancellation fence queued work and late results. Ambiguous POST outcomes
are never automatically replayed.

Recommendations/similar tracks, lyrics, Dislike and persistent/offline audio are
not implemented. Unlike is not Dislike. A working short-lived MP3 stream does not
establish persistent download rights or an offline entitlement lifecycle; see
[AUDIO_CACHE_REVIEW.md](AUDIO_CACHE_REVIEW.md).

## Catalog, workspace and playback

- Collection filtering searches admitted title/artist/album metadata only.
  Views stop at 2,000 positions or 100 pages; a new playback context contains at
  most 500 matching occurrences. Partial coverage is labelled. Loading more data
  does not grow an active queue, and no complete remote sort is offered.
- Queue occurrence identity now survives reindexing for both loaded and manual
  duplicates; original source positions remain provenance. Queue/current-position
  restoration after application restart is not implemented.
- Refusal fences prevent rejected metadata from returning as fresh despite
  Busy/I/O failure. A complete disk failure cannot durably record a new refusal:
  after restart, inherited metadata is therefore Stale until a successful new
  write, and only eligible transport errors allow labelled saved-data fallback.
  Known tombstones are never served. The 8,192-key/account fence limit pauses
  persistence until a successful Clear; this does not stop live playback. See
  [ADR-0026](adr/0026-refusal-fences-and-queue-occurrence-identity.md).
- The reviewed HTTPS descriptor/CDN policy is deliberately narrow, not an
  exhaustive list of Yandex storage hosts. A source's sixty-second admission
  lease can reject a new connection, resume or seek; an already playing track is
  not interrupted merely because the lease ages. Explicit fresh Play resolves
  again without changing identity or bypassing a refusal.
- Shutdown is cooperative. Bounded state deadlines cannot preempt a blocked
  native plugin call, and forced termination cannot guarantee native cleanup.

## Session and system storage

Secret Service and Secret Portal availability depends on the installed desktop
and backend. Missing, locked, denied or broken storage allows process-only login,
with no plaintext fallback. Only a newly Music-validated token establishes a
restored session. No automatic token-refresh or server-revocation flow is claimed.

Forget/delete must be confirmed. Uncertain native writes, deletion or personal
cache purge retain a restoration barrier and visible Retry. A failed barrier
write leaves restart protection unconfirmed. Memory-only login may resume after
active cleanup settles, while persistent writes remain paused. Portal forgetting
removes Karin's encrypted envelope, not the portal master key or remote consent.
Owner acceptance does not certify every wallet/portal failure case. [ADR-0018](adr/0018-persistent-session-and-settings.md)
describes the lifecycle.

## Cold rendering and evidence scope

Cloud first-appearance smoothness remains unresolved despite the accepted desktop
UI. Fresh-process footer observations at the accepted code retain gaps of roughly
0.77–1.12 seconds across local/hosted runs. Bounded profiling measured a
substantial LLVM optimization/JIT contributor in the Cloud Mesa/Xvfb stack; the complete
trigger, remaining time and ordinary Wayland relevance remain unresolved in
[#23](https://github.com/tracerxbrhd/karin/issues/23). Exact observations remain in [VALIDATION.md](VALIDATION.md).
No height reversal or successful initialized animation turns these observations
into a cold-smoothness PASS. Earlier failures remain in Git/PR history.

Xvfb/generated-media/HTTP fixtures do not independently prove physical GPU,
Wayland, heard service music or every hardware key/backend. Do not infer those
individual results from general owner acceptance. Missing cloud SDK/user
namespaces must be reported, never worked around by weakening the sandbox.
