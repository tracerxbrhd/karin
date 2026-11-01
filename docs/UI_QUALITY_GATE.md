<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# UI quality gate

Every substantial UI change must include real visual inspection and GTK
structural checks. A compiling widget tree, a visible window or a screenshot
alone is insufficient. Review the existing technical prototype without adding
fictional music catalogs, recommendations or player controls.

<a id="v04-native-music-gate"></a>

For popover geometry across `GtkNative` boundaries, use the existing bounded
[test input coordinate helper](../src/ui/music/test_input.rs) and stable actual
layout. GTK 4.14 may return no direct `compute_bounds` to the window; do not skip
the containment assertion or substitute guessed coordinates.

## v0.4 complete music-experience gate

The owner's [current specification](design/v0.4/DESIGN_SPEC.md) supersedes the
first Native Music visual target. Revision `a9eed45` is the before baseline:
its cloud checks passed, but its Fedora UI/UX was rejected. Keep previous
results; do not reuse old captures as evidence of the new implementation.

The client has three primary navigation destinations, a central music workspace,
one universal Track Info/Queue panel and a permanent Player Bar. Inspect real
music hierarchy, useful density and aligned controls rather than only clipping.
Home uses genuine loaded covers/collections rather than giant navigation cards.
The footer has functional Shuffle/Repeat and no visible Stop button; MPRIS Stop
is retained. Short errors, Retry, confirmations, tooltips and accessible names
remain necessary even when visible explanatory text is reduced.

### Fresh matched matrix

Capture the before and current revisions at **the same actual size, theme,
scale and page**. Use 420×720, 720×650, 900×650, 1280×800, 1440×900,
1650×930 (the owner-reported wide layout), 1920×1080 and actual window-manager
maximize; light/dark; scales 1 and 2. Cover current
`Closed`, `TrackInfo` and `Queue` modes (**96 current base cases**). The before
revision has no shared Queue mode: compare its genuine corresponding queue
presentation and label that structural difference; never synthesize a panel or
rename a different state to make a pair appear equivalent.

Record requested and achieved geometry, source revision, binary fingerprint,
library versions, theme/scale and motion settings. Inspect all accepted images,
including original HiDPI pixels. Supplement the matrix with Home, Library,
playlist/album, four-category Search/Show All, queue edits, account/Preferences,
active Shuffle/Repeat, empty/sparse/large collections, long metadata, absent art,
loading/error/Retry, unavailable tracks and visible focus. A six-row density
assertion at 900×650 with the panel closed is a useful minimum, not visual PASS.

### Whole-surface motion — measured graphics-ready scope

Separate first-use responsiveness from motion after recorded graphics
initialization. For each geometry/theme/mode, the fixture performs exactly one
unconditional real normal 240 ms open, six bounded offscreen native GSK renders
of the actual mapped panel at `.137/.317/.533/.787/.941/1` while fully open,
then one real normal close. Raw setup carries `scope: graphics-initialization`
and `quarter_phase_acceptance: not-assessed`; setup renders save no PNG, change
neither UI progress nor animation clock and are not proof frames. Only the
following mouse-driven native cycle is measured. No conditional retries or
production prewarming are permitted. Label this **graphics-initialized normal
motion**; it does not pass cold first-open responsiveness.


With normal animation enabled, record opening and closing near 0%, 25%, 50%,
75% and 100%. Background, border, fixed header/close, art, text and buttons must
have a common translation and stable relative geometry at every intermediate
frame. Each sequence must use **five distinct rendered native frame times and
indices**, with measured progress near each requested quarter. Reusing one
render for two labelled samples does not pass. Current motion filenames label
openness; present closing in chronological **100 → 75 → 50 → 25 → 0** order.
The original before observer labels elapsed fractions instead, so retain those
labels and do not pretend its quarters use the new panel's progress measure.
Check clipping and actual GTK allocation as well as screenshots. A fully
open image, moving cover alone or disabled animation cannot prove this gate.
TrackInfo ↔ Queue switches within the same open surface: fixed outer width,
header/close position and no repeated slide. Check reduced-motion separately.
Central actions must remain usable during motion and after it settles; no
instant full-width content jump or temporary covering of row controls is valid.

Record the actual GTK/Libadwaita/Mesa versions, renderer and frame timing. The
strict normal-motion fixture uses NGL/GskGLRenderer under Xvfb at both scales;
that is not proof of physical GPU rendering or Wayland. Local GTK 4.18/
Libadwaita 1.7 PASS does not override the initial `709f5b3` hosted strict-motion
FAIL on GTK 4.14/Libadwaita 1.5/Mesa 25.2 llvmpipe. A matching observer reproduced
a roughly 915 ms first reveal paint gap without renderer fallback or a missing
node; its internal cause remains unattributed. Preserve that scoped result in
[VALIDATION.md](VALIDATION.md), including the much smaller observer cost.
Modern GTK 4.18 NGL and actual Vulkan cold tests also missed strict phases;
changing backend/distribution alone is not a proven solution. Cold latency
remains unaccepted and must be tested on Fedora after an ordinary fresh start,
separately from repeated openings. A slow Cairo run can also miss quarters.
Report a sampling failure instead of
duplicating frames, disabling animation, stretching production timing or
weakening tolerances. Choosing a supported fixture renderer does not force a
production backend; retain separate Cairo structural checks and owner-local
default-renderer acceptance.

