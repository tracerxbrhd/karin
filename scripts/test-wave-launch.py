#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline normal startup, deprecated-alias forwarding and cleanup.

Run under Xvfb. A private bus without activatable services and fresh profiles
exclude remembered accounts; this never opens sign-in or starts a radio request.
"""

import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

APP = "io.github.tracerxbrhd.Karin"
OBJECT_PATH = "/io/github/tracerxbrhd/Karin"


def bus_call(method, *arguments):
    return subprocess.run(
        ["gdbus", "call", "--session", "--dest", APP, "--object-path", OBJECT_PATH,
         "--method", "org.gtk.Actions." + method, *arguments],
        capture_output=True, timeout=4,
    )


def primary_windows(pid):
    result = subprocess.run(
        ["xdotool", "search", "--onlyvisible", "--pid", str(pid)],
        capture_output=True, timeout=4,
    )
    return result.returncode, result.stdout.splitlines()


def assert_primary_owner(pid):
    result = subprocess.run(
        ["gdbus", "call", "--session", "--dest", "org.freedesktop.DBus",
         "--object-path", "/org/freedesktop/DBus", "--method",
         "org.freedesktop.DBus.GetConnectionUnixProcessID", APP],
        capture_output=True, timeout=4,
    )
    owner = re.fullmatch(rb"\(uint32 (\d+),\)\s*", result.stdout)
    assert result.returncode == 0 and owner, "missing application bus owner"
    assert int(owner.group(1)) == pid, "forwarding replaced the primary process"


def check_launch(binary, directory, alias):
    environment = os.environ.copy()
    for kind in ("CONFIG", "CACHE", "DATA", "STATE"):
        path = directory / kind.lower()
        path.mkdir(parents=True, mode=0o700)
        environment["XDG_" + kind + "_HOME"] = str(path)
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        primary = subprocess.Popen(
            [binary] + (["--experimental-wave"] if alias else []),
            env=environment, stdout=stdout, stderr=stderr,
        )
        try:
            deadline = time.monotonic() + 12
            while True:
                assert primary.poll() is None, "primary exited before its window"
                window_status, windows = primary_windows(primary.pid)
                actions = bus_call("List")
                if window_status == actions.returncode == 0 and b"quit" in actions.stdout:
                    assert b"enable-wave-experiment" not in actions.stdout, "obsolete Wave gate action"
                    break
                assert time.monotonic() < deadline, "window/actions startup deadline"
                time.sleep(0.05)
            assert windows, "normal primary has no visible window"
            assert_primary_owner(primary.pid)
            for arguments in ([], ["--experimental-wave"]):
                forwarded = subprocess.run(
                    [binary, *arguments], env=environment,
                    capture_output=True, timeout=8,
                )
                assert forwarded.returncode == 0 and primary.poll() is None, "normal activation forwarding failed"
                assert not forwarded.stderr, "forwarding emitted a warning"
                assert_primary_owner(primary.pid)
                status, current = primary_windows(primary.pid)
                assert status == 0 and current == windows, "forwarding created or lost an application window"
            assert bus_call("Activate", "quit", "[]", "{}").returncode == 0
            assert primary.wait(timeout=12) == 0, "primary cleanup failed"
            stderr.seek(0)
            diagnostic = stderr.read()
            assert b"WARNING" not in diagnostic and b"CRITICAL" not in diagnostic, "native startup/cleanup warning"
            assert b"Karin wave:" not in diagnostic, "signed-out launch must not start radio work"
        finally:
            if primary.poll() is None:
                primary.terminate()
                primary.wait(timeout=8)
    print("Wave launch: " + ("deprecated-alias" if alias else "ordinary") + " primary, normal+alias forwarding and cleanup passed (no account/radio)")


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--isolated-bus":
        binary = str(Path(sys.argv[2]).resolve(strict=True))
        help_result = subprocess.run([binary, "--help"], capture_output=True, timeout=4)
        alias_help = subprocess.run(
            [binary, "--experimental-wave", "--help"], capture_output=True, timeout=4,
        )
        assert help_result.returncode == alias_help.returncode == 0
        assert b"Deprecated no-op alias" in help_result.stdout
        assert alias_help.stdout == help_result.stdout and not alias_help.stderr
        with tempfile.TemporaryDirectory(prefix="karin-wave-profiles-") as temporary:
            root = Path(temporary)
            check_launch(binary, root / "ordinary", False)
            check_launch(binary, root / "deprecated-alias", True)
        return
    if len(sys.argv) != 2:
        raise SystemExit("Usage: test-wave-launch.py path/to/karin (under Xvfb)")
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    with tempfile.TemporaryDirectory(prefix="karin-wave-bus-") as temporary:
        config = Path(temporary) / "session.conf"
        config.write_text('''<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow receive_sender="*" eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
''')
        # No service directories: neither desktop wallets nor portals can be
        # activated. The permissive policy belongs only to this isolated test bus.
        result = subprocess.run(
            ["dbus-run-session", "--config-file", str(config), "--",
             sys.executable, str(Path(__file__).resolve()), "--isolated-bus", binary],
            timeout=60,
        )
        raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
