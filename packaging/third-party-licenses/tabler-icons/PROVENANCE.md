<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Tabler Icons

This directory retains the original MIT license for the selected SVG artwork
embedded in Karin. Copyright (c) 2020–2026 Paweł Kuna.

- Upstream: https://github.com/tabler/tabler-icons
- Release: **v3.52.0**
- Immutable revision: `4581ad20d8d13e24f6c3be75caaea1c4ad6c4b35`
- Selected artwork comes from `icons/outline/`, plus the filled heart for a
  confirmed Like. Filenames gain Karin's namespace and GTK's `-symbolic` suffix.
- Each drawable receives GTK's documented symbolic fill/stroke classes according
  to its effective original colors. Outlines use `transparent-fill foreground-stroke`;
  the filled heart uses `foreground-fill`. Invisible primitives, if present,
  receive `transparent-fill` without a stroke class. These explicit roles are
  needed by GTK 4.20+; the retained SVG colors support older full SVG renderers.
- This normalization changes only `class` attributes. Geometry, viewport,
  original colors, 2 px stroke width, round caps and joins remain unchanged.
- GResource embeds these files at build time. GTK performs native symbolic
  foreground recoloring; no network or system Tabler installation is required.
- Karin's original application icon and platform window controls are unchanged.

For each bundled `karin-{name}-symbolic.svg`, the original is
`icons/outline/{name}.svg` at the pinned revision. The one exception is
`karin-heart-filled-symbolic.svg`, copied from `icons/filled/heart.svg`.
The source resource manifest `resources/karin.gresource.xml` lists the exact
selected files. Original upstream metadata is retained.

GTK's symbolic format and class semantics are documented at
https://gitlab.gnome.org/GNOME/gtk/-/blob/4.22.5/docs/symbolic-icons.md .
