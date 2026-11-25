<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Licensing and attribution

Karin's code and original materials use **GPL-3.0-or-later**, unless a file states
otherwise. [LICENSE](../LICENSE) is the complete standard GPLv3 text; source
notices and Cargo metadata express the later-version grant. The original Karin
SVG icon is GPL-3.0-or-later. [AppStream metadata](../packaging/io.github.tracerxbrhd.Karin.metainfo.xml)
has a per-file `CC0-1.0` exception. Third-party code and service/user materials
retain their respective rights.

## Upstream components

The browser-session authentication contract is adapted from **Cassette v0.2.4**,
source commit `9d34251d01bcd414ffbec2bef7931090d33c9124`, Copyright 2023–2024
Vladimir Vaskov, GPL-3.0-or-later. Preserve the complete
[COPYING](../packaging/third-party-licenses/cassette-auth/COPYING),
[pinned provenance](../packaging/third-party-licenses/cassette-auth/PROVENANCE.md),
[configuration attribution](../src/auth/cassette_client.json.license) and source
copyright/SPDX notices. That provenance also credits legacy playback references
and the independently implemented local-first/background cache pattern.

The release provider uses app configuration published with Cassette. Karin's own
registration is separate. Registration ownership and permission for Karin's use
require evidence beyond source attribution.

## Code provenance audit

The review covers the tracked `master` tree at `55e30be444d32c9c772fd004c5812d4a6b54dfa2`.
It uses source notices, introduction commits, the locked dependency list and
pinned upstream references. The classifications describe engineering evidence
of origin. A formal clean-room process and service access require separate evidence.

