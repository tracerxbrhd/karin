<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0008: Local own-client PKCE experiment before application sign-in

Status: Accepted
Date: 2026-10-26
Partially supersedes: ADR-0006's unresolved registration prerequisite

## Context

The owner confirmed registration of a Karin-owned application of type
«Для авторизации пользователей», with `music:api-public`,
`music:major-public-api`, `login:info` and `login:avatar`. The consent preview
names Karin and includes both Music permissions. The development redirect is
`https://oauth.yandex.ru/verification_code`. This
[owner evidence](https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6411056234)
resolves the prior uncertainty about selecting Music scopes for a new multiuser
application. Registration alone does not prove code issuance, token exchange,
endpoint acceptance, subscription entitlement or permitted stream delivery.

The official [manual code flow](https://yandex.ru/dev/id/doc/ru/codes/screen-code)
and [authorization/code exchange](https://yandex.ru/dev/id/doc/ru/codes/code-url)
document S256 PKCE and a secretless initial exchange with `code_verifier`.
They support an owner-local experiment without borrowing an application's
identity or putting a confidential Client secret into a native binary.

The [owner's subsequent Fedora report](https://github.com/tracerxbrhd/karin/pull/6#issuecomment-6417990070)
confirms that Yandex displayed Karin consent and redirected to the registered
verification page with an opaque alphanumeric code, echoed `state` and an
additional `cid`. The initial diagnostic wrongly required seven digits and
rejected the code locally before any token POST. This is a Karin validation bug;
the observed browser flow does not establish token or Music API acceptance.

This evidence corrects the initial code-only design and its assumption that
returned state could not be observed. Transfer the full browser redirect through
hidden terminal input and verify it. An automatic production callback remains a
separate design and acceptance requirement.

The owner's later [DIRECT-path repeat](https://github.com/tracerxbrhd/karin/pull/6#issuecomment-6424867774)
confirmed fresh state validation, token HTTP 200 and Yandex ID HTTP 200 with the
matching Karin client and an ID account. Music `/account/status` still returned
HTTP 403; account presence and service availability remained unknown and the
catalog was skipped. An [anonymous curl control](https://github.com/tracerxbrhd/karin/pull/6#issuecomment-6424928210)
returned HTTP 200. Ethernet routing and absent conventional proxy variables
weaken a proxy/TUN explanation but do not prove identical public egress or the
reason for the authenticated denial. OAuth/ID success does not establish Music
access, and no raw error identifier from the actual 403 has been supplied.

## Decision

Implement `karin --oauth-diagnostic` as an explicit local terminal experiment,
separate from the GUI, anonymous status service and independent GStreamer player.
Read Karin's own Client ID and the full returned browser URL only through
bounded, hidden controlling-terminal input. Never consume credentials from
stdin, a pipe or redirection; refuse a missing controlling terminal before
browser or network use. Never request a Client secret, password, pasted OAuth token,
browser cookie or another application's registration. Do not read credential
arguments, `.env` files or persistent token stores.

Generate fresh cryptographically strong verifier/state values for each run and
use only S256. Launch the authorization URL through the Linux system-browser
handler, without WebKit or printing the URL. Use the exact registered development
verification redirect. The user copies the complete browser address-bar URL into
the local hidden redirect prompt. Validate its exact HTTPS origin/path and the
returned state against this attempt before exchanging the extracted code.
Reject ambiguous duplicates, missing response fields, unwanted parameters,
fragments and URL credentials. Accept the observed `cid` without interpreting
it. There is no code-only fallback or other state-validation bypass.

Treat the decoded code as a bounded, nonempty opaque value, rejecting control
characters and oversize without assuming a digit count or alphanumeric format.
Preserve it exactly and form-encode it safely for exchange. The full URL is
bounded to 4096 bytes and the decoded code to 2048 bytes. Neither the URL, code,
state nor verifier is printed, stored, logged or accepted through arguments or
environment variables. Report `ValidatedRedirect` only after a valid returned
state. PKCE independently binds the code to the diagnostic's verifier. Manual
URL transfer is not an automatic callback or application sign-in.

Exchange the one-time code at official Yandex OAuth without `client_secret`.
Keep access/refresh tokens only in memory for this process. Bound HTTP work and
response sizes, retain certificate validation, refuse redirects, redact original
network errors and avoid credential-bearing query parameters. Return typed fixed
error categories and status/boolean facts rather than account JSON or raw bodies.
Do not retry through alternative identities, regions or endpoints on a restriction.

Verify these facts separately and fail closed before later probes when their
prerequisites are not established:

1. A token was issued through the requested initial grant.
2. `login.yandex.ru/info` identifies the locally supplied `client_id` and contains
   an identity. Neither returned ID nor profile/avatar data is displayed.
3. The Music account/status endpoint recognizes an authenticated account, with
   service availability reported independently.
4. The separate fixed `GET https://api.music.yandex.net/account/about` returns
   its own HTTP/error result using the same Karin token. Run it after successful
   ID binding/account verification regardless of status refusal, malformed body,
   network failure or absent account. Accept HTTP 200 only for a JSON object with
   an object `result` and no top-level `error`; discard all fields. This minimal
   envelope does not claim a verified about schema or authenticated Music account.
5. One fixed read-only catalog request returns its own HTTP/result category,
   gated only by valid status with an authenticated Music account. About success
   cannot unlock it after failed status; about failure cannot suppress it after
   valid status.

Status and about retain separate results, labeled `Music /account/status` and
`Music /account/about`. About cannot alter status account/service facts. Preserve
the same five-second connect limit, fifteen-second total deadline, 1 MiB body
ceiling and no-redirect/no-retry policy for both methods. Overall diagnostic
success conservatively requires every planned stage, including about, to pass;
it is still not production authorization or stream-delivery approval.

A present token-response scope subset can identify missing requested grants.
The documented `scope` field is optional and is returned for a smaller grant;
its absence is not an independently introspected list of effective permissions.
Neither ID success nor scope labels establish Music authorization. A successful
catalog response would not establish subscriber entitlements or stream rights.

Classify Music HTTP 403 using the response already received, within the existing
1 MiB body bound and request deadline. Recognize only closed, non-private provider
identifiers in their expected field: currently exact `missing-required-scopes`
in a top-level string `error` or nested `error.name`. Emit a fixed Music-specific
category, never the original string value or surrounding JSON. Do not infer this code from
messages/descriptions, substrings, case variants or unrelated fields. The phrase
`API access restricted` is not an independently established provider identifier.
Unknown, malformed, oversized or unreadable error bodies remain generic
`Forbidden` with HTTP 403. Do not change the Yandex ID error path or promote any
403 to account/service facts. Those facts remain unknown, and catalog probing
stops after a forbidden Music status. Apply the same classification independently
to about; its planned probe still runs after failed status. Classification itself
never issues a follow-up request.

Do not repeat OAuth solely to investigate VPN, change region, route, OAuth
mechanism or application identity, retry a refusal or use another endpoint as an
access workaround. The fixed, independently reported about probe is diagnostic,
not a fallback or permission to reuse refused status data.
The classification narrows a diagnostic category; it does not grant access or
establish the actual cause of the owner's 403 without a recognized result.

Automated tests use local HTTP fixtures and synthetic credentials only. They
exercise successful and negative protocol, parsing and redaction paths without
a real account, browser consent or provider requests. Exercise opaque codes,
full-URL/state success, missing/mismatched state, duplicates, unwanted parameters,
the uninterpreted `cid`, expiry and provider denial. Hidden-input checks verify
that the local URL is not echoed and terminal settings are restored. Do not send
real local credentials to Codex Cloud, CI, GitHub or support logs.

## Consequences

The owner can conduct the next authorized experiment locally without a token
paste shortcut, Client secret distribution, fake signed-in UI or stream request.
Executed evidence must distinguish registration, code/token issuance, ID-client
binding, Music status, the independent about response and catalog response.
Browser consent, code/state return, secretless exchange and ID-client binding are now owner-observed on Fedora.
Music account access remains forbidden in the tested context, with its detailed
provider reason unknown. No live about result has been observed; its result must
remain separate from the owner's historical status denial. The diagnostics use
local fixtures in Cloud/CI; they do not reproduce an account login or establish Music permissions.

The GUI retains its unavailable Music-authentication capability. There is no
refresh, remote revocation, Secret Service write, persistent account session,
stream retrieval, DRM workaround or expanded Flatpak permission. Dropping tokens
at process exit does not revoke Yandex's server grant. Browser storage/history
belongs to the external browser and must not be published.

A production Linux callback, public-client token
lifecycle, verified Secret Service integration, service-policy/distribution
clarification and independent stream entitlement remain future gates. GPL and
scope selection do not override Yandex Music terms. Current Rust 1.98.1 and native
GTK/Libadwaita/GStreamer/Flatpak SDK baselines are preserved.

See [API feasibility](../API_FEASIBILITY.md), [local instructions](../BUILDING.md#local-oauth-diagnostic-gate-b1)
and [validation](../VALIDATION.md) for dated evidence, commands and pending results.
