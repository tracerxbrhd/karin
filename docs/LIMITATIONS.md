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

My Wave is an ordinary Home action after Music-validated sign-in, using the
existing player. The owner retested corrected candidate `2e4b978` on Fedora
without reporting new problems, after earlier confirmed real audio, all four
feedback events and continued batches. This general report is not a per-scenario
matrix or installed v0.5 Flatpak acceptance; the final ordinary-native/Flatpak
[procedure](BUILDING.md#my-wave-local-acceptance) remains pending.

Only **Слушать** starts a radio session. Startup, restored sign-in and Home
navigation do not issue Wave requests, and session IDs/batch markers remain
memory-only. The removed `radioFinished` is not sent. Fixed event/origin/phase/
finish diagnostics remain available; HTTP 400 on any supported event still
fails honestly without automatic replay.

An already admitted HTTP request cannot be unsent; logout/cancellation fence
queued work and late results. Ambiguous POST outcomes are never automatically
replayed. There is no separate user-facing Finish action: Pause retains radio,
while ordinary source selection or MPRIS Stop retires it internally.

Other recommendation shelves/similar tracks, lyrics, Dislike and persistent/offline audio are
not implemented. Unlike is not Dislike. A working short-lived MP3 stream does not
establish persistent download rights or an offline entitlement lifecycle; see
[AUDIO_CACHE_REVIEW.md](AUDIO_CACHE_REVIEW.md).
The v0.6 audio-cache foundation is a test-only filesystem prototype using generated
WAVs and a fake authority. It adds no production download/offline action, hidden
temporary recording or service-media source. Applicable storage/reuse permission
and account/device/track expiry, renewal, revocation and offline rules remain
unestablished; Issue #20 remains open. Synthetic cache hits are not offline Music.

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

## Theme input

Karin uses AdwStyleManager for Light/Dark/System. The Libadwaita warning about
`gtk-application-prefer-dark-theme` was reproduced with an external legacy GTK4
setting loaded before Adwaita initialization, using the unchanged earlier
binary. The specific writer/source on the owner's Fedora remains unestablished.
Karin does not suppress this warning or overwrite desktop settings. See the
[read-only theme check](BUILDING.md#theme-input-check).