| Area | Classification and evidence |
| --- | --- |
| Ordinary sign-in | **Cassette adaptation.** `src/auth/cassette.rs` adapts the published session grant and `cassette_client.json` carries the published app configuration. The legacy implicit attempt and WebKit cookie exchange support that flow. Generic navigation, sandbox, UI and provider boundaries include Karin's own work. [Initial integration](https://github.com/tracerxbrhd/karin/commit/09f76355684873f32aaf83b8050e047df6077143) · [Pinned upstream evidence](../packaging/third-party-licenses/cassette-auth/PROVENANCE.md) |
| OAuth, protected storage and logout | **Independent protocol and behavior.** The own-registration Code/S256 PKCE diagnostic uses a separate flow. Secret Service, Secret Portal, Music-validated restore and forgetting fences are Karin implementations. [Session lifecycle](adr/0018-persistent-session-and-settings.md) |
| Catalog, library, search and playlists | **Independent protocol implementations.** `api.rs` predates the adapted sign-in. `music.rs` and the catalog, playlist and reaction adapters implement reviewed endpoint and envelope contracts. Their bounded hydration, revision/occurrence rules and independent readback are Karin code. No Cassette, yamu, yamuse or MarshalX client library is linked. [Playlist writes](adr/0032-playlist-management.md) · [Collection index](adr/0035-incremental-whole-collection-index.md) |
| My Wave and listening history | **Independent protocol and behavior.** The Wave adapter retains the reviewed session envelope and `event.from` shape. The bounded listening ledger, occurrence history and retirement use the shared player. Previous/Shuffle and Wave history are local queue state. The client has no separate service-side listening-history integration. [My Wave](adr/0027-my-wave-lifecycle.md) · [Queue identity](adr/0026-refusal-fences-and-queue-occurrence-identity.md) |
| Audio delivery and playback | **Independent protocol and behavior.** `src/stream/` implements the shared legacy offer/XML/MP3 contract, corroborated across pinned Cassette, MarshalX and lifeart references. Karin's exact origin limits, bounded parsing, opaque sources and HTTP validation govern admission. The local GStreamer worker predates the adapted sign-in. The remote source, queue and MPRIS use that same worker. [Local engine](https://github.com/tracerxbrhd/karin/commit/226bf08) · [Guarded playback](adr/0015-guarded-audio-playback.md) |
| Metadata, artwork and audio cache | **Independent implementation of analogous behavior.** Cassette informed local-first lookup and background saving. Karin uses an account-scoped shared budget, bounded staging, integrity, scoped HTTP reuse and eviction. Cassette's storage and prefetch designs remain historical references. [Unified cache](adr/0029-unified-cache-and-audio-reuse.md) |
| Related tracks, lyrics and UI | **Independent protocol and behavior.** Supplemental text, related metadata and consented LRCLIB requests use reviewed wire shapes. The lyric parser, bounded native frame highlighting and UI are Karin implementations. The Karin icon is original. Tabler, Montserrat and Inter have their separate upstream licenses. [Lyrics and related tracks](adr/0034-bounded-track-lyrics-and-similar.md) |
| Tests and fixtures | **Independent test code and materials.** Wire fixtures model the reviewed schemas using synthetic values. Generated WAV, CC0 fixture audio, synthetic artwork and text exercise the normal application and do not contain Cassette assets or real account data. Legacy sign-in fixtures exercise the attributed provider. [Audio generator](../src/audio.rs) · [Audio cache fixture](../src/ui/music/persistence/tests/app_cache_acceptance.rs) |
| Build, packaging and CI | **Independent infrastructure with a legacy dependency.** `cassette-auth` selects the attributed provider and enables `auth-webkit`. Native and Flatpak release builds and CI still select it and carry the notice bundle. The pinned 0.8.0 manifest remains a historical build input. [Native packaging](../packaging/release/artifacts.py) · [Flatpak manifest](../packaging/flatpak/io.github.tracerxbrhd.Karin.json) · [CI](../.github/workflows/ci.yml) |
| Historical references | **Historical mentions and retained protocol credits.** Earlier reviews were consolidated into the active ADRs. Git retains the prior [authentication review](https://github.com/tracerxbrhd/karin/blob/497f9562d2b6f703cf49c873c51abf7e7fa8552d/docs/AUTHENTICATION_REVIEW.md) and [stream delivery review](https://github.com/tracerxbrhd/karin/blob/497f9562d2b6f703cf49c873c51abf7e7fa8552d/docs/STREAM_DELIVERY_REVIEW.md). |
| Notices and license sources | **Required notices.** The provider's headers, configuration sidecar and GPL text remain while the adaptation ships. Keep the pinned provenance, GPL for Karin, MIT and OFL asset notices, Cargo/WebKit binding notices and compiler-matched Rust runtime notices. The public protocol and cache credits record how those contracts were verified. |

The opt-in `--independent-auth` command is an independent protocol implementation.
It draws on public OAuth standards, Yandex ID's public contract and Karin's earlier
Code/PKCE, account-validation and protected-storage code. It runs before GTK and uses
a separate credential scope. [Authentication architecture](ARCHITECTURE.md#sign-in-and-private-state)

The audit supports retaining the existing audio, cache, catalog and UI implementations.
Retiring the legacy provider depends on verifying an independent Music grant and real
playback. Reassess affected code, features, packaging and notices at that point.

## Dependency notices

[DEPENDENCY_LICENSES.md](DEPENDENCY_LICENSES.md) inventories the locked Rust
packages, including optional/build dependencies; it is not a binary/runtime SBOM.
Regenerate it after dependency changes:

```sh
python3 packaging/license_inventory.py --output docs/DEPENDENCY_LICENSES.md
```

Preserve supplied LICENSE/COPYING/NOTICE/COPYRIGHT files, nested notices,
exceptions and cumulative `AND` obligations. Review actual source terms, not only
Cargo declarations. Four MIT-licensed 0.5.0 binding archives omit license text:
`javascriptcore6`, `javascriptcore6-sys`, `webkit6`, `webkit6-sys`. Their exact
MIT text and authenticated fallback are retained with
[VCS/archive provenance](../packaging/third-party-licenses/webkit6-rs-0.5.0/PROVENANCE.md).
Do not relax that fallback for other missing/unreviewed notices.

## Binary and corresponding-source obligations

The interface also bundles these original upstream assets:

- [Tabler Icons](../packaging/third-party-licenses/tabler-icons/PROVENANCE.md),
  a selected outline set under its original MIT license.
- Montserrat, under the original [SIL OFL 1.1](../packaging/third-party-licenses/fonts/Montserrat-OFL.txt).
- Inter, under the original [SIL OFL 1.1](../packaging/third-party-licenses/fonts/Inter-OFL.txt).

The [font provenance](../packaging/third-party-licenses/fonts/PROVENANCE.md)
records pinned official sources and hashes. Preserve these licenses, copyrights
and provenance with the embedded assets in Native and Flatpak packages.

[Source preparation](../packaging/flatpak/prepare-source.py) vendors locked crates
and copies their notices at original relative paths into `third-party-notices/`.
Keep `packaging/third-party-licenses/` in source snapshots and binary notice bundles.
The Native tar, Flatpak and CI binaries additionally carry compiler/target-matched
Rust std/compiler-builtins notices collected from the supplied library notices
and matching `rust-src`. [The collector](../packaging/release/rust_runtime.py)
records provenance/file hashes and rejects compiler mismatch.

Binding licenses do not replace GTK, Libadwaita, GLib/GIO, WebKitGTK, libsoup,
GStreamer, codec or runtime obligations. Review the exact libraries/plugins
being conveyed, including source, relinking and notices. Native packages do not
bundle shared system libraries; their observed dependencies are recorded separately.
The Flatpak installs notices under `/app/share/licenses/io.github.tracerxbrhd.Karin`.

Before conveying GPL-covered binaries, provide complete corresponding source and
build/install information through an applicable GPL section 6 method. A repository
link or LICENSE alone is not a universal substitute. [BUILDING](BUILDING.md)
describes pinned source, Native/Flatpak, BUILD-INFO and checksum generation.
Source-repeatability/checksum verification is not certified binary reproducibility.

## Integration and public-distribution gate

Four separate questions remain:

| Area | Established basis and remaining limit |
| --- | --- |
| Source code | GPL permits reuse/distribution subject to its conditions and retained notices. |
| OAuth registration | Karin has its own application registration. Ordinary sign-in still uses the configuration published with Cassette. The registration holder's reuse authorization is unresolved. |
| Music API | Owner-accepted sessions work with the legacy provider. An OAuth token issued to Karin's own registration was valid for Yandex ID. Both `/account/status` and `/account/about` returned HTTP 403, with the cause unconfirmed. |
| Integrated public distribution/content | No applicable provider/registration-holder approval or other sufficient basis is recorded. A subscription, local playback and HTTP cacheability do not establish it. |

The focused official review, rechecked on **2026-11-24**, used:

- [Yandex ID API terms](https://yandex.com/legal/authid_api/en/), published
  21.08.2026: §§2.5.1–2.5.2 separate other API requirements; §4.4 restricts
  API Key provision/transfer and needs clarification for this shared registration.
- [Music terms](https://yandex.ru/legal/music_termsofuse/ru/), Russia/Belarus
  Russian version effective 25.08.2025: §§2.1, 2.7–2.8 reserve material rights
  and describe personal noncommercial use within supplied capabilities/restrictions.
- [Official-program license](https://yandex.ru/legal/music_mobile_agreement/ru/),
  Russia/Belarus Russian version effective 03.04.2026: §§1.1/2.2 concern Yandex's
  program/materials, not a grant to Karin's independent Linux implementation.

These sources establish neither a blanket third-party ban nor affirmative approval
for this integration. The [finished Russian request](../packaging/release/YANDEX_REQUEST_RU.md)
contains the verified official channel and questions about registration, Music API,
embedded login, content/cache, branding and Native/Flatpak distribution. It has
not been sent. An applicable basis, its conditions and working permitted access
remain open in [#5](https://github.com/tracerxbrhd/karin/issues/5). The published
0.8.0 release and its preparation are recorded separately in
[#22](https://github.com/tracerxbrhd/karin/issues/22).

## Service and test materials

Covers, catalog data and lyrics are provider material, not original GPL artwork.
HTTP cache admission separates current access, storage directives, representation
validation and content restrictions; it is not a copyright license. Explicit
refusal, `no-store`, required revalidation and unsupported/protected media remain
binding. Credentials and signed media URLs are not persisted. LRCLIB lookup is
consented and memory-only; its API/code terms do not license every lyric.

Automated fixtures use original art/text/audio, not real music or lyrics.
The original audio bytes generated by the complete application cache fixture are
expressly dedicated to the public domain under [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/).
That dedication covers only those audio bytes; generator/application code retains
GPL. Licensed-resource admission binds the trusted declaration to account/track,
SHA-256, byte length and revision, rather than trusting a remote license label.
Withdrawing application admission does not revoke a valid CC0 dedication.
