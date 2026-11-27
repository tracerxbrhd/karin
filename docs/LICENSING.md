<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Licensing and attribution

Karin's original code and artwork use **GPL-3.0-or-later**, subject to per-file
notices. [LICENSE](../LICENSE) contains GPLv3. Source notices and Cargo metadata
record the later-version grant. The owner provided the original logo and README
banner, and the packaged PNG icons resize that logo. [AppStream metadata](../packaging/io.github.tracerxbrhd.Karin.metainfo.xml)
uses `CC0-1.0`. Third-party code and service materials keep their own terms.

## Upstream components

The browser-session authentication contract is adapted from **Cassette v0.2.4**,
source commit `9d34251d01bcd414ffbec2bef7931090d33c9124`, Copyright 2023–2024
Vladimir Vaskov, GPL-3.0-or-later. Preserve the complete
[COPYING](../packaging/third-party-licenses/cassette-auth/COPYING),
[pinned provenance](../packaging/third-party-licenses/cassette-auth/PROVENANCE.md),
[configuration attribution](../src/auth/cassette_client.json.license) and source
copyright and SPDX notices. The provenance credits the legacy playback references
and the independently implemented local-first and background cache behavior.

Bundled assets include [Tabler Icons](../packaging/third-party-licenses/tabler-icons/PROVENANCE.md)
under MIT, [Montserrat](../packaging/third-party-licenses/fonts/Montserrat-OFL.txt)
and [Inter](../packaging/third-party-licenses/fonts/Inter-OFL.txt) under SIL OFL 1.1.
The [font provenance](../packaging/third-party-licenses/fonts/PROVENANCE.md) records
the pinned official sources and hashes. Include their license and copyright
notices in Native and Flatpak packages.

## Code provenance audit

The review of `55e30be444d32c9c772fd004c5812d4a6b54dfa2` used source notices,
introduction commits, locked dependencies and pinned upstream references. The
table records implementation provenance. A formal clean-room process and
service access require separate evidence.

