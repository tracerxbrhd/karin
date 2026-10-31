<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# UI quality gate

Every substantial UI change must include real visual inspection and GTK
structural checks. A compiling widget tree, a visible window or a screenshot
alone is insufficient. Review the existing technical prototype without adding
fictional music catalogs, recommendations or player controls.

## v0.4 Native Music gate

The [approved Native Music specification](design/v0.4/DESIGN_SPEC.md) defines
the current four-zone composition: expandable icon rail, central music view,
controlled track-information presentation and permanent Player Bar. Keep visible
prose minimal while retaining short meaningful errors, destructive confirmations,
tooltips and accessible names. The [original SVG references](design/v0.4/README.md)
are design inputs, not running GTK captures or evidence of implemented features.

Compare the accepted v0.3 baseline integrated at `5e85e8d` with the actual v0.4
implementation at **matched size, theme, scale, page and pane state**. The base
matrix is four window states below × light/dark × scale 1/2 × information
open/closed: **32 cases per revision, 32 before/after comparisons**. Include
actual window-manager maximize, not only a maximized-sized undecorated window.
Record omissions/failures explicitly; never rename an old capture as current.

Supplement that matrix with populated Home, Library, playlist/album detail,
four-section search and Show All, queue, account/logout and Preferences, plus
loading/empty/error/Retry, long metadata, missing art and visible keyboard focus.
Inspect actual pixels, coherent hierarchy and useful density; the six-complete-
rows target at 900×650 with information closed is a minimum, not visual approval.
Check normal motion and reduced-motion behavior separately.

The following native input/state checks are mandatory:

- Once opened, information stays open through uncovered-area clicks, scrolling,
  navigation and wide→medium→compact→wide resizing. Close X, Ctrl+I or explicit
  compact Back closes it deliberately with focus return. A modal scrim or
  close/reopen workaround does not meet the medium nonmodal requirement.
  Both desktop sizes reserve central content width (wide 320+20 pixels; medium
  300+16), so the pane must not conceal track-row More menus or other actions.
  Exercise those controls after row Play with the default open preference.
  Repeated current-cover activation must keep information open and show the
  current track without restarting audio; it is not a second close action.
- Open intent is distinct from widget visibility. Diagnostics may suspend
  presentation without forgetting that intent; logout retires personal data and
  asynchronous artwork/metadata work immediately.
- Actual current-occurrence changes from mouse, Enter/Space, Next/Previous,
  queue selection, EOS and MPRIS update the open view in place. Include repeated
  track IDs at different queue positions and delayed old replies. Cover, title,
  artist, album and actions must refer to the same current snapshot.
- Explicit Details on another track changes information only. Pause, seek and
  volume preserve that selection; the next actual current-occurrence change
  resumes following. Opening, closing, resizing or navigation never replaces
  PlaybackContext or starts/stops/restarts the audio engine.
- Confirm one coherent visual system across all music pages, with real actions
  and honest loading/failure states. No offline, My Wave, lyrics, similar-track
  or recommendation block appears without its separately established contract.

