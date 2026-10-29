<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin

An independent native Linux client for Yandex Music, built with Rust, GTK4,
Libadwaita and GStreamer. The owner has verified ordinary Yandex sign-in, Music
account validation, liked tracks and personal-playlist listing on Fedora.
**v0.2 must add working search, playlist contents and actual audible Music
playback. Its sound acceptance is not yet met.** Karin is not affiliated with Yandex.

[Issue #9](https://github.com/tracerxbrhd/karin/issues/9) tracks this stage. Its
starting Fedora run found that search and opening personal playlists failed.
Those are defects to repair, not successful features inferred from fixtures.
Audio delivery is investigated in parallel with those repairs; synthetic WAV
does not satisfy the real-track criterion.

## Implemented slice

- A typed `MusicService` for authenticated account status, liked tracks, personal
  playlists and their contents, track/album details, and paged search for tracks,
  albums and artists. Its identity-neutral transport has bounded responses,
  deadlines, fixed errors and no automatic retries or redirects.
- A native music interface with loading/error/empty/session states, library and
  search results, detail navigation and a bounded metadata queue. Only returned
  service data is displayed; no demonstration catalog is shipped.
- A desktop sidebar/compact drawer, Home with working destinations, readable
  music rows and context actions, detail covers and a persistent selected-track
  bar. Public cover images use bounded asynchronous loading and a memory cache;
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
- The existing GStreamer local-media controller and explicitly synthetic developer
  diagnostics remain available. They are not substitutes for Music playback.
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
The owner's Fedora report in Issue #9 establishes real login, Music account,
liked tracks and playlist-list access. Cloud fixtures alone do not establish
those results. [Fresh local Fedora acceptance](docs/BUILDING.md#music-sign-in-and-fedora-acceptance)
must verify corrected search/playlist contents and any new delivery/playback path. Failures identify return/state, session-cookie collection,
OAuth exchange or Music validation rather than a generic provider block.
Session tokens are memory-only; logout clears local session/data and does not revoke the server
grant. Persistent sessions require a separately verified Secret Service lifecycle.
Playlist editing, My Wave, service-stream resolution and MPRIS are not enabled.
Like/unlike writes are explicit and require confirmed membership; no success is
inferred from a missing response. Playback controls visibly state the missing permitted-source
contract rather than playing generated WAV as music.

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
The confirmed session/library results are endpoint-specific. New search,
playlist-detail, playback and visual changes require fresh manual acceptance;
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
request and prints only the resulting domain report. Network access is never
required by automated tests. Executed results and pending manual checks are in
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
