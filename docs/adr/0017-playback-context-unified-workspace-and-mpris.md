<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0017: Playback contexts, a unified native workspace and MPRIS

Status: Accepted and owner-accepted in merged v0.2; v0.3 information/grid presentation extends this decision via ADR-0020
Date: 2026-10-31
Supersedes: ADR-0015's MPRIS deferral and ADR-0016's track-row activation opening details

PR #13 is now merged into `master`; the owner accepted its native and Flatpak
contextual playback/MPRIS/KDE baseline, with hardware media keys N/D. The final
“continue existing PR #13” workflow below is historical, not a restriction on
the authorized v0.3 branch/Draft PR. New protected persistence/cache/workspace
work uses ADRs [0018](0018-persistent-session-and-settings.md),
[0019](0019-bounded-account-cache.md) and
[0020](0020-seamless-music-workspace.md), and needs fresh acceptance.

## Context

The owner's Fedora run at `f0aa2bb` accepts corrected search, account actions and
preserved real Music playback, but rejects the overall UI. A working row should
start its actual track immediately and keep its containing collection as the
playback context. A desktop music application also needs usable navigation,
unified catalog search, an unobtrusive account control, a coherent player bar
and media-system integration. These changes must preserve the accepted voluntary
login, minimal-ID search normalization, MusicService and guarded GStreamer path.

## Decision

Keep one native GTK4/Libadwaita application and the existing audio worker. Model
a `PlaybackContext` with its source kind/identity, returned ordered tracks and
current position. A track-row mouse/Enter/Space activation selects that position
and starts the real resolver/player; details become a distinct labelled action.
Liked tracks, albums, playlists and search results supply their actual loaded
tracks, not an invented catalog or an eagerly downloaded full collection.
Next/Previous and EOS use that context and skip unavailable returned items;
paused/stopped intent is preserved. The queue/context remains bounded to 500
positions. Deliberate manual appends follow the loaded context; starting a new
context replaces the old context and its extra appends. Retain duplicate
positions and mark partial loaded collections rather than fetching indefinitely.
No UI navigation or late worker completion may replace a newer playback intent.
Stop, logout and close retain their existing
source cancellation and off-main-thread cleanup.

Use a compact desktop icon rail, with an explicit expand-labels action, and a
native drawer on narrow windows. Icon-only actions retain tooltips, accessible
names and keyboard access. Show a round account avatar using initials from the
already known account name unless an independently verified photo is available.
Its popover contains the known name/session facts, a fixed `https://id.yandex.ru/`
external-browser action and the existing Cancel-default logout confirmation.
Do not infer a profile photo, new account attribute or persistent login.

Search initially presents independent tracks, albums, artists and playlists
sections. Each uses actual typed API results, its own failure/empty state and
Show All/Retry/paging actions. One `type=all` search plus at most one existing
track-normalization batch supplies the initial result; category pages use the
existing bounded search path. Preserve the 50 track-position/50 distinct-ID
normalization limits, account/session epochs and provider pagination. Playlist
search summaries retain the actual owner/kind; an absent count remains unknown.
A validated playlist with missing/invalid display text may use the generic
**Плейлист** UI label, which is not presented as its real returned name. No
generated count, artwork or availability fills missing service data. A failed
section does not erase successful categories. Empty categories are omitted;
all-empty results have one message. Section Retry requests only that category,
while Back to All runs a fresh bounded aggregate query.

Recompose the persistent bar around actual cover/title/artist, transport,
progress/seek, volume and queue. The controls consume existing controller
snapshots and capabilities; no separate UI playback state or audio engine is
introduced. Everyday library/search controls remain separate from diagnostics.

Export MPRIS 2.2 through the existing GIO session D-Bus integration, at
`/org/mpris/MediaPlayer2`, owning exactly
`org.mpris.MediaPlayer2.io.github.tracerxbrhd.Karin`. Root Raise/Quit and Player
transport, position/seek and volume actions dispatch to the same native player.
Publish truthful PlaybackStatus/capabilities, microsecond position/duration,
PropertiesChanged and Seeked. Add numeric seek acknowledgement telemetry to the
existing worker snapshot only after a successful native seek; the bridge waits
for the counter/target acknowledgement before Seeked. Shared Play consumes
outstanding Ready preparation/resume state instead of retaining stale buffering
intent. The accepted resolver, source transport and underlying GStreamer
algorithms are preserved. Next/Previous must preserve paused/stopped intent;
stale SetPosition track identifiers cannot seek a replacement selection.

Metadata contains only the selected track's known display fields, duration, an
opaque generation-bound object path and optional validated local cover-file
artwork. The bridge's cover-only PNG cache is bounded to 256 pixels, with a
private random 0700 directory and 0600 file, removed on replacement/logout/close.
Bounded local Busy admission retries are not HTTP retries or music caching.
Never export a signed media/CDN address, OAuth/cookies, query, response
body or `xesam:url`. OpenUri is unsupported; MPRIS is not an arbitrary resource
input. Omit unimplemented optional LoopStatus and Shuffle properties and do not
advertise TrackList. Rate is fixed at 1.0; the standard Rate=0 action pauses.
Drop bus registration/name ownership on close.
Flatpak gains only the exact MPRIS own-name permission, without broad session-
bus, filesystem or Secret Service access.

## Validation and consequences

Check actual mouse/keyboard track activation, context origin/order/selection,
manual queue changes, Next/Previous/EOS, partial collections, cancellation and
cleanup through the existing synthetic resolver/GStreamer fixtures. Check real
session-bus methods/properties/signals and stale identifiers against local
synthetic state, then join that adapter to the real player in a native fixture.
No account, copyrighted recording, token or resource address enters Cloud/CI.

Inspect fresh populated UI and GTK allocations at compact/medium/wide/maximized
sizes, both themes and scale 2. Useful density and composition require visual
review in addition to clipping/focus assertions; the owner's previous UI FAIL
remains until a new manual result supersedes it. Run stable and Rust 1.98.1,
Clippy, tests, GTK/WebKit/GStreamer, license, offline release and CI checks.
Record executed results by revision in [VALIDATION.md](../VALIDATION.md).

One new Fedora build must verify the actual service track/context and sound,
account cancellation, unified search/details, the revised workspace, `playerctl`,
KDE's media widget and hardware media keys. Cloud bus/synthetic-media success
does not pass that check or the complete current Flatpak build/install/sandbox.
Successful technical playback and GPL reuse do not resolve the separate public
OAuth-registration/service-distribution conditions or persistent Secret Service
lifecycle. Continue in existing PR #13 without creating a PR, changing its base,
merging or closing ancestors automatically.
