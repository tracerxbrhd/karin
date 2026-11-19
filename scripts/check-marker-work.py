#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check actual native marker work counts; elapsed times remain observations."""
import json
from pathlib import Path
import sys


def main():
    if len(sys.argv) != 2:
        raise SystemExit("expected one native marker observation log")
    log = Path(sys.argv[1]).read_text()
    assert "test result: ok. 1 passed; 0 failed; 0 ignored;" in log
    records = [json.loads(line.split("KARIN_MARKER_PERF ", 1)[1])
               for line in log.splitlines() if "KARIN_MARKER_PERF " in line]
    assert [record["rows"] for record in records] == [500, 2000]
    for record in records:
        assert record["append_marker_evaluations"] == record["rows"]
        assert record["polls"] == 24
        assert record["stable_marker_evaluations"] == 0
        assert record["changed_selection_marker_evaluations"] == record["rows"]
        assert record["future_edit_marker_evaluations"] == record["rows"]
        assert record["source_window_identity_preserved"]
        assert record["current_occurrence_preserved"]
        assert record["network_requests"] == 0
        assert record["player_created"] is False
        assert record["physical_gpu_or_frame_latency_measurement"] is False
    print("Native catalog marker work passed: linear append, zero unchanged-tick scans")


if __name__ == "__main__":
    main()
