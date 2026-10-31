<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Karin v0.4 — Native Music: approved design specification

## Design acceptance / source of truth

The owner approved **variant B — Native Music** and the 4-zone composition:
left icon rail, central music workspace, persistent right track-information pane and bottom player bar.

**Rule 1: Minimal visible text.** Show music, artwork, hierarchy, iconography and clear affordances first. Keep visible explanatory prose rare and meaningful. For ambiguous/destructive actions, errors, empty states, authorization and accessibility, short text is better than an inscrutable icon.

**Rule 2: The right pane does not close when the user clicks outside.** Once explicitly opened it stays open, including through scrolling, main-area clicks, navigation and track changes. It closes only by an explicit action (close X / Ctrl+I; an intentional compact-view back action is also acceptable). Technical/diagnostics screens may suspend rendering while remembering user intent. Logout removes personal data.

**Rule 3: The right pane follows the playing track when playback changes.** On a new actual current queue occurrence (mouse/Enter/Space, Next, Previous, queue selection, EOS, MPRIS), update cover, title, artist, album, actions and context **in place**. Neither a new view nor a second playback controller is needed. Render no mixed old/new metadata. Do not restart audio due to UI updates.

## Visual art direction

Native Libadwaita foundation, restrained character, beautiful artwork and useful density, not a clone of Cassette, Spotify or Yandex Music. Dynamic product content supplies richness; backgrounds and chrome stay quiet. No glass, blur-heavy UI, decorative gradients behind normal controls, or massive empty cards.

- Navigation: 72–80 logical pixels by default; clear selected item, focus, tooltip and optional labels expansion.
- Group spacing: ~8/12/16/24 logical px, with local exceptions after inspecting real GTK allocation.
- Main track rows: ~60–66 px including ~44–52 px art; title prominent, artist legible, duration and secondary action quiet. Avoid full-width heavy row separators.
- Playlist header: cover ~96–145 px plus large title and compact facts, one primary Play.
- Playlist tiles: natural-width responsive square covers, short visible names, hover/focus; never stretch a single tile to fill the viewport.
- Track pane: about 280–340 logical px wide on suitable windows, cover about 224–272 px; title/artist/album compact; one concise actions row, not a duplicated transport console.
- Player Bar: permanent and visually stable. Left artwork/title/artist, central playback and progress, right queue/volume; controls must not drift with long titles. Compact layout stays readable and short.
- Typography: 2–3 clear levels for screen/collection/track names, restrained supporting text, enough contrast; no unreadable tiny labels.
- Icons: native symbolic icons and clear keyboard focus; the SVG drawings are *illustrations*, not mandatory application icons.
- Theme: proper system/light/dark via Libadwaita; mock colors illustrate tone, not a license to override system accessibility.

## Responsive behavior

The SVG files illustrate wide 1440×900, medium 900×650 and compact 420×720. Treat breakpoints as **content-driven** and test the actual GTK allocations; roughly 1280+ wide, 720–1279 medium and below 720 compact.

On wide windows an open pane can share space with the central list if the remaining list remains legible. On medium, an **explicitly open nonmodal side presentation** may overlay content, but clicking the uncovered area must not dismiss it. On compact, if a dismiss-on-backdrop overlay cannot meet the persistence requirement, use a **dedicated information subview** with explicit close/back. Preserve user open intent when resizing wide→medium→compact→wide. Do not snap the pane closed and reopen it with an animation glitch.

Keep minimum 6 fully legible track rows at 900×650 with the information pane closed where possible, subject to accessibility and normal desktop chrome.

## Right-pane state model / exact expected behavior

Separate concepts:

- `open_intent`: explicit user preference for seeing track information (not the same as `AdwOverlaySplitView::shows_sidebar()`).
- `is_available`: authenticated music view and a current/explicitly selected usable track.
- `presentation`: wide sidebar, medium nonmodal view, compact information subview.
- `info_target`: `FollowingCurrent` or `ExplicitSelection`, identified by actual queue occurrence/context where applicable.

State rules:

