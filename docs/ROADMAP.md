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
are defects in claimed workflows. At that baseline the queue is metadata-only
and no service track has audibly played in Karin. This is not a completed minimum
music player.

## v0.2 — First usable, audible music client

**Actual Yandex Music playback is mandatory in this stage**, alongside working
search and personal playlists. It is not a later stretch goal. Acceptance means
that the owner signs in on Fedora, opens the real library or catalog, selects an
entitled track and hears it through GStreamer/PipeWire, then exercises transport,
queue, error recovery and cleanup. Synthetic WAV, metadata, a delivery URL or a
player-shaped interface do not pass this criterion.

The earlier stacked increments remain unmerged. Continue corrections in the
existing PR #13 branch; do not create another branch/PR, change its base or close
ancestors during this work. The stage has followed these independent work areas:

1. Repair track/album/artist search, playlist contents, details and pagination;
   replace silently dropped actions with bounded independent work and stale-result
   checks. Show immediate loading pages, actionable errors, empty results and back
   navigation. In parallel, inspect Cassette's actual delivery/playbin chain and
   run a bounded, opt-in, owner-local delivery-info experiment through the current
   session. No user token or response body enters Cloud/CI/GitHub.
2. Exercise the implemented guarded `StreamResolver` → existing GStreamer path
   with an eligible real resource. Verify selected-track sound, play/pause/
   resume/stop, seek/progress, volume, next/previous, queue and EOS. Add truthful
   MPRIS/media-key integration when actual service playback works. Do not port
   extracted signing/decryption material or bypass restrictions.
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

The owner has now accepted the core audible path at `8f1b09d`. A new delivery
refusal must still identify its exact stage/category without changing identity,
region, TLS or protection handling. Preserve the working integration and use
Library playback as a regression check while repairing search and UX; do not
replace accepted audio with a generated signal or a new provider experiment.

## Release conditions

Keep the approved native stack, Rust stable and exact 1.98.1, current Flatpak SDK,
license notices and existing voluntary session provider. Real account tests stay
on the owner's Fedora desktop; CI uses synthetic fixtures. Public registration/
service-distribution approval, verified Secret Service/refresh/revocation and
content rights remain separate from successful technical interoperability.
No source license or account metadata alone grants those rights.

## Current implementation evidence

The Issue #9 series starts with [PR #10](https://github.com/tracerxbrhd/karin/pull/10)
(catalog contracts, independent actions and the owner-local delivery probe), then
[PR #11](https://github.com/tracerxbrhd/karin/pull/11) (native music workspace,
artwork and adaptive/HiDPI navigation), then
[PR #12](https://github.com/tracerxbrhd/karin/pull/12) (confirmed like/unlike and
owner-preserving liked-playlist reads). These are
implemented workflows with local synthetic tests, not yet owner-verified service
acceptance. No PR is merged automatically.

The [owner's PR #13 acceptance at `8f1b09d`](https://github.com/tracerxbrhd/karin/pull/13#issuecomment-6539311586)
confirms actual audible Yandex Music through GStreamer/PipeWire, working controls
and queue, and correct logout/close. This is the core service-audio PASS, not a
synthetic fixture result. The accepted resolver/player and current sign-in
provider must be preserved.

Track search still fails with HTTP 200 / `track-wire` /
`track-fields-unsupported`. The current correction replaces coupled search
metadata decoding with a minimal validated identity hit and one account-bound
`/tracks` batch of at most 50 distinct IDs within a 50-position page, with original
order/pagination and
explicit partial data. The account menu requires an explicitly labelled logout
and native Cancel-default confirmation. Search errors/retry, row Play/details,
Back, queue actions and everyday layout density receive targeted UX correction;
no unrelated redesign or inert feature controls are added.

The full v0.2 release is still blocked by live corrected search and UX, current
Flatpak verification and independently unresolved public OAuth-registration/
service-distribution conditions. MPRIS/media keys remain outside this correction.
Use [one updated Fedora build](BUILDING.md#v02-catalog-and-delivery-info-acceptance-on-fedora)
for search-to-play, safe logout and preservation of the accepted sound path.

After that owner acceptance, propose using **this existing PR #13** as the sole
integration PR: retain its ancestor history and existing branch, retarget it to
`master`, rerun CI/review against the new base and squash-merge once only if
approved. Close #6/#8/#10/#11/#12 as superseded only with explicit approval and
references to #13. No new branches/PRs, automatic base changes, merges, closures,
branch deletion or history rewriting are part of the current task.
