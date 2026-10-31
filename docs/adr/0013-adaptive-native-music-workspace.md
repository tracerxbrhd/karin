<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0013: Adaptive native music navigation and bounded cover loading

Status: Accepted foundation; persistent preferences/artwork and the v0.3 workspace extend it via ADR-0018, ADR-0019 and ADR-0020
Date: 2026-10-29

## Decision

Keep the native GTK4/Libadwaita application and existing authentication/service
boundaries. Use `Adw.OverlaySplitView` for persistent desktop navigation and a
compact drawer, with a single content HeaderBar and no duplicate section tabs.
Home contains working library/playlist/search/queue destinations rather than
an invented catalog. Details retain their back destination, loading and inline
errors. Diagnostics remain a separate destination. Appearance uses native
system/light/dark choices; these settings apply to the current process.

Cover metadata is optional and validated independently from catalog identity.
Only an exact reviewed public image origin and bounded template are accepted;
bad artwork does not invalidate a valid track. Decode only bounded PNG/JPEG,
off the GTK thread. Use a separate four-worker pool and bounded queue so images
cannot occupy authentication/catalog workers. Keep a bounded 24 MiB / 64-entry
decoded LRU in memory, without cookies, OAuth, redirects, disk caching or imported
artwork. Load visible mapped covers at scale-aware buckets and cancel/discard
unmapped, replaced or old-session results. GTK owns textures on its main thread.

The persistent bar displays actual selected metadata and the real queue. Until a
permitted resource reaches GStreamer, play/seek/progress/volume remain explicitly
unavailable and no playing position or sound is simulated. MPRIS cannot publish
service playback that has not happened. This does not defer Issue #9's mandatory
audible acceptance; the parallel owner-local delivery experiment remains active.

## Acceptance

Validate source models, image bounds/privacy/cancellation/cache/lifecycle with
loopback fixtures, and actual native textures at scale 1 and 2. Structural GTK
checks, keyboard navigation, full size/theme screenshots and cloud cleanup do
not replace Fedora/KDE/Wayland/HiDPI acceptance or establish audio delivery.
All graphical composition and placeholders are original/native theme elements;
no Cassette artwork or UI implementation is copied.
