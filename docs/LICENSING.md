<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Licensing and dependency review

Reviewed on **2026-10-24**, with the Gate B1 additions reviewed **2026-10-26**
and optional authentication bindings reviewed **2026-10-29**,
against the committed Cargo.lock and the Debian 13
development environment. Karin's own code and original materials are
**GPL-3.0-or-later**, unless an individual material states otherwise. The full
standard GPLv3 text is in [LICENSE](../LICENSE); [ADR-0005](adr/0005-licensing.md)
records the approved decision. Third-party files retain their original licenses.

The Issue #7 slice was reviewed on **2026-10-28** without adding a crate or
changing `Cargo.lock`. The thin Music adapter uses endpoint/schema knowledge
from the pinned yamu review, with original implementations and GPL SPDX notices;
that Music adapter imports no upstream OAuth defaults, signing/decryption code
or media. The subsequent Cassette auth adaptation described below is separate.
The locked dependency inventory and supplied notices remain applicable. Clean UI
captures use only signed-out states or explicitly synthetic test metadata.

PR #8 adds optional `webkit6`/`webkit6-sys` and
`javascriptcore6`/`javascriptcore6-sys` **0.5.0**, plus
`soup3`/`soup3-sys` **0.8.0**. All six Rust bindings declare MIT. Their notices
are preserved, including the exact-commit recovery for four archive omissions
described below. Default `cassette-auth` now selects these bindings; the inventory
continues auditing the all-feature graph, including the no-GUI Device Flow option.
Neither these licenses nor native WebKit availability establishes service access,
Linux embedded-auth approval or registration distribution rights.

### Cassette authentication adaptation

`src/auth/cassette.rs` adapts the published browser-session/token contract from
Cassette v0.2.4, commit `fc6c6e689bd0c60fa662b03aa2aa3c05534b4811`, under
**GPL-3.0-or-later**, preserving Copyright 2023–2024 Vladimir Vaskov. The published
shared application configuration is retained in `src/auth/cassette_client.json`;
it is upstream app material, not a user's password, OAuth token or session cookie.
The unmodified upstream GPL text and exact provenance are retained at
[`packaging/third-party-licenses/cassette-auth/`](../packaging/third-party-licenses/cassette-auth/).
The source snapshot carries these materials; distributors must preserve them
with Karin's GPL text, notices and corresponding source. This supplemental
source adaptation is outside the Cargo-only crate inventory; no crate license
is silently changed or used to stand in for its attribution.

The owner's functional-prototype scope permits this adaptation without requiring
a Karin-owned registration. GPL source reuse does **not** grant rights to the
shared OAuth registration, Music API/content or trademarks. Express approval
for Karin's reuse/public distribution and Linux WebKit is not established; this
is an unresolved condition, not a confirmed blanket service prohibition or a
technical token/API error. A successful private prototype test does not settle
public release conditions. No official-client Music header, stream-signing key,
DRM logic or protected media is imported. See [ADR-0011](adr/0011-cassette-session-auth-prototype.md)
and [the authentication review](AUTHENTICATION_REVIEW.md).

### v0.2 metadata parser and original presentation

Issue #9 adds pinned `roxmltree` **0.21.1** (`MIT OR Apache-2.0`) for bounded
metadata-only XML classification. Its MIT option is compatible with Karin's
GPL-3.0-or-later distribution; retain the supplied upstream license and notice.
This parser performs no external entity resolution, signing, decryption or media
delivery. The refreshed inventory contains **207 external crates**, **178** in
the Linux graph. The older counts below describe the preceding lockfile review.

Catalog repair, the delivery probe and subsequent original navigation/artwork
code retain Karin SPDX notices. Endpoint/model analysis cites pinned sources;
no new Cassette artwork, graphical assets, checksum prefix, media-signing key,
protected media or OAuth identity is imported. Public source licensing does not
settle the existing experimental registration or service-distribution conditions.
Fetched cover images are remote service material, not relicensed Karin assets;
they must remain bounded memory-only presentation, with no bundled private art.

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

[`DEPENDENCY_LICENSES.md`](DEPENDENCY_LICENSES.md) records **206 external crates**
across the resolved all-feature lockfile graph and **177** in Cargo's
`x86_64-unknown-linux-gnu` graph, including build dependencies. This includes
platform alternatives/build tools and is not a claim that every resolved package
is linked into the Linux executable. No crate is missing a reviewed declaration.
Four published binding archives omit their declared MIT license text; their
repository's actual license is preserved through a narrowly pinned fallback.

Reproduce the inventory with the trusted Cargo registry configuration:

```sh
python3 packaging/license_inventory.py --output docs/DEPENDENCY_LICENSES.md
```

In the prepared cloud environment, prefix the command with
`/workspace/karin-environment/run`. The script uses
`cargo metadata --locked --all-features`, never changes Cargo.lock and fails on
missing/unreviewed expressions or absent supplied notices unless the exact
reviewed fallback below applies. A successful inventory is not a substitute for reviewing new
dependency versions and their source-level notices.

