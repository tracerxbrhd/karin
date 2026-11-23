<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0034: Pinned track panel, lyrics and related metadata

Status: Accepted; current presentation/content contract.

## One panel and native presentation

Keep one permanent `Closed | TrackInfo | Lyrics | Queue` surface. Open intent,
availability, presentation and FollowingCurrent/ExplicitSelection are separate. Outside
clicks/navigation/resize preserve mode; explicit Close/Back/toggle owns closing and focus
return. New current occurrences resume following atomically; Pause/Seek/Volume and
structural reindexing preserve inspected selection. Equal service IDs cannot replace exact
occurrence identity. Presentation never starts sound or rewrites the queue; Info is
separate from row mouse/Enter/Space Play.

One outer SlidingPanel allocates its child at final width and translates the whole surface
under clipping, using 240 ms native Adwaita animation. Reserve central space
progressively; never independently animate text/artwork or cover central actions. Compact
uses a subview, retaining intent and player. Mode switches change content in place,
without replaying open or duplicating the widget tree. Footer is hidden before playback
intent and on Stop/clear/logout, retained during Pause/retryable failure; preserve zero
hidden height and 24 logical pixels inner bottom spacing. System is the sole available
UiStyle, independent of color scheme. Use AdwStyleManager, semantic colors/fonts, High
Contrast and Reduced Motion. Unknown/removed saved styles fall back without losing
settings; no removed experimental renderer/effects/cache or fictitious Karin design is
reintroduced.

## Content and privacy

Use existing `music::track_extras`, `lyrics` and player position, not a new clock, engine,
database or signer. `GET /tracks/{id}/supplement` supplies plain TEXT only from
`lyrics.fullLyrics` with explicit `hasRights == true`; excerpt/unknown rights is
unavailable, false rights is refusal. Do not use extracted modern `/lyrics` signing
material or interpret Yandex plain text as synchronized karaoke.

LRCLIB requests disclose external `https://lrclib.net/api/get` submission of public
title/first artist/optional album/duration. Require Cancel-default per-request consent or
separately saved default-off automatic permission. Automatic lookup is
visible-Lyrics-only, once per selection after missing synchronization; revocation and
account/request/pane fences remain effective. Send no Music/account ID, OAuth/cookies,
private collection or history. Serialize requests and respect spacing/Retry-After without
retries; require matching title/artist and compatible duration, bounded JSON and no
redirects. Prefer admitted LRC over later plain text. Keep actual source attribution;
404/instrumental/missing text is unavailable.

TEXT/LRC is literal, memory-only, at most 128 KiB/2,000 lines, without HTML or fabricated
timestamps. Parse bounded fractions/multiple tags/offsets and stable ties. Highlight only
the exact following occurrence from actual player position; Stop/EOS/other inspection
clears active state. No lyrics/raw replies/URLs enter cache, persistence, logs, D-Bus or
packages. Clipboard copy strips LRC metadata.

Virtualized synchronized rows interpolate the outgoing and incoming glyphs'
size/weight/contrast together over 300 ms sine easing on one GTK frame callback. Cap
presentation advancement at 30 ms per tick without changing playback-derived line
selection. Reserve each wrapped line's maximum emphasis height; bind recycled rows to
current emphasis. Seek/Reduced Motion apply final state immediately. High Contrast retains
full opacity. Manual scroll suspends Follow, not line selection or emphasis; scrollbar
gutter remains clear. Plain TEXT uses selectable labels with an honest unsynchronized
marker, not an editable karaoke substitute.

`GET /tracks/{id}/similar` validates the seed, inspects at most 200 candidates and
normalizes at most 50 distinct non-seed selectors in one canonical batch. Preserve
provider order and explicit omissions; show ten at a time without new page reads. Only
visible Info requests it. Existing Play/Next/Append/Details/reactions share the ordinary
queue/player; Similar never starts Wave or unsolicited per-row writes.

Measure incoming/outgoing motion in the same native frames using original text/audio
fixtures. [LRCLIB documentation](https://lrclib.net/docs.md) and [Cassette
provenance](../../packaging/third-party-licenses/cassette-auth/PROVENANCE.md) are
references, not rights to redistribute text. [BUILDING](../BUILDING.md) defines checks;
synthetic success does not establish every catalog result.
