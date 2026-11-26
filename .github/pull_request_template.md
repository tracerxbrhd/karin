<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

## Change

Describe the problem and the resulting behavior. Link the relevant issue or ADR.

## Verification

- [ ] Formatting, strict Clippy, tests and builds pass on stable and Rust 1.98.1.
- [ ] Relevant default, all-feature and minimal configurations are checked.
- [ ] Release/packaging and native fixtures are checked where affected.
- [ ] GitHub Actions passes for this HEAD; results are distinguished from earlier revisions.
- [ ] Documentation, Markdown/image links and script paths remain valid.
- [ ] GPL text, SPDX identifiers and required third-party notices are preserved.
- [ ] No credentials, personal account data, signed media URLs or music are included.

## UI and music changes

- [ ] Relevant [UI verification checks](../docs/BUILDING.md#ui-verification) cover the changed interface, playback and session behavior.
- [ ] Authentication automation uses synthetic local fixtures with the WebKit sandbox enabled. Shared captures use synthetic data.

## Results and remaining checks

Record what ran, its result and any environment limitation. Separate CI/fixtures
from owner-reported Fedora or Flatpak acceptance; preserve known failures and
unresolved issues. Include any new manual checks needed for this change.

Merge requires the owner's explicit authorization.
