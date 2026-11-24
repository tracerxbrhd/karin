#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Opt-in synthetic fixture only; no Cargo build, real account or production mode.
# control|trace TEST_ELF NEW_OUTPUT [ngl|cairo|opengl|vulkan|default] [xvfb|desktop]
# Desktop uses the existing display and GTK row activation, without screenshots.
# Interposition perturbs execution; missing hooks are not proof of absent work.
set -euo pipefail
umask 077
fail() { printf 'Karin cold observer: %s\n' "$1" >&2; exit 1; }
[[ $# -ge 3 && $# -le 5 ]] || fail 'expected-mode-binary-new-output-renderer-display'
case "$1" in control|trace) mode=$1 ;; *) fail 'unsupported-mode' ;; esac
display=${5:-xvfb}
case "$display" in xvfb|desktop) ;; *) fail 'unsupported-display-mode' ;; esac
renderer=${4:-ngl}
case "$renderer" in ngl|cairo|opengl|vulkan|default) ;; *) fail 'unsupported-renderer' ;; esac
unset LD_PRELOAD KARIN_COLD_TRACE_FILE KARIN_COLD_EVIDENCE_FILE KARIN_COLD_DESKTOP
for command in realpath python3 dbus-run-session timeout; do
  command -v "$command" >/dev/null || fail 'required-tool-unavailable'
done
if [[ "$display" == xvfb ]]; then
  for command in xvfb-run openbox xdotool wmctrl; do
    command -v "$command" >/dev/null || fail 'required-x11-tool-unavailable'
  done
else
  [[ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]] || fail 'existing-display-unavailable'
fi
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
binary=$(realpath -- "$2")
[[ -x "$binary" ]] || fail 'test-binary-unavailable'
[[ ! -L "$3" ]] || fail 'output-must-be-new'
output=$(realpath -m -- "$3")
[[ ! -e "$output" && ! -L "$output" ]] || fail 'output-must-be-new'
mkdir -- "$output"
mkdir -- "$output/config" "$output/cache" "$output/data" "$output/state" "$output/captures"
selector=ui::music::design_tests::native_first_footer_reveal_after_startup_without_graphics_initialization
"$binary" --list > "$output/test-list.txt"
python3 - "$output" "$binary" "$script_dir" "$selector" "$mode" "$renderer" "$display" <<'PY'
import hashlib, json, pathlib, shutil, sys
out, binary, scripts = map(pathlib.Path, sys.argv[1:4])
selector, mode, renderer, display = sys.argv[4:]
assert selector + ': test' in (out / 'test-list.txt').read_text().splitlines(), 'required-selector-unavailable'
inputs = [scripts / 'observe-cold-playerbar.sh', scripts / 'cold-playerbar-timing.c',
          scripts.parent / 'src/ui/music/design_tests.rs']
(out / 'observer-sources').mkdir()
for source in inputs:
    shutil.copyfile(source, out / 'observer-sources' / source.name)
receipt = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
           'observer_inputs': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs},
           'mode': mode, 'requested_renderer': renderer, 'display_mode': display,
           'input_driver': 'gtk-row-activation' if display == 'desktop' else 'x11-xtest',
           'private_synthetic_profile': True, 'timeout_seconds': 120,
           'binary_source_provenance': 'verify separately against the build receipt'}
