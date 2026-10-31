#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Run LAST among Cargo steps: two manifest roots otherwise share crate outputs.
# The baseline production sources stay at the accepted revision; only the same
# synthetic GTK evidence harness is added. No user account or remote API is used.
set -euo pipefail
repository=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output=$(realpath -m "${1:-$repository/build/ui-captures}")
baseline_revision=5e85e8d21a0e7075787d1242ed5abae0166f000c
temporary=$(mktemp -d "${TMPDIR:-/tmp}/karin-native-music-before.XXXXXX")
# The archive compilation replaces same-package outputs in the shared target.
# Invalidate only the current crate's timestamp for any later developer build;
# no tracked content changes, dependencies or production settings are altered.
trap 'touch "$repository/src/lib.rs"; rm -rf "$temporary"' EXIT
mkdir -p "$output"
cd "$repository"
if ! git cat-file -e "$baseline_revision^{commit}" 2>/dev/null; then
    git fetch --no-tags --depth=1 origin "$baseline_revision"
fi
git archive "$baseline_revision" | tar -x -C "$temporary"
cp src/ui/music/design_tests.rs "$temporary/src/ui/music/design_tests.rs"
printf '\n#[cfg(test)]\nmod design_tests;\n' >> "$temporary/src/ui/music.rs"
# Cargo's same-package filenames collide across manifest roots. Force this
# archived crate to compile even when its copied timestamps predate a cached
# output. Save the evidence binary immediately; do not run Cargo again afterward.
touch "$temporary/src/lib.rs"
cargo test --manifest-path "$temporary/Cargo.toml" --target-dir "$repository/target" \
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
    printf 'Accepted baseline production source: %s\n' "$baseline_revision"
    printf 'Current checkout (working-tree status follows): '; git rev-parse HEAD
    git status --short
    printf 'Added baseline-only test harness (same as current):\n'
    sha256sum src/ui/music/design_tests.rs
    printf 'Rebuilt baseline test executable:\n'
    sha256sum "$temporary/baseline-tests"
    printf 'Evidence: synthetic loopback MusicService and HTTP-decoded original raster fixtures.\n'
    printf '32 matched before cases: four sizes, two themes, two scales, pane open/closed.\n'
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
    GTK_A11Y=none GSK_RENDERER=cairo GSK_DEBUG=full-redraw GDK_SCALE="$scale" GDK_DPI_SCALE=1 \
        KARIN_TEST_CAPTURE_DIR="$directory" dbus-run-session -- xvfb-run -a \
        -s "-screen 0 $screen -dpi 96" "$temporary/baseline-tests" \
        native_design_matched_matrix_and_populated_surfaces --ignored --test-threads=1
done
