<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Roadmap

## Current product

The owner has accepted v0.8 functionality on Fedora. The final cleanup and CI
work is tracked in [PR #29](https://github.com/tracerxbrhd/karin/pull/29).
[VALIDATION](VALIDATION.md) records acceptance and exact evidence boundaries;
this roadmap does not announce a release, version change, tag or Flathub package.

The native application now includes protected sign-in, library/catalog search,
GStreamer playback, MPRIS, My Wave, confirmed reactions and playlist editing,
account-bound playback restoration, private HTTP audio caching, Lyrics/related
tracks, search-driven collection indexing and exact queue editing. System is
the available presentation. Karin Style remains reserved for designer materials.
Earlier milestones and merges are linked from
[acceptance history](VALIDATION.md#acceptance-history).

## Maintenance priorities

- Preserve Rust 1.98.1 compatibility, current native/Flatpak builds, protected
  session cleanup, bounded workers and narrow service/sandbox boundaries.
- Keep required CI focused on functional regressions and targeted visual/motion
  checks. Run the manual extended gallery when needed and preserve historical
  comparison sources in Git; measure actual run costs before claiming a speedup.
- Continue [#23](https://github.com/tracerxbrhd/karin/issues/23): explain and verify
  Cloud first-player rendering stalls without changing production timing,
  weakening assertions or concealing cold behavior through artificial prewarming.
- Maintain source-specific Lyrics availability and disclosed LRCLIB consent;
  unsupported modern signing material is outside the implementation.
- Integrate a real Karin design only after licensed assets, responsive states and
  accessibility materials arrive. Follow the
  [designer handoff](design/KARIN_STYLE_INTEGRATION.md) and
  [ADR-0033](adr/0033-independent-ui-style-and-color-scheme.md).

## Independent future decisions

- [#5](https://github.com/tracerxbrhd/karin/issues/5): applicable OAuth registration,
  service and public-distribution conditions. Working local sign-in/audio and GPL
  source licensing do not resolve these questions.
- [#22](https://github.com/tracerxbrhd/karin/issues/22): a separately authorized
  release process, declared scope/version, reproducible artifacts/source/notices
  and target-platform checks after the distribution conditions are resolved.
- Offline playback, downloads and mass prefetch require a separate design with
  actual access, lifetime, storage and revalidation rules. They are not part of
  the current online-revalidated private HTTP cache.

My Wave, reactions, real cache reuse, restoration and owned-playlist editing are
implemented and accepted milestones rather than missing release features.
No new v0.9 work is authorized by the final v0.8 cleanup. Current bounds and
remaining platform limitations are in [LIMITATIONS](LIMITATIONS.md).
