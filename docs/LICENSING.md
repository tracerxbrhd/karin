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

The published shared OAuth configuration is not Karin's own registration.
Source attribution does not establish its actual holder or permission for Karin's
reuse. Do not disguise that distinction or embed new private registration secrets.

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
| OAuth registration | Karin has its own registered application; the working provider ships Cassette's published shared configuration. Actual holder/reuse authority is not established by GPL. |
| Music API | Owner-accepted private sessions work. Karin's separate PKCE token/Yandex ID success followed by generic 403 on both account methods does not establish its Music grant or the cause of refusal. |
| Integrated public distribution/content | No applicable provider/registration-holder approval or other sufficient basis is recorded. A subscription, local playback and HTTP cacheability do not establish it. |

The focused official review on **2026-11-22** used:

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
not been sent. [#5](https://github.com/tracerxbrhd/karin/issues/5) remains open
pending an applicable basis, its conditions and working permitted access;
[#22](https://github.com/tracerxbrhd/karin/issues/22) also requires the owner's
specific publication decision. This engineering review is not legal clearance.

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
