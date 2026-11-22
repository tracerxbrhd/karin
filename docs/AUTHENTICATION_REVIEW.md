<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Authentication and sessions

The default `cassette-auth` feature enables `auth-webkit`. Ordinary sign-in uses
one native dialog: **Войти → Yandex → Music account validation → library**.
The owner accepted revision `97093ee` on Fedora and Flatpak; the reported scope
is recorded in [VALIDATION.md](VALIDATION.md). No real accounts or credentials
are used in automated Cloud/CI checks.

## Sign-in contract

1. Create a fresh ephemeral WebKitGTK6 network session with persistent browser
   credentials disabled. Open official Yandex authorization using the published
   Cassette registration and a fresh cryptographically random state.
2. Confirm the main document's exact HTTPS `music.yandex.ru/` return and matching,
   single-use state. Bounded parsing rejects ambiguous or duplicate fields.
   Attempts expire after ten minutes. Returned implicit tokens and personal
   fields are discarded without display.
3. Read only cookies applicable to `https://oauth.yandex.ru/token` from this
   voluntary Karin session into a bounded, redacted in-memory payload.
4. On a worker, exchange that session through official OAuth's `sessionid` grant
   using the same published registration. Enforce TLS, fixed endpoints, response
   limits, deadlines and no redirects. Cookies never reach Music API.
5. Keep the dialog open until MusicService validates the account. Then load
   library data through the existing service. A Yandex page or issued token is
   insufficient to report Music sign-in success.

This grant does not use PKCE. The separate local `--oauth-diagnostic` implements
Karin-owned Authorization Code + S256 PKCE with hidden redirect input; it is a
maintenance experiment, not the normal UI. Device Flow has no GUI entry and is
excluded from default/Flatpak builds. Users do not enter Client ID, client
secret, tokens or manually copied URLs in ordinary sign-in.

## Browser boundaries

Navigation and callback completion have different rules. Ordinary fragments on
verified HTTPS origins are allowed; OAuth-result fragments require strict return
handling. The trusted origin list is finite and exact: no wildcard/substring
host matching, arbitrary ports, HTTP downgrade or automatic identity fallback.
Add an origin only after confirming its provider role.

An absent WebKit frame name does not prove main-frame context. Unknown/subframe
rejections remain blocked without automatically destroying the whole attempt;
a callback in a nested frame cannot complete authorization. `about:blank` is a
service document, not an authenticated provider origin. Cancellation and the
first rejection retain their original cause despite subsequent cancelled-load
errors. Keep TLS and WebKit's sandbox enabled.

Do not inspect password forms/DOM, inject JavaScript, import another application's
profile or persist browser cookies. Optional navigation diagnostics contain one
fixed event/reason, verified frame context, and safe normalized origin metadata.
Never emit full URLs, paths, query/fragment values, phone/QR input, state, cookies,
OAuth results or provider response bodies.

## Persistence and logout

Only a Music-validated token may be remembered through native Secret Service or
the supported Secret Portal plus an authenticated encrypted token envelope.
Discovery, restore/validation, writes and deletion run off GTK's thread. Missing
or unavailable protected storage permits memory-only sign-in, with no plaintext
fallback. Cookies remain ephemeral. See [ADR-0018](adr/0018-persistent-session-and-settings.md).

Cancellation/session replacement invalidates pending browser, token and Music
results. Normal close can retain a confirmed stored session. Explicit logout
stops audio, clears personal views/cache and verifies credential deletion; an
unresolved cleanup remains visible and retryable. Turning remembering off deletes
the stored token while the in-memory session can continue. Local deletion does
not revoke a server grant. Refresh and remote revocation are not implemented.

## Provider identity and standards

The [Cassette provenance](../packaging/third-party-licenses/cassette-auth/PROVENANCE.md)
pins the adapted GPL sources and copyright. Karin adds strict return/state,
ephemeral storage, cancellation and Music validation; it does not copy upstream
substring matching, persistent cookie databases, URL logging, Android Music
identity headers or close-before-validation behavior.

The default native/Flatpak binary includes the published registration's
`client_id` and `client_secret` fields through `include_str!` in
[`CassetteProvider`](../src/auth/cassette.rs). They match the pinned public
Cassette source; they are extractable application configuration, not confidential
native-client credentials or a Karin-owned registration. The actual registration
holder and their authority to allow Karin's reuse are **not established** by that
source attribution. No values or account credentials belong in this review.

[RFC 8252 §8.12](https://www.rfc-editor.org/rfc/rfc8252.html#section-8.12) rejects
embedded native OAuth agents. Yandex's documented [mobile LoginSDK](https://yandex.ru/dev/id/doc/ru/mobileauthsdk/about)
WebView support does not establish standalone Linux WebKit permission or a
blanket provider ban. Express registration-reuse/public-distribution approval
remains unresolved independently of working private sessions and GPL reuse.
See [LICENSING.md](LICENSING.md) and [ADR-0011](adr/0011-cassette-session-auth-prototype.md).

## Integration review, 2026-11-22

The [registration guide](https://yandex.ru/dev/id/doc/ru/register-client) requires
registration and accurate application information. The
[user-authorization guide](https://yandex.ru/dev/id/doc/ru/register-auth) describes
registered platforms, consent identity and matching redirect addresses; it does
not confirm Karin's Music endpoint grant or Linux embedded-flow eligibility.
The owner's Karin registration, successful PKCE/token/Yandex ID binding and
**both** generic Music 403 results are established separate facts, including the
[TUN on/off result](https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6470853664).
Their cause remains unknown. No repeat account experiment or new endpoint probe
is required to restate them.

The current [Yandex ID API terms](https://yandex.com/legal/authid_api/en/), published
21.08.2026, explicitly make other-service APIs subject to their own terms
(§2.5.1) and possible additional access requirements (§2.5.2). §4.4 restricts
obtaining/providing/transferring an API Key for/to third parties. How those
conditions apply to the published shared registration and a distributed native
client needs provider clarification; successful token issuance is no answer.
The [licensing review](LICENSING.md#integration-and-public-distribution-gate)
separates this from GPL and service-content rights.

The existing `AuthProvider` boundary isolates registration/grant acquisition from
GTK and identity-neutral `MusicService`; browser mode and opaque attempts are
explicit. Only Music validation admits a session. There is no automatic provider
fallback. A provider-approved Karin registration would require the approved
grant, exact registered callback/browser policy and Music access conditions,
then local validation through this boundary. Merely replacing a Client ID, or
embedding a new private client secret, would not establish those contracts.
The working provider and separate opt-in PKCE diagnostic remain unchanged.

Existing local fixtures cover expired/invalidated worker tokens, delayed account
validation after logout, newer-session isolation, typed HTTP 401 versus generic
403, and cancelled/late protected-storage writes. A current-session 401 triggers
local logout and credential deletion with an explicit failure state; a failed restore with generic 403
does not invent revocation or a specific entitlement diagnosis. Network failures
remain retryable. These are local rejection/cleanup contracts, not a tested
remote-revocation API; refresh and remote revocation remain absent.

The [completed Russian request](../packaging/release/YANDEX_REQUEST_RU.md) is ready
for the owner through the documented official Yandex ID feedback form, requesting
routing to Music and the appropriate rights team. It has **not been sent**.
[Issue #5](https://github.com/tracerxbrhd/karin/issues/5) remains open until an
applicable integration basis, its conditions and working permitted Music access
are established. Engineering readiness does not establish that external decision.
