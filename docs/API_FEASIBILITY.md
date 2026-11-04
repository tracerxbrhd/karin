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
| Likes | Read membership/revision, perform one deliberate mutation, then independently confirm the desired state. |
| Playlist creation | One explicit create request followed by independent identity/title/visibility confirmation; ambiguous writes remain unresolved. |
| Online playback | Use the separate guarded [resolver](STREAM_DELIVERY_REVIEW.md) for an explicitly selected track. Availability metadata alone is insufficient. |
| My Wave experiment | Explicit `--experimental-wave` in the ordinary signed-in app; bounded session/batch/feedback integration, with live end-to-end compatibility pending. |

Ambiguous writes require read/confirmation, never fabricated success or POST
retries. Broader playlist edits, recommendations and lyrics are not shipped
workflows merely because backend methods exist. Ordinary Home keeps Wave
unavailable; its separately labelled local experiment does not establish live
service support or default readiness.

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

## Experimental My Wave

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

This is source-level evidence. Ordinary account access does not prove radio
permission. Cloud fixtures cannot establish real session creation, audibility,
accepted feedback or continuation with Karin's token. HTTP 200 with an admitted
feedback acknowledgment proves receipt under this envelope contract, not its
recommendation effect. Uncertain writes are not repeated; queued work and late
results are retired on account/logout/close, while an in-flight request cannot
be recalled. Only fixed stage/category, known HTTP status and bounded counts may
be reported, never wire bodies, session/batch/track identifiers or resources.

See [ADR-0027](adr/0027-experimental-wave-lifecycle.md) and the
[Fedora opt-in procedure](BUILDING.md#my-wave-local-acceptance). Keep default Wave
disabled and Issue #16 open until the owner confirms the whole real chain.
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
[offline-audio gate](AUDIO_CACHE_REVIEW.md). No DRM bypass, extracted official-client
signer/key or imported official-client cache is an acceptable fallback.

The [historical review](https://github.com/tracerxbrhd/karin/blob/97093ee71f8df7b47b5809ca8c1d4774a2c5b22d/docs/API_FEASIBILITY.md)
retains request-specific anonymous 200/451 and own-client failures. Automated
checks use loopback fixtures; private data, credentials, signed resources and
recordings never belong in Cloud/CI.
