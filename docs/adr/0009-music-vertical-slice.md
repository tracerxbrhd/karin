<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0009: Identity-neutral Music reads and an isolated development session

Status: Accepted for the scoped Issue #7 implementation
Date: 2026-10-28

## Context

The owner verified Karin-owned PKCE and Yandex ID, but both legacy Music account
methods returned generic HTTP 403 with TUN on/off. Separately, unmodified yamu
Device Flow accessed account/search/track/album data on Fedora. The same token
worked at status without the Android-client header. That observation narrows a
transport hypothesis; it does not authorize distributing another application's
OAuth identity or establish a permitted service-stream contract.

The [pinned Rust-client review](../RUST_API_CLIENT_REVIEW.md) found useful typed
endpoint/model designs but incompatible default identities, raw errors, token
formatting and transport policies. The [delivery review](../STREAM_DELIVERY_REVIEW.md)
found no confirmed unsigned, identity-neutral Music source: the inspected path
requires extracted signing material and another application's request identity.

## Decision

Keep one package and the approved GTK/Libadwaita/GStreamer/reqwest/serde stack.
Implement a small typed `MusicService` using reviewed endpoint/schema knowledge,
without adding yamu/yamuse, copying credentials/signing code or enabling their
default headers. Separate session/token ownership, service reads, native
presentation and the existing playback worker. Network work runs off GTK's main
thread, with bounded responses, timeouts, cancellation/stale-result handling and
fixed error categories; no raw provider body/message becomes a log or error.

Implement read-only account, liked-track and personal-playlist reads, playlist
contents, paged search and track/album details. Rendering uses real returned data,
with loading/error/empty states. Keep a bounded queue of selected track metadata.
Do not infer playback entitlement from metadata or invent a release catalog.
Mutations, likes/unlikes and recommendations wait for endpoint/permission checks.

Add Device Flow only behind the `experimental-device-auth` Cargo feature **and**
the explicit `--experimental-music` runtime switch. No OAuth client identity or
secret is embedded. A local native dialog accepts only application credentials
the owner is entitled to test, in memory, then shows the provider's code/lifetime
and system-browser/copy actions. No password, cookie, token-paste, file or
environment credential shortcut exists. Do not extract borrowed credentials as
an installation procedure or represent this experiment as approved distribution.
The public/default build and Flatpak exclude this flow.

Tokens and app credentials are memory-only. Logout/close clears local session and
data and invalidates pending UI results. A bounded in-flight job can retain a
temporary token clone until it ends; local disposal is not server revocation or
guaranteed erasure of HTTP/TLS/OS copies. Do not add persistent credentials until
a verified Secret Service lifecycle, refresh/revocation and authorized public
client path exist. Presence detection alone does not satisfy that gate.

Retain the independently verified own-client PKCE diagnostic unchanged. It is
neither removed nor promoted to Music authorization. The experimental path does
not silently retry refused own-client grants under another identity.

Keep Music playback visibly unavailable. Do not add stream URL negotiation,
extracted signing keys, encrypted-stream processing, media downloads or MPRIS
playback state without an independently established permitted source. Existing
local-file and synthetic diagnostics remain labeled developer tools and cannot
stand in for playing a selected service track.

## Consequences and acceptance

The implemented UI-to-session-to-service slice can be exercised locally without
shipping an official-app identity, introducing a new runtime or pretending that
the Music audio gate is solved. Application credentials are not included in
source, Cloud/CI, logs or screenshots. No additional Flatpak/keyring/filesystem
permission is needed; its offline release explicitly disables default features.

CI covers public and experimental source configurations on stable and exact Rust
1.98.1, local HTTP fixtures, native cleanup and screenshots. These checks do not
establish provider acceptance, first-party permissions, audible Music, Fedora
visual quality or the exact sandboxed artifact. Fresh owner-local acceptance,
public identity/distribution approval, persistent sessions and permitted audio
delivery remain open gates. See [validation](../VALIDATION.md),
[API facts](../API_FEASIBILITY.md) and
[Fedora instructions](../BUILDING.md#experimental-music-session-and-fedora-acceptance).