Use actual mouse/keyboard events in isolated GTK fixtures, then a separate
owner-local Fedora/KDE/Wayland/PipeWire run. Passing cloud captures, structural
tests or the earlier v0.3 acceptance does not pass v0.4 owner visual acceptance.
Existing [Draft PR #15](https://github.com/tracerxbrhd/karin/pull/15) stays Draft
until that review; do not create another implementation PR or merge it.

## Required matrix

The accepted native music shell uses adaptive navigation. Check the actual desktop
rail/sidebar and compact navigation, including labels/tooltips/accessibility for
icon-only controls, opening/focus/activation/closing, cross-page keyboard
focus and selected-page preservation across both directions of resizing. Check
Home's real cached liked-track preview, metadata rows, contextual actions and
the persistent selected-track bar. Play may begin real source resolution; Pause/seek and other transport
controls must reflect actual worker state/capability. An unresolved source must
never show an invented playing state or measured progress.

Use `KARIN_CAPTURE_SCALE=2 scripts/capture-ui.sh build/ui-captures/hidpi` for
the same logical-size matrix on a 3840×2160 Xvfb display. Inspect actual scale-2
textures as well as layout. Static synthetic native-fixture frames explicitly
use GTK reduced motion to avoid recording a dialog's opening fade; the ordinary
full-window capture, smoke and separate interactive WebKit processes retain
their normal motion settings. Record these conditions with the evidence.

Inspect the music interface and diagnostics at each size below, in **light and
dark themes**. Cover signed-out library/search/queue navigation and the single
native sign-in dialog without real credentials. Account/search/detail states
may use synthetic fixtures only in explicitly identified tests, never a release
catalog. Use actual allocated dimensions; distinguish requested size from the
size achieved under the window manager.

| Window state | Approximate size | Main checks |
| --- | --- | --- |
| Compact | 420×720 | Narrow navigation, readable wrapping, reachable actions and vertical scrolling |
| Medium | 900×650 | Appropriate header navigation, hierarchy and balanced density |
| Wide | 1440×900 | Bounded component widths, content aligned near the top, no oversized centered empty-state layout |
| Maximized | Available work area | Header/title/window controls stay intact; content scales with the actual available area |

Resize across navigation breakpoints in both directions. Check that the selected
page and current diagnostic state survive; only the appropriate navigation
control should be shown. Remaining space is allowed when the prototype has little
content, but meaningful content must not float in a disproportionately large
centered void or stretch into unreadably wide rows.

For each theme and relevant page:

- Inspect title, subtitle, HeaderBar, window controls and top edge. No glyph or
  control may be clipped, including when maximized.
- Check heading levels, margins, group spacing and reasonable row/button widths.
  Long status and error text must wrap without pushing actions off-screen.
  The persistent player's single-line track title may use native ellipsis only
  when its complete text is available through a tooltip and accessible label.
  Playlist-tile metadata may use two wrapped lines with native ellipsis only
  when its full title is available in a tooltip/accessibility label and detail
  view. These exceptions do not apply to headings, errors, status text or action
  labels.
- Use Tab and Shift+Tab through navigation, menu and action buttons. Verify a
  visible focus indicator, logical focus order, Enter/Space activation and
  keyboard access to every implemented action. Check Ctrl+Q shutdown.
- Check text correctness and contrast in light/dark themes. Do not rely on color
  or an icon alone for connection, loading or error status.
- Inspect existing initial, checking, unauthenticated/unavailable, error and
  diagnostic-result states where they can be reached safely. Use test fixtures
  only in explicitly identified tests; never present invented service data as a
  working production integration.
- Keep audio tests clearly labelled as synthetic. Do not make a screenshot run
  play physical sound or contact an account without an explicit action.
- Check music detail/back navigation, result paging and stale-search handling,
  session cancel/logout, empty and unavailable-item text, queue editing and the
  actual player state. Disabled player controls must not suggest
  successful service playback. Default builds must not expose Device Flow.
- Assess useful density as well as clipping: search input/results must remain
  primary, playable rows and Details must have distinct understandable actions,
  Back must be easy to find, and queue selection/removal/clear must be legible.
  Check one plain-language search failure with Retry and optional safe details;
  no duplicated global/inline error or technical code should dominate the page.
- Judge the composition as a music application: navigation, account avatar,
  covers, primary play actions and the player bar must have a clear hierarchy.
  A structural minimum-row assertion or absence of clipped widgets does not
  pass this design review. Avoid repeated account banners, redundant headings,
  oversized separators and large dead areas around populated lists. Inspect
  actual populated states, including long real-shaped titles and missing artwork.
- For artwork changes, test validated Music metadata through the ordinary
  `CoverArt` rendition builder, counted fixture HTTP, decode and GTK texture
  attachment with the symbolic fallback hidden. Pre-seeding RAM or supplying a
  replacement URL does not cover this path. Include independent image failures,
  cancellation/logout, queue pressure, HiDPI and an empty-RAM disk-cache restart
  with no extra HTTP request. Keep actual current-account cover display on
  Fedora as a separate manual gate; synthetic colored pixels do not close it.
- For unified search, inspect independent real-data track/album/artist/playlist
  sections, their loading/empty/error states and section-specific Show All/Retry.
  One failed section must not erase successful sections or become a fabricated
  empty result. Successful empty categories are omitted; all-empty results have
  one message. Section Retry must fetch that category only; Back to All runs a
  fresh bounded aggregate query. Verify that opening details and returning
  retains its query/section/paging, and a late reply cannot replace a newer query.
- Verify account menu → **Выйти из аккаунта** → native confirmation. Cancel must
  have default focus/response; Enter/Escape/cancel must preserve the current
  session, queue and active audio until explicit confirmation. Opening the menu
  alone must not log out. Check stale confirmation responses after session
  changes, focus return, compact geometry and mouse/keyboard activation.
- Exercise normalized search-row activation to actual Play, and the separate
  explicit detail/Back path, with synthetic loopback
  records, including unusual search display fields and partially missing lookup
  records. Assert one batched normalization request rather than per-row requests,
  retained provider pagination and disabled unavailable tracks. Owner Fedora
  acceptance remains separate from this fixture.
- Activate rendered track, playlist, album, artist and queue rows with actual
  mouse input and keyboard Enter/Space. A playable track must start its exact
  context position; collection rows open their details. Assert the action and
  expected service request, including after refresh replaces rows. Clicking nested
  buttons/menus must preserve their own action; empty/nonactivatable rows must
  do nothing. Direct calls to detail handlers or emitting a row's keybinding
  signal alone do not prove real user activation. The input-driven loopback
  fixture is `native_catalog_rows_mouse_keyboard_details_and_delivery`.
- Open the sign-in dialog at every size/theme. Automated checks use the explicitly
  injected offline provider or local browser-session fixtures; check loading,
  stage-specific errors, retry, Escape/cancel and focus return. No build mode may expose Client
  ID/secret, device codes, pasted tokens or URL fields in this user journey.
  Only validated Music account access may complete sign-in; test callback/state,
  cancellation and late responses against local fixtures. Clear old errors on
  logout, cancel and a fresh attempt; use native message rows with wrapped text.
- When the WebKit component changes, check ephemeral-session construction and
  destruction, trusted origin display, exact main-document return/state handling,
  asynchronous own-session cookie collection/cancellation and callback interception
  separately from provider permission. Exercise actual local page loading with the sandbox
  enabled where supported; report namespace/sandbox restrictions as blocked.
- Never capture real application credentials, verification codes, auth/browser
  pages or account/library data for public evidence. Cloud capture should use
  clean signed-out states; authenticated fixture coverage must be explicit.

## Playback context and desktop controls

Play a row from a multi-track library, playlist, album or search page. Check
which returned tracks become its playback context, the selected position,
Next/Previous and EOS, including skipping unavailable loaded items, retaining
duplicate positions, paused/stopped intent and the 500-position context limit.
Appending/removing/clearing queue entries, starting a different
context and navigating away must have explicit, predictable effects. A partial
loaded page is not an entire remote collection. Retry/error/logout/close must
not restart stale audio or silently discard the user's newer queue action.

Verify that the persistent player's artwork, title, transport, progress, seek,
volume and queue controls match the actual selected source and worker state.
Check empty, resolving, playing, paused, stopped, ended and failed states at
compact and desktop sizes. A screenshot of a Play icon does not prove sound.

Run a real isolated session-bus fixture and exercise
the same native player through its exported methods, properties and signals.
Compare capabilities and position/seek units with actual snapshots; verify no
second audio engine, arbitrary URI input, stale track identifier, media address,
credential or cookie is exposed. Bus-name ownership/release and application
shutdown need lifecycle checks. Synthetic bus/media fixtures prove that contract
only. Seeked must follow a successful worker acknowledgement, including a seek
close to the current position; rejected/queued-only/stale seeks must not emit a
false acknowledgement. Check optional cover-file cleanup and private file modes.
Unsupported repeat/shuffle controls must not appear via optional MPRIS properties.
Fedora acceptance must separately verify an actual selected Music track,
`playerctl`, KDE's media widget and physical media keys, in the native artifact
and, when built, the current Flatpak sandbox. No manual result is passed until
it is actually performed.

## Retained session, storage and information-workspace gates

Treat the accepted v0.3 service/audio/session/cache behavior as the regression
baseline, not a pass for v0.4 presentation or new backend/failure scenarios.
ADR-0021 supersedes only the old modal information presentation. At every
relevant size/theme/scale:

- Inspect restoring, validated, invalid session, unavailable/locked store,
  cancelled prompt and retryable deletion-error states. An issued or stored
  token alone must not appear authenticated before Music validation.
- Inspect remember-login effective availability, save/forget progress and the
  durable pending-forget state. Turning remember-login off may preserve current
  playback; logout must stop it and confirm saved-credential cleanup. No final
  successful logout may hide an unknown delete/cache-clear outcome. Cancel from
  the native logout confirmation remains the safe default.
- Check persisted system/light/dark appearance and PreferencesDialog pages for
  authorization and storage. Labels, toggles, maximum-size controls, measured
  usage, clear progress/errors and repeated open/close must remain accessible
  with keyboard, long messages and compact windows. No secret, token value,
  account identifier, portal key or raw backend error is displayed.
  Check a failure after scrolling to Storage: the inline operation outcome must
  be reachable/visible, while repeated unchanged snapshots must not steal focus.
- While actual generated fixture media plays, change cache policy, clear cache, open/close
  track information, move through playlists/search and resize both directions.
  These presentation/storage operations must preserve current audio, track ID,
  queue/context and MPRIS state unless the user explicitly performs a playback
  or logout action. Clearing cache must not remove the stored session or the
  live MPRIS cover file.
- Inspect cache-hit, loading, explicit refresh, saved/potentially stale fallback,
  disabled-cache and empty/error states. A saved library/playlist is not current
  authorization or confirmed membership. Never show an offline-available badge
  or Offline switch unless a permitted audio cache actually exists.
- Inspect the right track pane on wide windows and its adaptive compact
  presentation. Check large/missing cover, long full title/credits/album,
  selected-versus-playing state, close/focus return and only verified artist/
  album navigation. Pane opening/closing must not implicitly Play or Stop.
  Its menu action and Ctrl+I stay disabled on Diagnostics; switching back to
  music restores the appropriate action without changing playback.
- Activate tracks through mouse/Enter/Space and through distinct Details/pane
  controls. Preserve exact source occurrence/duplicates, manual appends and the
  visible partial-context boundary; a different information selection must not
  silently replace the playing queue.
- Inspect playlist tiles at one-column through wide-grid sizes, including a
  sparse one-item collection, long/missing title, unknown counts, absent covers,
  loading/empty/error/Retry and keyboard focus. Confirm navigation using actual
  clicks/Enter/Space. Creation appears only when its server-confirmed workflow is
  implemented; ambiguous writes must not create a fake local tile or replay POST.
- Keep My Wave and similar-track blocks hidden when their required real
  contracts are not established. Source stubs and a renamed normal queue do not
  pass their capability gates.

Automated persistence/cache captures must use isolated synthetic XDG directories,
a fake credential boundary and local fixtures, as well as the existing explicit
`--ui-fixture` provider. They must not restore a developer's real saved session,
read a personal cache, unlock a real wallet or contact Yandex. Native keyring and
Flatpak Secret Portal prompts are owner-local checks; never publish personal
wallet/auth windows. Fixture passes and Xvfb do not establish those real backends.

## GTK structure and behavior

Run formatting, Clippy without warnings, tests, the build, GTK smoke test and
GStreamer diagnostic. Structural checks must cover actual window/widget
allocation, adaptive navigation visibility, page selection and normal close/
resource release. Check that widgets remain inside the content area and that
scrolling exposes diagnostics on short windows.
For `OverlaySplitView`, use its public `shows_sidebar()` state at every width:
Libadwaita may keep a closed sidebar mapped outside the viewport. Always check
main content and assert that it reclaims the closed pane's width; keep all
geometry checks for an open sidebar. A mapped closed pane alone is not visible
clipping, and the correction must not skip open-pane or content assertions.
That rule still applies to navigation containers; it does not substitute for
v0.4's controlled `GtkRevealer` information checks. Verify its actual allocation,
nonmodal hit testing, compact underlying-view hiding and explicit open intent,
including after the native reveal animation settles. No automatic dismiss/reopen
handler may be used to make the visibility assertion pass.

Screenshots supplement these assertions by revealing hierarchy, clipping and
focus problems that structure tests cannot judge. Conversely, a plausible image
does not prove that the controls work or the window closes correctly.

## Cloud capture procedure and evidence

When Xvfb and capture tools are available, run the real GTK application with a
clean session D-Bus. Use the normal widget implementation and native Libadwaita
theme. Capture the matrix, inspect the images themselves and record the commit,
actual dimensions, theme, page and any action/focus state. Screenshots must be
fresh for the reviewed code; changing filenames on old images is not evidence.

For maximized-window evidence, use a window manager and request maximize. If the
cloud cannot provide a managed/maximized window, mark that matrix cell unrun;
a full-size undecorated window is not equivalent. Document the Xvfb/window-manager
versions and distinguish cloud evidence from target-desktop acceptance.

The repository capture helper runs a real application in isolated Xvfb/Openbox
sessions, with a 1920×1080 virtual screen, native light/dark themes and no account
data. It captures the visible application rectangle, including keyboard focus
and silent diagnostic activation, then closes with Ctrl+Q:

```sh
cargo build --workspace --locked
scripts/capture-ui.sh build/ui-captures
```

The helper always selects explicit `--ui-fixture` mode before opening/closing the
sign-in dialog at all four sizes in both themes. This injected offline provider
is automation-only; ordinary startup uses the real Cassette-compatible provider.
These captures never load a real authorization page or claim successful provider
login. Mark fixture evidence clearly and use local WebKit/HTTP fixtures to cover
the session grant; the real Fedora owner test is a separate acceptance gate.

It requires Xvfb, Openbox, ImageMagick (`import`), xdotool, wmctrl and xwininfo,
plus the normal GTK/GStreamer libraries and SVG loader. In the prepared cloud
visual environment use `/workspace/karin-environment/run-visual` to invoke those
commands. Generated images, geometry and capture metadata go in ignored
`build/ui-captures`; review fresh outputs before choosing any images to publish.
Record the binary fingerprint and whether the worktree is dirty, so a baseline
HEAD is never mistaken for the revision of an uncommitted build.

The helper requests `GSK_DEBUG=full-redraw` with the Cairo renderer to avoid stale
partial redraw pixels during X11 resize/page-change capture. This is a capture
setting, not a change to the application's normal renderer or a retouch of the
images. Inspect the actual PNGs; if an image preview omits repeated content,
verify the relevant original pixels or an independent crop before treating that
preview as an application defect.

Capture only Karin and clean synthetic cloud content. Do not commit or upload
user-supplied screenshots containing personal data, account information,
unrelated applications or desktop elements without separate permission. Preserve
the user's private review evidence outside the public checkout. Any shared cloud
image must be inspected for accidental private content first.

## PR evidence and acceptance

The PR description/checklist must include:

- The layout change and its intended behavior at compact, medium and wide sizes.
- Links or artifacts for the inspected cloud screenshots, including theme and
  size coverage; a matrix identifying any omitted or failed case.
- Executed structural checks and their actual results.
- A keyboard/focus/text review result; do not infer it from mouse navigation.
- Remaining Fedora/KDE/Wayland manual checks, left pending until performed.
- Any licensing or original/third-party asset changes, with notices preserved.

Fedora 44 / KDE Plasma / Wayland acceptance remains a separate manual gate. Check
window decorations/top-edge title behavior, maximize/restore, live resizing,
both themes, keyboard/focus, HiDPI and accessibility there. Xvfb success never
changes that gate to passed. Record user-reported functional acceptance and
revised visual acceptance separately in [VALIDATION.md](VALIDATION.md).
