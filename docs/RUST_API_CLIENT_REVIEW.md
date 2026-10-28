<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Gate B1: Rust Music API client review

**Recommendation: continue Karin's own API adapter.** Both candidates can accept
an existing token without running their OAuth flow, but both send an official
Android application identity in API requests. Accepting a Karin-issued token in
a constructor does not establish that Music accepts that token. Neither library
resolves Karin's observed authenticated HTTP 403.

This is a source review, not an adoption or architecture decision. No dependency,
lockfile, OAuth mechanism, application identity, native stack, SDK or stream
implementation is changed. No library was executed, compiled or given a token.
Only public source/metadata and upstream CI results were inspected. No embedded
credential or signing-key values are reproduced here.

## Reviewed revisions

| Candidate | Current source reviewed | Declared license / Rust minimum |
| --- | --- | --- |
| [`okhsunrog/yamu`][yamu-manifest], 0.1.2 | `9803f319b9ea6f81befbc3117e4592132ed07d0e` (`main`, 2026-07-15) | MIT OR Apache-2.0 / 1.97 |
| [`flxxxxddd/yamuse`][yamuse-manifest], 0.3.2 | `368c74646520e8bcd30c44a9887e1bf99cb58172` (`main` / `v0.3.2`, 2026-08-03) | MIT / 1.85 |

The latter is the Rust crate `yamuse`, not the unrelated similarly named desktop
project. Conclusions are specific to these immutable revisions.

## API coverage and integration

| Question | yamu | yamuse |
| --- | --- | --- |
| Separate API from OAuth? | `Client::new(token)` / builder token injection; `default-features = false` excludes optional OAuth | Builder token injection and construction without OAuth; device flow remains compiled but need not be called |
| Search and typed models | Search categories, pagination/correction and typed tracks/albums/artists; some sections remain raw JSON | Typed search/suggestions and result blocks; models tolerate missing/unknown fields and some type mismatches |
| Library and playlists | Revisioned liked tracks, batch expansion, playlist listing/detail, owner/kind and UUID addressing; revision-aware mutations | Likes for multiple entity types, personal playlists and detail, revision-aware edits and other mutations; account cache or explicit UID needed by personal methods |
| Transport / dependencies | Async reqwest 0.13, Tokio, serde, thiserror and URL; default GET backoff/retries | Async reqwest 0.12, Tokio, serde and thiserror; default GET retries; unconditional crypto/signing dependencies, default download feature |
| GTK / GStreamer | No base GTK/player integration; optional media features include FFmpeg/tagging | No GTK/GStreamer integration; optional realtime/download functionality is separate |
| `/account/about` evidence | No endpoint/model found | No endpoint/model found |

Coverage above means source implementations exist, not that those methods are
accepted for Karin. Both would require async execution outside GTK's main thread,
domain-model mapping and explicit transport/token/privacy boundaries. Neither
requires replacing GStreamer. Neither provides a confirmed schema for the new
`/account/about` diagnostic, which therefore checks only an object-valued result
envelope and makes no identity/entitlement inference.

## Reasons against direct adoption

- **Application identity:** [yamu's builder][yamu-client] unconditionally inserts
  the official Android `x-yandex-music-client` header, with no public replacement
  HTTP client/header control. Disabling OAuth does not remove it.
  [yamuse's transport][yamuse-transport] also inserts an official Android identity;
  its override options do not make that default suitable for Karin.
- **Borrowed credentials and delivery code:** yamu's optional OAuth module
  documents extracted official-application Client ID/secret defaults. yamuse's
  device OAuth module contains official-client credentials, while signing and
  encrypted-stream modules remain unconditional even with download disabled.
  yamu also offers optional signing/download/decryption features. None is an
  authorized alternative to Karin's own PKCE or a permitted delivery contract.
- **Privacy and diagnostics:** [yamu requests/errors][yamu-request] retain raw API
  bodies and server messages; account models retain personal/unknown fields.
  [yamuse's client][yamuse-client] derives `Debug` over an inner ordinary token
  string and cached account, despite its transport's redacted formatter. Its
  errors retain provider text and combine 401/403 into an unauthorized category.
  These public types cannot be sent directly to Karin's safe reports or logs.
- **Bounds and policy:** both read API bodies with `response.bytes()` without
  Karin's 1 MiB ceiling, default to GET retries, and use ordinary reqwest redirect
  behavior. Preserving Karin's deadlines, closed errors and no-retry/no-redirect
  policy would require changes; wrapping only the final result is insufficient.

## MSRV and licensing assessment

yamu's [upstream CI][yamu-ci] for the reviewed SHA passed tests, Clippy and package
checks on pinned Rust 1.97.0. That is favorable evidence for 1.98.1, **not an
executed exact-1.98.1 integration check** with Karin's selected features/lockfile.
yamuse declares 1.85; its [current upstream run][yamuse-ci] passed stable tests but
failed the MSRV 1.85.0 job and lint. The available annotation only reports exit
101, so the cause is unresolved. That failure does not prove incompatibility
with 1.98.1. Neither candidate has been independently built on 1.98.1 here.

MIT and Apache-2.0 are compatible in principle with Karin's GPL-3.0-or-later.
Any future copied code or dependency must retain the applicable third-party
license/copyright notices and, where applicable, NOTICE/attribution and modification
notices; copying does not relabel third-party code as original Karin material.
This review is not a full transitive/native distribution-license audit of either
candidate. Their permissive code licenses do not authorize borrowed application
identities, service access, signing material or Music content.

Useful ideas for later consideration are typed endpoint/result models, bounded
pagination/batching and playlist revision/conflict handling. Do not copy relaxed
schema repair into identity/permission decisions. A future library proposal would
need a separately approved, identity-neutral API-only surface, redacted bounded
transport, exact Rust 1.98.1/Flatpak checks and a reviewed license inventory.
For this gate, keep the existing adapter and independent player; no fork or
library integration is implemented. See [API feasibility](API_FEASIBILITY.md).

[yamu-manifest]: https://github.com/okhsunrog/yamu/blob/9803f319b9ea6f81befbc3117e4592132ed07d0e/Cargo.toml
[yamu-client]: https://github.com/okhsunrog/yamu/blob/9803f319b9ea6f81befbc3117e4592132ed07d0e/src/client/mod.rs#L166
[yamu-request]: https://github.com/okhsunrog/yamu/blob/9803f319b9ea6f81befbc3117e4592132ed07d0e/src/client/request.rs#L33
[yamu-ci]: https://github.com/okhsunrog/yamu/actions/runs/29408533569
[yamuse-manifest]: https://github.com/flxxxxddd/yamuse/blob/368c74646520e8bcd30c44a9887e1bf99cb58172/Cargo.toml
[yamuse-client]: https://github.com/flxxxxddd/yamuse/blob/368c74646520e8bcd30c44a9887e1bf99cb58172/src/client.rs#L36
[yamuse-transport]: https://github.com/flxxxxddd/yamuse/blob/368c74646520e8bcd30c44a9887e1bf99cb58172/src/transport.rs#L161
[yamuse-ci]: https://github.com/flxxxxddd/yamuse/actions/runs/30821642742
