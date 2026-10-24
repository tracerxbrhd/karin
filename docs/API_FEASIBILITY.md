# Yandex Music API feasibility

Checked: **2026-10-24**, anonymous probes at approximately **11:40 UTC**.

Karin uses an unofficial service interface. A successful HTTP response establishes
only the behavior observed for that request, from this cloud environment, at this
time. It does not establish authenticated access, a playback entitlement, a stable
API contract, or permission to distribute a third-party music client.

## Status meanings

- **Verified** — the stated behavior was observed in a real request. The scope of
  that observation is stated explicitly.
- **Documented but unverified** — an official document or community implementation
  describes the mechanism, but it was not exercised successfully for Karin.
- **Blocked** — an identified access or prerequisite problem prevents the stated
  capability in the current workflow. This does not mean it is unavailable to
  every user or in every region.
- **Unsupported** — Karin deliberately does not implement the capability.

## Capability matrix

| Capability | Status | Sources and check | Actual result and limits |
| --- | --- | --- | --- |
| Anonymous account/service status | Verified | [Community account adapter][account]; anonymous `GET /account/status` | HTTP **200**, JSON `result.account` and `result.permissions`. `account.serviceAvailable` was **false**; no `uid` or login was returned. This verifies an anonymous status probe, not access to an account or music. Permission labels in this anonymous response do not prove that audio can be played. |
| Registration and general Yandex OAuth consent | Documented but unverified | [Official application registration][register], [authorization-code consent][oauth-code]; documentation read over HTTPS | Official documentation requires an application registered by its developer and user consent. No Karin OAuth application, callback, Music scope, or token grant was validated. Generic Yandex ID authorization does not prove Yandex Music API access. |
| Authorization for Karin to access Music | Blocked | Official OAuth sources above; [community device flow][device] and [community README][readme] reviewed | The community client describes Device Flow and token-based access, but its client-specific authentication settings are not authorization for Karin. No third-party client credentials were copied, no authorization was attempted, and no user password/token was requested. A supported grant for Karin's own registered client and Music permissions remains a prerequisite. |
| Authenticated user data and personal library | Blocked | [Account methods][account], [README][readme]; anonymous status probe | No user identity was returned. An authorized Music credential and a permitted integration are missing. Personal data endpoints were not called. Anonymous `account/status` success must not appear as signed-in UI state. |
| Search | Blocked | [Community search adapter][search]; anonymous `GET /search?text=Beethoven&type=track&page=0&nocorrect=true` | HTTP **451**, JSON `error.name = "Unavailable For Legal Reasons"`, empty error message. No search results were obtained. The exact reason and regional/account applicability were not independently established. |
| Track metadata | Blocked | [Community tracks adapter][tracks]; anonymous `GET /tracks/121313` | HTTP **451**, same error envelope. Neither the identity nor contents of this example track were verified. This is evidence for this route/request, not a catalog-wide claim. |
| Album metadata | Blocked | [Community albums adapter][albums]; anonymous `GET /albums/30480` | HTTP **451**, same error envelope. No album payload was obtained. |
| Artist metadata | Blocked | [Community artists adapter][artists]; anonymous `GET /artists/3000/brief-info` | HTTP **451**, same error envelope. No artist payload was obtained. |
| Locating an available audio stream | Documented but unverified | Community `tracks_download_info` and `tracks_file_info` methods in [tracks adapter][tracks] | These are implementation evidence only. No stream URL, audio bytes, signing mechanism or encrypted delivery was requested or implemented in Karin. Availability depends on an authorized client, account entitlement, content and service restrictions. |
| Real Yandex Music playback in Karin | Blocked | Authentication/access findings above; [Music terms][music-terms] and [Plus terms][plus-terms] | There is no validated end-to-end path from an authorized Karin account to a permitted playable stream. The diagnostic GStreamer tone is independent of this path and is not Yandex Music content. |
| “My Wave” | Documented but unverified | Community [radio methods][radio], [rotor sessions][rotor], [wave settings][wave] | Source code describes personalized stations, sessions and feedback. No session was created and no feedback was sent. Authenticated recommendations, sequencing and listening-history behavior remain unverified. |
| Subscription and playback entitlements | Documented but unverified | [Plus terms][plus-terms], [Music terms][music-terms], community [README][readme] | Official terms tie content access to subscription, location and service restrictions. The community README describes restrictions on anonymous listening; this is not a verified Karin playback allowance. No subscribed or unsubscribed account was tested. A subscription alone does not establish permission for Karin's unofficial API integration. |
| DRM/access-control bypass, borrowed OAuth client secrets, password collection and redistribution of music | Unsupported | Project `AGENTS.md`; service terms | Not implemented and not part of the technical foundation. A failed access check will not trigger alternate identities, regions, credentials or delivery paths intended to bypass it. |

