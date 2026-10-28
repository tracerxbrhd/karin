#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Capture Karin only, in an isolated Xvfb/Openbox session; never a user's desktop.
set -eu
repository=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
binary="$repository/target/debug/karin"
if [ "${1-}" != --session ]; then
    output_dir=$(realpath -m "${1:-$repository/build/ui-captures}")
    mkdir -p "$output_dir"
    test -x "$binary"
    {
        printf 'Base commit (working-tree changes may follow):\n'
        git -C "$repository" rev-parse HEAD
        git -C "$repository" status --short
        printf '\nRendered binary and UI sources:\n'
        sha256sum "$binary"
        sha256sum "$repository/src/ui.rs" "$repository/src/ui/playback.rs" \
            "$repository/src/ui/music.rs" "$repository/src/music.rs" \
            "$repository/src/session.rs" "$repository/src/session/device.rs" \
            "$repository/src/queue.rs" "$repository/src/main.rs" \
            "$repository/src/playback.rs" "$repository/src/capabilities.rs"
        printf '\nExperimental UI requested: %s\n' "${KARIN_CAPTURE_EXPERIMENTAL:-0}"
        if command -v dpkg-query >/dev/null 2>&1; then
            dpkg-query -W xvfb openbox imagemagick xdotool librsvg2-common
        fi
        openbox --version
        printf '\nCapture: isolated Xvfb 1920x1080 at 96dpi; cairo/full-redraw; light and dark.\n'
    } > "$output_dir/capture-info.txt"
    for theme in light dark; do
        env ADW_DEBUG_COLOR_SCHEME="prefer-$theme" GTK_A11Y=none GSK_RENDERER=cairo GSK_DEBUG=full-redraw \
            GDK_SCALE=1 GDK_DPI_SCALE=1 \
            dbus-run-session -- xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
            "$0" --session "$output_dir/$theme"
    done
    exit 0
fi
output_dir=$2
mkdir -p "$output_dir"
openbox --sm-disable > "$output_dir/openbox.log" 2>&1 &
wm_pid=$!
app_pid=
cleanup() {
    if [ -n "$app_pid" ]; then kill "$app_pid" 2>/dev/null || true; fi
    kill "$wm_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM
# Openbox starts asynchronously. A visible GTK window does not mean that the
# root window's EWMH capabilities are ready for xdotool windowactivate yet.
wm_failure() {
    echo "$1" >&2
    cat "$output_dir/openbox.log" >&2
    xprop -root _NET_SUPPORTING_WM_CHECK _NET_SUPPORTED >&2 || true
    exit 1
}
wm_ready=false
attempt=0
while [ "$attempt" -lt 80 ]; do
    kill -0 "$wm_pid" 2>/dev/null || wm_failure 'Openbox exited before becoming ready'
    wm_check=$(xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null | sed -n 's/.*window id # \(0x[0-9a-fA-F]*\).*/\1/p')
    wm_supported=$(xprop -root _NET_SUPPORTED 2>/dev/null || true)
    if [ -n "$wm_check" ]; then
        case "$wm_supported" in
            *'_NET_ACTIVE_WINDOW'*) wm_ready=true; break ;;
        esac
    fi
    attempt=$((attempt + 1))
    sleep 0.1
done
[ "$wm_ready" = true ] || wm_failure 'Openbox did not publish EWMH activation support within 8 seconds'
if [ "${KARIN_CAPTURE_EXPERIMENTAL:-0}" = 1 ]; then
    "$binary" --experimental-music > "$output_dir/application.log" 2>&1 &
else
    "$binary" > "$output_dir/application.log" 2>&1 &
fi
app_pid=$!
window=
attempt=0
while [ "$attempt" -lt 80 ]; do
    window=$(xdotool search --onlyvisible --pid "$app_pid" --name '^Karin$' 2>/dev/null | head -n 1 || true)
    if [ -n "$window" ]; then break; fi
    attempt=$((attempt + 1))
    sleep 0.1
