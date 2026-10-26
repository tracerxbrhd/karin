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
application. It does not prove code issuance, token exchange, endpoint acceptance,
subscription entitlement or permitted stream delivery.

The official [manual code flow](https://yandex.ru/dev/id/doc/ru/codes/screen-code)
and [authorization/code exchange](https://yandex.ru/dev/id/doc/ru/codes/code-url)
document S256 PKCE and a secretless initial exchange with `code_verifier`.
They support an owner-local experiment without borrowing an application's
identity or putting a confidential Client secret into a native binary.

The verification page displays a code for manual entry, but its documentation
does not promise a displayed/returned state in that presentation. Generic callback
responses document code/error plus state. Consequently code-only entry cannot
honestly establish a successful state roundtrip. A production callback remains
a separate design and acceptance requirement.

## Decision

Implement `karin --oauth-diagnostic` as an explicit local terminal experiment,
separate from the GUI, anonymous status service and independent GStreamer player.
Read Karin's own Client ID and the one-time authorization code only through
bounded, hidden controlling-terminal input. Never consume credentials from
stdin, a pipe or redirection; refuse a missing controlling terminal before
browser or network use. Never request a Client secret, password, pasted OAuth token,
browser cookie or another application's registration. Do not read credential
arguments, `.env` files or persistent token stores.

Generate fresh cryptographically strong verifier/state values for each run and
use only S256. Launch the authorization URL through the Linux system-browser
handler, without WebKit or printing the URL. Use the exact registered development
verification redirect. State is sent; the code-only mode reports that returned
state is not observed rather than comparing state to itself or claiming callback
validation. PKCE binds the exchanged code to the diagnostic's verifier. This
limited experiment must not be exposed as application sign-in.

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
4. One fixed read-only catalog request returns its own HTTP/result category.

A present token-response scope subset can identify missing requested grants.
The documented `scope` field is optional and is returned for a smaller grant;
its absence is not an independently introspected list of effective permissions.
Neither ID success nor scope labels establish Music authorization. A successful
catalog response would not establish subscriber entitlements or stream rights.

Automated tests use local HTTP fixtures and synthetic credentials only. They
exercise successful and negative protocol, parsing and redaction paths without
a real account, browser consent or provider requests. A protocol-level parser
checks a complete returned redirect and rejects missing/mismatched state and
provider denial. That tested parser does not establish that the manual page
actually returns state, and the CLI does not ask the user to paste a URL. Do not send real local
credentials to Codex Cloud, CI, GitHub or support logs.

## Consequences

The owner can conduct the next authorized experiment locally without a token
paste shortcut, Client secret distribution, fake signed-in UI or stream request.
Executed evidence must distinguish registration, code/token issuance, ID-client
binding, Music identity and catalog response. Until the owner runs the diagnostic
on Fedora, none of the latter results is claimed as working.

The GUI retains its unavailable Music-authentication capability. There is no
refresh, remote revocation, Secret Service write, persistent account session,
stream retrieval, DRM workaround or expanded Flatpak permission. Dropping tokens
at process exit does not revoke Yandex's server grant. Browser storage/history
belongs to the external browser and must not be published.

A production Linux callback, validated returned state, public-client token
lifecycle, verified Secret Service integration, service-policy/distribution
clarification and independent stream entitlement remain future gates. GPL and
scope selection do not override Yandex Music terms. Current Rust 1.98.1 and native
GTK/Libadwaita/GStreamer/Flatpak SDK baselines are preserved.

See [API feasibility](../API_FEASIBILITY.md), [local instructions](../BUILDING.md#local-oauth-diagnostic-gate-b1)
and [validation](../VALIDATION.md) for dated evidence, commands and pending results.
