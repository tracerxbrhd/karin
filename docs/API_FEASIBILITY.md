# Yandex Music API feasibility

Gate A official-document review: **2026-10-25**. Initial anonymous cloud probes:
**2026-10-24**, approximately **11:40 UTC**; retained below with their original scope.

Karin uses an unofficial service interface. A successful HTTP response establishes
only the behavior observed for that request, from this cloud environment, at this
time. It does not establish authenticated access, a playback entitlement, a stable
API contract, or permission to distribute a third-party music client.

## Gate A decision: Music authorization is blocked, not simulated

**Blocked:** no Music-specific grant, catalog/library/playback permission or
permitted stream-delivery contract has been established for a Karin-owned client.
The official Yandex ID code flow does document PKCE and a secretless initial
exchange; that finding must not be described as a lack of general OAuth support.
A Yandex ID login/profile token is not evidence of Music authorization.

Gate A returns an explicit `AuthenticationUnavailable` capability with the reason
`MusicGrantNotEstablished`. It does not enable sign-in, manual token input,
account views or Yandex stream resolution. The independent GStreamer player can
be completed and tested with locally generated media without this grant. Its
successful playback is not a Yandex Music playback result.

### Official-document decision table

The mechanisms below were **read**, not executed. No client was registered, no
owner/user login was requested, and no OAuth token/device-code endpoint was called.
“Documented but unverified” means the official document describes the behavior;
it does not certify a particular Karin registration or permission.

| Question / capability | Verdict | Evidence checked on 2026-10-25 | Observation / limit | Next authorized action |
| --- | --- | --- | --- | --- |
| Retrieve the official OAuth/ID and Music/Plus documents | Verified | Eleven HTTPS document GETs, all HTTP 200; source list below | Verified document retrieval and contents only, not an OAuth grant | Recheck current official terms/docs before changing the capability |
| Appropriate general app category for a multiuser client | Documented but unverified | [Registration types][register], [user-authorization registration][register-auth], [API registration][register-api] | User-authorization apps are intended for end users and can request at most three permission groups. API-access apps operate only on the developer's data, cannot request `login` permissions, and have a fixed verification-page redirect. Merely choosing “API access” is not a multiuser Music integration | Obtain provider confirmation of Music rights and the appropriate category before the owner registers Karin; app type cannot subsequently be changed |
| Native/public Linux registration and redirect | Documented but unverified | [User-authorization registration][register-auth], [code request][oauth-code] | The registration guide lists Web, iOS and Android. It does not document a Linux-specific platform. Web Redirect URIs must match registered scheme/host/port/path; query/fragment variation is described. The code page also requires an allowed registered redirect. No Karin loopback/custom-scheme callback or moderation was tested | Confirm the supported Linux/public-client callback with Yandex; register an exact approved URI and validate callback/state handling before use |
| System-browser authorization code with PKCE | Documented but unverified | [Code request and exchange][oauth-code] | `response_type=code`, `state`, `code_challenge`, `code_challenge_method=S256` (preferred), and `code_verifier` are documented. The token-exchange text explicitly says `client_secret` is not required when PKCE and `code_verifier` are used. Authorization codes are documented as lasting ten minutes. No live grant was requested | Prefer this candidate public-client flow only after Music permission and owner registration are established; never ship a confidential secret in a desktop binary |
| Device authorization flow | Documented but unverified | [Device-code flow][official-device] | `/device/code`, user/device codes, verification URL, polling `interval`, `expires_in`, `authorization_pending` and expiry/error handling are documented. The exchange prose describes app ID/password and `client_secret`; it provides no PKCE/secretless exemption equivalent to the code flow. Prose says ten minutes while the sample uses a different `expires_in`; runtime expiry must come from the response | Confirm public-client eligibility and actual expiry/error behavior for Karin before selecting this flow; do not copy another app's registration |
| Permission grant is client-specific | Documented but unverified | [Code request][oauth-code], [device request][official-device] | `scope`/`optional_scope` must come from permissions assigned at registration; docs refer owners to their own client's information page. Arbitrary requested scope strings cannot establish permission | Obtain the actual Music permission names and provider approval for Karin's own registration; do not query unrelated client IDs |
| Music catalog, personal library and playback grants for Karin | Blocked | Official sources above; [Music terms][music-terms] | The reviewed public sources do not establish available Music grants for a new public Karin client or an authorized Music API contract. The registration guide's ability to select other permissions is not proof that Music-specific rights are available. No Karin-owned registered client/grant was provided or validated in this workflow | Owner obtains official Music integration/grant and distribution clarification; this is an unresolved prerequisite, not a claim that every third-party integration is impossible |
| Generic Yandex ID user information proves Music access | Unsupported | [ID user-information API][id-info] | This endpoint returns identity/profile fields according to token rights; no Music entitlement is established by successful ID authentication. A token usable for ID need not be usable for Music | Test Music identity, rights, subscription and stream entitlement independently after an approved flow exists |
| Refresh lifecycle for a public native client | Documented but unverified | [Refresh tokens][refresh] | Refresh, replacement token handling and `invalid_grant` are documented. The page describes app ID/password and does not state the initial-code PKCE exemption for refresh. Code/device token responses carry expiry. Secretless refresh for Karin has not been established | Confirm the provider-supported public-client lifecycle; if refresh is not supported, use a permitted reauthorization strategy rather than embedding a secret |
| Local logout versus remote revocation | Documented but unverified | [Token invalidation][invalidate] | Deleting ordinary tokens locally logs the application out but does not revoke the server grant. User revocation in Yandex ID also revokes refresh tokens. Device-token revocation is documented with app ID and secret; `unsupported_token_type` applies if no device ID was supplied | Design Secret Service deletion and truthful logout/revocation UX after authorization becomes viable; confirm public-client remote-revocation support separately |
| Third-party client / distribution permission | Blocked | [ID program requirements][id-requirements], [Music terms][music-terms], [Plus terms][plus-terms] | No express permission for Karin's intended Music integration/distribution was established. Applicability of the program requirements to this independent client remains unresolved; a noncommercial/GPL project is not automatically approved | Owner requests provider clarification before enabling an authenticated integration; preserve third-party notices and service/content rights separately |
| Borrowed clients, passwords, cookies, manual token-paste, DRM or access-control bypass | Unsupported | Issue #3, `AGENTS.md`, service restrictions | None is an acceptable alternative to a Karin-owned authorized flow | Do not implement or request these mechanisms |
| Live Yandex stream resolution/playback | Blocked | Missing Music grant, permitted delivery contract and actual entitled-account evidence | No stream URL/audio bytes/signing or encrypted delivery path was requested or implemented. A typed future resolver boundary is not a working service adapter | After permission is established, perform separately consented Music account/entitlement/delivery checks; never infer this result from local GStreamer playback |

