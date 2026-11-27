<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Contributor instructions

Karin is an independent native Linux client for Yandex Music. Preserve the
working application; add only confirmed behavior within the requested scope.
Read [ARCHITECTURE](docs/ARCHITECTURE.md) before changing a contract. Record the
decision and its rationale in the relevant section.

## Architecture and safety

- Keep Rust 1.98 compatibility (test exact 1.98.1 and stable), GTK4/Libadwaita,
  GStreamer, reqwest/serde, GIO D-Bus/MPRIS and Cargo/Flatpak. Do not casually
  raise dependency/runtime requirements or use `--ignore-rust-version`.
- Keep one MusicService, MusicQueue and PlaybackController/GStreamer engine.
  Separate widgets, API, sessions, cache and platform adapters. Blocking I/O and
  parsing belong off GTK; bounded work, cancellation, weak owners and account/
  request/source generations must prevent late or reentrant state updates.
- Preserve ordinary sign-in with the attributed Cassette-compatible provider.
  WebKitGTK6 is sign-in only: ephemeral session, exact trusted navigation,
  callback and state checks. Never read passwords/DOM, import another session,
  persist cookies, weaken TLS or disable the WebKit sandbox.
- Admit and save tokens only after Music validation. Use protected storage or
  memory-only login, never plaintext fallback. Preserve forgetting barriers,
  cancellation, logout cleanup and honest Retry; do not invent refresh/revocation.
- Never send real credentials/account content to Cloud/CI/GitHub. Use original
  synthetic fixtures. Logs, D-Bus and errors must exclude secrets, personal data,
  cookies and signed media URLs. Private captures require publication permission.
- Playback entitlement, HTTP cacheability and distribution rights are distinct.
  Preserve the resolver's origin/path/protection checks; no DRM bypass, extracted
  keys, arbitrary media URIs or OAuth/cookies forwarded to CDNs.
- Keep the account-scoped shared cache budget, complete-file integrity, HTTP
  freshness/revalidation, atomic publication, LRU and Clear/Disable fences.
  Cache failure must preserve admitted playback; no disk work under GTK locks.
- Preserve opaque queue occurrences, duplicates, source provenance and actual
  MPRIS state. Search/sort/server edits cannot rewrite active playback. Writes
  require independent readback; ambiguous POSTs are never replayed. Wave uses
  the existing bounded feedback ledger, not a second player or persisted radio.
- Preserve pinned-panel intent, actual-position Lyrics and reduced-motion behavior.
  External lyrics require consent; text remains memory-only. Do not introduce
  fake content, inert controls or unavailable styles presented as functional.
- Preserve [GPL, licenses and upstream provenance](docs/LICENSING.md). Source
  licensing does not establish OAuth-registration, service or content rights.

## Development and handoff

- Follow the user's branch/PR and publication authorization. Use normal commits;
  no force-push, merge, tag, Release or store submission without authorization.
- Keep docs concise; audit consumers before deletion. Retain required notices,
  build inputs and recoverable history rather than adding archival report files.
- Follow [BUILDING](docs/BUILDING.md#development-checks): formatting, strict Clippy,
  relevant default/all/minimal tests, locked release and affected native/package
  checks. Never weaken sandbox, geometry, motion or timing assertions for a pass.
- UI changes need actual native input, structure and visual inspection at relevant
  sizes, themes, HiDPI and accessibility settings. Ignored registration is no PASS.
- Verify final-head CI, links, licenses, source/ELF identity and packaging. Report
  executed checks and relevant limits. Distinguish synthetic fixtures from
  live-service, installed-package and physical-desktop acceptance.
