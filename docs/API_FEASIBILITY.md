<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Music API boundary

Karin uses an unofficial Yandex Music interface through its own typed
`MusicService` adapter. Service behavior can change independently. Metadata
success establishes neither playback entitlement nor public-distribution rights.
[VALIDATION.md](VALIDATION.md) records actual owner and automated acceptance.

## Supported contracts

| Area | Current boundary |
| --- | --- |
| Account | Validate the actual Music account before admitting a session; Yandex ID success alone is insufficient. |
| Library and playlists | Account-bound liked-track and own/liked playlist reads, strict owner/kind identities, bounded contents and explicit partial/end/error states. |
| Search and details | Independent track/album/artist/playlist sections and paging. Validate minimal track-hit IDs, then normalize through one canonical batch lookup. |
| Track reactions | Distinct Like/Unlike and account Dislike/removal. Valid dual reads enable paired mutations confirmed from both collections. Unavailable Dislike stays hidden; a fresh independent Like read preserves established Like-only confirmation. Only confirmed current-occurrence Dislike addition invokes Next once. |
| Playlist creation and editing | Owned create/add/remove/reorder/rename/visibility/delete use current ownership, exact occurrences and independent readback; ambiguous writes remain unresolved. Contents use revision, while metadata/delete have no proven server revision CAS. The editor snapshot is bounded at 2,000 exact positions. |
| Track text | Legacy Yandex supplement full text with explicit `hasRights == true`; separately disclosed, user-requested LRCLIB plain/LRC lookup by public metadata. No extracted modern lyrics signer. |
| Similar tracks | Seed-bound `/tracks/{id}/similar`, at most 50 admitted selectors normalized in one canonical batch; ordinary queue/player/reactions. |
| Online playback | Use the separate guarded [resolver](STREAM_DELIVERY_REVIEW.md) for an explicitly selected track. Availability metadata alone is insufficient. |
| My Wave | Ordinary signed-in Home using the shared player; explicit Listen starts radio. Four admitted feedback events use original batch markers and actual Playing time; no automatic startup or radio restoration. |

Ambiguous writes require independent readback, never fabricated success or POST
replay. The owner has accepted v0.8 functionality; see
[VALIDATION](VALIDATION.md) for reported scope rather than inferring every optional
catalog/backend/failure scenario from the general acceptance.

## Transport and models

[`src/music.rs`](../src/music.rs) fixes the origin to HTTPS
`api.music.yandex.net`, with a five-second connection limit, fifteen-second
normal request deadline and 1 MiB response ceiling. It disables redirects and
automatic retries. Validate token expiry/account binding and request generations;
late results must not restore another session's data. UI code receives domain
models and safe fixed errors rather than provider JSON, URLs, headers or tokens.
Only known error codes refine categories; generic 403 identifies no specific
missing scope, subscription, region or client restriction.

Search admits at most 50 raw track-hit positions and 50 distinct lookup IDs per
page, preserving provider order, repeated occurrences and composite selectors.
Canonical lookup avoids N+1 requests. Required identity/authorization fields stay
strict; unsupported optional search display fields do not discard the whole
result. Missing/unavailable data remains explicit. See
[ADR-0016](adr/0016-search-identity-and-batch-normalization.md).

Ordinary loaded collection views retain their 2,000-position/100-page startup
bound. A settled nonempty query or Enter in an eligible incomplete collection
can start or continue whole-collection indexing. It reads a compact liked or
playlist manifest (at most 4 MiB/20,000 original positions), then batches of at
most 50 and a final full membership/revision recheck. Work follows the current
view and search intent. The process-only index supports Unicode title/artist/
album search, title/artist/provider-order sorting, and added-date sorting only
when all occurrence timestamps are valid. A bounded 200-row result window can
reach positions beyond the old prefix; playback still snapshots at most 500
occurrences. Unsupported larger collections and incomplete/changed membership
are labelled explicitly. Indexing/filtering/sorting never rewrites an active
queue. See [ADR-0035](adr/0035-incremental-whole-collection-index.md), extending
[ADR-0025](adr/0025-loaded-collection-search-and-playback-provenance.md).

