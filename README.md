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
playback and Linux desktop media controls. Karin is independent and is not
affiliated with or endorsed by Yandex.
The application interface is currently in Russian.

## Features

- Ordinary Yandex sign-in, protected remembered sessions, liked tracks and playlists.
- Global catalog search and Unicode collection search/sorting. Searching eligible
  collections indexes up to 20,000 positions in bounded batches.
- Confirmed owned-playlist creation and editing, including exact duplicate occurrences.
- One pinned Track Info / Lyrics / Queue panel with related tracks and ordinary
  playback, queue and reaction actions.
- Yandex plain text and optional LRCLIB synchronized lyrics after disclosed consent.
  Animated line emphasis and Follow use the actual player position.
- Contextual queues, Play Next, drag/keyboard reordering, Shuffle/Repeat, seek and
  volume. Ordinary queue and position restore without autoplay.
- Explicit Resume renews an expired source for the same occurrence and position.
- My Wave from Home, started only by **Слушать**, using the same player and MPRIS.
- Confirmed Like/Dislike; adding Dislike to the current occurrence advances once.
- Adaptive System presentation, independent System/Light/Dark colors and HiDPI.
  The future designer's Karin style remains visibly unavailable.
- One account-scoped cache for metadata, artwork and eligible online-revalidated
  MP3 responses, with a shared 2 GiB default budget, Clear and Retry.

## Screenshots

![System presentation with the player and synchronized lyrics](packaging/screenshots/karin-system-dark.png)

Installed v0.8 System presentation with original synthetic tracks and text;
no personal account or service content was used.
[Capture provenance](packaging/screenshots/karin-system-dark.png.license).

## Status and limitations

The owner completed Fedora acceptance of v0.8 functionality; the cleanup and
optimized CI were merged in [PR #29](https://github.com/tracerxbrhd/karin/pull/29).
The engineering candidate is **0.8.0-rc.1**, not a published release.
[Validation](docs/VALIDATION.md) separates owner acceptance, automated checks and
remaining release gates. No public tag, GitHub Release or Flathub publication
is announced.

The Music API is unofficial and may change. Lyrics availability and matching
are provider-dependent. Private audio caching requires current online access and
HTTP reuse proof; offline playback and mass downloads are not implemented.
Public service/content distribution conditions and Cloud cold-first-player
frame gaps remain unresolved. See [limitations](docs/LIMITATIONS.md),
[cache policy](docs/AUDIO_CACHE_REVIEW.md) and [roadmap](docs/ROADMAP.md).

## Build and run

Install dependencies using the [Native/Flatpak build guide](docs/BUILDING.md), then:

```sh
cargo build --release --locked
./target/release/karin
```

Use **Войти** to sign in normally. Flatpak uses the
[repository manifest](packaging/flatpak/io.github.tracerxbrhd.Karin.json) and the
same application code.

## Development

Read [AGENTS.md](AGENTS.md), [architecture](docs/ARCHITECTURE.md) and the
[build guide](docs/BUILDING.md).

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
```

CI also checks exact Rust 1.98.1, feature boundaries, native fixtures and packaging.

## License

Karin's code and original materials use [GNU GPL-3.0-or-later](LICENSE), unless
otherwise stated. Third-party notices and attribution for adapted Cassette
authentication components are retained; see [licensing](docs/LICENSING.md).
The software license does not grant access to Yandex Music, its API or content.
