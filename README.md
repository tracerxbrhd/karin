<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin

**Native Yandex Music client for Linux.**

[![Rust](https://img.shields.io/github/languages/top/tracerxbrhd/karin?logo=rust)](https://github.com/tracerxbrhd/karin/search?l=Rust)
[![MSRV 1.98](https://img.shields.io/badge/MSRV-1.98-blue?logo=rust)](Cargo.toml)
[![GTK 4.14+](https://img.shields.io/badge/GTK-4.14%2B-4a86cf?logo=gtk)](https://www.gtk.org/)
[![Libadwaita 1.5+](https://img.shields.io/badge/Libadwaita-1.5%2B-4a86cf)](https://gnome.pages.gitlab.gnome.org/libadwaita/)
[![GStreamer](https://img.shields.io/badge/audio-GStreamer-008080)](https://gstreamer.freedesktop.org/)
[![Flatpak source build](https://img.shields.io/badge/Flatpak-source%20build-4a86cf?logo=flatpak)](packaging/flatpak/io.github.tracerxbrhd.Karin.json)
[![CI on master](https://github.com/tracerxbrhd/karin/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/tracerxbrhd/karin/actions/workflows/ci.yml)
[![GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSE)

Listen to your library in a native GTK4/Libadwaita application, with GStreamer
playback and Linux desktop media controls. Karin is an independent project and
is not affiliated with or endorsed by Yandex.

## Screenshots

![Liked tracks with collection search and the player](docs/screenshots/liked.png)

<p>
  <img src="docs/screenshots/playlists.png" width="49%" alt="Owned and liked playlist collections" />
  <img src="docs/screenshots/track-info.png" width="49%" alt="Dark theme with the pinned track information panel" />
</p>

Actual GTK captures with synthetic library data and original test artwork.
[Capture details](docs/screenshots/README.md).

## Features

- Yandex sign-in, liked tracks, personal playlists and catalog search.
- Collection-local search, track details and server-confirmed playlist creation.
- Distinct Like/Dislike controls with account-bound read-after-write confirmation.
- Contextual queues, Shuffle/Repeat, seek and volume through GStreamer.
- My Wave personal radio from Home, using the same queue, player and MPRIS.
- A pinned track information / queue panel, responsive layouts, light/dark themes and HiDPI.
- MPRIS media controls and metadata for KDE Plasma and other Linux desktops.
- Protected remembered sign-in and an account-scoped metadata/artwork cache.

## Status

The v0.4 baseline is merged into `master` through
[PR #15](https://github.com/tracerxbrhd/karin/pull/15). The owner accepted its native
functionality, UI and Flatpak build on Fedora 44 with KDE Plasma, Wayland and PipeWire.
This is a development milestone, not a published release; the Cargo package
version remains `0.0.0`. See [validation](docs/VALIDATION.md) for the accepted scope.

The accepted stability update [PR #24](https://github.com/tracerxbrhd/karin/pull/24)
is merged at `920fd71`. **My Wave** is now an ordinary Home function after
Music-validated sign-in. Press **Слушать** to start it; launching Karin,
restoring sign-in or opening Home creates no radio session. The owner retested
the corrected `2e4b978` candidate on Fedora without reporting new problems,
following earlier confirmed real audio, feedback and batch continuation.
Later owner acceptance includes the installed Flatpak; follow the [Native/Flatpak procedure](docs/BUILDING.md#my-wave-local-acceptance).

The owner accepted confirmed Dislike→Next, the ordinary queue and My Wave on
Fedora Native and installed Flatpak after [PR #27](https://github.com/tracerxbrhd/karin/pull/27)
merged at `ddf93ea`. Removal and Like do not advance playback; late confirmations
are guarded by queue occurrence and session generations. See the
[backlog audit](docs/ISSUES_AUDIT.md) for remaining independent work.

The Music API is unofficial and may change. Separate recommendation shelves,
lyrics and offline audio are not implemented. Cold first-player appearance in Cloud
still has unresolved frame gaps; desktop acceptance does not establish their
cause or resolution. See [known limitations](docs/LIMITATIONS.md).

PR #26 provides one cache policy: enabled by default, one 2 GiB allowance,
combined usage and Clear, with a native slider for 500 MiB, 1/2/3/4 GiB or
Unlimited. Metadata, artwork and authority-gated audio share one disk coordinator,
including in-flight writes. The owner accepted the preceding `707539b` Native
and Flatpak infrastructure; the new unified interface needs its own acceptance.
Real Music remains online-only: storage/reuse is disabled until an applicable
permission and the provider's online revalidation rule are established. PR #26
remains Draft; generated-audio tests do not establish service caching or offline
Music. See the [source/rights review](docs/AUDIO_CACHE_REVIEW.md) and the
[prepared provider questions](docs/AUDIO_CACHE_PROVIDER_REQUEST.md).

## Build and run

Install the native dependencies using the [Fedora and Flatpak build guide](docs/BUILDING.md), then:

```sh
cargo build --release --locked
./target/release/karin
```

Use the ordinary **Войти** action to sign in with your Yandex account.
Flatpak builds use the [repository manifest](packaging/flatpak/io.github.tracerxbrhd.Karin.json)
and the same application code.

## Development

Read [AGENTS.md](AGENTS.md), the [architecture](docs/ARCHITECTURE.md) and
[build guide](docs/BUILDING.md) before changing the application.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
```

CI also checks the pinned Rust 1.98.1 toolchain, feature configurations and
native fixtures. The badge tracks `master`; proposed changes have their own PR checks.
The [roadmap](docs/ROADMAP.md) describes the next work.

## License

Karin's code and original materials use [GNU GPL-3.0-or-later](LICENSE), unless
otherwise stated. Third-party notices and the attribution for the adapted
Cassette authentication components are retained; see [licensing](docs/LICENSING.md).
The software license does not grant access to Yandex Music, its API or music content.
