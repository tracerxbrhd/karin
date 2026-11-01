<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Retained failed observations

The fresh unchanged a70 default-environment 2× wide/light/TrackInfo close
missed the 75% openness quarter after a 100,002 microsecond native frame gap
(1.0 → 0.496166). C7 default-environment 2× compact/light/Queue close missed
that quarter after 66,668 microseconds (1.0 → 0.3811). Their raw observations
remain here; neither full default-environment phase gate passed.

Separate single controlled LP_NUM_THREADS=2 runs passed both scales on a70
(33.05/47.41 seconds) and C7 (34.42/47.64 seconds). The current candidate has
its own source-matched proof. This environment change does not erase failures,
identify their internal cause, change production or accept cold first use.
See docs/VALIDATION.md for the earlier cold/hosted failures and scope.
