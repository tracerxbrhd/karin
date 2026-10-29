<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin Roadmap

## v0.0 — Technical foundation

The native Rust/GTK4/Libadwaita foundation, independent GStreamer controller,
bounded API boundary, licensing and reproducible Cargo/Flatpak source preparation
are in place. The owner ran the diagnostic prototype on Fedora 44 / KDE Plasma /
Wayland / PipeWire. Generated audio validates the engine, not Music playback.
See [validation](VALIDATION.md) for revision-specific evidence and limitations.

## v0.1 — Signed-in metadata prototype

[ADR-0011](adr/0011-cassette-session-auth-prototype.md) preserves the normal
`Войти` → Yandex → Music account validation journey without developer credentials
or manual tokens. It supersedes the historical blocked-provider decision.
The independent own-client PKCE result (Yandex ID 200, Music status/about 403)
remains separate; it is not the normal login path.

The owner's [Issue #9](https://github.com/tracerxbrhd/karin/issues/9) report at
PR #8 baseline `797ba3a` confirms real sign-in, Music account, liked tracks and
personal-playlist listing. Search and opening playlist contents failed; these
are defects in claimed workflows. The queue is metadata-only and no service
track has audibly played in Karin. This is not a completed minimum music player.

## v0.2 — First usable, audible music client

**Actual Yandex Music playback is mandatory in this stage**, alongside working
search and personal playlists. It is not a later stretch goal. Acceptance means
that the owner signs in on Fedora, opens the real library or catalog, selects an
entitled track and hears it through GStreamer/PipeWire, then exercises transport,
queue, error recovery and cleanup. Synthetic WAV, metadata, a delivery URL or a
player-shaped interface do not pass this criterion.

Use 2–4 coherent PRs or stacked increments without automatically merging the
open PRs. While #6/#8 remain open, build on the live #8 head and target the
preceding feature branch. The first increment contains actual catalog/navigation
fixes and begins the audio work **in parallel**, before broad visual polish:

1. Repair track/album/artist search, playlist contents, details and pagination;
   replace silently dropped actions with bounded independent work and stale-result
   checks. Show immediate loading pages, actionable errors, empty results and back
   navigation. In parallel, inspect Cassette's actual delivery/playbin chain and
   run a bounded, opt-in, owner-local delivery-info experiment through the current
   session. No user token or response body enters Cloud/CI/GitHub.
2. On confirmation of an eligible unprotected resource, implement a strict
   `StreamResolver` → existing GStreamer controller path immediately. Verify real
   selected-track sound, play/pause/resume/stop, seek/progress, volume, next/
   previous, queue and EOS. Add truthful MPRIS/media-key integration when playback
   works. Do not port extracted signing/decryption material or bypass restrictions.
3. Build the music-first adaptive shell, covers, library/search/queue destinations,
   complete player bar and best-validated likes/playlist interactions. Inspect
   actual screenshots and GTK structure across sizes/themes/HiDPI/keyboard; Fedora
   desktop and live-service sign-off stay separate from cloud checks.

[CASSETTE_PARITY.md](CASSETTE_PARITY.md) distinguishes Cassette v0.2.4's shipped
workflows, API helpers and stubs. Evaluate My Wave, recommendations, lyrics and
preferences using actual endpoint and lifecycle evidence, without publishing
nonfunctional controls. Playlist writes require revision/conflict handling and
read-after-write confirmation. Media caching/offline rights and persistent
credentials require separate review.

If provider authorization or protection blocks every assessed delivery option,
record the **exact failed stage** and safe owner-local evidence in
[STREAM_DELIVERY_REVIEW.md](STREAM_DELIVERY_REVIEW.md). Continue independent
catalog/UX work, but keep **v0.2 audible-player acceptance NOT MET**. Do not rename
an unfinished player as complete or replace music with a generated signal.

## Release conditions

Keep the approved native stack, Rust stable and exact 1.98.1, current Flatpak SDK,
license notices and existing voluntary session provider. Real account tests stay
on the owner's Fedora desktop; CI uses synthetic fixtures. Public registration/
service-distribution approval, verified Secret Service/refresh/revocation and
content rights remain separate from successful technical interoperability.
No source license or account metadata alone grants those rights.
