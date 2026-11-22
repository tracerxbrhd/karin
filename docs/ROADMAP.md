<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Roadmap

## Current product

The owner has accepted v0.8 functionality on Fedora. Cleanup and optimized CI
were merged in [PR #29](https://github.com/tracerxbrhd/karin/pull/29).
The next engineering candidate is `0.8.0-rc.1`; it is not a published release.
[VALIDATION](VALIDATION.md) records acceptance and exact evidence boundaries.

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

## Public release gates

- [#5](https://github.com/tracerxbrhd/karin/issues/5): applicable OAuth registration,
  service and public-distribution conditions. Working local sign-in/audio and GPL
  source licensing do not resolve these questions.
- [#22](https://github.com/tracerxbrhd/karin/issues/22): prepare the versioned
  candidate, validated metadata, pinned source-based packaging and verifiable
  artifacts/source/notices now. Publication still needs applicable integration
  rights, a decision on #23 and the owner's specific release authorization.
- Offline playback, downloads and mass prefetch require a separate design with
  actual access, lifetime, storage and revalidation rules. They are not part of
  the current online-revalidated private HTTP cache.

My Wave, reactions, real cache reuse, restoration and owned-playlist editing are
implemented and accepted milestones rather than missing release features.
This release-readiness work does not start v0.9. Current bounds and remaining
platform limitations are in [LIMITATIONS](LIMITATIONS.md).
