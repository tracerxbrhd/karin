<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0037: Independent native OAuth

**Status** Draft pending Music access and Fedora acceptance.

## Functional specification

- Run `karin --independent-auth login`, `restore` or `logout` before GTK starts.
  Login uses a Karin-owned user-authorization registration and opens the system
  browser. Yandex credentials and cookies stay in the browser. The existing
  desktop provider and the owner-tested `--oauth-diagnostic` each have their
  own entry point. The new command validates Music account access from the
  terminal.
- Bind only `127.0.0.1` on the explicitly registered port before launching the
  browser. Use a fixed port from 1024 through 65535 and the exact
  `http://127.0.0.1:<port>/oauth/callback` URI. Accept one matching response for a
  fresh S256 verifier and state, including state on OAuth refusal. Bound request
  size, wait time and cancellation. The browser receives a generic result that
  discloses no credentials. [RFC 8252 §7.3](https://www.rfc-editor.org/rfc/rfc8252.html#section-7.3) · [RFC 7636 §4](https://www.rfc-editor.org/rfc/rfc7636.html#section-4)
- Request `login:info`, `music:api-public` and `music:major-public-api` for
  the account checks. Reject a returned scope set that omits any of them.
  An omitted scope field still requires independent ID and Music checks.
- Exchange the opaque code once via the official HTTPS token endpoint. Send
  `grant_type=authorization_code`, the code, Client ID and verifier in the form.
  Use the documented initial PKCE exchange without a client secret. Keep
  redirects disabled on the token and identity requests, bound the work and
  redact URLs, OAuth data, account details and original response bodies.
  [Yandex authorization and exchange](https://yandex.ru/dev/id/doc/ru/codes/code-url)
- Verify the token's registration and user through Yandex ID, then call the
  existing `MusicService::account` and check that the authenticated account
  matches the ID user. Registration mismatch, refusal, missing account,
  expiry, cancellation or a network/response error blocks the current
  operation. Use typed outcomes and report successful Music validation only
  after both checks. [Yandex ID user information](https://yandex.ru/dev/id/doc/ru/user-information)
- Cancellation closes the callback listener and fences subsequent results.
  A blocking request already in progress remains subject to its finite network
  timeout. No response authorizes a session past the attempt's cutoff.
- Use existing Secret Service and Secret Portal backends in a separate
  independent-credential scope. Save only after validation and only on
  explicit `login --remember`. Restore requires the same Client ID and new
  ID and Music checks. Local logout deletes this scope. Serialize independent
  credential operations. Protected writes require a durable fence until the
  result is confirmed. An interrupted write or uncertain deletion requires
  `logout` again before restoration can resume. Expiry requires a fresh authorization
  attempt. Secretless refresh remains unconfirmed. The user can revoke the
  grant separately in Yandex ID settings.
  [Token refresh](https://yandex.ru/dev/id/doc/ru/tokens/refresh-client) · [Logout and revocation](https://yandex.ru/dev/id/doc/ru/tokens/token-invalidate)

## Provenance and remaining provider gate

The design sources are public OAuth standards, Yandex ID documentation, Karin's
independently authored PKCE diagnostic and its existing account and protected
storage boundaries. The implementation took place in the Karin repository
under normal development conditions, with access to its history.
[Earlier Karin diagnostic](https://github.com/tracerxbrhd/karin/pull/6)

Yandex documents an exact redirect scheme, host, port and path for its listed
Web, iOS and Android platforms. Acceptance of the registered native loopback
URI remains to be verified for Karin. The earlier Yandex-hosted verification
page was used for the manual diagnostic. The owner's own multiuser OAuth
registration offered Music and Login scopes, and the code exchange and Yandex
ID check succeeded. Music `/account/status` and `/account/about` returned
generic HTTP 403. The observed results leave the cause and available Music
grant unresolved. Yandex ID API terms §2.5.1–2.5.2 allow separate service
conditions and access requirements. Obtain the applicable registration,
endpoint, streaming and distribution terms before switching the default.
[Registration](https://yandex.ru/dev/id/doc/ru/register-auth) · [Owner's test](https://github.com/tracerxbrhd/karin/issues/5#issuecomment-6470853664) · [Yandex ID API terms](https://yandex.com/legal/authid_api/en/)

## Fedora acceptance and 0.8.x migration

1. With an authorized Karin-owned registration and confirmed Music grant, use
   its exact registered IPv4 callback port. On Fedora verify the revision with
   `--version`, then run the matching Native package and the installed Flatpak
   from a terminal. Verify that a browser denial or Ctrl-C exits cleanly. Enter
   the Client ID and registered port at the login prompts. Restore prompts for
   the same Client ID, while logout only operates on local storage. Share only
   sanitized result categories. [Build and packaging commands](../BUILDING.md)

   ```sh
   ./run-karin --independent-auth login
   ./run-karin --independent-auth login --remember
   ./run-karin --independent-auth restore
   ./run-karin --independent-auth logout
   ./run-karin --independent-auth restore
   ```

   For Flatpak replace `./run-karin` with
   `flatpak run io.github.tracerxbrhd.Karin`. Verify absence after logout. If
   Music returns 403 after successful ID validation, record that result in
   [#5](https://github.com/tracerxbrhd/karin/issues/5) and stop the independent
   login checks. Cloud fixtures cover protocol and failure paths only.
2. Test the ordinary application separately through first launch, sign-in,
   restart and logout, including search, library, playlists, playback, My
   Wave, queue, MPRIS, cache and a recoverable network failure. This exercises
   the current desktop provider.
3. A future desktop switch requires fresh consent under the Karin registration
   and successful Music validation before storing its token. Keep 0.8.x
   credentials and account data isolated during verification. Retire old
   credentials through existing protected logout and forgetting after the new
   desktop path is accepted. Admit only credentials issued by the Karin
   registration. Provider-side grant removal remains the owner's choice at the
   [Yandex ID access page](https://id.yandex.ru/personal/data-access).
