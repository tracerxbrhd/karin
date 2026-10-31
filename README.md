<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin

An independent native Linux client for Yandex Music, built with Rust, GTK4,
Libadwaita and GStreamer. The owner's Fedora 44 / KDE Plasma / Wayland / PipeWire
run at **`f0aa2bb`** reports **Search PASS, Account PASS and Playback PASS**, while
**UI FAIL** remains blocking. The earlier [real-audio acceptance at `8f1b09d`](https://github.com/tracerxbrhd/karin/pull/13#issuecomment-6539311586)
is preserved: an actual Yandex Music track was heard, with working controls,
queue and stopping on logout/close. The next correction improves the native
workspace, playback context and media-system controls while retaining that
integration. New-build acceptance, current Flatpak and public-distribution
permission remain separate. Karin is not affiliated with Yandex.

[Issue #9](https://github.com/tracerxbrhd/karin/issues/9) tracks this stage. Its
starting Fedora run found that search and opening personal playlists failed.
Those are defects to repair, not successful features inferred from fixtures.
Work continues in the existing [PR #13](https://github.com/tracerxbrhd/karin/pull/13).
Synthetic WAV or MP3 fixtures do not replace the owner's real-track evidence or
the new build's playback regression check.

## Implemented slice

- A typed `MusicService` for authenticated account status, liked tracks, personal
  playlists and their contents, track/album details, and paged search for tracks,
  albums, artists and playlists. Its identity-neutral transport has bounded
  responses, deadlines, fixed errors and no automatic retries or redirects.
- Track search reads minimal validated IDs and retrieves normalized track models
  in one bounded batch, preserving result order and provider pagination. Missing
  tracks are reported as partial data; unavailable returned tracks stay visible
  with disabled playback. Search display fields do not gate otherwise valid IDs.
  Unified search exposes independent real-data sections with category paging and
  Retry; the initial search makes at most one GET and one normalization batch.
- A native music interface with loading/error/empty/session states, library and
  search results, detail navigation and a bounded playback context. Only returned
  service data is displayed; no demonstration catalog is shipped.
- A default desktop icon rail with expandable labels and a compact drawer,
  readable music rows, detail covers and a persistent player bar. Activating a
  track starts its actual loaded collection context; details have a separate
  action. The round account avatar uses known-name initials, with an external
  account-page action. Logout is an explicitly labelled menu action with a native
  confirmation and Cancel as the safe default. Public cover images use bounded
  asynchronous loading and a memory cache;
  malformed or missing art has a native placeholder. Theme preferences support
  system/light/dark for the current process. No playback position is simulated.
- Explicit like/unlike controls read current membership, send one user-requested
  change and confirm it with an independent read. Unknown outcomes remain unknown;
  Refresh reads state without replaying a write. Liked playlists retain their
  actual owner and open through the same detail/paging flow. These new service
  interactions still need owner-local acceptance.
- One native `Войти` dialog loads the actual Yandex login in an isolated,
  ephemeral WebKitGTK6 view. The default `cassette-auth` provider adapts Cassette's
  published session-cookie → OAuth `sessionid` grant. No Client ID/secret fields,
  device codes, pasted tokens, application registration or manual URL transfer
  are part of this journey. Music account validation must pass before the dialog
  closes and real liked tracks/playlists are fetched.
- WebKit is confined to that authentication window; the application remains native
  GTK/Libadwaita. Cancellation clears the browser session and discards late work.
- The independent own-client `--oauth-diagnostic` remains unchanged: hidden full
  redirect input, returned-state validation, S256 PKCE, secretless initial
  exchange and separate sanitized status/about outcomes.
- An account-bound `StreamResolver` checks an explicit full MP3 offer, retrieves
  a bounded XML descriptor and constructs an opaque, short-lived legacy resource.
  The existing GStreamer controller accepts that validated HTTPS source as well
  as local files. Explicit Play, pause/resume, Stop, progress/seek, volume,
  Next/Previous, queue selection and EOS use actual worker snapshots. Protected,
  preview, unknown-origin and expired sources fail visibly; no generated audio
  is substituted for a service track.
- Playback contexts preserve loaded order, duplicate positions and the selected
  occurrence within the 500-track queue limit. Manual appends follow the context;
  a new context replaces old extras. Next/Previous/EOS skip unavailable loaded
  tracks, without pretending a partial page is a complete remote collection.
- MPRIS 2.2 uses the existing player through GIO session D-Bus, with truthful
  capabilities, transport, worker-acknowledged seek, volume and locally known
  metadata. Optional cover artwork uses a private temporary cover-only PNG,
  removed on replacement/logout/close. No media
  address, OAuth or cookie is exported. KDE/media-key and current Flatpak
  acceptance of this new bridge still require the owner's local run.
- Metadata and media use separate credential boundaries: OAuth goes only to the
  fixed Music API; descriptor/CDN requests receive no OAuth or cookies. Remote
  playback preserves TLS verification, rejects redirects/manifests and stores no
  music files or signed addresses. The metadata-only diagnostic remains separate.
- The existing local-media controller and explicitly synthetic developer
  diagnostics remain available, independently of service playback.
- Offline protocol/model tests, native GTK/GStreamer smoke checks, public and
  experimental build checks, and Rust stable / exact 1.98.1 CI lanes.

The owner verified own-client PKCE and Yandex ID on Fedora; **both Music account
endpoints returned HTTP 403**, with TUN on and off. A separate unmodified yamu
Device Flow accessed account/search/track/album data. The same yamu-issued token
worked at status without the Android-client header. These facts do not authorize
shipping the official application's OAuth identity or establish stream rights.
The exact cause of Karin's generic 403 remains unknown. Changing the browser UI
does not establish a working Music grant.

The own-client PKCE result is not reused as the normal sign-in path. The
prototype instead uses the published OAuth registration already used by Cassette,
with upstream GPL notices preserved. That registration's identity is distinct
from Karin's own app; the consent page may name the upstream registration.
Code-license permission, service/client-identity distribution conditions and
technical interoperability are separate questions. No express provider approval
for Karin's Linux embedded use or redistribution is established, and no blanket
provider prohibition is inferred. See [the scoped decision](docs/adr/0011-cassette-session-auth-prototype.md).

The default build and Flatpak enable this real provider; a no-feature compatibility
build deliberately omits it. Device Flow remains a library-only optional experiment.
The owner's Fedora reports establish real login, Music account, liked tracks,
playlist-list access and the core audible path at `8f1b09d`, followed by accepted
track search/account actions/preserved playback at `f0aa2bb`. Cloud fixtures alone
do not establish those results. [Fresh local Fedora acceptance](docs/BUILDING.md#v02-catalog-and-delivery-info-acceptance-on-fedora)
must verify the revised workspace, row-to-sound contexts, unified search,
desktop controls and preservation of those working paths. Failures identify
return/state, session-cookie collection,
OAuth exchange or Music validation rather than a generic provider block.
Session tokens are memory-only; logout clears local session/data and does not revoke the server
grant. Persistent sessions require a separately verified Secret Service lifecycle.
Playlist editing and My Wave are not enabled.
Like/unlike writes are explicit and require confirmed membership; no success is
inferred from a missing response. The earlier MP3 offer/descriptor refusal is
historical: the owner subsequently heard an actual track through this resolver
and player. Use the [single Fedora acceptance guide](docs/BUILDING.md#v02-catalog-and-delivery-info-acceptance-on-fedora)
to check the new workspace/context/media-control journey and guard against audio
regressions.
The historical legacy checksum is assessed separately from modern extracted
signing keys and DRM; the latter are not implemented. See
[ADR-0015](docs/adr/0015-guarded-legacy-mp3-playback.md).

See [the sign-in decision](docs/adr/0011-cassette-session-auth-prototype.md),
[authentication research](docs/AUTHENTICATION_REVIEW.md),
[API evidence](docs/API_FEASIBILITY.md), [stream-delivery review](docs/STREAM_DELIVERY_REVIEW.md)
and [local Fedora acceptance](docs/BUILDING.md#music-sign-in-and-fedora-acceptance).

## Build and run

Install stable Rust (minimum supported series: 1.98; compatibility checks use
1.98.1) and the native development packages described in
[BUILDING.md](docs/BUILDING.md), then:

```sh
cargo build --workspace --locked
cargo run --locked
```

The primary manual acceptance platform is Fedora 44 / KDE Plasma / Wayland /
PipeWire. [Issue #3](https://github.com/tracerxbrhd/karin/issues/3) reports that
the v0.0 native and Flatpak prototypes launch and diagnostic audio works there.
The confirmed session/library/audio results are revision-specific. New search,
playlist-detail and visual changes require fresh manual acceptance, with a
regression check of the accepted playback path;
startup alone is not visual signoff. Cloud validation uses Debian 13 / Xvfb.
In the prepared Codex Cloud environment, prefix native build commands with
`/workspace/karin-environment/run-auth`, whose image includes WebKitGTK6.
Default builds require the WebKit native package listed in [BUILDING.md](docs/BUILDING.md).

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --all-features --locked
cargo build --workspace --locked
./target/debug/karin --audio-self-test
./target/debug/karin --playback-self-test
GTK_A11Y=none GSK_RENDERER=cairo dbus-run-session -- xvfb-run -a ./target/debug/karin --smoke-test
```

CI also checks Rust 1.98.1, including an offline release build matching Flatpak's
Cargo command. See [BUILDING.md](docs/BUILDING.md) for the pinned-toolchain commands.

Both audio self-tests are silent and require no sound device.
`--playback-self-test` decodes generated local media through the real controller;
it is separate from the finite pipeline check in `--audio-self-test`.
`--smoke-test` checks the real window and navigation without contacting the API.
The separate optional `--probe-service` command makes a real anonymous network
request and prints only the resulting domain report. External network access is never
required by automated tests; network fixtures use loopback only. Executed results and pending manual checks are in
[VALIDATION.md](docs/VALIDATION.md).

## Project documentation

- [Architecture and concurrency](docs/ARCHITECTURE.md)
- [API feasibility and evidence](docs/API_FEASIBILITY.md)
- [Authentication contract and prototype boundary](docs/AUTHENTICATION_REVIEW.md)
- [Fedora, cloud and Flatpak build instructions](docs/BUILDING.md)
- [Validation and acceptance](docs/VALIDATION.md)
- [UI quality gate](docs/UI_QUALITY_GATE.md)
- [Inspected cloud UI screenshots](docs/ui/README.md)
- [Known limitations](docs/LIMITATIONS.md)
- [License scope and dependency review](docs/LICENSING.md)
- [Cassette v0.2.4 capability audit](docs/CASSETTE_PARITY.md)
- [Roadmap](docs/ROADMAP.md) and [ADRs](docs/adr/)

The Flatpak manifest and offline Cargo source preparation are in `packaging/`.
GNOME 50 and the Freedesktop 25.08 Rust extension are retained, with no additional
sandbox permissions for the music slice. The manifest explicitly enables `cassette-auth` while excluding the legacy
Device Flow feature. Full sandboxed builds remain unavailable in this
cloud container because Bubblewrap namespace creation is denied; the new revision
must be packaged and accepted on Fedora independently of the reported v0.0 result.

## License

Karin's own code and original materials are licensed under
**GNU GPL-3.0-or-later**, unless a file states otherwise. See [LICENSE](LICENSE)
for the standard GPLv3 text and [the licensing decision](docs/adr/0005-licensing.md)
for scope and distribution considerations. Third-party components retain their
own licenses and notices.

GPL does not grant permission to use Yandex Music, its API, trademarks or content,
and does not replace the service's terms. No copyrighted audio is distributed by
this repository.