1. Open current information using current cover or Ctrl+I. Without a current track, use a real empty state if that view is explicitly available; no dummy title.
2. While open, click background, scroll, go to playlist/album/search, use keyboard focus: keep `open_intent` and visible pane.
3. On any **actual playing-track change**, including same track ID at a different position: switch to `FollowingCurrent` and atomically update the target snapshot and artwork request. Ignore stale asynchronous responses. Keep open.
4. An explicit request to see another track's information switches to `ExplicitSelection` without changing audio. Pause/seek/volume do not erase it; the *next actual current-item change* returns to `FollowingCurrent`.
5. Close X / Ctrl+I clears `open_intent` and closes the representation once, with correct focus return. Do not reopen on later playback.
6. On resize use a representation transition that preserves `open_intent`; avoid automatic closed-state setters overriding it. An unavailable/diagnostics view may temporarily hide but restore safely upon return.
7. Logout/account switch immediately retires personal information and all old jobs; never show stale art, previous account metadata or closed pane's hit area.
8. No spontaneous Play/Stop/Queue replacement from panel open/close, selection, resize or navigation. MPRIS reflects the same PlaybackController.

### Existing code that needs a targeted audit

- `src/ui.rs` builds the right `adw::OverlaySplitView` and applies the `max-width: 1180sp` and `760sp` breakpoints.
- `src/ui/music/workspace.rs` has `InfoSelection.following_current`, `sync_track_pane`, `sync_info_visibility`, `toggle_track_info` and a `connect_collapsed_notify` branch that currently closes the sidebar.
- `src/ui/music/track_pane.rs` owns the view; it should **not** own an independent playback state.
- The existing `src/ui/music/workspace/tests.rs` tests explicit selection separately from a current playing item. Extend those tests for restored following behavior.

Do not rely on a simple `set_show_sidebar(true)` loop that fights libadwaita's outside click behavior; do not hide a malfunction behind an instant reopen. Confirm actual GTK behavior under mouse and keyboard. Consult the runtime-supported properties rather than assuming a newer Libadwaita API exists.

## Per-screen UX

**Home:** genuine library shortcuts and cached/verified items, visual album/playlist treatments; no invented recommendations, fake recently-played list or repeated instructional paragraphs.

**Library, playlist, album:** hero cover/title/action hierarchy, compact song rows, clear current song emphasis, minimal visible secondary facts, server-confirmed creation button integrated with tiles.

**Search:** four genuine independent sections (tracks, albums, artists, playlists), visual grids where suitable, compact songs; retain Show All/Back/Retry/paging and independent failures, with no fabricated content.

**Queue:** retain source context and duplicate occurrences; visually distinguish current and up-next; keep correct manual add/remove/clear semantics.

**Right pane:** large cover, good title/artist hierarchy, album/action row, no repeated contexts or second Player Bar; navigation only with validated IDs.

**Preferences:** can be more descriptive than music screens, because saved login, protected credentials, cache policy and destructive Clear require clarity.

**Errors/empty/loading:** clear nonintrusive state, actionable retry, safe text; avoid displaying invented data or a successful state when metadata isn't available.

## Architectural boundaries

Retain Rust/GTK4/Libadwaita/GStreamer, MusicService, StreamResolver, PlaybackController, the 500-position PlaybackContext, existing account safety, secure session, account-scoped cache, MPRIS 2.2 and Flatpak permissions. No Electron or web shell, no second audio state, no extra background daemon.

Offline audio, My Wave, similar songs and lyrics remain unsupported unless independently verified and explicitly authorized in a different project scope. Do not introduce placeholders masquerading as features.

## Acceptance matrix

- Build real layouts: 420×720, 900×650, 1440×900 and real maximize; light/dark; scale 1 and 2; with/without pane.
- Mouse + keyboard: Ctrl+I/X close, click-outside, scroll, navigate, resize back and forth, tab order, selection details independent from Play, focus restoration.
- Playing-track updates: click, Enter/Space, Next/Previous, queue pick, EOS and MPRIS. Include duplicate IDs at different positions, stale artwork and delayed metadata responses.
- Visual compare old vs new at matched sizes, real GTK screenshots (not this reference SVG), inspect original images and GTK allocations. Check contrast, no clipping, data density and coherent spacing.
- stable + exact Rust 1.98.1, fmt/clippy/tests, GTK/WebKit/GStreamer/MPRIS, Flatpak when available.
- Fedora/KDE/Wayland/PipeWire and live service acceptance remain owner-local; no private screenshots or real audio/credentials in CI.

## Iteration / approval

One `feature/v0.4-native-music-ui` branch and **existing Draft PR** (created with these references). Implement in coherent commits, update docs and design tests, and leave PR Draft until owner visually accepts real Fedora screenshots. **Do not open a second implementation PR or merge automatically.**