## Lyrics and related tracks in v0.8

`music::track_extras` uses `GET /tracks/{id}/supplement` for legacy Yandex
plain text. Admit `lyrics.fullLyrics` only with explicit `hasRights == true`;
the `lyrics.lyrics` opening excerpt cannot masquerade as the full song. Missing
full text or unknown rights are unavailable. Explicitly refused rights have
their own permission state. Transport and schema failures remain separate.
The current modern `/lyrics` path in reviewed upstream code needs extracted
client HMAC material and is not implemented. This is a restriction of that
specific route, not a blanket claim that Yandex text is forbidden or unavailable.

LRCLIB lookup discloses its external source and public metadata submission.
It requires per-request consent or the separately saved default-off automatic
preference. Automatic lookup runs only for visible Lyrics after missing
synchronization, once per selected track; revocation and request/session fences
remain effective. `GET https://lrclib.net/api/get` carries title, first artist,
optional album/duration and a truthful Karin User-Agent/project URL. It carries
no Music token, cookie, Music/account ID, private collection or history. The
existing HTTP client disables redirects; require bounded JSON, matching title/
artist and compatible duration. Prefer admitted synchronized LRC and preserve
it when a later same-selection reply contains plain text only. A 404, an
instrumental track or a missing text is reported as unavailable. One shared
worker gate enforces sequential requests and the documented spacing/Retry-After;
there is no undisclosed fallback or automatic retry. Lyrics remain memory-only with source
attribution, literal rendering, bounded 128 KiB/2,000-line parsing and active-line
lookup from the existing player's actual position. No text or transient resource
is added to cache, playback persistence, D-Bus or logs.

`GET /tracks/{id}/similar` must return the selected seed identity. Inspect at most
200 wire candidates, admit at most 50 distinct non-seed selectors and use one
canonical batch to normalize the result in provider order. Unsupported optional
display fields do not invalidate checked identities. Omitted/unavailable results
are counted, not synthesized. Current account binding is checked around the
requests; Play/Next/Append/Details/reactions retain the accepted single-player
and confirmation contracts. This read does not start My Wave.

