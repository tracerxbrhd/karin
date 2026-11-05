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
| Track reactions | Distinct Like/Unlike and account Dislike/removal. Valid dual reads enable paired mutations confirmed from both collections. Unavailable Dislike stays hidden; a fresh independent Like read preserves established Like-only confirmation. Owner-confirmed at `1877a6c`; revised current-Dislike Next and installed Flatpak acceptance remain separate. |
| Playlist creation | One explicit create request followed by independent identity/title/visibility confirmation; ambiguous writes remain unresolved. |
| Online playback | Use the separate guarded [resolver](STREAM_DELIVERY_REVIEW.md) for an explicitly selected track. Availability metadata alone is insufficient. |
| My Wave | Ordinary signed-in Home using the shared player; explicit Listen starts radio. Real sessions/audio/four feedback events/continuation confirmed; owner repeated the corrected candidate without reporting new issues. Final normal-startup and installed Flatpak v0.5 acceptance remain separate. |

Ambiguous writes require read/confirmation, never fabricated success or POST
retries. Broader playlist edits, recommendations and lyrics are not shipped
workflows merely because backend methods exist. Ordinary Home offers My Wave
after Music validation without starting it automatically. The owner's general
candidate report does not establish an unreported per-scenario acceptance matrix.

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

Local collection search issues no per-key metadata requests. UI views admit
2,000 positions/100 pages and playback snapshots at most 500 occurrences; neither
filtering nor continuation rewrites an active queue. See [ADR-0025](adr/0025-loaded-collection-search-and-playback-provenance.md).

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
and claims no measured recommendation effect. The accepted four-event protocol
remains intact. The owner confirmed real server reactions and opposite-mark
interaction at `1877a6c`; revised Next and installed Flatpak checks remain separate.
See [ADR-0028](adr/0028-confirmed-track-reactions.md) and the
[local procedure](BUILDING.md#track-reaction-acceptance). Fixtures and owner
functional results establish no public API/distribution permission.

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

The owner confirmed real sessions, normalized tracks, heard Music, accepted
`radioStarted`/`trackStarted`/`trackFinished`/`skip` and several further batches.
The first eventless HTTP 400 report was inconclusive. The repeat at `a69dc79`
identified an accepted retirement `skip` followed by rejected `radioFinished`
on source change. That extra event is absent from the reviewed modern MarshalX
event list; Cassette's reviewed Flow does not send its defined constant either.
Karin now excludes it from its outgoing modern protocol. Retirement still drains
one interrupted track's final skip when appropriate, then cancels locally; EOS
does not gain another skip. This is local cleanup, not a remote deletion claim.
Accepted `from`, original markers and actual heard-time fields stay unchanged.
The owner then repeated candidate `2e4b978` on Fedora without reporting new
problems and authorized ordinary integration. Do not infer individual unreported
Stop/keyring/hardware scenarios or installed Flatpak v0.5 acceptance from that.

Cloud fixtures cannot establish real service behavior. HTTP 200 with an admitted
feedback acknowledgment proves receipt under this envelope contract, not its
recommendation effect. Uncertain writes are not repeated; queued work and late
results are retired on account/logout/close, while an in-flight request cannot
be recalled. Only fixed stage/category/event/lifecycle classifications, known HTTP status and bounded counts may
be reported, never wire bodies, session/batch/track identifiers or resources.

See [ADR-0027](adr/0027-experimental-wave-lifecycle.md) and the
[Fedora procedure](BUILDING.md#my-wave-local-acceptance). Keep Issue #16 open until
the final ordinary-startup and installed Flatpak acceptance is reported.
Successful local use still does not resolve public-distribution Issue #5.

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