### CI environment and selector coverage

Keep Ubuntu 24.04 as the compiler, package, sandboxed-WebKit and functional
baseline: 29 named native functional selectors remain in each stable/MSRV
lane, with selected HiDPI repeats. Run the same two strict native visual
selectors in dedicated `Native UI <toolchain> (Debian 13)` jobs for both Rust
stable and exact 1.98.1, at both scales, on the modern native stack matching
prepared cloud evidence (GTK >= 4.18, Libadwaita >= 1.7), with the explicit
recorded graphics-initialization/measured-cycle separation above. This environment
does not raise Karin's application minimum, change Flatpak SDK or replace a
failed Ubuntu result with an invented PASS.

The compiled selector check must account for all **31 = 29 + 2** names across
these jobs; omitting the strict tests, weakening quarter tolerances or extending
production duration is not permitted. Both quality lanes also retain ordinary
captures, matched a9 before captures and source/package/renderer provenance in
`karin-native-visual-<lane>-<github.sha>`. Ubuntu retains its functional
`karin-ui-<github.sha>` evidence. Inspect actual run results before claiming
success; a YAML definition and local PASS are not hosted evidence.

### Input, queue and playback policy

- Exercise actual mouse/Enter/Space, cover/Ctrl+I, footer Queue/Ctrl+J, mode
  changes, repeat activation, X/compact Back, focus return, outside clicks,
  scrolling, navigation and both directions of all breakpoints. Preserve mode
  and open intent; Diagnostics may suspend presentation, logout clears it.
  Include expanded navigation at 720 pixels: actual content below 620 pixels
  uses the dedicated panel subview rather than squeezing unusable columns.
- Check current occurrence following through Play, Next/Previous, queue pick,
  EOS and MPRIS, including duplicate IDs and late metadata/artwork replies.
  Explicit Details never plays. Pause/Seek/Volume retain explicit selection;
  the next actual current occurrence resumes following without a pane restart.
- Queue uses one widget tree and the existing local context. Test current and
  noncurrent removal, clear, appends, unavailable items, duplicates, partial
  collections and 500 positions. Its displayed next/history order must match
  actual policy; rapidly replaced rows cannot dispatch stale actions.
- Test Shuffle on/off without changing the current occurrence, bounded cycles,
  actual Previous history and queue edits. Test Repeat Off/Queue/One, EOS versus
  explicit Next/Previous, combinations with Shuffle, new contexts, logout and
  close. A resolver failure must not create an automatic retry loop.
  Repeat One needs an actual visible “1”, in addition to correct active state,
  tooltip and accessible name; a differently shaped unlabeled repeat icon alone
  does not satisfy that visual distinction.
- Exercise writable MPRIS Shuffle and LoopStatus (`None`, `Track`, `Playlist`),
  invalid writes, PropertiesChanged and capability updates. Native and D-Bus
  controls share one policy and the same PlaybackController. Retain Stop,
  acknowledged Seeked, opaque IDs, private local cover files and bus cleanup;
  no token, cookie, account ID or signed media address crosses D-Bus.

Screenshots supplement native geometry/input/worker tests. Generated local media
and synthetic loopback metadata are test evidence only. New Fedora/KDE/Wayland/
PipeWire, audible Music and current Flatpak acceptance remain owner-local.
[PR #15](https://github.com/tracerxbrhd/karin/pull/15) stays Draft; no new PR or
merge is authorized by a successful cloud run.

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
dark themes**. Cover signed-out library/search and queue-panel navigation and the single
native sign-in dialog without real credentials. Account/search/detail states
may use synthetic fixtures only in explicitly identified tests, never a release
catalog. Use actual allocated dimensions; distinguish requested size from the
size achieved under the window manager.

| Window state | Approximate size | Main checks |
| --- | --- | --- |
| Compact | 420×720 | Narrow navigation, readable wrapping, reachable actions and vertical scrolling |
| Compact boundary | 720×650 | Shared panel remains nonmodal; central actions stay reachable |
| Medium | 900×650 | Appropriate header navigation, hierarchy and balanced density |
| Wide boundary | 1280×800 | Content/panel split and footer alignment survive the transition |
| Wide | 1440×900 | Bounded component widths, content aligned near the top, no oversized centered empty-state layout |
| Owner wide | 1650×930 | Address the reported gap between central content and the panel |
| Full HD | 1920×1080 | Useful wide composition, natural collection tile sizes and integrated panel |
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
Shuffle/LoopStatus must match the local queue policy and remain writable through
the official values, with PropertiesChanged after actual mode changes. Stop
remains available externally even though its visible footer button is removed.
Fedora acceptance must separately verify an actual selected Music track,
`playerctl`, KDE's media widget and physical media keys, in the native artifact
and, when built, the current Flatpak sandbox. No manual result is passed until
it is actually performed.

## Retained session, storage and information-workspace gates

Treat the accepted v0.3 service/audio/session/cache behavior as the regression
baseline, not a pass for v0.4 presentation or new backend/failure scenarios.
ADRs 0022/0023 extend the former information presentation and local playback
policy without replacing secure-session/cache ownership. At every
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
v0.4's universal-panel checks. Verify actual motion/surface allocation,
nonmodal hit testing, compact underlying-view hiding, selected mode and explicit
open intent during intermediate frames as well as after the animation settles. No automatic dismiss/reopen
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
