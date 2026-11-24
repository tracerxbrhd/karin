<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0001: Native Linux application stack

Status: Accepted; current implementation contract.

## Decision

Use Rust, GTK4/Libadwaita and GStreamer in one Cargo package. Keep Yandex Music behind a
typed service boundary; WebKitGTK6 is an authentication component only. Do not replace the
native application with a browser wrapper or a second player.

Keep the Rust 1.98 baseline, test exact 1.98.1 and current stable, and preserve locked
dependencies and the current GNOME 50 / Freedesktop Rust 25.08 Flatpak SDK. Dependencies
and native feature floors require demonstrated compatibility before change. Never bypass
Cargo's rust-version check.

## Consequences

Native development needs GTK/GStreamer libraries. Flatpak is a separate sandboxed source
build, not proof that a Native binary works on every distribution.
[BUILDING](../BUILDING.md) defines executable checks and packaging instructions;
[LICENSING](../LICENSING.md) defines source/notices and public-service boundaries.