### WebKit binding archive notice recovery

The `javascriptcore6`, `javascriptcore6-sys`, `webkit6` and `webkit6-sys` **0.5.0**
archives declare MIT but omit license files. Their published `Cargo.toml` and
`.cargo_vcs_info.json` identify `webkit6-rs` commit
`545df16de5ca5bba4b1b8b456f75a508c98d5ba2`. The repository-root
[upstream LICENSE at that commit][webkit-license] is retained unmodified at
[`packaging/third-party-licenses/webkit6-rs-0.5.0/LICENSE`](../packaging/third-party-licenses/webkit6-rs-0.5.0/LICENSE):
**1069 bytes**, SHA-256
`44dad7b2e199b0d355adf4437df5c9bb18633d47804242f24dab5a633faba5f7`, copyright
**2023 World / Rust**. It is not a Karin-authored or GPL-relabeled text.

The [provenance record](../packaging/third-party-licenses/webkit6-rs-0.5.0/PROVENANCE.md)
includes immutable source, VCS paths and archive checksums verified against
Cargo.lock. The generator uses this text only for the four explicit name/version
pairs after checking repository, MIT declaration, VCS commit/path and license
hash. Missing fallback files, changed license/source provenance, other versions and unknown
omissions fail review. It copies the upstream LICENSE and its provenance into
each affected crate's generated notices without altering archives or vendored
crate sources. The two soup3 archives already supply their actual MIT notices
and require no fallback.

| Declared terms / reviewed case | Compatibility basis and obligations |
| --- | --- |
| MIT, Apache-2.0, ISC, BSD-3-Clause, BSL-1.0, Zlib; alternatives including Unlicense | Permissive grants can be combined with GPLv3 while retaining their notices. For dual/multiple licensing, a compatible offered option is sufficient; obligations attached by `AND` remain cumulative. Apache-2.0 is compatible with GPLv3, not a statement about GPLv2-only. |
| Legacy `MIT/Apache-2.0` in `option-operations` and `serde_urlencoded` | Upstream supplies both license files; treated as the customary offered MIT/Apache alternatives, not an invented new license. Preserve both supplied texts in the notice bundle. |
| Unicode-3.0; `(MIT OR Apache-2.0) AND Unicode-3.0` | Reviewed the actual Unicode notice in ICU4X/`unicode-ident`: permissive copying/modification grant with copyright/permission-notice retention and name/trademark restrictions. The additional Unicode notice is retained even when MIT is selected for code. |
| `Apache-2.0 WITH LLVM-exception` in `target-lexicon` | Reviewed the supplied Apache license and LLVM exceptions. The additional permission does not prevent GPLv3 combination; retain the entire supplied text. WASI's target-only alternative also permits plain MIT/Apache. |
| `ring 0.17.14`: `Apache-2.0 AND ISC` | Inspected `LICENSE`, `LICENSE-BoringSSL`, `LICENSE-other-bits`, nested once_cell licenses and `third_party/fiat/LICENSE`. Both Apache and ISC obligations apply, with nested notices preserved. This version does **not** declare the older `OpenSSL` license expression; no blanket OpenSSL exception is being added to Karin. |
| `r-efi` offers `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | A compatible permissive option is available; this crate is not in the inspected Linux target graph. |
| Gate B1: `rustix 1.1.5`, `linux-raw-sys 0.12.1`, `errno 0.3.14` | Reviewed supplied COPYRIGHT/license files. The first two offer MIT, Apache-2.0 or Apache-2.0 with LLVM exception; `errno` offers MIT/Apache-2.0. A compatible permissive choice exists and original notices are retained. These support bounded, no-echo local terminal input. `base64`, `ring`, `subtle` and `zeroize` become direct dependencies at their already reviewed locked versions. |
| Optional WebKit/JavaScriptCore 0.5.0 and soup3 0.8.0 Rust bindings | MIT permits GPLv3 combination with the original copyright/permission notice retained. Preserve the reviewed upstream WebKit license for the four archive omissions and supplied soup3 notices. This covers Rust bindings, not the native engine/runtime or service rights. |

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
| Optional WebKitGTK6, JavaScriptCore and libsoup | Native components have their own LGPL-2.1-or-later/BSD-style and per-file terms, separate from the MIT Rust bindings. They are supplied by the host or GNOME 50 runtime; no extra native module/runtime version or sandbox grant is introduced. Distributors must preserve the exact components' notices and satisfy applicable source/relinking obligations. Review the actual runtime revision and bundled subcomponents before a release; the Cargo inventory is not that review. |
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
paths, including nested notices. For the four pinned archive omissions it also
copies the recovered upstream LICENSE and provenance into each crate's notice
directory. It covers all resolved all-feature crates so optional and platform/build
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

[webkit-license]: https://gitlab.gnome.org/World/Rust/webkit6-rs/-/raw/545df16de5ca5bba4b1b8b456f75a508c98d5ba2/LICENSE
