<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0011: Browser-session authentication

Status: Accepted; current ordinary sign-in contract.

## Decision

Keep **Войти → Яндекс → Music account validation → library** behind AuthProvider, separate
from GTK and identity-neutral MusicService. Default `cassette-auth` enables `auth-webkit`.
Ordinary users never configure OAuth applications or enter Client ID/secret, tokens,
device codes or copied callbacks.

Adapt the published Cassette session flow with its disclosed shared registration:
authorization request, voluntary browser session, then official OAuth `sessionid`
exchange. This grant is not PKCE. The separate owner-local `--oauth-diagnostic` uses
Karin-owned Code/S256 PKCE; it is not a fallback or ordinary sign-in UI. Device Flow has
no GUI entry and is excluded from default/Flatpak builds.

Each attempt owns a fresh ephemeral WebKitGTK6 session, random CSRF state and a ten-minute
lifetime. Completion requires the confirmed main document's exact HTTPS `music.yandex.ru/`
callback and matching single-use state. Reject ambiguous fields; discard returned implicit
tokens/personal fields without display. Ordinary navigation has a finite exact
HTTPS-origin policy, separate from the callback. Verified ordinary fragments are allowed;
OAuth-result fragments require strict return handling. Neither an absent frame name nor a
subframe callback proves main-document completion. Block untrusted auxiliary navigation
without unconditionally destroying the main session. `about:blank` is not a login origin.
Keep the first failure when app-cancelled loading produces secondary errors.

After a valid return, read only this session's cookies applicable to the fixed
`https://oauth.yandex.ru/token` URI into bounded redacted memory. Exchange on a worker
with fixed endpoints, TLS, deadlines, response bounds and no redirects. Cookies never
reach Music API. Token issuance or Yandex ID alone cannot complete the dialog:
MusicService must validate the current Music account first. Cancellation/logout/account
replacement fences browser, exchange and validation results. Current-session HTTP 401
follows local logout/forget; generic 403 proves no particular region, subscription, scope
or revocation cause.

Never inspect password fields/DOM, inject login-reading JavaScript, import other browser
profiles, persist cookies, weaken TLS or disable WebKit sandboxing. Optional navigation
diagnostics contain only fixed event/reason, verified frame context and safe normalized
origin metadata, never URLs/paths/query/fragment, phone/QR data, state, credentials or raw
responses. Protected token persistence is a separate [session
lifecycle](0018-persistent-session-and-settings.md).

## Provenance and limits

Preserve [Cassette attribution and immutable
sources](../../packaging/third-party-licenses/cassette-auth/PROVENANCE.md). Published
registration fields are not Karin-owned credentials; source licensing does not establish
registration-holder authority or public API/content rights. [RFC 8252
§8.12](https://www.rfc-editor.org/rfc/rfc8252.html#section-8.12) rejects embedded native
OAuth agents; mobile SDK WebView support does not establish Linux eligibility. See
[LICENSING](../LICENSING.md) for the unresolved external decision.
