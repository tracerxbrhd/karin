#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Use a separate target directory: manifest roots must not share crate outputs.
# The refinement baseline is the exact PR head before this iteration, a70bbf7.
# Its existing source and synthetic test harness are archived without adaptation.
# No user account or remote API is used.
set -euo pipefail
repository=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output=$(realpath -m "${1:-$repository/build/ui-captures}")
baseline_revision=a70bbf714e3d7ea775c03839eb01f0b1d9dd4f62
baseline_target=${CARGO_TARGET_DIR:-$repository/target/native-music-baseline}
capture_renderer=${KARIN_CAPTURE_RENDERER:-cairo}
motion_only=${KARIN_BASELINE_MOTION_ONLY:-0}
temporary=$(mktemp -d "${TMPDIR:-/tmp}/karin-native-music-before.XXXXXX")
trap 'rm -rf "$temporary"' EXIT
mkdir -p "$output"
cd "$repository"
if ! git cat-file -e "$baseline_revision^{commit}" 2>/dev/null; then
    git fetch --no-tags --depth=1 origin "$baseline_revision"
fi
git archive "$baseline_revision" | tar -x -C "$temporary"
# Keep the archived composition and the complete initialized-motion fixture.
# Copying the new harness into it would no longer demonstrate the old UI.
# Force the archived crate to compile even when its copied timestamps predate a
# cached output. Its isolated target cannot replace the current application.
touch "$temporary/src/lib.rs"
cargo test --manifest-path "$temporary/Cargo.toml" --target-dir "$baseline_target" \
    --lib --all-features --locked --no-run --message-format=json > "$temporary/artifacts.jsonl"
python3 - "$temporary" <<'PY'
import json
import shutil
import sys
from pathlib import Path

temporary = Path(sys.argv[1])
executables = []
for line in (temporary / 'artifacts.jsonl').read_text().splitlines():
    record = json.loads(line)
    if record.get('executable') and record.get('profile', {}).get('test'):
        assert not record['fresh'], 'Archived baseline must have been rebuilt'
        executables.append(record['executable'])
assert len(executables) == 1
shutil.copy2(executables[0], temporary / 'baseline-tests')
PY
{
    printf 'Previous-iteration baseline production/test source: %s\n' "$baseline_revision"
    printf 'Current checkout (working-tree status follows): '; git rev-parse HEAD
    git status --short
    printf 'Unmodified archived test harness:\n'
    sha256sum "$temporary/src/ui/music/design_tests.rs"
    printf 'Rebuilt baseline test executable:\n'
    sha256sum "$temporary/baseline-tests"
    printf 'Evidence: synthetic loopback MusicService and HTTP-decoded original raster fixtures.\n'
    if [ "$motion_only" = 1 ]; then
        printf 'Motion-only observation; the separately retained static matrix is not regenerated.\n'
    else
        printf '96 matched before cases: eight geometries, two themes, two scales, closed/track/queue.\n'
    fi
    printf 'Requested capture renderer: %s; motion JSON records the actual native class.\n' "$capture_renderer"
    printf 'Software Mesa raster worker limit: LP_NUM_THREADS=%s (unset means Mesa default).\n' "${LP_NUM_THREADS:-unset}"
    printf 'Static captures force full redraw; normal-motion checks unset GSK_DEBUG for ordinary damage tracking.\n'
    printf 'Baseline Queue is captured in its existing pinned universal panel.\n'
    printf 'Baseline motion also measures a normal cycle after separately recorded graphics initialization; cold responsiveness is unaccepted.\n'
    printf 'True maximization: isolated Openbox; neither screenshots nor fixtures accept Fedora.\n'
    rustc --version
    for package in gtk4 libadwaita-1 gstreamer-1.0 webkitgtk-6.0; do
        printf '%s: ' "$package"; pkg-config --modversion "$package"
    done
} > "$output/comparison-info.txt"
unset KARIN_V04_ASSERT_DENSITY
for scale in 1 2; do
    if [ "$scale" = 1 ]; then screen=1920x1080x24; else screen=3840x2160x24; fi
    directory="$output/before/scale$scale"
    mkdir -p "$directory"
    if [ "$motion_only" != 1 ]; then
        GTK_A11Y=none GSK_RENDERER="$capture_renderer" GSK_DEBUG=full-redraw GDK_SCALE="$scale" GDK_DPI_SCALE=1 \
            KARIN_TEST_CAPTURE_DIR="$directory" dbus-run-session -- xvfb-run -a \
            -s "-screen 0 $screen -dpi 96" "$temporary/baseline-tests" \
            native_design_matched_matrix_and_populated_surfaces --ignored --test-threads=1
    fi
    env -u GSK_DEBUG GTK_A11Y=none GSK_RENDERER="$capture_renderer" GDK_SCALE="$scale" GDK_DPI_SCALE=1 \
        KARIN_TEST_CAPTURE_DIR="$directory" dbus-run-session -- xvfb-run -a \
        -s "-screen 0 $screen -dpi 96" "$temporary/baseline-tests" \
        native_universal_panel_whole_surface_motion_and_mode_switch --ignored --test-threads=1
done
