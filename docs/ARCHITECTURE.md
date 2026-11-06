<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Architecture

Karin uses Rust, GTK4, Libadwaita and GStreamer. UI, transport, credentials, cache
and playback have separate owners. Decisions are recorded in [ADR](adr/).

```mermaid
flowchart LR
    UI[GTK controller] --> Service[MusicService]
    Service --> API[Yandex Music API]
    UI --> Queue[MusicQueue]
    UI --> Resolver[StreamResolver]
    Resolver --> Player[PlaybackController]
    Player --> GST[GStreamer worker]
    Desktop[MPRIS] <--> UI
```

## Components

| Area | Modules | Responsibility |
| --- | --- | --- |
| Catalog | `music`, `api`, `application` | Typed service access and separate anonymous diagnostics |
| Authentication | `auth`, `auth::cassette`, `ui::auth`, `oauth` | Ordinary sign-in and independent PKCE diagnostic |
| Session | `session`, `credentials`, `session_lifecycle`, `settings` | Live tokens, protected persistence and non-secret preferences |
| Data | `cache_budget`, `cache`, `catalog_cache`, `artwork`, `audio_cache` | One shared resource policy; typed metadata/artwork and separately authority-gated audio stores |
| Playback | `queue`, `stream`, `audio_cache::bridge`, `playback` | Context/order policy, resource/cache admission and one media worker |
| Desktop | `mpris`, `ui::music::system_media` | Session D-Bus commands and actual snapshots |
| Presentation | `ui::music` and its view modules | Native views, user intents and guarded result admission |

GIO workers perform bounded blocking work; main-context futures admit results
under current account/request/source generations. Twelve lanes each retain one
running and one latest pending job. The Wave lifecycle uses a separate
bounded FIFO lane because feedback must not be coalesced or retried. Source
changes, logout and close retire old work. GTK textures/updates stay on the main thread; deferred Like reads use weak
ownership outside render-time borrows. Reqwest owns its HTTP runtime.

## Authentication and protected sessions

The attributed Cassette-compatible provider uses one ephemeral WebKitGTK6 dialog
with strict navigation/return/state checks. No password inspection, JavaScript
injection, profile import or cookie persistence is allowed. MusicService validates
Music access before success/persistence; ID/cache data is insufficient. See [AUTHENTICATION_REVIEW.md](AUTHENTICATION_REVIEW.md).

Native persistence uses Secret Service; Flatpak uses a Secret Portal secret and
an authenticated encrypted token envelope. Unavailable storage permits memory-only
login, never plaintext fallback. Preferences are private, versioned XDG data.
Serialized, generation-bound operations and durable forget barriers fence late
writes/restoration. Uncertain deletion/purge remains visible with Retry.
Remember-off forgets the saved credential without stopping the live session;
logout also stops audio and clears personal views/cache. Close may retain an
allowed session/cache. Forgetting is local, with no claimed refresh or remote
revocation. [ADR-0018](adr/0018-persistent-session-and-settings.md) defines details.

## Catalog, filtering and playback context

MusicService validates account/IDs, bounds work and returns typed models. Search
accepts minimal hit IDs and one canonical batch of at most fifty distinct IDs,
preserving order, duplicates, unavailable records and paging. Optional display
fields cannot invalidate valid IDs. Reaction/playlist writes require independent
confirmation; ambiguous POSTs are not replayed.

`music::interactions` distinguishes Like, Unlike, account Dislike and removing
Dislike. Current-track reactions use compact live reads of both collections,
with account/track generations and a bounded write lane. A deliberate reaction
sends one POST and confirms both memberships/revision when dual reads are
available; opposite marks are not optimistically inferred. If Dislike cannot be
verified, a fresh independent Like read preserves the existing Like-only control
and its established confirmation path. Dislike remains hidden/unknown. Only a
confirmed Dislike addition on the same current occurrence invokes the existing
Next path once. Account/request generation and the queue-owned occurrence identity
fence late results, including repeated Track IDs; a replaced playback intent must
not be advanced. Removing Dislike and changing Like never advance playback.
A refusal or ambiguous result leaves playback alone and offers read-only recovery;
authorization loss still follows normal session teardown.
[ADR-0028](adr/0028-confirmed-track-reactions.md) separates account membership
from the unchanged Wave feedback protocol.

Continuous views admit fifty-position portions, at most 2,000 positions/100 pages.
Initial filling stops at three pages; further reads require user demand. Errors
preserve rows with read Retry. Remote end, missing/stale data and local limits
remain distinct. Like mutations retire continuation until explicit refresh,
without changing playback.

