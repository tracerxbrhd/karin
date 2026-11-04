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

Apply the [UI quality gate](https://github.com/tracerxbrhd/karin/blob/master/docs/UI_QUALITY_GATE.md) when the UI changes.
Mark items not affected by this change explicitly rather than claiming an unrun check.

- [ ] Real GTK screenshots and structure checked at compact, medium, wide and maximized sizes.
- [ ] Light/dark themes, HiDPI, resize, keyboard focus and mouse activation checked.
- [ ] Playback, queue occurrences, Shuffle/Repeat, seek, volume and MPRIS remain consistent.
- [ ] Pinned Info/Queue behavior, loading/error/Retry and cleanup checked where affected.
- [ ] Authentication uses synthetic local fixtures in automation, with WebKit sandbox enabled.
- [ ] Only clean synthetic captures are shared; owner desktop images are excluded.

## Results and remaining checks

Record what ran, its result and any environment limitation. Separate CI/fixtures
from owner-reported Fedora or Flatpak acceptance; preserve known failures and
unresolved issues. Include any new manual checks needed for this change.

Merge requires the owner's explicit authorization.
