<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Cassette v0.2.4 capability audit

Source audit reviewed 2026-10-29; Karin status refreshed 2026-10-31 for v0.3, following [Issue #9](https://github.com/tracerxbrhd/karin/issues/9).
This is a source audit, not a live test of Cassette or a claim that its service
permissions transfer to Karin. The reference is tag `v0.2.4`, resolved to
[`9d34251d01bcd414ffbec2bef7931090d33c9124`][cassette]. Source links below use that
immutable revision. No artwork, screenshots, audio or interface implementation
from Cassette is included in this audit.

The [ALT GNOME wiki][wiki] and [Flathub listing][flathub] were read on the review
date for product context. These are moving descriptions, not version-pinned
acceptance evidence. The wiki lists service-wide search under a future **0.4**
release; global search is Karin's own addition, not claimed v0.2.4 parity.

## Evidence vocabulary

- **Implemented:** Karin contains the indicated workflow. Live provider
  acceptance is stated separately; fixture success alone never supplies it.
- **Partial:** only part of the workflow exists, or a reported defect remains
  under correction. Metadata selection is not audio playback.
- **Not present:** no exposed Karin workflow; no placeholder is counted as one.
- **Blocked:** a specific delivery, protection, permission or lifecycle gate
  prevents enabling the workflow; an untested endpoint is not a confirmed denial.

The owner accepted the merged v0.2 `master` on Fedora 44 / KDE Plasma / Wayland /
PipeWire, including Flatpak: 17 PASS, 0 FAIL and 1 N/D (hardware media keys).
Library/search, actual Music playback, contextual controls, MPRIS registration,
KDE Media Widget and shutdown are an accepted baseline. PR #13 is merged. Earlier
PR #8/#12 failures remain revision-specific history in VALIDATION, not current
Karin status. New v0.3 persistence/cache/workspace behavior still needs independent
checks and owner acceptance. Private screenshots, names, IDs and provider
payloads are intentionally absent from this document.

## Capability matrix

| Capability | What v0.2.4 source actually implements | Karin status and evidence |
| --- | --- | --- |
| Ordinary Yandex sign-in and account | [Begin view][begin] and [API client][api] couple WebKit session login to token/account work | **Implemented; owner-verified** sign-in and Music account. Karin retains its own exact-origin/state, ephemeral-session and validation checks; shared-registration distribution conditions remain unresolved. |
| Plus/subscription handling | [No-Plus dialog][plus] and [player][player] distinguish account/playback constraints | **Partial:** account validation is implemented; no subscription-success badge or playback entitlement is inferred from it. |
| Liked tracks and personal playlists | [Playlists view][playlists] loads own and liked playlists; [playlist view][playlist] loads tracks | **Implemented; baseline library owner-PASS.** Actual own/liked playlist identities, details and loaded paging are retained; v0.3 cover tiles/cache are a new presentation/storage layer requiring fresh acceptance. |
| Global track/album/artist search | Not established as a shipped v0.2.4 workflow; the wiki places service search in future 0.4 | **Implemented; owner baseline search PASS.** Four independent sections also include playlists. Track hits validate IDs, then use one bounded canonical batch; v0.3 retains that path rather than importing Cassette search stubs. |
| Track/album details and covers | [Track info][track-info], [cover widget][cover] and [cache][cache] implement metadata/image presentation | **Implemented in code:** typed metadata, paged artist details and bounded scale-aware covers with native placeholders. Prior baseline texture/layout checks are recorded in VALIDATION; the new persistent artwork and right pane require fresh checks and owner acceptance. Returned metadata is not a stream grant. |
| Play/pause, seek, next/previous, volume, queue, repeat/shuffle/EOS | [Player][player] and [track-list mode][track-list] implement actual transport and queue; playbin receives a resolved resource | **Implemented and owner-accepted for the baseline:** actual entitled Music audio, transport, seek/volume, contextual queues and shutdown. Repeat/shuffle are not implemented. Generated media is only an automated regression source; new v0.3 sound regression remains required. |
| MPRIS/media keys and now-playing | [MPRIS player][mpris] exposes state, position, volume, queue capabilities and transport | **Implemented; owner baseline registration/KDE Media Widget and Flatpak MPRIS PASS.** Hardware media keys are N/D. Same-player GIO bridge, truthful metadata/capabilities, confirmed seek and cleanup remain; v0.3 requires regression acceptance. |
| My Wave/stations and feedback | [Flow mode][flow] creates a radio session, obtains batches and sends lifecycle feedback; [stations view][stations] selects stations | **Hidden in normal UI; source-backed backend implemented for v0.3.** Modern session/batch/feedback methods and an observed-Playing coordinator are separate from a shipped radio experience. Effective Karin session/tracks/feedback plus real listening are unverified; no fake ordinary-queue Wave appears. See the [current capability review](#v03-service-capability-review). |
| Track likes/dislikes | [Track like/dislike buttons][like], [likes controller][likes] and [API methods][api] implement changes/state | **Implemented for like/unlike:** exact account membership, one deliberate write and independent revision/membership confirmation; conflicts and ambiguous outcomes require GET-only refresh. Native loopback controls are tested; live acceptance is pending. Dislike is not exposed. |
| Playlist creation, track insertion/removal, deletion and visibility | [Create button][create], [track-playlist menu][track-playlist], [playlist view][playlist] and API mutation methods provide concrete actions | **v0.3 creation extension:** implement only the source-confirmed create contract with one explicit POST and independent server confirmation. Lost/ambiguous outcomes are not local success or automatic retries. Broader playlist insertion/removal/deletion are not claimed by a Create tile; live v0.3 acceptance remains pending. |
| Playlist rename/reorder/playlist-like controls | API methods and models exist, but the [playlist view][playlist] comments out rename and explicitly blocks edit/like buttons as `NOT_IMPLEMENTED` | **Not present.** Do not equate an API helper, commented handler or visible disabled button with a shipped workflow, in either application. |
| Recommendations | API client implements playlist recommendations; My Wave has its separate flow | **Not present:** no fabricated recommendation feed. Playlist recommendations remain a candidate; My Wave requires its real listening/feedback lifecycle. Neither is represented as a shipped Karin feed. |
| Text/synced lyrics | [Lyrics panel][lyrics] renders text and time-synchronized lines; the API request obtains a signed lyrics resource | **Blocked for the reviewed implementation:** the inspected lyrics paths rely on official-client extracted signing material. Karin does not import it. A separately permitted plain-text source is not yet established. |
| Explicit/child/replaced/unavailable metadata | [Info marks][marks], filtering and preferences distinguish these flags | **Partial:** availability is represented; richer metadata markers/filtering need typed models and fixture/live verification. Never silently substitute a different track. |
| Cache/offline playlist or album media | [Cache subsystem][cache] and playlist controls save/delete media | **Audio blocked / not established:** no permitted Karin third-party persistent-audio/offline-entitlement lifecycle is established. Official-client offline behavior and a subscription do not supply that contract. Metadata and explicitly cacheable normalized covers are a separate v0.3 implementation; see AUDIO_CACHE_REVIEW. |
| Theme, adaptive panels, preferences and notifications | [Preferences][preferences], [theme switcher][theme] and player/sidebar widgets implement settings and presentation | **Accepted baseline shell; v0.3 extends it:** persisted system/light/dark settings, adaptive right track information, native playlist tiles and storage/session preferences. Notifications are not claimed. New implementation, actual screenshots and Fedora/Flatpak acceptance are recorded separately. |
| Persistent account session | Cassette uses its own storage/session lifecycle | **v0.3 protected lifecycle:** native Secret Service or available Flatpak Secret Portal, Music validation before use/save, durable forget barrier and verified deletion. No cookie/profile import, plaintext fallback, refresh or remote-revocation claim. Real wallet/portal acceptance remains distinct from synthetic fixture success. |

## v0.3 service capability review

Reviewed 2026-10-31 against the same pinned Cassette source and the existing
pinned Rust-client reviews. These are protocol/source facts and synthetic-test
implementation scope, not a live account or permission result.

**Playlist creation** follows the explicit create method in
[Cassette's API client](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/yam-client.vala)
and the pinned yamu/yamuse mutation contracts. Karin sends one account-bound
`POST /users/{uid}/playlists/create` with bounded title/visibility, then independently
reads `/users/{uid}/playlists/{kind}`. Own identity/title and explicit requested
visibility must agree before success. A lost response, missing privacy or failed
confirmation remains unknown/unconfirmed, with no optimistic tile or POST replay.
Broader playlist editing is not enabled by this method. New live creation still
requires the owner's consenting local test.

**Modern Wave** uses the pinned Cassette API/model/flow chain:

- `POST /rotor/session/new` with the actual user-wave seed and track/model request.
- `POST /rotor/session/{id}/tracks` with a bounded actual queue.
- `POST /rotor/session/{id}/feedback` with the original batch marker and event,
  ISO8601 UTC timestamp and the neutral protocol source `unofficial-client-own`.

Source references are the same API client, its
[rotor objects](https://github.com/Rirusha/Cassette/tree/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/objects/rotor),
[flow mode](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/player/mods/flow.vala)
and [client utilities](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/utils.vala).
The `from` label is not a replacement OAuth identity or official-client header.
Pinned yamu instead implements old station-track GETs; pinned yamuse supplies
old station-feedback form payloads with numeric timestamps. Those route/payload
contracts are not silently mixed with modern session JSON.

Karin's `music::wave` backend has bounded session/batch/feedback operations and a
pure coordinator driven by actual `PlaybackState::Playing` and monotonic heard
time. Selection/Ready/Loading, seek distance and declared duration cannot invent
listening. Duplicate occurrences and their original batch markers remain distinct;
uncertain POSTs are not replayed. Session IDs/markers are memory-only, redacted,
nonserializable and excluded from cache/MPRIS/support output. No second player,
startup radio request or normal Wave destination is enabled.

The missing product gate is one consenting local Karin run confirming the modern
session, track batch and required feedback access, followed by integration of
those events with the same real player for a heard track. Source code and a
successful ordinary library session do not prove that capability. Until it is
established, the prepared backend remains hidden and no radio/recommendation UI
claims support. Persistent audio has its independent
[rights/entitlement blocker](AUDIO_CACHE_REVIEW.md), unrelated to these API routes.

## Rust-client comparison and implementation order

`okhsunrog/yamu` HEAD was rechecked on 2026-10-29 and remains
[`9803f319b9ea6f81befbc3117e4592132ed07d0e`][yamu] (0.1.2). Its library/search,
revision-aware mutations and typed models are useful protocol references, not a
reason to import its default OAuth identity, raw error bodies or delivery keys.
Karin continues using its own small service adapter. See
[RUST_API_CLIENT_REVIEW.md](RUST_API_CLIENT_REVIEW.md) and
[STREAM_DELIVERY_REVIEW.md](STREAM_DELIVERY_REVIEW.md).

The accepted v0.2 delivery/search implementation is preserved. v0.3 proceeds
in one Draft PR from master: protected session/settings, bounded metadata/cover
cache, then information/playlist UX and joint regression. Audio persistence,
My Wave, similar tracks and lyrics have independent permission/contract gates;
unsupported functions remain hidden rather than replaced with a generated queue.

This matrix records observed implementation boundaries. Executed checks and
revision-specific acceptance belong in [VALIDATION.md](VALIDATION.md); current
run instructions belong in [BUILDING.md](BUILDING.md). A future change must update
its status using actual code and evidence rather than treating this audit as a
promise that every endpoint works with the current user session.

[cassette]: https://github.com/Rirusha/Cassette/tree/9d34251d01bcd414ffbec2bef7931090d33c9124
[wiki]: https://alt-gnome.wiki/apps/cassette/
[flathub]: https://flathub.org/apps/space.rirusha.Cassette
[begin]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/begin/begin-view.vala
[api]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/yam-client.vala
[plus]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/no-plus-dialog.vala
[playlists]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/views/playlists.vala#L130-L151
[playlist]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/views/playlist.vala#L76-L180
[track-info]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/track-info-panel.vala
[cover]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/cover-image.vala#L61-L104
[cache]: https://github.com/Rirusha/Cassette/tree/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/cachier
[player]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/player/player.vala#L492-L520
[track-list]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/player/mods/track-list.vala
[mpris]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/player/mpris.vala#L66-L200
[flow]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/player/mods/flow.vala#L46-L115
[stations]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/views/stations.vala
[like]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/buttons/just/like-button.vala
[likes]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/likes-controller.vala#L35-L109
[create]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/buttons/playlist-create-button.vala
[track-playlist]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/buttons/menu/track-playlist-options-button.vala
[lyrics]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/lyrics/lyrics-panel.vala#L57-L124
[marks]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/info-marks.vala#L36-L78
[preferences]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/preferences/preferences-dialog.vala#L25-L120
[theme]: https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/widgets/theme-switcher.vala
[yamu]: https://github.com/okhsunrog/yamu/tree/9803f319b9ea6f81befbc3117e4592132ed07d0e
