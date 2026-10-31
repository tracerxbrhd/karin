<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0020: Adaptive track information and playlist tiles

Status: Accepted for v0.3 implementation; visual and owner acceptance remain separate
Date: 2026-10-31
Extends: ADR-0017's accepted navigation, contextual playback, account and player bar

## Context

The merged v0.2 application is an owner-accepted native/Flatpak music prototype.
Its compact expandable navigation, circular account avatar, four-section search,
row-to-play activation, explicit Details action, safe logout confirmation,
persistent player bar and MPRIS form the working UI baseline. v0.3 adds useful
information and collection presentation without replacing that shell, the player
or MusicService, and without copying another application's graphics or layout.

## Decision

Add a native reparentable track-information view, with its own scrollable
content and explicit close action. On wide windows it belongs in an adaptive
right pane above 1180 logical pixels. At 1180 or narrower, use the split's
collapsed overlay instead of a permanent column, and do not auto-open it merely
because a track starts. The persisted wide-pane preference follows the playing
track by default; cover activation or Ctrl+I explicitly opens information and
Close persists the choice. Details may select metadata independently. Resizing
preserves selection, source and playback. Verify actual allocated widths in the
structural/visual matrix.

The view consumes the controller's actual selected/playing track and known
context. Display a large validated cover, complete title, artist and album
metadata, current playback/selection status and supported actions. Album and
artist navigation require independently verified identities from typed service
models; never infer an identifier from a display name or unrelated credit.
Missing data is omitted or represented honestly, not filled by generated facts.

Opening or closing information is not a Play/Stop action. Row mouse/Enter/Space
still starts its exact occurrence in the loaded PlaybackContext, including
duplicates. Details remain a separate deliberate action. Pane Play/Add/Details
must preserve the corresponding source binding rather than silently replacing
it with the first matching identifier. Queue updates, EOS and selected-track
changes must update the pane and existing player bar coherently. Do not create
a second audio state machine or restart a track merely because UI navigation
or a breakpoint changes.

Use native adaptive playlist tiles, with bounded cover slots, optional real
counts, readable names, keyboard activation and actual detail navigation. Sparse
collections retain natural tile widths instead of stretching a lone cover across
the window; narrow layouts wrap to one column. Missing covers use native icons.
A tile's two-line title may use native ellipsis only when the full name remains
available through its tooltip/accessibility label and the detail view. Headings,
errors and action labels retain the stricter no-clipping gate.

Loading, empty and error/retry states remain real collection states. Expose a
first create tile only when the confirmed playlist-creation contract is wired
through MusicService. One user action makes one write; only an independently
confirmed server result is success. Lost/ambiguous write outcomes stay distinct
from rejection, and GET-only refresh must not replay creation or fabricate a
local playlist. Do not offer unsupported editing actions as inert decoration.

Retain the loaded-context bound of 500 positions, clear current/next/manual-add
presentation and the existing source/partial-collection explanation. Removing,
clearing and navigating must keep their explicit existing effects. Keep the
accepted player-bar composition and actual controller/MPRIS capabilities;
change only what is needed for coherent information/queue presentation.

Extend the native PreferencesDialog with persisted appearance, protected-login
availability and storage controls. Explain unavailable secure storage, failed
forget/clear, cache-disabled retained files, measured usage and save errors in
ordinary native rows. Do not add developer OAuth fields, manual tokens or
unimplemented Offline controls.
Operation outcomes belong beside the Storage controls rather than only in a
page description that may have scrolled away. Reveal/focus a new terminal
message once; repeated background snapshots must not steal keyboard focus.

A similar-track block requires a confirmed real endpoint. My Wave requires its
actual station/session/batch/listening-feedback lifecycle. Keep those sections
hidden when the required contract/capability is not established; an ordinary
queue, sample album or static recommendation is not an implementation.

## Validation and consequences

Exercise actual mouse/keyboard tile and pane actions, source/duplicate binding,
playback-driven updates, stale-session results, pending cover cancellation,
loading/empty/error states and resize transitions with synthetic fixtures. Check
that a pane selection distinct from the playing track is identified clearly.
Preference/cache operations must not interrupt the accepted audio path.

Inspect fresh whole-window and populated-state PNGs at approximately 420×720,
900×650, 1440×900 and WM-maximized, both themes and scale 2. Check density,
large/absent covers, long titles, current/up-next hierarchy, focus, full reachable
scroll content and footer separation, alongside GTK allocation/lifecycle tests.
A compiling layout or a successful capture script is not a visual review.

The owner's prior baseline acceptance does not accept this new workspace. Repeat
ordinary login/restore, library/search/playlist navigation, actual entitled-track
sound, contextual controls and KDE MPRIS on Fedora 44/Wayland/PipeWire, both
native and current Flatpak. Hardware media keys remain unconfirmed until
actually exercised. No personal screenshots or real service data belong in
Cloud/CI artifacts. Public service/distribution conditions remain independent.