Endpoint comparisons and pinned source provenance are retained in
[ADR-0034](adr/0034-bounded-track-lyrics-and-similar.md), including MarshalX,
released Cassette and the [LRCLIB API](https://lrclib.net/docs.md). No upstream
lyrics, private signer or response is copied into the repository. LRCLIB's
software license does not license every lyric for redistribution. Availability
and matching remain provider-dependent; automated original-text fixtures do not
prove every real catalog result or distribution right.

## Track reactions

Unlike and removing Dislike are independent operations; neither adds the other
mark. The pinned [MarshalX account helpers](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/likes.py#L451)
corroborate a disliked-track collection, `add-multiple`/`remove` form writes and
revision acknowledgments. [Cassette v0.2.4](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/yam-client.vala#L578)
has separate singular helpers; Karin does not try alternate routes after refusal.
Published Like/Dislike helpers describe clearing an opposite mark when adding
one. Karin confirms both states from the service rather than inventing the
complement or issuing compensating POSTs. Unknown, conflicting, stale or
unauthorized state cannot enable a false success. If the Dislike contract cannot
be read, a fresh independent liked-track membership read keeps Like/Unlike usable
through its existing confirmed path. That fallback leaves Dislike unknown and
hidden, without guessing a persisted state. Recovery reads are GET-only.

The account mutation is separate from recommendation-only negative Wave events.
The [modern event model](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/rotor/session_event.py#L10)
does not by itself establish Karin's negative-feedback lifecycle. Confirmed
Dislike addition on the unchanged current occurrence now delegates to existing
Next, including its established Wave Skip semantics. This adds no new Wave event
and claims no measured recommendation effect. The accepted four-event protocol remains intact. Race, refusal and account-change
guards use synthetic fixtures; current acceptance is in [VALIDATION](VALIDATION.md).

## My Wave

The existing `music::wave` adapter uses the published modern JSON session flow:
`POST /rotor/session/new`, then session-bound `/tracks` and `/feedback` POSTs.
First/next batches undergo one bounded canonical lookup, with the same strict
identity/order/composite normalization as search. Session/batch identity is
opaque and memory-only; status-only unknown/terminated replies stop admission.
The controller binds original markers to exact queue occurrences and uses the
existing resolver/player/MPRIS. Listening comes from the worker's actual Playing
ledger, not media position, selection or GTK callback elapsed time.

The core request shapes are corroborated by
[Cassette v0.2.4](https://github.com/Rirusha/Cassette/blob/9d34251d01bcd414ffbec2bef7931090d33c9124/src/client/api/yam-client.vala#L948)
and [modern MarshalX](https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/rotor_sessions.py).
Their optional feedback `from` placement differs; Karin retains its existing
Cassette `event.from` contract. The reviewed yamu/yamuse station routes do not
confirm this modern flow and are not automatic fallbacks. No new identity,
OAuth mechanism, signing secret or API dependency is introduced.

Local retirement can drain one interrupted heard track's final skip, then cancels
without a session-finished event. Completed EOS/unheard tracks gain no extra skip.
An in-flight HTTP request cannot be unsent; account/logout/close fence queued work
and late replies. Ambiguous POSTs are not replayed. Safe diagnostics expose only
fixed stage/category/event/lifecycle, known status and bounded counts, never
session/batch/track IDs, resources or bodies.
[ADR-0027](adr/0027-experimental-wave-lifecycle.md) records the protocol.

## Authorization evidence and client choice

The owner registered a user-authorization app with `music:api-public`,
`music:major-public-api`, `login:info` and `login:avatar`. Scope selection alone
does not establish an effective Music grant. Its PKCE experiment issued a token
and passed Yandex ID
client/account binding, but Music `/account/status` and `/account/about` returned
generic HTTP 403. The exact cause was not established. Those observations do not
explain the working Cassette-compatible provider or justify an identity, header,
region or access-control workaround. The [original owner result](https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6470853664)
and [token/header comparison](https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6471631991)
remain separate evidence. Current ordinary sign-in is described in
[AUTHENTICATION_REVIEW.md](AUTHENTICATION_REVIEW.md).
The [2026-11-22 integration review](AUTHENTICATION_REVIEW.md#integration-review-2026-11-22)
records the official registration/API conditions and the completed owner-send
request. It adds no new login trial or explanation for those generic 403 results.

Reviewed [yamu 0.1.2](https://github.com/okhsunrog/yamu/tree/9803f319b9ea6f81befbc3117e4592132ed07d0e)
and [yamuse 0.3.2](https://github.com/flxxxxddd/yamuse/tree/368c74646520e8bcd30c44a9887e1bf99cb58172)
accept tokens but default to official Android identity headers. Their OAuth,
signing, raw-error and retry behavior differs from Karin's policy; the reviewed
yamuse also derives unredacted token/account Debug. Neither is a dependency. Adoption requires identity-neutral bounded transport,
reviewed notices and exact MSRV checks; neither was independently validated
with Karin on 1.98.1. Service permission remains separate.

## Permission and evidence limits

Source reuse, successful local interoperability, subscription rights and public
service distribution are distinct. GPL applies to Karin's software, not Yandex
content or trademarks. Preserve [licensing/provenance](LICENSING.md), evaluate
current [Music terms](https://yandex.ru/legal/music_termsofuse/) and
[Plus conditions](https://yandex.ru/legal/yandex_plus_conditions/) before changing
integration/distribution scope, and retain the separate
[audio storage/reuse gate](AUDIO_CACHE_REVIEW.md). No DRM bypass, extracted official-client
signer/key or imported official-client cache is an acceptable fallback.

The [historical review](https://github.com/tracerxbrhd/karin/blob/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d/docs/API_FEASIBILITY.md)
retains request-specific anonymous 200/451 and own-client failures. Automated
checks use loopback fixtures; private data, credentials, signed resources and
recordings never belong in Cloud/CI.