| Area | Classification and evidence |
| --- | --- |
| Ordinary sign-in | **Cassette adaptation.** `src/auth/cassette.rs` adapts the published session grant. `cassette_client.json` contains the published configuration. Karin adds navigation, sandbox, UI and provider boundaries. [Initial integration](https://github.com/tracerxbrhd/karin/commit/09f76355684873f32aaf83b8050e047df6077143) |
| Independent sign-in and storage | Karin's opt-in `--independent-auth` uses public OAuth/Yandex ID contracts, its earlier Code/S256 PKCE and account validation, and a separate credential scope. Secret Service, Secret Portal, validated restore and logout fences are Karin implementations. [Sign-in architecture](ARCHITECTURE.md#sign-in-and-private-state) |
| Catalog, playlists and My Wave | Karin's `api.rs` predates the adapted sign-in. Catalog and playlist code implements reviewed contracts without linking Cassette, yamu, yamuse or MarshalX client libraries. Wave implements the reviewed envelope and `event.from`, with a local listening ledger and queue history. [Catalog and queue](ARCHITECTURE.md#catalog-and-queue) |
| Audio delivery and cache | Karin's `src/stream/` implements the offer/XML/MP3 contract corroborated by pinned Cassette, MarshalX and lifeart references. Its shared GStreamer worker predates the sign-in adaptation. Cassette informed local-first lookup and background saving. Karin implements its account-scoped budget, integrity checks, HTTP reuse and eviction. [Local engine](https://github.com/tracerxbrhd/karin/commit/226bf08) · [Playback and cache](ARCHITECTURE.md#playback-and-cache) |
| Lyrics, related tracks and UI | Karin implements the parser, native frame highlighting and UI. Text, related data and consented LRCLIB requests use reviewed wire formats. [Presentation and desktop](ARCHITECTURE.md#presentation-and-desktop) |
| Test code and fixtures | Karin uses synthetic wire data, text and artwork, generated WAV and CC0 fixture audio. Legacy sign-in fixtures exercise the attributed provider. [Audio generator](../src/audio.rs) · [Audio cache fixture](../src/ui/music/persistence/tests/app_cache_acceptance.rs) |
| Build and packaging | `cassette-auth` enables `auth-webkit` and selects the attributed provider. Native, Flatpak and CI carry the corresponding notices. The [pinned 0.8.0 manifest](../packaging/release/0.8.0/io.github.tracerxbrhd.Karin.json) remains available for rebuilding that release. |

Git retains the earlier [authentication review](https://github.com/tracerxbrhd/karin/blob/497f9562d2b6f703cf49c873c51abf7e7fa8552d/docs/AUTHENTICATION_REVIEW.md)
and [stream delivery review](https://github.com/tracerxbrhd/karin/blob/497f9562d2b6f703cf49c873c51abf7e7fa8552d/docs/STREAM_DELIVERY_REVIEW.md).
Verifying an independent Music grant and playback comes before replacing the
release provider and reassessing the affected code, features, packaging and notices.

## Dependency notices

[DEPENDENCY_LICENSES.md](DEPENDENCY_LICENSES.md) inventories locked Rust packages,
including optional and build dependencies. Run this after dependency changes.

```sh
python3 packaging/license_inventory.py --output docs/DEPENDENCY_LICENSES.md
```

Review the source terms and preserve supplied LICENSE, COPYING, NOTICE and
COPYRIGHT files, including nested notices, exceptions and cumulative `AND`
obligations. The four MIT-licensed 0.5.0 archives `javascriptcore6`,
`javascriptcore6-sys`, `webkit6` and `webkit6-sys` omit the license text. The
[VCS and archive provenance](../packaging/third-party-licenses/webkit6-rs-0.5.0/PROVENANCE.md)
records the exact MIT text and authenticated fallback for those four archives.

## Binary and corresponding-source obligations

[Source preparation](../packaging/flatpak/prepare-source.py) vendors locked crates
and preserves their notice paths under `third-party-notices/`. Keep
`packaging/third-party-licenses/` in source snapshots and binary notice bundles.
The [runtime collector](../packaging/release/rust_runtime.py) records provenance
and hashes for compiler- and target-matched Rust std/compiler-builtins notices
in Native, Flatpak and CI packages, using library notices and matching `rust-src`.

The exact GTK, Libadwaita, GLib/GIO, WebKitGTK, libsoup, GStreamer and codec
libraries conveyed determine their own source, relinking and notice obligations.
Native records dependencies on shared system libraries separately. Flatpak
installs notices in `/app/share/licenses/io.github.tracerxbrhd.Karin`.

Before releasing binaries, publish the complete corresponding source, with locked
vendor inputs, notices and build and installation instructions. Link each Native
and Flatpak release directly to its matching archive in `tracerxbrhd/karin-source`.
Retain past archives and verify anonymous downloads against the build receipt.
GitHub's automatic source downloads contain the tagged repository tree.
[Publication procedure](BUILDING.md#stable-flatpak-channel) · [GPLv3 §6(d)](https://www.gnu.org/licenses/gpl-3.0.html#section6)

## Integration and public-distribution gate

| Area | Established basis and remaining limit |
| --- | --- |
| Source code | GPL permits reuse/distribution subject to its conditions and retained notices. |
| OAuth registration | Karin has its own application registration. Ordinary sign-in still uses the configuration published with Cassette. The registration holder's reuse authorization is unresolved. |
| Music API | The owner accepted ordinary sign-in and playback in 0.8.2 and 0.8.3. Karin's own-registration token passed Yandex ID checks. In the recorded diagnostic, `/account/status` and `/account/about` returned HTTP 403 with no confirmed cause. |
| Integrated public distribution/content | No applicable provider/registration-holder approval or other sufficient basis is recorded. A subscription, local playback and HTTP cacheability do not establish it. |

The review dated **2026-11-24** used these terms.

- [Yandex ID API terms](https://yandex.com/legal/authid_api/en/), published
  21.08.2026: §§2.5.1–2.5.2 separate other API requirements; §4.4 restricts
  API Key provision/transfer and needs clarification for this shared registration.
- [Music terms](https://yandex.ru/legal/music_termsofuse/ru/), Russia/Belarus
  Russian version effective 25.08.2025: §§2.1, 2.7–2.8 reserve material rights
  and describe personal noncommercial use within supplied capabilities/restrictions.
- [Official-program license](https://yandex.ru/legal/music_mobile_agreement/ru/),
  Russia/Belarus Russian version effective 03.04.2026: §§1.1/2.2 concern Yandex's
  program/materials, not a grant to Karin's independent Linux implementation.

[#5](https://github.com/tracerxbrhd/karin/issues/5) records the historical review.
[#35](https://github.com/tracerxbrhd/karin/issues/35) records the independent sign-in work.
The [archived draft for Yandex](https://github.com/tracerxbrhd/karin/blob/36ac6820d619fb9aa6c7e235a445ef66f55c610e/packaging/release/YANDEX_REQUEST_RU.md)
remains unsent. [#22](https://github.com/tracerxbrhd/karin/issues/22) records the
packaging and publication of 0.8.0.

## Service and test materials

Covers, catalog data and lyrics retain provider and rightsholder terms. HTTP
cache admission follows the service decision, storage directives, representation
validation and content restrictions. LRCLIB lookup requires consent and keeps
lyrics in memory. Its API and code terms leave individual lyric rights separate.

Automated fixtures use original art, text and audio. The original audio bytes
generated by the complete application cache fixture are dedicated to the public
domain under [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/).
Generator and application code retain GPL. For licensed-resource admission,
Karin binds a trusted declaration to account, track, SHA-256, length and
revision. A valid CC0 dedication survives withdrawal of application admission.
