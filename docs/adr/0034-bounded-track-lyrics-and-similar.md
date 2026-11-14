<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0034: Bounded track lyrics and related metadata

Status: v0.8 implementation decision; live content acceptance is separate.
Date: 2026-11-14
Extends: ADR-0016, ADR-0022 and ADR-0031

## Context

The accepted Track Info surface and single player already identify a selected
track and expose actual media position. Similar tracks and lyrics are independent
read-only capabilities; a working Wave session does not implement either one.
The reviewed modern Yandex `/lyrics` signer in Cassette/MarshalX depends on
extracted official-client signing material. Karin does not include that signer
or material, but this does not make the unrelated legacy supplement or a public
external text provider technically unavailable.

## Decision

Keep transport and normalization in `music::track_extras`, text/LRC parsing in
`lyrics`, and native presentation in `ui::music::track_extras`. Reuse the existing
MusicService HTTP clients, validated account binding, request retirement and
Track Info pane. There is no new audio controller, clock, OAuth provider, content
database or permanent service.

For Yandex text use `GET /tracks/{id}/supplement` and the published
`lyrics.fullLyrics` only when `hasRights == true`. The `lyrics.lyrics` opening
excerpt is not presented as a complete text. Missing full text or an absent/false
rights flag is unavailable, not a successful empty fake text. Preserve
normal HTTP/schema errors separately. This legacy path supplies plain text; it
does not pretend to supply modern synchronized text or call the excluded signer.

Offer LRCLIB as a separately named, explicit user action. Before the request,
the interface discloses that public title, first artist, optional album and
known duration go to `https://lrclib.net/api/get`. Do not send Music account or
track IDs, OAuth, cookies, headers, listening history or private collection names.
The native confirmation dialog defaults to Cancel; only its explicit Search
response submits metadata. Consent is per request, with no new saved preference.
The existing no-redirect HTTP client supplies a truthful Karin User-Agent and
accepts bounded JSON only. Require matching title/artist and compatible duration;
404, instrumental or missing text are unavailable. Prefer admitted LRC over
plain text, with an explicit source label. This request is not an automatic
fallback on every current-track change.

Serialize external requests across MusicService clones; respect the explicit
request spacing and provider Retry-After, with no automatic retry. Retired
account/track/pane replies cannot publish. Text remains in process memory only;
no lyrics, raw provider response or resource URL enters personal persistence,
AudioCache, D-Bus, logs, tests or packages. External API access/source attribution
is not a license to redistribute lyrics or a Music access grant.

The text value accepts at most 128 KiB and 2,000 lines. LRC supports timestamp
fractions, multiple timestamp tags, bounded signed offsets and stable ordering
of simultaneous lines. Reject invalid/control-heavy/oversized input. Metadata
tags do not become fabricated lyric lines, and text is displayed literally,
without HTML interpretation. Active-line lookup derives from the existing
PlaybackController position, including backward seek and Pause; it owns no
elapsed-time accumulator. Lyrics is a distinct mode of the permanent universal
panel, loaded lazily when that mode is visible. It has one full-height scroll
area, not a GtkExpander or an embedded text field in Track Info. Plain TEXT uses
selectable, spaced stanza/line labels and an honest “Без синхронизации” marker;
no fabricated timestamps or editable TextView stand in for karaoke. A native
Lyrics menu can copy the complete displayed Unicode text to the local clipboard,
with LRC timestamps/metadata stripped by the existing parser. Selectable labels
retain ordinary fragment selection and Ctrl+C. Synchronized lines use GtkListView
virtualization and semantic past/current/future contrast. Automatic following
centers the active area through a native 180 ms adjustment animation for adjacent
lines, respecting reduced motion; a Seek jump moves immediately rather than
replaying elapsed text. This animation changes only scroll position and owns no
playback clock. Wheel/keyboard/touch scrolling suspends following until the user
chooses “Следовать за воспроизведением”. Highlighting uses Playing/Paused/Ready
position only while FollowingCurrent and the opaque inspected/current occurrence
bindings agree. Explicit Details of another occurrence cannot borrow the playhead
even when Track IDs repeat. Stop, EOS or another inspected occurrence clears the
active line. Changing selection/account
clears old content and retires its work.

For recommendations use `GET /tracks/{id}/similar`, validate the returned seed
track identity, admit at most 200 wire candidates and normalize at most 50 distinct
non-seed selectors in one existing canonical batch. Preserve provider order;
malformed optional candidates, unavailable/missing rows and truncation count as
omissions, never invented tracks. Account binding is checked around both reads.
The independent Similar section is a normal heading in Track Info and loads
when that information view is visible. It displays ten normalized results at a time,
with user-requested Show More up to the admitted fifty and no new metadata query
per display page. Use the ordinary artwork, Details, reaction, Play, Append and
Play Next paths; recommendations neither start Wave nor acquire a separate
queue/player. Reactions share the existing write gate/independent confirmation
and do not start unsolicited per-row membership reads.

## Evidence and consequences

The endpoint shapes were compared with current MarshalX
`9ca132e5c18a72aab779c1fc68c169380a9357d0` track/supplement/similar models
and released Cassette v0.2.4 `9d34251d01bcd414ffbec2bef7931090d33c9124`
lyrics/API/player implementation. Cassette main
`bce825a13c1d1f6d0337dbea914c2d66b1ac07ba` is a libtape rewrite without that
lyrics route. [LRCLIB API documentation](https://lrclib.net/docs.md), retrieved
2026-11-14, permits anonymous API requests with its identification/rate policy.
A nonexistent synthetic-metadata lookup returned HTTP 404. A separate anonymous
lookup by known public metadata returned HTTP 200 with nonempty TEXT and LRC,
`instrumental: false` and numeric duration. Only these safe shape/status flags
were retained: no response/text was saved or printed, and no OAuth/cookies,
private account or Music IDs were sent. This confirms a positive public-provider
contract, not Yandex-session or Fedora application acceptance. See
[API_FEASIBILITY](../API_FEASIBILITY.md); no upstream code,
private signer or test lyrics are copied. Synthetic TEXT/LRC and local HTTP
fixtures test normalization, negative/error cases, identity/bounds, literal text,
clock lookup and stale/cancelled presentation. Native fixtures exercise the real
pane and player.

Availability and matching depend on each provider's current catalog. Yandex
plain text and LRCLIB plain/synchronized text must be accepted separately on
Fedora; Cloud fixtures prove neither live availability nor copyright/service
permission. Keep issue #18 open until the enabled real-content workflows are
accepted. Public-distribution #5/#22 and the absent Karin design remain separate.
See [VALIDATION](../VALIDATION.md) and the
[one final Fedora procedure](../BUILDING.md#v08-final-fedora-acceptance).
