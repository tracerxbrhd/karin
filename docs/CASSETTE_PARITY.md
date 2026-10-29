<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Cassette v0.2.4 capability audit

Reviewed 2026-10-29 for [Issue #9](https://github.com/tracerxbrhd/karin/issues/9).
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

The owner's Issue #9 report establishes real Karin sign-in, Music account
validation, liked tracks and personal-playlist listing on Fedora, at PR #8's
`797ba3a` baseline. Search and opening personal playlists failed in that run.
No actual Karin service audio was heard. Private screenshots, names, IDs and
provider payloads are intentionally absent from this document.

## Capability matrix

| Capability | What v0.2.4 source actually implements | Karin status and evidence |
| --- | --- | --- |
| Ordinary Yandex sign-in and account | [Begin view][begin] and [API client][api] couple WebKit session login to token/account work | **Implemented; owner-verified** sign-in and Music account. Karin retains its own exact-origin/state, ephemeral-session and validation checks; shared-registration distribution conditions remain unresolved. |
| Plus/subscription handling | [No-Plus dialog][plus] and [player][player] distinguish account/playback constraints | **Partial:** account validation is implemented; no subscription-success badge or playback entitlement is inferred from it. |
| Liked tracks and personal playlists | [Playlists view][playlists] loads own and liked playlists; [playlist view][playlist] loads tracks | **Partial:** real liked tracks and personal-playlist list verified. Personal-playlist detail failed at baseline; repair and new Fedora acceptance are required. Liked-playlist listing is not yet exposed. |
| Global track/album/artist search | Not established as a shipped v0.2.4 workflow; the wiki places service search in future 0.4 | **Partial:** typed search/detail code exists, but the baseline Fedora search failed. Contract/dispatch fixes need a fresh real query test. |
| Track/album details and covers | [Track info][track-info], [cover widget][cover] and [cache][cache] implement metadata/image presentation | **Partial:** typed metadata and detail navigation exist; cover art and a complete artist destination are separate additions. Returned metadata is not a stream grant. |
| Play/pause, seek, next/previous, volume, queue, repeat/shuffle/EOS | [Player][player] and [track-list mode][track-list] implement actual transport and queue; playbin receives a resolved resource | **Partial/blocked:** real local-file GStreamer controller exists; the Music queue is metadata-only until a permitted service resource is resolved. **v0.2 audible acceptance is NOT MET.** Synthetic WAV does not change that result. |
| MPRIS/media keys and now-playing | [MPRIS player][mpris] exposes state, position, volume, queue capabilities and transport | **Not present** for Music. Add only when the Music controller can publish truthful actual playback state. |
| My Wave/stations and feedback | [Flow mode][flow] creates a radio session, obtains batches and sends lifecycle feedback; [stations view][stations] selects stations | **Not present:** a station list alone would not implement My Wave. Session feedback must follow actual listening. `rotor_station_info` and `rotor_station_stream` in the v0.2.4 API client are empty methods, unlike its implemented session endpoints. |
| Track likes/dislikes | [Track like/dislike buttons][like], [likes controller][likes] and [API methods][api] implement changes/state | **Not present** as a Karin mutation workflow at baseline. Good follow-up candidate: explicit owner action, account-bound write, safe conflict/error handling and read-after-write confirmation. |
| Playlist creation, track insertion/removal, deletion and visibility | [Create button][create], [track-playlist menu][track-playlist], [playlist view][playlist] and API mutation methods provide concrete actions | **Not present** at baseline. Revision-aware changes must not replay positional edits after a conflict. Destructive actions require a native confirmation. |
| Playlist rename/reorder/playlist-like controls | API methods and models exist, but the [playlist view][playlist] comments out rename and explicitly blocks edit/like buttons as `NOT_IMPLEMENTED` | **Not present.** Do not equate an API helper, commented handler or visible disabled button with a shipped workflow, in either application. |
| Recommendations | API client implements playlist recommendations; My Wave has its separate flow | **Not present:** no fabricated recommendation feed. A bounded read-only recommendation request is a feasible follow-up, but has no owner acceptance yet. |
| Text/synced lyrics | [Lyrics panel][lyrics] renders text and time-synchronized lines; the API request obtains a signed lyrics resource | **Blocked for the reviewed implementation:** the inspected lyrics paths rely on official-client extracted signing material. Karin does not import it. A separately permitted plain-text source is not yet established. |
| Explicit/child/replaced/unavailable metadata | [Info marks][marks], filtering and preferences distinguish these flags | **Partial:** availability is represented; richer metadata markers/filtering need typed models and fixture/live verification. Never silently substitute a different track. |
| Cache/offline playlist or album media | [Cache subsystem][cache] and playlist controls save/delete media | **Blocked:** no Karin service-media cache or offline claim. Neither Cassette source nor a subscription alone establishes Karin's download/storage rights. Cover thumbnails are a distinct non-audio concern. |
| Theme, adaptive panels, preferences and notifications | [Preferences][preferences], [theme switcher][theme] and player/sidebar widgets implement settings and presentation | **Partial:** native theme/adaptive/keyboard checks exist; the requested music-first shell, covers and preference workflow remain implementation/review items. Xvfb is not Fedora/KDE/Wayland sign-off. |
| Persistent account session | Cassette uses its own storage/session lifecycle | **Not present by design:** Karin's token/session stays in memory; persistent credentials require a verified Secret Service/refresh/revocation lifecycle. Do not copy another app's cookie storage. |

## Rust-client comparison and implementation order

`okhsunrog/yamu` HEAD was rechecked on 2026-10-29 and remains
[`9803f319b9ea6f81befbc3117e4592132ed07d0e`][yamu] (0.1.2). Its library/search,
revision-aware mutations and typed models are useful protocol references, not a
reason to import its default OAuth identity, raw error bodies or delivery keys.
Karin continues using its own small service adapter. See
[RUST_API_CLIENT_REVIEW.md](RUST_API_CLIENT_REVIEW.md) and
[STREAM_DELIVERY_REVIEW.md](STREAM_DELIVERY_REVIEW.md).

The issue's development order is intentional:

1. Repair search and playlist-detail contract/dispatch/navigation, **in parallel**
   with a bounded owner-local authenticated delivery-info experiment. Supply a
   testable application, not only this matrix.
2. If an eligible source is confirmed, connect its resolver to the existing
   GStreamer worker immediately and verify an actual library/search track on
   Fedora. Source resolution, actual playing state and audible output are
   separate checks. If source authorization/protection blocks this, record the
   exact failed stage and keep the overall sound criterion open.
3. Add covers, music-first adaptive navigation and the best-validated likes/
   playlist actions in coherent subsequent PRs, with image/structural checks.
   My Wave/lyrics/offline/persistence remain explicit capabilities, not decorative
   buttons masking unsupported APIs.

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