done
test -n "$window"
timeout 5 xdotool windowactivate --sync "$window"
capture() {
    sleep 0.6
    # Read the visible screen region, not GTK's possibly stale backing buffer.
    # -window restricts the result to this clean application's client rectangle.
    import -silent -screen -window "$window" "$output_dir/$1.png"
    xwininfo -id "$window" > "$output_dir/$1.geometry.txt"
    xprop -id "$window" _NET_WM_STATE >> "$output_dir/$1.geometry.txt"
}
resize() {
    wmctrl -ir "$window" -b remove,maximized_vert,maximized_horz
    sleep 0.2
    timeout 5 xdotool windowsize --sync "$window" "$1" "$2"
    timeout 5 xdotool windowmove --sync "$window" 48 48
}
scroll_to_top() {
    # Keyboard focus can reveal an entire preferences group after resizing.
    # Keep initial-page matrix frames at the top; transport frames stay natural.
    xdotool mousemove --window "$window" 350 200
    xdotool click --repeat 8 --delay 40 4
}
for shape in compact medium wide; do
    case "$shape" in
        compact) resize 420 720 ;;
        medium) resize 900 650 ;;
        wide) resize 1440 900 ;;
    esac
    xdotool key --clearmodifiers alt+1
    xdotool key --clearmodifiers ctrl+l
    capture "$shape-overview"
    xdotool key --clearmodifiers ctrl+f
    capture "$shape-search"
    xdotool key --clearmodifiers ctrl+j
    capture "$shape-queue"
    xdotool key --clearmodifiers ctrl+l
    xdotool key --clearmodifiers alt+2
    scroll_to_top
    capture "$shape-diagnostics"
done
wmctrl -ir "$window" -b add,maximized_vert,maximized_horz
xdotool key --clearmodifiers alt+1
xdotool key --clearmodifiers ctrl+l
capture maximized-overview
xdotool key --clearmodifiers ctrl+f
capture maximized-search
xdotool key --clearmodifiers ctrl+j
capture maximized-queue
xdotool key --clearmodifiers ctrl+l
xdotool key --clearmodifiers alt+2
scroll_to_top
capture maximized-diagnostics
# Keyboard activation uses only the generated WAV and the default fakesink.
resize 900 650
xdotool key --clearmodifiers alt+2
xdotool key --clearmodifiers Return
sleep 1
# Preparation temporarily disables its button, so GTK may move keyboard focus.
# Re-enter the diagnostics action before tabbing to the now-ready Play control.
xdotool key --clearmodifiers alt+2
xdotool key --clearmodifiers Tab
xdotool key --clearmodifiers shift+Tab Tab
capture keyboard-focus-diagnostics
xdotool key --clearmodifiers Return
sleep 0.4
capture keyboard-activation
xdotool key --clearmodifiers space
capture playback-paused
xdotool key --clearmodifiers Tab Return
capture playback-stopped
resize 420 720
xdotool key --clearmodifiers alt+2
xdotool key --clearmodifiers Tab Tab Tab Tab Tab Tab Tab
xdotool key --clearmodifiers ctrl+End
# Read-only native-library text follows the last interactive row. Scroll to
# expose it as well; keyboard focus/activation is captured separately above.
xdotool mousemove --window "$window" 350 560
xdotool click --repeat 8 --delay 60 5
capture compact-diagnostics-bottom
xdotool key --clearmodifiers ctrl+q
attempt=0
while kill -0 "$app_pid" 2>/dev/null && [ "$attempt" -lt 80 ]; do
    attempt=$((attempt + 1))
    sleep 0.1
done
if kill -0 "$app_pid" 2>/dev/null; then
    echo 'Application did not close via Ctrl+Q within 8 seconds' >&2
    exit 1
fi
wait "$app_pid"
app_pid=
