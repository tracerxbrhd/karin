<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0010: Single-purpose sign-in with an explicit Music-provider gate

Status: Historical; provider-activation decision superseded by [ADR-0011](0011-cassette-session-auth-prototype.md)
Date: 2026-10-29
Supersedes: ADR-0009's developer-credential/Device Flow GUI

The record below preserves the earlier own-registration-only scope and its
blocked result. The owner's subsequent correction permits the Cassette-compatible
functional prototype without a Karin-owned registration. ADR-0011 activates that
scoped provider while retaining the single-purpose UI and Music validation gate;
this record is not the current build/run instruction.

## Context

The owner's [PR #8 correction](https://github.com/tracerxbrhd/karin/pull/8#issuecomment-6486318822)
requires `Войти` → focused Yandex sign-in → automatic dialog completion after
Music validation → the person's actual library/search. End users must never
configure OAuth applications or copy codes, tokens or callback URLs. WebKitGTK6
is explicitly permitted for this authentication window only; Karin's application
interface remains native Rust/GTK4/Libadwaita/GStreamer.

[Source review](../AUTHENTICATION_REVIEW.md) found that Cassette's visible
WebKit flow is coupled to persistent cookies, a sessionid grant and an existing
hardcoded application identity/secret. That does not authorize its use by Karin.
Karin's own-client secretless PKCE and Yandex ID succeeded, but Music status and
about returned generic HTTP 403. The exact provider reason is unknown. The
working yamu experiment does not isolate flow from registration/grant identity.

RFC 8252 rejects embedded native OAuth user agents. Yandex's mobile LoginSDK
explicitly supports WebView in its iOS/Android scope; standalone Linux WebKitGTK
permission remains unestablished. Official code docs allow initial secretless
PKCE, while registration matches callback scheme/host/port/path. No supported
automatic Linux callback or approved distributable Music identity is configured.

## Decision

Replace the developer login form with one native `Adw.Dialog`. Every GUI mode
omits Client ID/secret fields, device-code instructions, manual callbacks and
token entry. Keep loading, retry/cancel and fixed native messages, and clear stale
API errors when a session ends or a new attempt starts. The old Device Flow
module may remain feature-gated for library-level checks, without a GUI entry.

Use a dedicated `AuthProvider`/session boundary that separates provider login,
token issuance and validated Music account access. The public provider returns
an explicit integration-unavailable result: no real provider is activated and no
identity-only Yandex page is launched. Navigation, cookies, ID success or token
issuance alone must never close the dialog as authenticated. A future provider
must validate Music before completion, then fetch actual library data and enable
search through the existing identity-neutral `MusicService`.

Prepare generic fresh state/S256 PKCE and automatic fixed-loopback callback
infrastructure with exact parsed URI/state checks, bounded expiry and cancellation.
Tests use synthetic loopback fixtures only. Infrastructure does not establish
registration or provider acceptance; never assume RFC dynamic-port behavior when
Yandex documents matching registered ports.

Offer optional `auth-webkit` using `webkit6` 0.5 with Karin's GTK Rust 0.10 series,
limited to an isolated ephemeral authentication view. No JavaScript injection,
cookie extraction, persistent browser data or credential-bearing logging is
allowed. Origin/callback decisions use parsed exact policy, never substring
matching. Native packages are Fedora `webkitgtk6.0-devel` or Debian/Ubuntu
`libwebkitgtk-6.0-dev`; GNOME 50 supplies the runtime. Default/public Flatpak
continues excluding optional auth features. Dependency availability is separate
from authorization.

Before activating a provider, establish Karin's permitted Music identity/grant,
supported registered automatic callback, browser-path eligibility and applicable
token lifecycle/distribution conditions. If embedded authentication is unsupported
or unacceptable, keep the native dialog and explain a system-browser button with
automatic return. That fallback requires the same approved identity/callback; it
is not configured today. Document the discrepancy from the requested embedded
experience explicitly. Do not bundle another application's identity/secret or ask
ordinary users to register applications.

Keep the existing developer-only own-client PKCE diagnostic unchanged and
separate. Its hidden manual URL input is neither a normal login path nor an
automatic callback acceptance test. Session tokens remain memory-only; persistent
Secret Service, refresh and remote revocation need separate verification.
Playback remains blocked pending a permitted real stream-delivery contract.

## Consequences and acceptance

The current deliverable is a truthful blocked sign-in dialog and reusable
authentication infrastructure, not achieved Cassette-style live login. The
observable provider evidence remains generic Music 403 from the owner's prior
test; an unconfigured provider must not present a newly observed live rejection.

Locked stable and exact Rust 1.98.1 checks, optional-WebKit compatibility, synthetic
protocol/callback tests and clean GTK captures can validate implementation within
their scope. They cannot prove live consent, Music rights, Fedora/KDE/Wayland
visual acceptance or the exact Flatpak artifact. Once provider prerequisites are
resolved, consented local acceptance must cover first/existing login, callback,
token/Music validation, real library/search, denial/timeout/cancel, stale work,
logout/restart, focus, themes, sizes and HiDPI. Report executed checks separately
in [VALIDATION.md](../VALIDATION.md); preserve the independent audio gate.