(out / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
PY
run_env=(env -u LD_PRELOAD -u KARIN_COLD_TRACE_FILE -u KARIN_COLD_EVIDENCE_FILE
  -u KARIN_COLD_DESKTOP -u GSK_DEBUG -u GSK_RENDERER
  G_DEBUG=fatal-warnings GTK_A11Y=none LP_NUM_THREADS=2
  XDG_CONFIG_HOME="$output/config" XDG_CACHE_HOME="$output/cache"
  XDG_DATA_HOME="$output/data" XDG_STATE_HOME="$output/state"
  KARIN_TEST_CAPTURE_DIR="$output/captures")
[[ "$renderer" == default ]] || run_env+=(GSK_RENDERER="$renderer")
if [[ "$display" == xvfb ]]; then
  run_env+=(GDK_BACKEND=x11 GDK_DPI_SCALE=1 GDK_SCALE=1)
  session=(dbus-run-session -- xvfb-run -a -s '-screen 0 1920x1080x24 -dpi 96')
else
  run_env+=(KARIN_COLD_DESKTOP=1)
  session=(dbus-run-session --)
fi
if [[ "$mode" == trace ]]; then
  command -v cc >/dev/null || fail 'compiler-unavailable'
  cc -O2 -Wall -Wextra -Werror -shared -fPIC "$output/observer-sources/cold-playerbar-timing.c" -ldl \
    -o "$output/timing.so" > "$output/compiler.log" 2>&1 || fail 'shim-compile-failed'
  run_env+=(LD_PRELOAD="$output/timing.so" KARIN_COLD_TRACE_FILE="$output/trace.jsonl"
    KARIN_COLD_EVIDENCE_FILE="$output/evidence.jsonl")
fi
# Only the exact synthetic test process receives the preload. Its constructor
# removes diagnostic variables before the fixture spawns input/WM subprocesses.
timeout --signal=TERM --kill-after=5s 120s "${session[@]}" "${run_env[@]}" \
  "$binary" --exact "$selector" --ignored --test-threads=1 --nocapture \
  > "$output/fixture.log" 2>&1 || fail 'fixture-failed-see-retained-log-and-invocation'
python3 - "$output" <<'PY'
import json, pathlib, sys
out = pathlib.Path(sys.argv[1])
receipt = json.loads((out / 'invocation.json').read_text())
log = (out / 'fixture.log').read_text()
assert 'test result: ok. 1 passed; 0 failed; 0 ignored;' in log, 'fixture-result'
obs = [json.loads(line.split('Karin native footer observation: ', 1)[1])
       for line in log.splitlines() if 'Karin native footer observation: ' in line]
assert [o['transition'] for o in obs] == ['startup-first-start', 'startup-explicit-stop',
                                       'startup-repeat-start', 'startup-repeat-stop'], 'transitions'
expected = {'ngl': {'GskGLRenderer', 'GskNglRenderer'}, 'opengl': {'GskGLRenderer', 'GskNglRenderer'},
            'cairo': {'GskCairoRenderer'}, 'vulkan': {'GskVulkanRenderer'}}
summary = dict(receipt, synthetic_fixture_passed=True, cold_latency_accepted=False, transitions=[])
for index, o in enumerate(obs):
    assert (o['transition_duration_ms'] == 240 and o['animations_enabled']
            and o['prior_playback_completed'] == (index >= 2)), 'observation-contract'
    if receipt['requested_renderer'] in expected:
        assert o['renderer'] in expected[receipt['requested_renderer']], 'renderer-fallback'
    if receipt['display_mode'] == 'desktop':
        assert o['input_driver'] == 'gtk-row-activation' and not o['image_exports_enabled'], 'desktop-scope'
    summary['transitions'].append({k: o[k] for k in (
        'transition', 'renderer', 'display', 'scale', 'input_driver', 'after_paint_count',
        'intermediate_frame_count', 'activation_duration_us', 'activation_to_endpoint_us',
        'maximum_native_frame_gap_us', 'sum_native_frame_gaps_us',
        'observation_callback_total_us', 'observation_callback_max_us')})
if receipt['mode'] == 'trace':
    rows = [json.loads(line) for line in (out / 'trace.jsonl').read_text().splitlines()]
    keys = {'stage', 'start_us', 'end_us', 'elapsed_us', 'thread_cpu_us', 'gtk_thread', 'tid'}
    stages = {'gtk-init', 'gst-init', 'gst-element-create', 'gst-element-state',
              'main-context-iteration', 'footer-request-open', 'footer-request-close',
              'memory-texture-create', 'pixbuf-texture-create', 'gsk-render',
              'gsk-render-texture', 'llvm-run-passes', 'llvm-compile-global',
              'gtk-layout-measure', 'gtk-layout-allocate', 'gtk-snapshot-child',
              'gtk-snapshot-to-node', 'gdk-gl-make-current', 'observer-stack-capture',
              'observer-context-query',
              'app-row-activation'}
    assert rows and len(rows) <= 50000 and all(set(r) == keys and r['stage'] in stages
        and type(r['gtk_thread']) is bool
        and all(type(r[k]) is int for k in keys - {'stage', 'gtk_thread'}) for r in rows), 'trace-schema'
    requests = [r for r in rows if r['stage'] in {'footer-request-open', 'footer-request-close'}]
    assert [r['stage'] for r in requests] == ['footer-request-open', 'footer-request-close'] * 2, 'trace-transition-coverage'
    evidence = [json.loads(line) for line in (out / 'evidence.jsonl').read_text().splitlines()]
    assert all(r['type'] in {'llvm-stack', 'gl-context', 'gsk-renderer'} for r in evidence), 'evidence-schema'
    summary['context_and_stacks'] = evidence
    summary['stage_counts'] = {s: sum(r['stage'] == s for r in rows) for s in sorted(stages)}
    summary['coverage_note'] = 'Missing hooks can mean below-threshold work or internal library binding; not zero cost.'
    def union_us(spans, start, end):
        union = []
        for a, b in sorted((max(r['start_us'], start), min(r['end_us'], end)) for r in spans):
            if b <= a:
                continue
            if union and a <= union[-1][1]:
                union[-1][1] = max(union[-1][1], b)
            else:
                union.append([a, b])
        return sum(b - a for a, b in union)
    for phase, o in zip(summary['transitions'], obs):
        start, end = o['activation_started_monotonic_us'], o['endpoint_observed_monotonic_us']
        overlap = [r for r in rows if r['start_us'] < end and r['end_us'] > start]
        active = [r for r in overlap if r['gtk_thread']]
        main = [r for r in active if r['stage'] == 'main-context-iteration']
        phase['slowest_main_iteration'] = max(main, key=lambda r: r['elapsed_us']) if main else None
        phase['stage_union_us'] = {s: union_us([r for r in active if r['stage'] == s], start, end)
                                   for s in sorted(stages) if any(r['stage'] == s for r in active)}
        phase['llvm_union_us'] = union_us([r for r in active if r['stage'].startswith('llvm-')], start, end)
        phase['gst_worker_union_us'] = {s: union_us([r for r in overlap if not r['gtk_thread'] and r['stage'] == s], start, end)
                                        for s in ('gst-init', 'gst-element-create', 'gst-element-state')}
        phase['overlapping_spans_note'] = 'Stage unions overlap; do not add them as disjoint costs.'
(out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary))
PY
