<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Recovered upstream WebKit6 Rust license

Reviewed 2026-10-29. The accompanying `LICENSE` is an **unmodified upstream MIT
notice**, copyright **2023 World / Rust**. It is third-party material and is not
relicensed under Karin's GPL notice on this provenance document.

The published crates `javascriptcore6`, `javascriptcore6-sys`, `webkit6` and
`webkit6-sys`, all **0.5.0**, declare MIT but omit a license file in their archives.
Each archive's `Cargo.toml` identifies
`https://gitlab.gnome.org/World/Rust/webkit6-rs`, and `.cargo_vcs_info.json`
identifies commit **545df16de5ca5bba4b1b8b456f75a508c98d5ba2**.

The exact repository-root [LICENSE at that commit][license] was retrieved over
HTTPS on 2026-10-29. It is **1069 bytes**, SHA-256
**44dad7b2e199b0d355adf4437df5c9bb18633d47804242f24dab5a633faba5f7**.

| Published crate | VCS path | Archive SHA-256, matching Cargo.lock |
| --- | --- | --- |
| javascriptcore6 0.5.0 | javascriptcore6 | `a9f6cb26e6e70d0f21807bd62f5304edd0094b0b476adb0410cb2bc765131736` |
| javascriptcore6-sys 0.5.0 | javascriptcore6/sys | `a67c47e63f3a29c6c78f9d9c5f372e3ca7628ccff77621e54412182e9f83433b` |
| webkit6 0.5.0 | webkit6 | `bad37100b83a482d3a6742bc61c92279dac04bcbbfe4594dff6c5336fc076f5c` |
| webkit6-sys 0.5.0 | webkit6/sys | `5ac8187279534b19b34396aaf5ddfee820590f6371680d140d422fa580028c94` |

`packaging/license_inventory.py` permits this fallback only for these exact
name/version pairs, repository, MIT declaration and VCS commit/path, and checks
the license SHA-256 before use. It copies `LICENSE` and this provenance into each
affected crate's generated notice directory without modifying registry archives
or vendored crate sources. Unknown omissions, missing fallback files, changed
source provenance or changed license bytes fail the inventory.

The additional `soup3` and `soup3-sys` **0.8.0** archives supply their own MIT
LICENSE files (SHA-256
`7ade43ec5e1a5e43000aa871d290dff4c0e9e2e16b7f6bea2ecf091e3d056d31`);
they use the ordinary notice-preservation path, with no fallback.

These are Rust-binding notices. The native WebKitGTK, JavaScriptCore and libsoup
components retain their own licenses and distribution/source obligations. Neither
code license grants OAuth application-identity rights or Yandex Music access.

[license]: https://gitlab.gnome.org/World/Rust/webkit6-rs/-/raw/545df16de5ca5bba4b1b8b456f75a508c98d5ba2/LICENSE
