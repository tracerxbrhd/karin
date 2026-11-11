<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Licensing

Karin's own code and original materials use **GPL-3.0-or-later**, unless a file
states otherwise. [LICENSE](../LICENSE) contains the complete standard GPLv3
text; the later-version grant is expressed in source notices and Cargo metadata.
[ADR-0005](adr/0005-licensing.md) records the decision. Third-party code, fonts,
theme icons, service content and user material retain their respective rights.

## Adapted code and notices

The browser-session authentication contract is adapted from Cassette v0.2.4,
revision `fc6c6e689bd0c60fa662b03aa2aa3c05534b4811`, under GPL-3.0-or-later,
Copyright 2023–2024 Vladimir Vaskov. Preserve:

- [Cassette COPYING](../packaging/third-party-licenses/cassette-auth/COPYING)
  and [provenance](../packaging/third-party-licenses/cassette-auth/PROVENANCE.md),
  including the pinned authentication and legacy playback references;
- the [application-configuration attribution](../src/auth/cassette_client.json.license)
  beside `src/auth/cassette_client.json`;
- existing source copyright and SPDX notices.

The shared configuration identifies the published upstream OAuth registration.
It must not be presented as Karin's own registration. Source-code permission,
registration reuse, service access and public distribution require separate
consideration; GPL does not grant Yandex API, music-content or trademark rights.
Express approval for Karin's registration reuse/public distribution and Linux
embedded authentication remains unestablished. Local interoperability does not
settle those conditions. See [authentication](AUTHENTICATION_REVIEW.md).

The independent Rust cache scheduling follows the released Cassette
v0.2.4/v0.2.5 local-first/background-save pattern. The
[pinned source review](AUDIO_CACHE_REVIEW.md#cassette-source-audit) credits
Vladimir Vaskov and GPL-3.0-or-later. No cache source, reversible encoding,
graphics, client secrets or new signing material is copied by this change;
existing authentication/playback attribution remains intact.

## Locked Rust dependencies

[DEPENDENCY_LICENSES.md](DEPENDENCY_LICENSES.md) is the reproducible inventory:
207 external crates in the all-feature lockfile graph, 178 in the inspected Linux
graph, including build dependencies. These counts do not imply every package is
linked into the executable. No incompatible requirement was identified in that
reviewed graph using the available compatible license alternatives.

Regenerate it after a dependency change:

```sh
python3 packaging/license_inventory.py --output docs/DEPENDENCY_LICENSES.md
```

The generator uses locked metadata, preserves supplied notices and rejects
missing or unreviewed terms. Review actual sources when adding dependencies;
retain cumulative `AND` obligations, Unicode notices, exceptions and nested
notices such as ring's Apache/ISC material. A Cargo declaration alone does not
replace source-level notice review.

Four MIT-licensed 0.5.0 binding archives omit their license text:
`javascriptcore6`, `javascriptcore6-sys`, `webkit6` and `webkit6-sys`. Their exact
upstream MIT text and [VCS/archive provenance](../packaging/third-party-licenses/webkit6-rs-0.5.0/PROVENANCE.md)
are retained under `packaging/third-party-licenses/webkit6-rs-0.5.0/`.
The pinned fallback checks names, versions, repository, VCS paths and license
SHA-256. Preserve it unmodified; other missing notices must fail review.
The soup3 bindings supply their own MIT notices.

## Native components and distribution

Rust-binding licenses do not replace the licenses of GTK, Libadwaita, GLib/GIO,
WebKitGTK, JavaScriptCore, libsoup, GStreamer, codecs or system runtimes. These
components have LGPL, BSD-style and other per-file terms. Review the exact
libraries/plugins/runtime being conveyed, including source, relinking and notice
obligations. A GNOME runtime manifest or Cargo inventory is not its complete SBOM.
Build tools installed on a developer machine are not automatically shipped.

`packaging/flatpak/prepare-source.py` vendors locked dependencies and creates
`third-party-notices/`. The inventory copies supplied LICENSE, COPYING, NOTICE,
COPYRIGHT and related files at their original relative paths, including nested
notices and the reviewed binding fallback. The Flatpak manifest installs Karin's
GPL text, this document and the notice bundle under `/app/share/licenses/karin`.
Keep `packaging/third-party-licenses/` in source snapshots and binary notice bundles.

Before conveying a covered binary, provide complete corresponding source and
build/install information through an applicable GPL section 6 method. Preserve
dependency and runtime distributors' obligations. A repository link or LICENSE
file alone is not a universal substitute. Inspect generated source snapshots for
unintended files before distribution. v0.4 acceptance does not publish a release.

## Service material

Remote covers and catalog metadata are service material, not original GPL artwork.
Karin's bounded data cache respects explicit HTTP cache restrictions and freshness.
Private audio caching separates current playback authorization, actual HTTP
storage directives, representation validation and content restrictions. Its
local HTTP admission is not a provider entitlement or a copyright license.
Explicit refusals and protected/unsupported media are not overridden. Signed
media URLs and credentials are never persisted. Public-distribution and
applicable content rights remain separate unresolved questions in #5/#22;
HTTP cacheability and source-code licensing do not settle them.

The original audio bytes generated by the full application cache fixture are
expressly dedicated to the public domain under
[CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/). Only that original
audio output has this dedication; its Rust generator and application source keep
their GPL-3.0-or-later notices. The production licensed-resource policy binds the
trusted declaration to account/track, SHA-256, byte length and an admission
revision, rather than trusting a remote `license` label. Withdrawing application
admission does not claim that a valid CC0 dedication itself can be revoked.
Public screenshots use original fixture art. The [storage/reuse boundary](AUDIO_CACHE_REVIEW.md)
remains separate from the working [online player](STREAM_DELIVERY_REVIEW.md),
and the OAuth registration/public-distribution uncertainty above remains separate
from both.
