<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0034: Bounded track lyrics and related metadata

Status: v0.8 implementation decision; live content acceptance is separate.
Date: 2026-11-14
Refined: 2026-11-22
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
excerpt is not presented as a complete text. Missing full text or an absent
rights flag makes the text unavailable. An explicit false flag produces a
permission message without offering a retry of that refusal. Transport and
schema failures have their own error and Retry. This path supplies plain text
through the legacy contract.

Offer LRCLIB as a separately named, explicit user action. Before the request,
the interface discloses that public title, first artist, optional album and
known duration go to `https://lrclib.net/api/get`. Do not send Music account or
track IDs, OAuth, cookies, headers, listening history or private collection names.
The native confirmation dialog defaults to Cancel; only its explicit Search
response submits metadata. An unchecked option may also save the independent
`lyrics_external_auto` preference. It defaults off, persists in the existing
private settings, and can be revoked in Preferences. It contains no account
identity and survives logout. Consent to one lookup alone leaves it off.
The existing no-redirect HTTP client supplies a truthful Karin User-Agent and
accepts bounded JSON only. Require matching title/artist and compatible duration;
404, instrumental or missing text are unavailable. Prefer admitted LRC and
keep a matched timeline if a later reply for the same selection supplies only
plain text. Display its actual source. Without the saved preference, every
external lookup requires the explicit disclosed action. With the preference,
attempt one lookup for each visible Lyrics selection after the Yandex result
lacks synchronization. Opening the app, playing a track with Lyrics closed and
background retries do not send external metadata.

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
selectable, spaced stanza/line labels and an honest “Текст без синхронизации” marker;
no fabricated timestamps or editable TextView stand in for karaoke. Plain
Yandex TEXT never starts a timeline. A prominent “Найти синхронизированный текст”
action leads to the disclosed external search. Its successful LRC reply replaces
the presentation without restarting playback or seeking the audio. A native
Lyrics menu can copy the complete displayed Unicode text to the local clipboard,
with LRC timestamps/metadata stripped by the existing parser. Selectable labels
retain ordinary fragment selection and Ctrl+C. Synchronized lines use GtkListView
virtualization. Each `LyricRow` paints one selectable, start-aligned GtkLabel
at a Pango scale of 1.00 to 1.28. Variable fonts receive a continuous
`wght` value from 400 to 700. For static fonts, GTK snapshots the same Pango
layout at adjacent subpixel offsets across a width up to 0.06 em. It applies
the theme foreground opacity once to the combined strokes. The letters grow
in weight while their advances follow the Pango size. GTK draws text selection and
visible focus directly. Themed foreground opacity rises from 76% to 100%.

A second GtkLabel measures that specific line at maximum emphasis, wrapped at
the available width. The row uses GTK height-for-width measurement and paints
only the selectable label. Each line's reservation keeps its neighbors stable
through the transition. The scroll view allocates a native scrollbar gutter
so wrapped text stays clear of the scrollbar while Follow is moving.

At a natural line boundary the old line loses size, weight and contrast as
the new one gains them. The Lyrics view uses one GTK frame-clock callback for
all moving rows and the pending Follow scroll adjustment. Both text directions
start on the same frame. Text and centering use Libadwaita sine easing over
300 ms of presentation time. Each subsequent GTK tick advances these transitions
by the elapsed interval, capped at 30 ms. At a
healthy refresh cadence the transition takes about 300 ms of wall time.
Following a compositor stall the letters continue changing over subsequent
frames. Playback snapshots still select the current line as soon as they arrive.
If another timestamp arrives during the transition, the newly outgoing line
shrinks from its current emphasis while earlier outgoing lines continue their
own transitions. A recycled row reads
the current position and emphasis when it binds, including during Follow. A
seek applies the final styles and Follow destination immediately. Reduced Motion
applies the enlarged, bold current line and the enabled Follow destination
immediately. High Contrast keeps all text at full foreground opacity. Wheel,
keyboard and touch scrolling cancel the pending centering adjustment and
suspend Follow until requested again. Text transitions and playback-derived
line selection continue during manual scrolling.

Highlighting uses Playing/Paused/Ready position only while FollowingCurrent
and the opaque inspected/current occurrence bindings agree. On entering Lyrics
from an inspected row, bind to the playhead
when the source context proves it is the exact current occurrence. Inspected
duplicates and ambiguous ID-only selections keep their own identity. Stop, EOS or another inspected occurrence clears the
active line. Changing selection/account clears old content and retires its
transitions and requests. Line timestamps never imply word or syllable timing.

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

Yandex Music documents synchronized lyrics that follow playback and a separate
view for reading plain text.
[Yandex Music player guide](https://yandex.ru/support/music/ru/listening-and-settings/player-functions#tekstomuzyka).

The released Cassette GTK row gives both label sizes the same start alignment
and wrapping. It uses the theme's `title-4` and `title-1` sizes. Its panel
demotes the old line and promotes the new one by switching label visibility.
The published screenshot shows the larger, heavier current line amid quieter
neighbors. This visual hierarchy informed the Karin design. Karin interpolates
the size of the glyphs in its visible label using the existing player snapshots
and selection model.
[GitHub v0.2.5 row template](https://github.com/Rirusha/Cassette/blob/b1c55e5fb71e60c9ff5421db42695f4dd5526a26/data/ui/lyrics-line.ui#L11-L35) ·
[line selection](https://github.com/Rirusha/Cassette/blob/b1c55e5fb71e60c9ff5421db42695f4dd5526a26/src/widgets/lyrics/lyrics-panel.vala#L34-L47) ·
[visibility binding](https://github.com/Rirusha/Cassette/blob/b1c55e5fb71e60c9ff5421db42695f4dd5526a26/src/widgets/lyrics/lyrics-line.vala#L60-L74) ·
[GitLab synchronized-text screenshot](https://gitlab.gnome.org/Rirusha/Cassette/-/blob/c09c69a2d7e84be25bf03111a98c5e462e68d21d/data/images/3-sync-lyrics.png).

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
