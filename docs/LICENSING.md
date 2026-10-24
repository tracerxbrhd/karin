<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Licensing and dependency review

Reviewed on **2026-10-24** against the committed Cargo.lock and the Debian 13
development environment. Karin's own code and original materials are
**GPL-3.0-or-later**, unless an individual material states otherwise. The full
standard GPLv3 text is in [LICENSE](../LICENSE); [ADR-0005](adr/0005-licensing.md)
records the approved decision. Third-party files retain their original licenses.

## License text and scope

`LICENSE` is copied byte-for-byte from the Debian base image's standard
`/usr/share/common-licenses/GPL-3` (35,149 bytes, SHA-256
`3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986`). It includes
all GPLv3 terms and the standard application guidance, with no Karin edits.
References are the [GNU text](https://www.gnu.org/licenses/gpl-3.0.txt) and the
[SPDX GPL-3.0-or-later text](https://raw.githubusercontent.com/spdx/license-list-data/main/text/GPL-3.0-or-later.txt).
Comparison with the latter found presentation differences and a different final
GNU guidance-link path, with the license terms unchanged. The
`GPL-3.0-or-later` grant is expressed by the source notices and metadata; the
standard GPLv3 license document itself is unchanged.

The scope includes Karin's original Rust/Python source, original icon and original
documentation/build materials. It does not relicense third-party dependencies,
GNOME theme icons, fonts, user screenshots, Yandex content or service responses.
No third-party source or license text is overwritten with a Karin SPDX header.

## Locked Rust dependencies

[`DEPENDENCY_LICENSES.md`](DEPENDENCY_LICENSES.md) records **197 external crates**
across the resolved lockfile and **168** in Cargo's
`x86_64-unknown-linux-gnu` graph, including build dependencies. This includes
platform alternatives/build tools and is not a claim that every resolved package
is linked into the Linux executable. No crate is missing a declared license or
supplied notice/license file in the reviewed registry sources.

Reproduce the inventory with the trusted Cargo registry configuration:

```sh
python3 packaging/license_inventory.py --output docs/DEPENDENCY_LICENSES.md
```

In the prepared cloud environment, prefix the command with
`/workspace/karin-environment/run`. The script uses `cargo metadata --locked`,
never changes Cargo.lock and fails on missing/unreviewed expressions or absent
supplied notices. A successful inventory is not a substitute for reviewing new
dependency versions and their source-level notices.

| Declared terms / reviewed case | Compatibility basis and obligations |
| --- | --- |
| MIT, Apache-2.0, ISC, BSD-3-Clause, BSL-1.0, Zlib; alternatives including Unlicense | Permissive grants can be combined with GPLv3 while retaining their notices. For dual/multiple licensing, a compatible offered option is sufficient; obligations attached by `AND` remain cumulative. Apache-2.0 is compatible with GPLv3, not a statement about GPLv2-only. |
| Legacy `MIT/Apache-2.0` in `option-operations` and `serde_urlencoded` | Upstream supplies both license files; treated as the customary offered MIT/Apache alternatives, not an invented new license. Preserve both supplied texts in the notice bundle. |
| Unicode-3.0; `(MIT OR Apache-2.0) AND Unicode-3.0` | Reviewed the actual Unicode notice in ICU4X/`unicode-ident`: permissive copying/modification grant with copyright/permission-notice retention and name/trademark restrictions. The additional Unicode notice is retained even when MIT is selected for code. |
| `Apache-2.0 WITH LLVM-exception` in `target-lexicon` | Reviewed the supplied Apache license and LLVM exceptions. The additional permission does not prevent GPLv3 combination; retain the entire supplied text. WASI's target-only alternative also permits plain MIT/Apache. |
| `ring 0.17.14`: `Apache-2.0 AND ISC` | Inspected `LICENSE`, `LICENSE-BoringSSL`, `LICENSE-other-bits`, nested once_cell licenses and `third_party/fiat/LICENSE`. Both Apache and ISC obligations apply, with nested notices preserved. This version does **not** declare the older `OpenSSL` license expression; no blanket OpenSSL exception is being added to Karin. |
| `r-efi` offers `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | A compatible permissive option is available; this crate is not in the inspected Linux target graph. |

No incompatible license requirement was identified in the reviewed declarations
and uncommon source notices. This is a scoped engineering review, not legal
advice or a guarantee about future versions, other targets or a final packaged
artifact. Relevant compatibility guidance:
[GNU's license list](https://www.gnu.org/licenses/license-list.html) and
[GPLv3 compatibility discussion](https://www.gnu.org/licenses/gpl-faq.html#GPLCompatMatrix).

## Native libraries and distribution components

The Rust binding crates' licenses do not replace the native libraries' licenses.
`ldd target/debug/karin` was inspected: GTK, Libadwaita, GLib and GStreamer are
dynamically linked, along with their system dependencies. Installed Debian
copyright files were reviewed for the principal components below.

| Component | Reviewed native scope |
| --- | --- |
| GTK4, Libadwaita, GLib/GIO and libsecret | Principally LGPL-2.1-or-later; individual bundled files/materials have their own notices. Dynamic use is compatible with the GPLv3 application, while native-library source/relinking and notice duties remain with distributors. libsecret presence is inspected over D-Bus; Karin currently does not link a credential-storage implementation. |
| GStreamer core and installed base/good plugins | Principally LGPL-2.0-or-later / LGPL-2.1-or-later with per-file exceptions. The current pipeline uses a synthetic source, conversion/resampling and a sink, not a downloaded codec pack. Additional plugins/codecs must be reviewed for their actual licenses and applicable patent/distribution constraints. |
| D-Bus | The installed shared library offers `GPL-2.0-or-later OR AFL-2.1`; the GPL-later route permits GPLv3 combination. Karin currently uses GIO's D-Bus implementation and does not bundle a bus daemon. Separate utility/package files can have different terms. |
| glibc and GCC runtime | glibc is principally LGPL-2.1-or-later; libgcc uses GPLv3 with GCC Runtime Library Exception. These are native runtime components, not relicensed by Karin. |
| OpenSSL 3 in the development environment | Apache-2.0. Karin's HTTP implementation uses rustls/ring, not reqwest's OpenSSL backend. An installed development package is not proof that it is shipped or linked. Old OpenSSL versions would require their own review. |
| Rust standard library/toolchain, GNOME SDK, Blueprint and build tools | Build tools are not automatically part of the shipped application. Rust's distributed components include their own MIT/Apache/LLVM-related notices; retain applicable notices for linked runtime pieces. The SDK and its tools retain their own component licenses. |
| GNOME Flatpak runtime | A separately managed runtime with many component licenses. The initial manifest is not a complete runtime SBOM. A full Flatpak build is blocked in this cloud container, so final runtime/extension/license-content validation remains a release prerequisite. |

Do not add codec bundles, proprietary media, third-party artwork, or runtime
libraries to a release merely because they are installed in a developer machine.
The exact shipped native libraries, plugins, fonts/icons and runtime revisions
must be inventoried when making a release, including any extra notices or source
obligations not captured by Cargo metadata.

## Preserving notices and corresponding source

`packaging/flatpak/prepare-source.py` vendors the locked source without changing
its license files and runs the inventory tool. A generated
`build/flatpak-source/third-party-notices/` tree preserves supplied
LICENSE/LICENCE/COPYING/NOTICE/COPYRIGHT/AUTHORS files at their original relative
paths, including nested notices. It covers all resolved crates so platform/build
alternatives are not silently discarded; the inventory distinguishes scope.
The manifest installs Karin's LICENSE, this document and that tree under
`/app/share/licenses/karin`. Do not commit generated vendored sources or notices
as if they were original GPL code.

Before conveying a binary or Flatpak, provide the complete corresponding source
and build/install information required by the GPL using an applicable section 6
method. That includes Karin modifications, dependency sources actually needed to
build the covered artifact, and required notices; a GitHub link or a LICENSE file
alone is not a universal substitute. Preserve runtime/library distributors'
notices and satisfy their applicable obligations. Source generated from a worktree
must be reviewed before distribution, including licenses and unintended files.

This PR does not publish a release or certify the unbuilt Flatpak. GPL grants
rights in software, **not** permission to access Yandex Music, use its API or
trademarks, or distribute its audio. Service terms, account rights and the
blockers in [API_FEASIBILITY.md](API_FEASIBILITY.md) remain separate.