Collection filtering reuses rows and an NFKC → casefold → NFKC title/artist/album
index. Typing fetches no pages/metadata; bounded cover work may resume. Partial
coverage is labelled; next-part reads stay bounded and source changes reset the
query. An incomplete prefix is not complete remote search or sorting.

Play snapshots an aligned window of up to 500 matches in provider order, including
the selected item; at most four windows are cached. Opaque feed binding and original
admitted positions distinguish duplicates across windows/reindexing. Filtering,
appending and navigation never rewrite active playback. MusicQueue also owns a
bounded opaque identity token for every admitted occurrence, including manual
duplicates. Removing another row preserves that token; a replacement context
gets new tokens. Info and MPRIS share this identity without publishing it.
ADR-0016 and ADR-0024 cover normalization/loading;
[ADR-0025](adr/0025-loaded-collection-search-and-playback-provenance.md) defines filtering/provenance.

## Playback controller

StreamResolver admits the reviewed unprotected full-MP3 path as a sealed HTTPS
source. OAuth stays at Music API; credentials never reach descriptor/CDN or worker.
Exact origins/paths, TLS, bounded XML and no redirects exclude arbitrary URIs,
manifests and protection/identity fallbacks. The sixty-second admission lease
does not stop existing playback; later connection/resume/seek may require fresh
Play. See [stream delivery](STREAM_DELIVERY_REVIEW.md) and
[ADR-0015](adr/0015-guarded-legacy-mp3-playback.md).

PlaybackController sends commands through a 32-entry channel to one worker-owned
`playbin`. Snapshots supply actual state, position/duration, seekability, volume
and safe errors. A worker-owned cumulative Playing clock excludes pause and seek
distance independently of GTK scheduling. Commands alone do not imply Playing; Seeked requires worker
acknowledgement. Local-file diagnostics remain; service audio is not persisted.
Shutdown bypasses channel capacity and releases the pipeline in `Null` before
completion, without GTK joining the worker.

MusicQueue owns Shuffle, Repeat Off/Queue/One, upcoming order and actual history.
Repeat One handles natural EOS; explicit Next/Previous still navigate. Queue
repeat covers admitted items only, without library autofetch or error retry loops.
Modes survive normal clear/context replacement and reset on logout or close.
[ADR-0023](adr/0023-local-playback-order-and-mpris-modes.md) defines the policy
shared by native controls and MPRIS.

## My Wave

The existing `music::wave` protocol/observation components feed the same queue,
resolver and player through `ui::music::radio`. The Home Hero is available after
Music-validated sign-in in the ordinary application. Listen/Pause/Resume delegate
to the same lifecycle/player. Only explicit Listen creates a radio session;
application startup, protected-login restoration and opening Home create none.
The deprecated `--experimental-wave` CLI alias has no effect on availability,
activation or application identity. Only pending startup
has a Cancel action. There is no separate user-facing Finish control.

