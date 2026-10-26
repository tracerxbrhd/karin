<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0006: Gate A separates Music authorization from public OAuth plumbing

Status: Accepted
Date: 2026-10-25

Update, 2026-10-26: the registration uncertainty in this dated decision is
partially superseded by [ADR-0008](0008-local-oauth-pkce-diagnostic.md). The owner
confirmed a Karin-owned multiuser application with two Music scopes. That fact
does not establish code/token issuance, Music API access, stream delivery or
distribution permission; those distinctions and the unavailable GUI remain.

## Context

Karin needs authorization for Yandex Music catalog, personal library and audio
delivery, rather than an identity-only Yandex ID login. Issue #3 requires an
evidence-backed decision while the independent GStreamer playback engine proceeds.
The official-document review and scoped request evidence are recorded in
[API_FEASIBILITY.md](../API_FEASIBILITY.md), with URLs, dates, methods and limits.

The official code-flow documentation describes S256 PKCE and explicitly permits
the initial token exchange without `client_secret` when `code_verifier` is used.
Thus generic public-client OAuth is not rejected as technically impossible.
However, the reviewed public sources do not establish Music-specific grants or
an authorized stream-delivery contract for a Karin-owned registration. No owner
client, account consent or Music entitlement has been validated.

Registration distinguishes user-authorization apps from developer-only API apps.
It documents Web/iOS/Android platforms, but no tested Linux callback for Karin.
Device, refresh and remote device-revocation documentation describes client
authentication without an equivalent explicit secretless exemption. These are
future lifecycle questions, not reasons to borrow another application's client.

The Music/Plus terms and ID program requirements impose service/content,
territorial, subscription and program restrictions. Their applicability to
Karin's intended integration and distribution remains unresolved; neither GPL
licensing nor a paid subscription establishes service permission.

## Decision

| Capability | Verdict | Resulting behavior |
| --- | --- | --- |
| General browser-code / S256 PKCE mechanism | Documented but unverified | Candidate only; no identity-only sign-in UI |
| Device flow and public-client token lifecycle | Documented but unverified | No flow until eligibility and secretless lifecycle are confirmed for Karin |
| Karin-owned Music catalog/library/playback authorization | Blocked | Return `AuthenticationUnavailable` with `MusicGrantNotEstablished` |
| Music stream resolution and playback | Blocked | No live resolver, service playback control or authenticated view |
| Anonymous HTTP observations | Verified only in their stated request/environment scope | Preserve access restrictions and distinguish API reachability from authorization |
| Passwords, borrowed clients, browser cookies, token-paste and bypasses | Unsupported | Do not add or request them |
| Independent local-media GStreamer playback | Separate workstream | Complete and verify it without claiming Music integration |

Do not register a client or request account login on the owner's behalf. The next
owner action is to obtain official confirmation of Music-specific rights,
integration/distribution permission, registration category and public Linux
callback/lifecycle. Only then can the owner register Karin and consent to a real
Music-specific end-to-end test. Do not solicit confidential secrets or tokens in
chat. If an approved flow becomes available, use a system-browser or verified
device flow, robust expiry/cancellation/error handling and Secret Service.

Keep a typed future permitted-stream boundary, but fail closed before resolution.
No test stream, locally generated audio or successful Yandex ID request may be
used as a substitute for an authorized Yandex Music stream result.

## Consequences

Gate A has an explicit no-go decision for the current Music integration, rather
than a permanently disabled promise or invented signed-in state. This is not a
claim that every third-party integration is universally impossible or prohibited.
The decision can be revisited with new official permission and actual consented
evidence. Anonymous 451 responses remain restrictions with a limited cloud/request
scope; they do not trigger alternate countries, identities or delivery routes.

Playback-controller correctness, local-media tests and UI diagnostics remain
independently deliverable. Their acceptance must not be presented as Music login,
subscription, stream-delivery or service-policy approval.
