<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Selected real GTK captures

These eight untouched 1× PNGs are original synthetic fixture captures, licensed
GPL-3.0-or-later. All show the information pane open. They contain no real
account, service artwork, credential or music recording. Full paired light/dark,
1×/2×, open/closed/maximized evidence and geometry belong to the PR CI artifact;
this selection supplements that matrix.

Before: accepted production source `5e85e8d21a0e7075787d1242ed5abae0166f000c`,
with the shared test-only evidence harness, executable SHA-256
`def3af020aa5609585a80b6deed0f1650e4ffa83df3f79ed8fc636756aee4a1d`.
After: source-frozen Native Music implementation, executable SHA-256
`be5b842cfead7ba8a795aa3d10e9180f5820ae4c60e5d81893d5ebe7eb18aeb8`.
Later changes to test assertions do not alter the production composition;
hosted captures identify their own immutable source and executable separately.

Environment: isolated Xvfb/Openbox, Cairo/full-redraw, GTK4 4.18.6,
Libadwaita 1.7.6, generated covers fetched/decoded over loopback HTTP.
These are actual widgets, not the approved SVG sketches, and do not establish
Fedora/KDE/Wayland/PipeWire acceptance.

```text
07e83d1507a53f34ab1d30efda371e5b1420fa8dc70dd9bfe29d369ac37ab1b4  before-compact-dark.png  420x720
bb6cc195793a36ea09002b929e46628c7580b22c14f00cb8a89675c557f50034  after-compact-dark.png  420x720
184be97aaba9270c30f4ecc9e09939337b2e8651c35019cc87b36b6fe6916f63  before-medium-dark.png  900x650
b267015b9a9428c5d9769b4de36355912fe405a2802ff813c361863372a69a69  after-medium-dark.png  900x650
f6c22b402f005daeb3971cbf0b38b219a5e00426fe4376cfc62f45e17c80bc2d  before-wide-dark.png  1440x900
1ca0eef69971481112389a2be6c53a1bfcf15d412216449e7acfb672d71d2180  after-wide-dark.png  1440x900
a49fb4e11e132b74f77e98df826c1623ffe0c782c237512680be4943eedc2c4f  before-wide-light.png  1440x900
e41753c6c3f2d43eea5dd220bf7b9e16acf027dfe175db45b8df86ec78550a7e  after-wide-light.png  1440x900
```
