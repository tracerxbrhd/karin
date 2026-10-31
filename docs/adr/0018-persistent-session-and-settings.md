<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0018: Protected session persistence and durable preferences

Status: Accepted for the consenting v0.3 prototype; executed checks and desktop acceptance are recorded separately
Date: 2026-10-31
Supersedes: ADR-0009/0011's memory-only ordinary session boundary; ADR-0013's current-process-only preferences

## Context

The merged v0.2 baseline has owner-reported native and Flatpak sign-in, Music
playback and KDE Media Widget acceptance. Its token disappears at process exit.
The v0.3 task authorizes remembering a validated session through protected system
storage, while retaining the existing Cassette-compatible provider. It does not
authorize copying a browser profile, persisting WebKit cookies/sessionid, adding
another OAuth identity, inventing refresh or claiming server revocation.

A detectable keyring name or portal version is not proof that a token can be
stored, restored and deleted. Native KDE/GNOME and Flatpak can expose different
backends and prompt behavior. Every unavailable/locked/cancelled/corrupt/unknown
outcome must be explicit without blocking GTK or falling back to plaintext.

## Decision

Keep token ownership, system credential I/O, session lifecycle, preferences and
GTK presentation separate. Use existing GIO/session D-Bus and cryptography rather
than introducing a credential daemon or a second application runtime.

For a native process, use the Secret Service API of the user's configured
keyring, including the standard unlock/prompt lifecycle. This is the system
credential API, distinct from MPRIS: credentials never enter media metadata,
logs or diagnostic traces. Requests and interactive prompts have finite timeouts
and cancellation. A rejected or still-locked store remains a visible failure.
Probe does not itself store a credential or prove a successful lifecycle.
The standard `plain` OpenSession algorithm is local session-bus transport;
the system keyring owns persistence. It is not a plaintext credential file.

For Flatpak, select the Secret Portal path. Retrieve the app-scoped portal
secret through the official file-descriptor/request contract and use
HKDF-SHA256 with AES-256-GCM for an authenticated encrypted OAuth-token envelope
in Karin's private XDG data space. Keep the portal key in memory only. Authenticate version/framing and
reject corrupted, truncated or undecryptable envelopes. Use private directories
and files, atomic replacement and no credential-bearing paths or debug values.
If the portal/backend is absent, denied or broken, keep ordinary sign-in
available for the current process; do not silently add direct Secret Service
access or a plaintext fallback.

Persist only the required access token, and only after independent Music account
validation accepts that current session. Do not persist passwords, browser
cookies, sessionid, OAuth diagnostics, CDN resources or signed addresses. On
startup, load preferences and recover a saved credential off the GTK thread,
then validate it with MusicService before enabling authenticated operations.
A stored token or a previous successful account is not current authorization.
Keep restoring, validated, invalid and storage-unavailable states distinct.
Transient network failure is not evidence that the saved token was revoked.

Retain generation-bound work and serialize credential mutations. A cancelled
restore/save, old login callback or logout must not resurrect an old session or
overwrite a newer preference. Save/delete results are not success merely because
a request was queued. Explicit logout stops the existing player, clears local
session/personal presentation, purges the profile's persistent metadata/cover cache and
requests verified saved-credential absence. Keep replacement login blocked while
active cleanup I/O runs. After it settles unsuccessfully, ordinary memory-only
login may continue with restoration/new persistent writes/cache paused in the
current process. A successfully recorded barrier extends restoration suppression
across restart. If the barrier write itself fails, restart protection and final
forget remain explicitly unconfirmed; do not promise durable suppression from
an in-memory flag. A deletion or cache-clear failure stays visible and
retryable; no final successful logout is claimed while cleanup is unresolved.

The `forget_pending` preference is a durable non-secret restoration barrier.
Persist it before asynchronous forget work and do not restore while it remains
set. Clear it only through the lifecycle's confirmed completion. Preference save
reservations are made before scheduling workers, preventing an older save from
overwriting a newer choice or deletion barrier.
Validated replacement saves also record this barrier before deleting/replacing
an older credential, then clear it using the same settings reservation only
after confirmed storage. An obsolete save cannot clear a newer logout barrier.
A pending cache purge is a separate cleanup obligation; saving a newly validated
memory-only session cannot clear that obligation or its forget barrier.