### Terms: documented restrictions and unresolved interpretation

The Russian Music terms state in **2.7** that the service/materials/player/database
are intended for personal noncommercial use within the service's technical
capabilities, and restrict copying, reproduction, processing, distribution and
other specified uses without agreement. **2.8** reserves the provider's ability
to impose/change technical, legal and other restrictions. These clauses do not
supply Karin with an express Music API or content-distribution license.

The [ID program requirements][id-requirements] reference clause 2.4.5 of the ID
service terms and list, among other examples, programs aimed at automated
collection/structuring of user data and copying/imitation of Yandex services.
Whether and on what conditions Karin's intended independent client is permitted
under these requirements is **unresolved**; this review neither approves that
use nor declares every third-party client prohibited. It requires provider/owner
clarification before enabling Music authorization.

The Plus document retrieved here contains multiple territorial variants. The
Russian section (effective **2026-10-23** as displayed) ties content availability
to the user's location and applicable subscription/benefits, allows additional
service/device/concurrent-playback restrictions (**1.7**), and requires an
authorized user (**2.1**). Those subscription conditions are not a grant to a
third-party API client. Do not apply one territorial variant to every user or
infer that subscription alone authorizes Karin's integration. Distributing
GPL-covered application code does not license Music content, Yandex trademarks
or service access. This is an engineering evidence review, not legal clearance.

### Safe next owner action

First obtain official confirmation of the named Music catalog/library/playback
grants, permitted service/stream integration and third-party distribution terms
for a public native Linux application. Then the owner can choose the confirmed
registration type, register **Karin's own** client, confirm supported redirect and
secretless expiry/refresh/revocation behavior, and supply public configuration
through the normal project configuration process. No confidential client secret,
password, browser cookie or access token belongs in chat, source or logs.

Only after those prerequisites are established should the project request consent
for an end-to-end account test, implement the approved browser/device flow and
Secret Service lifecycle, and independently validate Music-specific entitlement
and permitted delivery. No account login or registration is requested now,
because a working Music-specific route has not been established.

### Gate A research method and source provenance

