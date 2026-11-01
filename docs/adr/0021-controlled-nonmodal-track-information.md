<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0021: Controlled nonmodal track information

Status: Accepted for v0.4 implementation; executed checks and owner visual acceptance are recorded separately
Date: 2026-10-31
Extends: ADR-0017's existing controller/context ownership
Supersedes: ADR-0020's collapsed right-overlay presentation and wide-only open preference

## Context

The owner accepted v0.3's music path and pane opening/closing mechanics, but not
its visual design. The approved [Native Music specification](../design/v0.4/DESIGN_SPEC.md)
requires information to stay open until an explicit close, including through
outside clicks, navigation and resizing. A collapsed modal sidebar that closes
on its backdrop cannot satisfy that contract. Reopening it after dismissal
would mask the wrong interaction and interrupt the accepted animation.

Information also needs an identity distinct from the current track ID. The
same ID may occur twice in a context. Explicitly inspecting another track must
remain independent of sound until the next actual current-occurrence change.
No new player, service endpoint, asynchronous runtime or persistent secret is
needed for this presentation change.

## Decision

Use a permanent GTK overlay containing the existing central workspace and one
right-aligned `GtkRevealer`. The revealer hosts the same track-information
component, with native slide-left animation and reduced-motion support. It has
no dismissing backdrop, modal input grab or outside-click close handler. Resize
changes its allocation/presentation rather than reparenting the component or
closing and reopening it.

| Logical window width | Open information presentation |
| --- | --- |
| 1280 and above | 320-pixel right pane; the central view reserves 340 pixels including a 20-pixel gap |
| 720–1279 | 300-pixel nonmodal right pane; the central view reserves 316 pixels including a 16-pixel gap |
| Below 720 | Fill-width information subview; underlying central view is hidden until explicit close |

These content-driven boundaries are verified against actual GTK allocation,
including expanded navigation, long metadata, display scale and desktop chrome.
The permanent Player Bar remains outside this information presentation.
Both desktop presentations allocate space beside the music view rather than
covering its row actions. An interim native test found that a medium overlay
could conceal a newly played row's More menu while default open intent was
active. Reserving the medium pane's width keeps those real actions reachable;
it preserves the approved nonmodal behavior without a scrim or auto-dismiss.
The reference permits a medium overlay, but does not require covering controls.

Keep four concepts separate:

- **Open intent:** the existing non-secret `track_info_open` setting records an
  explicit user choice. It is not inferred from a revealer's instantaneous
  state or overwritten because a breakpoint or Diagnostics hides presentation.
- **Availability:** an authenticated music view and usable current/explicit
  information target determine whether personal information can be rendered.
  Diagnostics may suspend rendering while preserving intent; logout clears
  personal snapshots and retires old asynchronous work.
- **Presentation:** wide, medium and compact allocation derives from the
  current window; it does not own session, selection or audio state.
- **Target:** `FollowingCurrent` or `ExplicitSelection`. A current occurrence
  is identified using its context/source, position and track identity, with the
  player's occurrence-change hook distinguishing repeated IDs. The target is
  owned by the workspace controller, not the view or GStreamer worker.

Cover activation always shows current information and keeps an already open
pane open; it is not an outside-click dismissal. Ctrl+I is the explicit toggle. If there is no current queue item, it preserves
usable explicit metadata; closing returns focus to the music page when the
current-cover control is disabled. Explicit Details may select
another track without changing PlaybackContext or audio. Pause, seek and volume
do not erase that choice. A new actual current occurrence from direct Play,
queue selection, Next/Previous, EOS or MPRIS returns to `FollowingCurrent` and
updates the open view in place. Swap its metadata/actions snapshot together;
cancel or generation-check old artwork/detail results so no mixed old/new
information appears.
Removing another queue row may reindex the same current occurrence without
changing the target or advancing its epoch. Removing the actual current row
does constitute a current-occurrence change, including a duplicate-ID successor.

Explicit X/Ctrl+I (or a dedicated compact Back action) clears open intent and
returns keyboard focus appropriately. Later playback must not reopen a closed
view. Information selection, open/close, navigation and responsive transitions
never initiate Play/Stop, replace the context or create a second audio state.

The presentation continues to consume typed MusicService models, bounded
artwork and existing confirmed action capabilities. No unsupported lyrics,
similar tracks, offline controls or My Wave block is introduced. Session/cache,
MPRIS and exact Flatpak permissions retain their earlier contracts.

## Validation and consequences

Exercise actual mouse/keyboard outside clicks, scroll/navigation, explicit
close/focus, Diagnostics suspension, both directions of all breakpoints and
logout. Test current-occurrence following through every supported playback
entry, including duplicate positions, explicit selection and delayed responses.
Assert the same queue/controller and uninterrupted generated fixture playback
during presentation-only actions.

Inspect matched baseline/current actual GTK captures at four sizes, both themes,
1×/2× and open/closed information, plus populated pages, error states and focus.
Follow [UI_QUALITY_GATE.md](../UI_QUALITY_GATE.md#v04-native-music-gate), with
stable/exact-1.98.1, GTK/WebKit/GStreamer/MPRIS and available Flatpak checks.
Execution belongs in [VALIDATION.md](../VALIDATION.md); this decision is not a
test PASS. Real Fedora/KDE/Wayland/PipeWire visual acceptance remains owner-local
and keeps PR #15 Draft until supplied. The original SVG references do not prove
native rendering or authorize copying service artwork into public fixtures.
