# E final C14 published-original review

All28 actual files in the final E partition were opened individually at native resolution, including HiDPI originals. Their public-copy SHA256 values match the exact C14 source PNGs28/28. The final count is10 initialized-panel frames plus18 startup/Stop/paused images; it intentionally follows the actual retained observations rather than forcing C12's historical30-image count.

- C14 binary: `c927b8e1dfa405398c8c2dc68ca402d3cd899e7867f08cbfc2aa08bf54331bb5`
- Source manifest: `3faefc3e5d8e69ce475e148ffef92cecb5ccbaf6631ac14661993cbbf1db7bf4`
- Scope: these28 assigned originals, plus one separately recorded source-original header recheck. No claim that E manually opened every one of260 static images.

## Observed structure

No new separate structural P0/P1 was found. Medium TrackInfo opening keeps background, border, heading, cover, title/artist/album and actions together on one moving/clipped surface. Compact Queue closing likewise keeps Back, queue source/count, rows/art/text/actions together. Closing images are named by openness: chronological review is100→075→050→025→000. The footer remains readable and usable throughout the initialized panel sequence.

The central Liked cover changes responsive tier144→88 when its available width crosses640 during medium panel opening; central row reflow is distinct from the rigid moving panel. The final short Info cluster and controls fit the available viewport.

Fresh-start images show an actually hidden footer and no Info surface before the real row click. Partial first-reveal samples show genuine loading state:1× still has placeholder art at its full-reveal sample, while2× has already decoded it; the heart is unconfirmed until later. Paused light/dark captures have decoded artwork and a confirmed blue circular heart beside metadata. Stop partials are natural clipping of the actual native closing allocation; both final endpoints contain no residual footer and release the list viewport. The separate raw lifecycle audit verifies child visibility throughout positive-height closing frames and native hidden child/Revealer/host minimum/natural widths(0,0).

## Unaccepted cold behavior

**Cold first-use cadence remains FAIL / unaccepted.** Actual adjacent native frame gaps are900,018µs at1× and850,017µs at2×, with only one intermediate footer frame per scale. These stills cannot support a claim of smooth startup animation. They use a fresh application and fresh synthetic XDG/Mesa disk cache; no root cause or Fedora performance is established. The historical116,669µs record remains separate. Initialized-renderer panel proof is not a substitute for this cold gate.

## Header recheck

Root's initial multi-image view suggested a clipped header at850dark. Independently opening the exact unchanged original alone showed the full title, avatar, menu and all window controls. SHA: `3e8266359214c83961f476a39636b09cbfad1cc5197a7b6fa88c7abb68161856`. The suspicion was withdrawn by both reviewers; no GTK/capture defect was established and no rerun/source change was made.

## Exact28 reviewed gallery originals

Relative to `docs/design/v0.4/evidence-final-refinement/`; `result.json` records each original source mapping, PNG dimensions and SHA256.

- `current/motion-scale1/animation-medium-dark-track-opening-000-scale1.png`
- `current/motion-scale1/animation-medium-dark-track-opening-025-scale1.png`
- `current/motion-scale1/animation-medium-dark-track-opening-050-scale1.png`
- `current/motion-scale1/animation-medium-dark-track-opening-075-scale1.png`
- `current/motion-scale1/animation-medium-dark-track-opening-100-scale1.png`
- `current/motion-scale2/animation-compact-light-queue-closing-000-scale2.png`
- `current/motion-scale2/animation-compact-light-queue-closing-025-scale2.png`
- `current/motion-scale2/animation-compact-light-queue-closing-050-scale2.png`
- `current/motion-scale2/animation-compact-light-queue-closing-075-scale2.png`
- `current/motion-scale2/animation-compact-light-queue-closing-100-scale2.png`
- `current/cold-scale1/footer-startup-explicit-stop-frame000-scale1.png`
- `current/cold-scale1/footer-startup-explicit-stop-frame002-scale1.png`
- `current/cold-scale1/footer-startup-explicit-stop-frame004-scale1.png`
- `current/cold-scale1/footer-startup-explicit-stop-frame008-scale1.png`
- `current/cold-scale1/footer-startup-first-start-frame000-scale1.png`
- `current/cold-scale1/footer-startup-first-start-frame003-scale1.png`
- `current/cold-scale1/footer-startup-first-start-frame005-scale1.png`
- `current/cold-scale1/startup-footer-paused-scale1-dark.png`
- `current/cold-scale1/startup-footer-paused-scale1-light.png`
- `current/cold-scale2/footer-startup-explicit-stop-frame000-scale2.png`
- `current/cold-scale2/footer-startup-explicit-stop-frame002-scale2.png`
- `current/cold-scale2/footer-startup-explicit-stop-frame003-scale2.png`
- `current/cold-scale2/footer-startup-explicit-stop-frame007-scale2.png`
- `current/cold-scale2/footer-startup-first-start-frame000-scale2.png`
- `current/cold-scale2/footer-startup-first-start-frame003-scale2.png`
- `current/cold-scale2/footer-startup-first-start-frame005-scale2.png`
- `current/cold-scale2/startup-footer-paused-scale2-dark.png`
- `current/cold-scale2/startup-footer-paused-scale2-light.png`

Review added only these external report files. No production/test source, gallery/docs, builds or native runtime were changed or executed during this image review. No Fedora, full Flatpak, hardware-graphics, real-account or audible-music acceptance is claimed.