## What was actually sent

All live probes were anonymous HTTPS GET requests through the existing cloud
proxy with normal certificate verification. Headers were
`Accept: application/json` and
`User-Agent: Karin/0.0 technical-feasibility-check`. There was no `Authorization`
header, cookie jar, OAuth flow, password, account token or downloaded audio. No
service state was changed. No redirect or alternate host was used to work around
the 451 responses.

The account response was a normal JSON envelope with `invocationInfo` and
`result`. Its relevant structure was:

```text
result.account: now, region, serviceAvailable=false
result.permissions: until, values, default
account uid/login: absent
```

The opaque numeric region is not interpreted as a country. This document does
not retain request identifiers, account tokens or full response bodies. The
metadata IDs above are concrete request inputs, not verified catalog fixtures.

To repeat only the non-authenticated status check:

```sh
curl --fail-with-body --max-time 15 \
  -H 'Accept: application/json' \
  -H 'User-Agent: Karin/0.0 technical-feasibility-check' \
  -w '\nHTTP %{http_code}\n' \
  'https://api.music.yandex.net/account/status'
```

Run any live check deliberately; it is not an automated test dependency. Inspect
the status and schema rather than assuming a previous result will recur. Do not
add tokens to this command or publish future authenticated response bodies.

## Source quality and unresolved prerequisites

The community library explicitly calls itself unofficial and reverse engineered.
The inspected source revision is pinned below so its claims remain auditable.
It is useful protocol evidence, not an official compatibility or legal promise.
Its hosted token/radio documentation returned **403** from this environment;
those pages were not used as verified evidence. The linked repository source was
retrieved successfully instead.

Official OAuth registration and authorization-code documentation returned HTTP
200. They describe Yandex ID authorization, but do not establish that Karin can
obtain the Music permissions needed here. The official Music terms returned HTTP
200 and describe personal/noncommercial use and service-imposed restrictions
(notably sections 2.7–2.8). The inspected Plus terms describe subscription content,
location and device constraints. Neither set of terms was treated as an express
grant to use the unofficial Music API from Karin.

Before implementing sign-in or real music playback, resolve:

1. Karin's own registered client, a supported public/native-client grant,
   callback handling and required Music permissions; do not embed a confidential
   client secret in a desktop binary.
2. Permission and service-policy compatibility for the intended third-party
   client, including the applicable Music/Plus terms and distribution model.
3. Actual authenticated behavior from a supported location and account, followed
   by independent tests of subscription/content entitlements and audio delivery.
4. Secure token lifecycle through Secret Service, cancellation, expiry/revocation
   handling and redaction, before an authentication UI is enabled.

These prerequisites do not block the anonymous status diagnostic or the
independent synthetic GStreamer diagnostic. They do block claims that Karin can
currently sign users in, browse their library, run My Wave or play their music.

## Sources

All sources were inspected on 2026-10-24. Community source links are pinned to
commit `9ca132e5c18a72aab779c1fc68c169380a9357d0`; official documents may change.

[register]: https://yandex.ru/dev/id/doc/ru/register-client
[oauth-code]: https://yandex.ru/dev/id/doc/ru/codes/code-url
[music-terms]: https://yandex.ru/legal/music_termsofuse/ru/
[plus-terms]: https://yandex.ru/legal/yandex_plus_conditions/ru/
[readme]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/README.md
[account]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/account.py
[device]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/device_auth.py
[search]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/search.py
[tracks]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/tracks.py
[albums]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/albums.py
[artists]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/artists.py
[radio]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/radio.py
[rotor]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/rotor_sessions.py
[wave]: https://github.com/MarshalX/yandex-music-api/blob/9ca132e5c18a72aab779c1fc68c169380a9357d0/yandex_music/_client/wave.py