Original batch markers stay attached to opaque occurrence identities in memory.
Feedback and continuation use one serialized bounded lane; an uncertain POST
terminates continuation without replay. Pause preserves context and excludes
paused time. Ordinary source selection or MPRIS Stop retires radio and restores
modes; account/logout/close cancel queued work. Local retirement uses no separate
network session-finished event: only an interrupted heard track can add one final
skip; completed EOS and unheard tracks add none. Exact admitted event and local
origin/phase/finish classifications accompany safe HTTP diagnostics. JSON/body,
identities and listening payloads never enter those logs. See
[ADR-0027](adr/0027-experimental-wave-lifecycle.md) and the
[Fedora acceptance procedure](BUILDING.md#my-wave-local-acceptance).

## Metadata and artwork cache

The private account cache stores reduced metadata/permitted normalized art, never
credentials, raw responses, signed resources or audio. Hashed paths are not
encryption. Metadata retains its fifteen-minute freshness and art its seven-day
maximum; HTTP restrictions can shorten or forbid art persistence. Records keep
bounds/integrity checks, atomic writes and eviction. Their resource budget is
shared with the separately admitted audio store as described below.

Stable reads can use fresh data; mutable pages are network-first. Only eligible
network/timeout errors allow labelled saved-data fallback. Cached values never
establish authentication, membership/revision or write success. Per-reply leases
are rechecked at presentation; a fresh reply does not hide retained stale origins.
Catalog leases also bind a key epoch: known API refusals retire cached replies
and older writers before storage work. Fixed tombstones and bounded cleanup keep
Busy/I/O failures from reviving refused entries. After restart, account binding
or a policy change, metadata needs a new successful write before fresh reuse;
otherwise it is eligible only for labelled saved-data fallback. Artwork keeps
its separate HTTP/TTL policy. State admission never holds its mutex over disk I/O.
Refresh bypasses fresh hits. Cache-off stops persistent reads/writes; Clear keeps
session/audio/textures, while logout purges personal records.
[ADR-0019](adr/0019-bounded-account-cache.md) defines the cache contract;
[ADR-0026](adr/0026-refusal-fences-and-queue-occurrence-identity.md) defines the
refusal fence and stable queue identity extensions.

### Audio storage and playback selection

`cache_budget` coordinates the ordinary application's metadata/artwork and audio
stores through the same shared instance. One enabled policy defaults to **2 GiB**
and offers 500 MiB, 1/2/3/4 GiB or Unlimited. Reservations account for concurrent
writes and bounded staging; shared LRU/fairness protects small metadata within
the one total. Disk-space headroom is checked in every mode. Unlimited removes
the user byte cap, while retaining record/count bounds and the free-space guard.
Worker-only scans, trims and publication never hold the GTK admission-state
mutex over disk I/O. Preferences expose one integer Scale, total measured usage,
Clear and operation Retry; limit maintenance is coalesced for 200 ms while
Disable/Clear admission fences are immediate. Version-1 quotas migrate to the
version-2 enum without replacing credentials or cache files.
[ADR-0030](adr/0030-unified-cache-budget-and-preferences.md) defines migration,
staging accounting, metadata fairness and local disk safeguards.

`audio_cache` remains typed production code, opened on a worker under the app's
XDG cache `audio` directory. Account-scoped opaque keys, generation fences,
integrity/atomic publication and an exclusive namespace owner guard reads/writes
and restart. Per-resource 64 MiB, 1024-record and four-pending bounds remain;
these safety bounds are independent of the shared user quota.

Fresh StreamResolver admission precedes cache selection. `StorageAuthority`
separately authorizes storage/reuse and checks a current in-memory permission
lease before a hit or publication. The real `MissingStorageContract` authority
refuses admission: neither a streaming URL nor account/subscription metadata is
converted into a storage permission. Real Music consequently keeps the accepted
online path without saving audio. The immediate target is online-revalidated
reuse, not an offline mode with invented retention times.

The shared bridge hands an admitted hit's verified read-only file descriptor to
the same PlaybackController. A decoder failure before any actual Playing can
fall back once to its admitted online source in that same worker. A failure after
Playing remains a normal player error, preserving Wave's actual-playing ledger.
It starts an admitted cache write only after actual Playing; cancellation, Clear,
logout and account changes fence old work before cleanup. Clear can unlink a
record without invalidating an already admitted active descriptor. Normal close
may retain admitted records for fresh online revalidation on restart. Generated
media and the fake authority remain test-only inputs to these production paths.
[ADR-0029](adr/0029-synthetic-audio-cache-foundation.md) and
[AUDIO_CACHE_REVIEW](AUDIO_CACHE_REVIEW.md) define the admission gate.

## GTK workspace, MPRIS and Flatpak

Home/Liked/Playlists lead to native collections and catalog search. Total duration
requires complete known data and a checked sum. Row mouse/Enter/Space plays its
exact occurrence; Details is separate.

One surface holds `Closed`, `TrackInfo` or `Queue`. Outside clicks, navigation
and resize preserve mode; current-occurrence changes resume following, while
Details may inspect another track without playing. The entire surface translates
under a native 240 ms animation; compact layout uses a nonmodal Back subview.
See [ADR-0022](adr/0022-universal-music-panel.md).

The footer appears for real playback intent, stays through Pause/retryable failure
and hides on Stop/clear/logout. Its child conceals at the closed native endpoint
to release hidden width. Metadata, Like and Dislike are separate focus/click
targets; resize retains the same controls/controller. Reaction pending/error
state never changes the current source or opens Track Info. Confirmed current
Dislike addition alone delegates to Next after presenting the confirmed state.

The in-process GIO MPRIS 2.2 adapter exposes actual state, opaque track paths and
temporary cover PNGs, never credentials, account IDs or media URLs. Flatpak keeps
`org.mpris.MediaPlayer2.io.github.tracerxbrhd.Karin` own-name permission and uses
portal storage rather than broad session-bus access. See [BUILDING.md](BUILDING.md),
[VALIDATION.md](VALIDATION.md) and [LIMITATIONS.md](LIMITATIONS.md).
