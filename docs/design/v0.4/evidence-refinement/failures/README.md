<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Retained failed observations

The fresh unchanged a70 default-environment 2× wide/light/TrackInfo close
missed the 75% openness quarter: 100,002 μs elapsed between the retained
initial and first moving frame (1.0 → 0.496166); the consecutive after-paint
gap before that moving frame was 50,001 μs. C7 default-environment 2× compact/light/Queue close missed
that quarter after 66,668 microseconds (1.0 → 0.3811). Their raw observations
remain here; neither full default-environment phase gate passed.

Separate single controlled LP_NUM_THREADS=2 runs passed both scales on a70
(33.05/47.41 seconds) and C7 (34.42/47.64 seconds). The current candidate has
its own source-matched proof. This environment change does not erase failures,
identify their internal cause, change production or accept cold first use.
See docs/VALIDATION.md for the earlier cold/hosted failures and scope.

At head `144492f`, push/MSRV and pull_request/stable modern jobs also failed
2× wide/light/Queue closing. Both consecutive after-paint gaps were 50,001 μs;
100,002 μs was the retained-initial→first-moving interval, not a consecutive
100 ms paint gap. Their observations and job links are retained here.

A separate single normal-damage control pair passed with the same frozen C9
source/cache/LP2 and unmodified 240 ms/phase/geometry/setup guards. Motion unsets
GSK_DEBUG rather than forcing full app redraw; static capture retains full-redraw.
The control is not proof of a unique cause, cold smoothness or future hosted
success. Never rerun or broaden timing guards merely to replace a failed trace.
