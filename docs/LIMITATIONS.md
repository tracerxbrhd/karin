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
existing player. Corrected feedback, real audio and continued batches were
confirmed on Fedora; PR #25 then merged at `860672b` with ordinary Native and
installed Flatpak owner acceptance. Issue #16 is closed. This overall report does
not certify each optional failure scenario, portal backend or physical media key.
The [procedure](BUILDING.md#my-wave-local-acceptance) remains available for regressions.

Only **Слушать** starts a radio session. Startup, restored sign-in and Home
navigation do not issue Wave requests, and session IDs/batch markers remain
memory-only. The removed `radioFinished` is not sent. Fixed event/origin/phase/
finish diagnostics remain available; HTTP 400 on any supported event still
fails honestly without automatic replay.

An already admitted HTTP request cannot be unsent; logout/cancellation fence
queued work and late results. Ambiguous POST outcomes are never automatically
replayed. There is no separate user-facing Finish action: Pause retains radio,
while ordinary source selection or MPRIS Stop retires it internally.

Account Dislike and removal use a source-backed contract with independent
Like/Dislike readback. The new controls require successful state reads; refusals
or schema conflicts leave Dislike hidden/unknown, without guessed state or
mutation retry. A fresh independent Like membership read can still enable the
existing Like/Unlike path and its confirmation; a Dislike refusal must not
disable working Like. The fallback never establishes Dislike state.
The owner confirmed live server reactions and opposite-mark interaction at
`1877a6c`, then explicitly accepted the corrected Dislike→Next, ordinary queue and
Wave behavior on Fedora Native and installed Flatpak after #27 merged at `ddf93ea`.
Issue #17 is closed. Individual race/failure cases are covered by automated guards,
not inferred as special owner tests. Unlike is not Dislike; opposite marks are confirmed independently.
Only confirmed Dislike addition on the unchanged current occurrence invokes Next.
For Wave this reuses its existing listening/Skip lifecycle, with no new negative
Wave event or recommendation-effect guarantee. See
[ADR-0028](adr/0028-confirmed-track-reactions.md).

Other recommendation shelves/similar tracks and lyrics are not implemented.
The v0.6 audio store, player bridge and cleanup are ordinary production code.
The owner accepted the available functions/tests at `707539b` on Native and
installed Flatpak. `0709729` remains the last fully accepted PR #26 revision;
newer available functions and later acceptance are recorded separately. One shared policy/budget/usage
and Clear covers all permitted categories, with
2 GiB by default and an explicit Unlimited choice guarded by available disk space.
The private HTTP-cache path independently downloads the selected source while the
existing player starts immediately. It saves only complete MP3 responses whose HTTP and
content restrictions allow storage. It does not issue a provider entitlement or
call HTTP cacheability a copyright license. Every local replay requires fresh
Music access and a verified representation; a track ID or renewed signed URL
alone cannot prove unchanged bytes. Unusable validators, an explicit refusal,
`no-store`, cancellation or a failed cache leave permitted network playback
available. Cache lookup and bookkeeping remain off GTK.

**At `31d873d` the owner confirmed real first playback and complete background
saving on Fedora Native; same-track and post-restart local replay still failed
with `resource-changed`.** The revised identity binds the logical descriptor
object/encoding separately from physical CDN placement. Scoped validators remain
required; an unproved modern boundary retains the valid record online without
repeated cache-body writes. Older records can establish the new binding through
a complete validated replacement. Graphical indicators derive complete local
presence, not offline availability or current playback permission.

The earlier `6ec2407` playback failure remains historical; the exact rejected
host and the stage of its separately reported generic Wave error were not
established in Cloud. The corrected replay and indicators require final owner
acceptance. Local HTTP/generated-audio checks exercise production code, not the
live CDN or installed Flatpak. A resource without sufficient scoped/full-digest
proof stays online with a finite reason. PR #26 and Issue #20 remain open.
Public client distribution questions remain separate in #5/#22. See
[AUDIO_CACHE_REVIEW.md](AUDIO_CACHE_REVIEW.md) and the
[single acceptance procedure](BUILDING.md#pr-26-corrected-cache-acceptance).
The [post-acceptance audit](PR26_REVISION_AUDIT.md) remains historical evidence. Downloads,
mass prefetch and offline playback are outside this iteration. A later offline
mode needs its own actual lifetime/revalidation rules; it is not a prerequisite
invented for transparent online-revalidated caching.

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
