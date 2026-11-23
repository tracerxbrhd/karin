<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Tabler Icons

This directory retains the original MIT license for the selected SVG artwork
embedded in Karin. Copyright (c) 2020–2026 Paweł Kuna.

- Upstream: https://github.com/tabler/tabler-icons
- Release: **v3.52.0**
- Immutable revision: `4581ad20d8d13e24f6c3be75caaea1c4ad6c4b35`
- Original files are copied byte-for-byte from `icons/outline/`, plus the filled
  heart for a confirmed Like. Only filenames gain Karin's namespace and GTK's
  `-symbolic` suffix; artwork, viewport and stroke width are unchanged.
- GResource embeds these files at build time. GTK performs native symbolic
  foreground recoloring; no network or system Tabler installation is required.
- Karin's original application icon and platform window controls are unchanged.

For each bundled `karin-{name}-symbolic.svg`, the original is
`icons/outline/{name}.svg` at the pinned revision. The one exception is
`karin-heart-filled-symbolic.svg`, copied from `icons/filled/heart.svg`.
The source resource manifest `resources/karin.gresource.xml` lists the exact
selected files. All source SVG bytes are retained, including upstream metadata.
