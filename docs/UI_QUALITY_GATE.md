<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# UI quality gate

Every substantial UI change must include real visual inspection and GTK
structural checks. A compiling widget tree, a visible window or a screenshot
alone is insufficient. Review the existing technical prototype without adding
fictional music catalogs, recommendations or player controls.

## Required matrix

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
  when its complete text is available through a tooltip and accessible label;
  this exception does not apply to headings, errors, status text or action labels.
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
  visible permitted-source blocker. Disabled player controls must not suggest
  successful service playback. Default builds must not expose Device Flow.
- Open the sign-in dialog at every size/theme. Check loading, truthful provider
  blocking, retry, Escape/cancel and focus return. No build mode may expose Client
  ID/secret, device codes, pasted tokens or URL fields in this user journey.
  Only validated Music account access may complete sign-in; test callback/state,
  cancellation and late responses against local fixtures. Clear old errors on
  logout, cancel and a fresh attempt; use native message rows with wrapped text.
- If the optional WebKit component changes, check ephemeral-session construction
  and destruction, trusted origin display and callback interception separately
  from provider permission. Exercise actual local page loading with the sandbox
  enabled where supported; report namespace/sandbox restrictions as blocked.
- Never capture real application credentials, verification codes, auth/browser
  pages or account/library data for public evidence. Cloud capture should use
  clean signed-out states; authenticated fixture coverage must be explicit.

## GTK structure and behavior

Run formatting, Clippy without warnings, tests, the build, GTK smoke test and
GStreamer diagnostic. Structural checks must cover actual window/widget
allocation, adaptive navigation visibility, page selection and normal close/
resource release. Check that widgets remain inside the content area and that
scrolling exposes diagnostics on short windows.

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

The matrix also opens/closes the public blocked login dialog at all four sizes
in both themes. These captures never load a real authorization page or claim a
successful provider login.

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