On 2026-10-25 the agent fetched the eleven official document URLs below over
HTTPS through the existing cloud proxy with certificate verification enabled,
using `User-Agent: Karin/0.1 documentation-feasibility-check`. All returned HTTP
200 at the same URL. HTML was reduced to readable text for inspection outside
the public repository. No OAuth authorization, device-code, token, refresh or
revocation endpoint was exercised, and no authenticated/private response was
retained. Official pages may change; these findings describe the inspected
versions. The live API observations below keep their separate dates and scope.

| Official source | Specific finding used |
| --- | --- |
| [Registration types][register] | User-authorization versus developer API-access category and permission groups |
| [User-authorization registration][register-auth] | Platform choices, redirect components, requested permissions and owner registration |
| [API-access registration][register-api] | Developer-oriented access and fixed verification redirect |
| [Code request / exchange][oauth-code] | Registered scopes/redirects, state, S256 PKCE and secretless initial exchange |
| [Device-code flow][official-device] | Device-code polling, expiry fields, errors and documented exchange authentication |
| [Token invalidation][invalidate] | Local deletion versus server revocation and device-token constraints |
| [Token refresh][refresh] | Refresh request and public-client lifecycle questions |
| [ID user-information API][id-info] | Identity/profile rights are distinct from Music entitlement |
| [ID program requirements][id-requirements] | Program/service-policy restrictions requiring clarification |
| [Music terms][music-terms] | Personal use, service/content restrictions and provider limits |
| [Plus terms][plus-terms] | Territorial/subscription/device conditions, not a third-party API grant |

## Gate A anonymous API recheck (2026-10-25)

**Verified only in this scope:** five anonymous HTTPS GETs from the prepared
Codex Cloud egress, **10:19:01–10:19:05 Europe/Samara (UTC+04:00)**, against the
same baseline host, paths/query and `Karin/0.0 technical-feasibility-check`
User-Agent. Normal proxy/TLS verification was retained. There was no
`Authorization`, cookie jar, redirect following, retry, region/identity change
or alternate service endpoint. Responses were inspected in memory and reduced
to bounded status/schema facts; no private response body was retained.

| Request to `https://api.music.yandex.net` | Observed result | What it does not establish |
| --- | --- | --- |
| `GET /account/status` | HTTP **200**, JSON account object, identity absent, `serviceAvailable=false` | No login, Music grant, account entitlement or playable content |
| `GET /search?text=Beethoven&type=track&page=0&nocorrect=true` | HTTP **451**, recognized JSON legal-restriction envelope | No search results; exact restriction reason/territorial or account applicability remains unknown |
| `GET /tracks/121313` | HTTP **451**, recognized JSON legal-restriction envelope | No track metadata; the example ID is an input, not a validated catalog fixture |
| `GET /albums/30480` | HTTP **451**, recognized JSON legal-restriction envelope | No album metadata |
| `GET /artists/3000/brief-info` | HTTP **451**, recognized JSON legal-restriction envelope | No artist metadata |

The precise observable blocker is rejection of **these anonymous requests from
this cloud egress**. The response does not independently explain the cause or
prove that every region, Fedora installation, authorized account or content item
has the same result. No authorized comparison is available because a permitted
Karin Music grant is unestablished. The user reported anonymous HTTP 200 on
Fedora for the status diagnostic; no Fedora metadata/search 451 result was
reported or tested here. This investigation changes neither location nor
identity to evade the restriction, and requests no stream URLs or audio bytes.
The next authorized action is provider clarification and, only after approval
and consent, a separately scoped Music-specific test—not a bypass attempt.

## Initial foundation evidence (2026-10-24)

The following matrix and request details are retained as historical evidence.
They do not establish current authorized behavior, all regions or the result on
Fedora. User-reported Fedora anonymous HTTP 200 is recorded separately in
`VALIDATION.md`; no Fedora 451 or authorized Music request was observed here.

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

The initial foundation sources in the matrix were inspected on 2026-10-24.
The Gate A official-document review above was performed on 2026-10-25.
Community source links are pinned to
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

[register-auth]: https://yandex.ru/dev/id/doc/ru/register-auth
[register-api]: https://yandex.ru/dev/id/doc/ru/register-api
[official-device]: https://yandex.ru/dev/id/doc/ru/codes/screen-code-oauth
[invalidate]: https://yandex.ru/dev/id/doc/ru/tokens/token-invalidate
[refresh]: https://yandex.ru/dev/id/doc/ru/tokens/refresh-client
[id-info]: https://yandex.ru/dev/id/doc/ru/user-information
[id-requirements]: https://yandex.ru/dev/id/doc/ru/requirements