Remember-login defaults to enabled as a preference and becomes effective only
when protected storage is supported. Disabling it deletes the saved credential
without requiring the current validated in-memory session to stop. Show the
actual saved preference even when a backend is unavailable: permit enabled to
disabled to record forget intent and surface unresolved deletion, but do not
permit enabling until protected storage is supported. Logout and
normal window close have different meanings: close may retain a successfully
remembered login, while logout deliberately forgets it. Neither operation
revokes the remote Yandex grant. No refresh-token flow is introduced.
An already validated required save/forget worker finishes independently of
window presentation, retaining the original provider expiry and an application
hold; close does not cancel a write halfway and silently assume it vanished.
An application/main-context work fence delays a replacement window's settings
and credential I/O until retired durable operations and queued preference merges
finish. Pane visibility still changes immediately during such work; only that
non-secret field is merged afterwards, without overwriting the lifecycle barrier.

Portal forgetting deletes and verifies absence of Karin's encrypted credential
envelope. It does not delete the portal's per-app master key or revoke provider
consent. Native forgetting verifies that Karin's matching credential items are
absent; an unconfirmed system-service result remains an error. A non-secret
pending-CreateItem marker retains ambiguous native write intent: a timed-out
write followed by an empty item query does not alone prove that no late write
can commit. Keep uncertainty/barriers rather than declaring false deletion.
Only exact definitive access-denied/still-locked CreateItem refusals can retire
the marker without a successful write; timeout, cancellation, transport and
unknown replies retain the uncertainty.
Only fixed non-secret transaction state belongs in that marker. Token/key
buffers owned by Rust use zeroizing wrappers; opaque GLib, HTTP-library and OS
copies are not claimed to be universally wiped.

## Preferences and paths

Store non-secret preferences at
`$XDG_CONFIG_HOME/io.github.tracerxbrhd.Karin/preferences.json`, using GLib's XDG
location (already redirected into the app's private config area in Flatpak).
Use schema version 1, a bounded file, a 0700 app directory and a 0600 regular
file with atomic replacement. Reject unsupported values and unsafe file types.
Malformed settings recover through explicit safe-default status rather than
publishing file contents. Recovery disables remembering and retains a durable
forget barrier; defaults must not silently revive an unresolved saved login. File/keyring work belongs on bounded workers.

Credential files are separate, under
`$XDG_DATA_HOME/io.github.tracerxbrhd.Karin/credentials/`: Flatpak uses
`session.enc` for the ciphertext envelope and required portal locator; native
Secret Service uses only `native-write.pending`, the non-secret uncertain-write
tombstone. Native token bytes remain in the system keyring. Neither file is a
plaintext credential fallback or a diagnostic log.

Persist actual appearance, remember-login intent, cache enable/size and track-pane
visibility. Keep transient loading/error/progress state out of preferences.
Storage settings contain no token, account identifier, browser state or service
response. Cache data and credential data occupy separate stores, so clearing
cache cannot remove the session.

## Validation and consequences

Use synthetic D-Bus/HTTP/storage fixtures for successful recovery, corruption,
locked/absent service, prompts/cancellation, timeout, invalid Music token,
transient network failure, disable/forget, late saves/restores and restart
barriers. Inspect ordinary GTK restoring/error/settings states and ensure old
work cannot repaint a replacement session or restart audio.

Actual KWallet, GNOME keyring, Secret Portal/backend and current Flatpak
availability must be tested in their real desktop/sandbox. Source inspection,
D-Bus fixtures and a portal interface version do not pass those gates. The
accepted v0.2 native/Flatpak result does not automatically accept persistence.
Retain the exact MPRIS own-name grant; any additional sandbox grant needs a
specific demonstrated need and separate validation.

This protected local prototype does not settle the reused registration's public
service/distribution conditions. The sign-in contract in ADR-0011 remains intact;
its own-client PKCE diagnostic stays separate and memory-only.

## Inspected platform references

- [Secret Service specification](https://specifications.freedesktop.org/secret-service-spec/latest/) defines the system service/session/item/prompt contract.
- [Secret Portal version 1](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Secret.html) provides an app-scoped master secret through a file descriptor and request response; it is not an OAuth-token API.
- [GNOME 50 libsecret graph](https://github.com/GNOME/gnome-build-meta/blob/0ed65ce92e19a47e63f9359a22e6fdf5a17f75e0/elements/sdk/libsecret.bst) pins libsecret 0.21.8.2 in the inspected source graph. That does not prove its presence in an installed local runtime, and Karin adds no libsecret binding.
- [KWallet portal implementation](https://github.com/KDE/kwallet/blob/1c0422a0e5cfc71cc80285b77d47139b9bf9fb88/src/runtime/ksecretd/kwalletportalsecrets.cpp) implements a 64-byte per-app secret; the same pinned source supplies the standard Secret Service session contract and KDE portal registration. Source support is not proof that a particular Fedora portal backend is installed/running.

The backend review was performed on 2026-10-31. Actual GNOME/KDE/Flatpak
availability and successful save/restore/delete remain separately recorded
execution/owner evidence.
