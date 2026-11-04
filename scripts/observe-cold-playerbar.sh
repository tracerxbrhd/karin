#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Opt-in synthetic fixture only. No Cargo build, real account or production use.
# Run CPU-exclusively twice with different new output directories:
#   scripts/observe-cold-playerbar.sh control /absolute/native-tests /tmp/cold-control
#   scripts/observe-cold-playerbar.sh trace   /absolute/native-tests /tmp/cold-trace
# Verify source/build provenance separately. Interposition perturbs execution;
# absence of a hook does not establish coverage or a physical-GPU diagnosis.
set -euo pipefail
umask 077
fail() { printf 'Karin cold observer: %s\n' "$1" >&2; exit 1; }
[[ $# == 3 ]] || fail 'expected-mode-binary-new-output-directory'
case "$1" in control|trace) mode=$1 ;; *) fail 'unsupported-mode' ;; esac
unset LD_PRELOAD KARIN_COLD_TRACE_FILE
for command in realpath python3 dbus-run-session xvfb-run openbox xdotool wmctrl; do
  command -v "$command" >/dev/null || fail 'required-tool-unavailable'
done
binary=$(realpath -- "$2")
[[ -x "$binary" ]] || fail 'test-binary-unavailable'
[[ ! -L "$3" ]] || fail 'output-must-be-new'
output=$(realpath -m -- "$3")
[[ ! -e "$output" && ! -L "$output" ]] || fail 'output-must-be-new'
mkdir -- "$output"
mkdir -- "$output/config" "$output/cache" "$output/data" "$output/state" "$output/captures"
selector=ui::music::design_tests::native_first_footer_reveal_after_startup_without_graphics_initialization
"$binary" --list > "$output/test-list.txt"
python3 - "$output/test-list.txt" "$selector" <<'PY'
import pathlib, sys
if sys.argv[2] + ': test' not in pathlib.Path(sys.argv[1]).read_text().splitlines():
    raise SystemExit('Karin cold observer: required-selector-unavailable')
PY
run_env=(env -u LD_PRELOAD -u KARIN_COLD_TRACE_FILE -u GSK_DEBUG
  G_DEBUG=fatal-warnings GTK_A11Y=none GSK_RENDERER=ngl GDK_BACKEND=x11
  GDK_DPI_SCALE=1 GDK_SCALE=1 LP_NUM_THREADS=2
  XDG_CONFIG_HOME="$output/config" XDG_CACHE_HOME="$output/cache"
  XDG_DATA_HOME="$output/data" XDG_STATE_HOME="$output/state"
  KARIN_TEST_CAPTURE_DIR="$output/captures")
if [[ "$mode" == trace ]]; then
  command -v cc >/dev/null || fail 'compiler-unavailable'
  script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
  cc -O2 -Wall -Wextra -shared -fPIC "$script_dir/cold-playerbar-timing.c" -ldl \
    -o "$output/timing.so" > "$output/compiler.log" 2>&1 || fail 'shim-compile-failed'
  run_env+=(LD_PRELOAD="$output/timing.so" KARIN_COLD_TRACE_FILE="$output/trace.jsonl")
fi
# Only the exact synthetic test process gets the opt-in preload. Its constructor
# removes the two diagnostic variables before that process spawns children.
dbus-run-session -- xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96' \
  "${run_env[@]}" "$binary" --exact "$selector" --ignored --test-threads=1 --nocapture \
  > "$output/fixture.log" 2>&1 || fail 'fixture-failed'
python3 - "$output" "$binary" "$mode" <<'PY'
import hashlib, json, pathlib, sys
out, binary, mode = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
log = (out / 'fixture.log').read_text()
assert 'test result: ok. 1 passed; 0 failed; 0 ignored;' in log, 'fixture-result'
obs = [json.loads(line.split('Karin native footer observation: ', 1)[1])
       for line in log.splitlines() if 'Karin native footer observation: ' in line]
assert [o['transition'] for o in obs] == ['startup-first-start', 'startup-explicit-stop',
                                       'startup-repeat-start', 'startup-repeat-stop'], 'transitions'
summary = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'mode': mode,
           'synthetic_fixture_passed': True, 'cold_latency_accepted': False, 'transitions': []}
for index, o in enumerate(obs):
    assert (o['transition_duration_ms'] == 240 and o['animations_enabled']
            and o['renderer'] == 'GskGLRenderer'
            and o['prior_playback_completed'] == (index >= 2)), 'observation-contract'
    summary['transitions'].append({'transition': o['transition'], 'frames': o['after_paint_count'],
        'intermediate_frames': o['intermediate_frame_count'],
        'max_native_gap_us': max(f['native_frame_gap_us'] or 0 for f in o['frames'])})
if mode == 'trace':
    rows = [json.loads(line) for line in (out / 'trace.jsonl').read_text().splitlines()]
    keys = {'stage', 'start_us', 'end_us', 'elapsed_us', 'thread_cpu_us', 'gtk_thread', 'tid'}
    stages = {'gtk-init', 'gst-init', 'gst-element-create', 'gst-element-state',
              'main-context-iteration', 'footer-request-open', 'footer-request-close',
              'memory-texture-create', 'pixbuf-texture-create', 'gsk-render',
              'gsk-render-texture', 'llvm-run-passes', 'llvm-compile-global'}
    assert rows and all(set(r) == keys and r['stage'] in stages
        and type(r['gtk_thread']) is bool
        and all(type(r[k]) is int for k in keys - {'stage', 'gtk_thread'}) for r in rows), 'trace-schema'
    opens = [r for r in rows if r['stage'] == 'footer-request-open']
    assert len(opens) == 2, 'trace-opening-coverage'
    llvm_observed = any(r['stage'].startswith('llvm-') for r in rows)
    summary['llvm_api_coverage'] = 'observed' if llvm_observed else 'not-observed'
    summary['opening_spans'] = []
    for opening in opens:
        iterations = [r for r in rows if r['stage'] == 'main-context-iteration' and r['gtk_thread']
                      and opening['end_us'] <= r['start_us'] <= opening['end_us'] + 300000]
        if not iterations:
            summary['opening_spans'].append({'slow_iteration_observed': False})
            continue
        w = max(iterations, key=lambda r: r['elapsed_us'])
        spans = sorted((r['start_us'], r['end_us']) for r in rows if r['stage'].startswith('llvm-')
                       and r['gtk_thread'] and w['start_us'] <= r['start_us'] and r['end_us'] <= w['end_us'])
        union = []
        for start, end in spans:
            if union and start <= union[-1][1]:
                union[-1][1] = max(union[-1][1], end)
            else:
                union.append([start, end])
        summary['opening_spans'].append({'main_wall_us': w['elapsed_us'],
            'main_thread_cpu_us': w['thread_cpu_us'], 'llvm_spans': len(spans),
            'llvm_union_us': sum(end - start for start, end in union) if llvm_observed else None})
(out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary))
PY
