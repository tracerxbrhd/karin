<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0037: Incremental current-marker updates without renderer workarounds

Status: v0.8 implementation decision; matched measurements are separate evidence.
Date: 2026-11-14
Extends: ADR-0024, ADR-0026 and ADR-0035

## Context

Each appended catalog row previously pruned and synchronized every registered
current-track marker again. Constructing a collection therefore repeated work
on an increasing prefix. The ordinary player snapshot timer also synchronized
all markers when current occurrence/context had not changed. These are Karin
application costs independent of the separately observed Cloud first-render
LLVM/software-rendering stall in issue #23.

## Decision

Initialize a newly registered row's marker directly from the current immutable
context/occurrence. Keep full marker reconciliation for real presentation/context
changes, including new current occurrence, queue permutation, refreshed projection
and mode-dependent matching. A cheap exact presentation snapshot guard skips a
repeated full scan on unchanged player ticks. Source/position/occurrence checks
remain authoritative; equal provider IDs cannot mark the wrong duplicate.

Retain weak widget ownership and bounded cleanup. Do not add a second marker
truth model or use a timer-driven cache that can lag selection changes. Real
current changes and newly visible/rebound rows still receive their native style
and accessibility updates. Audio, resolver requests, MPRIS state, native motion,
Reduced Motion and theme semantics are unchanged.

Measure before/after with the same original synthetic 500-row and 2,000-row
construction and unchanged-current update workloads, same toolchain/libraries,
renderer, scale and display setup, with no concurrent heavy work. Record actual
source/binary identities, marker-operation counts, wall-time samples and the
instrumentation boundary. Counts explain the avoided work; noisy wall time is
reported as observed, not invented as a guaranteed physical-desktop speedup.
Execution receipts and results belong in [VALIDATION](../VALIDATION.md).

## Executed matched observation — 2026-11-14

Six fresh native processes, three samples per lane, ran in interleaved order
with one private profile each and no competing heavy work. Both lanes used Rust
1.98.1 unoptimized test ELFs, GTK 4.18.6, Cairo/full-redraw and Xvfb 1920×1080,
96 dpi, scale 1. The baseline is accepted production `949d1f9` with only the
identical observer fixture/module added. The candidate is the pinned development
source set; a final exact-head build/CI is separate.

| Rows | Append marker calls, before → after | Append median, µs | 24 unchanged updates: marker calls | 24 updates median, µs |
| --- | ---: | ---: | ---: | ---: |
| 500 | 125,250 → 500 | 584,515 → 366,038 | 12,000 → 0 | 21,634 → 9,296 |
| 2,000 | 2,001,000 → 2,000 | 3,566,342 → 1,664,445 | 48,000 → 0 | 50,841 → 11,992 |

Real selection and future-edit reconciliation still evaluated every applicable
marker; source-window identity and current occurrence assertions passed. Neither
lane created a player or performed HTTP. A second narrow change prunes cache
presence weak targets only when its real 500 ms batch is due, rather than every
80 ms player tick; it has no independent timing claim. The table describes the
whole matched candidate workload, not an isolated attribution of every saved
microsecond to one line of code.

Observer SHA-256: `8138b2eef4754d8d3a9c28d571c7c38850196bde25387adf97622a13c39b1af4`.
Baseline ELF: `ca1fbecbdab95713c90179d778c6524de9788c1dbee608e92f91fb6bc2447b86`.
Candidate ELF: `99921b14cd4574dedcb6623030b4c9c6c52f23882028daf71b5ae70131a21b52`.
Full conditions, candidate source-map identity and the separate unaccepted NGL
cold observations are recorded in [VALIDATION](../VALIDATION.md#v08-matched-application-work-measurement).
These values measure GTK main-thread construction/update work, not scrolling
FPS, production release speed, first-frame latency or physical Fedora Wayland.

## Consequences

This is a narrow application hot-path change, not an assertion that the complete
startup, GPU or network stack is optimized. The unchanged cold Player Bar
observer remains required. Do not change production renderer choice, animation
duration, geometry/timing/sandbox gates, add artificial delays/frames, or prewarm
rendering to turn that observation green. Existing failure/baseline evidence is
retained. Only a separately explained verified result or explicit owner
resolution can close #23.

Native tests must cover new-row style, exact duplicates, current/queue changes,
removed weak rows, search/sort/index projections, and no repeated full scan for
unchanged snapshots. Theme/HiDPI/keyboard/normal-motion/Reduced-Motion checks and
existing audio/cache/Wave/MPRIS regressions remain necessary. Cloud/Xvfb
measurements do not establish behavior on physical Fedora Wayland/PipeWire.
