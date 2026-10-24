<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0004: Native Cargo checks and a vendored Flatpak source snapshot

Status: Accepted
Date: 2026-10-24

## Context

Karin must be buildable on Fedora 44 and in Debian-based cloud development without
depending on a Yandex account or on physical audio hardware. Flatpak is the
preferred distribution format, but the cloud container denies Bubblewrap user
namespace creation. GTK/Libadwaita and GStreamer still need meaningful runtime
checks beyond a successful Rust build.

## Decision

- Keep one Cargo package and commit `Cargo.lock`.
- Use GTK >= 4.14 and Libadwaita >= 1.5 feature baselines, compatible with the
  current cloud libraries and the Ubuntu 24.04 CI runner.
- Run formatting, Clippy, unit tests, build, a synthetic GStreamer/fakesink
  diagnostic and actual GTK startup/shutdown under Xvfb in CI. Network fixtures
  remain local and require no user tokens.
- Start Flatpak packaging with GNOME SDK/Platform 50 and its Freedesktop 25.08
  stable Rust SDK extension.
- Prepare an ignored source snapshot with `cargo vendor --locked`, then perform
  the Flatpak Cargo build with `--offline --locked`. Keep generated Cargo source
  replacement configuration out of the development checkout.
- Grant no host filesystem or Secret Service access before those capabilities
  exist. Preserve Flatpak sandboxing; do not work around cloud namespace denials
  with privileged builds.

## Consequences

The lockfile fixes Rust source dependency versions. Stable compilers, SDK updates
and host system libraries may still change, so this is reproducible dependency
selection rather than a claim of bit-identical binaries. A prepared environment
snapshot retains the exact tested compiler. The license choice was subsequently
resolved as GPL-3.0-or-later in [ADR-0005](0005-licensing.md); public distribution
still needs the corresponding source/notices review, package metadata/release
review and a successful sandboxed Flatpak build.

The cloud can validate native library initialization and fakesink operation. The
user has since reported functional startup and audible synthetic output on
Fedora/Wayland/PipeWire, recorded separately in [validation](../VALIDATION.md).
Repeat visual acceptance of the revised layout on Fedora, desktop accessibility
and full Flatpak packaging remain explicit manual acceptance checks.
