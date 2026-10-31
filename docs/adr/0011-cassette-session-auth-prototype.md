<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0011: Cassette-compatible browser-session authentication prototype

Status: Accepted for implementation and consented local Fedora validation; live acceptance pending
Date: 2026-10-29
Supersedes: ADR-0010's requirement to keep all real providers blocked pending a Karin-owned registration

## Context

The owner corrected PR #8's functional-prototype scope: Karin need not use its own
OAuth registration, and must implement the published working Cassette chain.
The normal journey remains `Войти` → actual Yandex login → automatic session/token
exchange → successful Music account validation → real library and search.
No developer credentials, application registration, Device Flow code copying,
pasted tokens or manual redirect transfer belong in this journey.

Karin's own-client PKCE/ID 200 followed by Music 403 is an observed result, not a
suitable default login route. Cassette v0.2.4 and inspected main instead use an
implicit authorization request, their published shared registration and a
cookie-backed `sessionid` exchange at official Yandex OAuth. This is a materially
different grant/identity combination; it does not explain the earlier generic 403.
See [immutable source references](../AUTHENTICATION_REVIEW.md).

## Decision

Enable `cassette-auth` by default, depending on `auth-webkit`, and explicitly in
the existing Flatpak prototype. Keep `AuthProvider` separate from GTK and the
unchanged identity-neutral `MusicService`. The provider uses the same published
application configuration as the pinned Cassette implementation, transparently
attributed under GPL-3.0-or-later. No official-client Music request header,
stream-signing key, DRM implementation or unrelated client identity is imported.

The auth dialog owns a fresh ephemeral WebKitGTK6 session. It loads real Yandex
HTTPS login with a cryptographically random state. Return matching uses exact
parsed HTTPS Music origin/path and the matching returned state, not Cassette's
substring check. Main-document completion is verified separately from subframe
navigation. State is single-use and attempts expire and cancel. Do not claim
S256 PKCE for this implicit/sessionid contract; the independent own-client
code/PKCE diagnostic remains unchanged.

After a valid return, asynchronously retrieve only cookies applicable to the
fixed official OAuth token URI from this session. Keep them in a bounded,
redacted, zeroizing in-memory payload; never read passwords/DOM, inject JavaScript,
import another application's profile, enumerate a persistent cookie store or
write cookies to disk. Exchange on a worker with fixed endpoints, TLS checking,
no redirects, bounded response/timeouts and safe allowlisted errors. Cookies are
never supplied to Music API. The dialog remains open until MusicService accepts
the account; token issuance or a Yandex page alone cannot complete sign-in.
Then independently load liked tracks and personal playlists through existing code;
playlist contents, search, detail views and queue retain their current behavior.

Cancellation/logout invalidates late results, drops token/session state and clears
native error rows. No persistent sessions, refresh, remote revocation or verified
Secret Service writes are claimed. GStreamer remains independent; real Music
streams and audio entitlement remain a separate unresolved gate.

## Permission, uncertainty and errors

These are separate categories:

- **Established source-license terms:** inspected Cassette auth files grant
  GPL-3.0-or-later reuse subject to notices/corresponding source. Preserve the
  upstream copyright, full license and adaptation provenance. Published shared
  application configuration is not a user's token, password or session secret.
- **Unresolved service/identity conditions:** source licensing does not grant
  rights to the OAuth registration, Yandex API/content or trademarks. Explicit
  approval for Karin's reuse/distribution and Linux WebKit has not been obtained.
  Do not describe this uncertainty as a confirmed service ban, approved public
  release, or a technical reason to replace this prototype with a blocked stub.
  RFC 8252 rejects embedded native OAuth; Yandex mobile LoginSDK WebView support
  does not establish Linux eligibility. Record this discrepancy explicitly.
- **Technical outcomes:** return/state mismatch, missing OAuth-session cookies,
  cookie-read failure, `/token` HTTP/category, unsupported token response and
  Music account/library/search HTTP/category are concrete failed stages. Do not
  diagnose unknown failures as missing rights/scopes/subscription or change
  region, identity or mechanism automatically to hide a refusal.

The consent page can name the upstream shared registration rather than Karin's
own registration. The UI and documentation must not misrepresent that identity.
A successful prototype test does not settle public distribution conditions.

## Validation and consequences

Automated tests use synthetic local HTTP/WebKit fixtures only. Smoke/capture modes
must explicitly avoid the live provider; building the normal login does not
permit real account use in Cloud/CI. Check stable and exact Rust 1.98.1, default,
all-feature and no-feature builds, native dialog/browser cancellation and sandbox
lifecycle. Preserve GTK Rust 0.10, WebKit 0.5 and the existing GNOME 50 SDK/runtime.

The [single Fedora acceptance procedure](../BUILDING.md#music-sign-in-and-fedora-acceptance)
checks actual login, the session grant, Music account validation, real liked
tracks/playlists and search. Until the owner performs it, the working live path
is **unverified**, and fixture success must not be called finished authentication.
Record only revision/build and coarse stage results; never publish personal
screens, cookies, tokens, parameterized URLs, headers or raw provider responses.
[VALIDATION.md](../VALIDATION.md) distinguishes executed checks from this pending
real-account acceptance. Public release conditions remain a separate decision.
