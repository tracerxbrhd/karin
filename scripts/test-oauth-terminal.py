#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline PTY checks. Every case stops before browser launch or HTTP requests.

Only synthetic invalid markers are entered. No OAuth identity or account is used.
"""

import argparse
import fcntl
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import termios
import time


PROMPT = b"Karin Client ID: "
MARKER = b"SYNTHETIC-NOT-A-CLIENT"


def controlling_terminal():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


def read_until(master, child, expected, timeout=10):
    output = bytearray()
    deadline = time.monotonic() + timeout
    while expected not in output:
        remaining = deadline - time.monotonic()
        assert remaining > 0, "OAuth diagnostic did not display its local prompt"
        ready, _, _ = select.select([master], [], [], min(remaining, 0.1))
        if ready:
            output.extend(os.read(master, 16384))
        elif child.poll() is not None:
            raise AssertionError("OAuth diagnostic exited before its local prompt")
    return output


def exercise_terminal(binary, label, entered, expected, environment, browser_marker):
    master, slave = os.openpty()
    child = None
    try:
        original = termios.tcgetattr(slave)
        child = subprocess.Popen(
            [binary, "--oauth-diagnostic"],
            stdin=slave,
            stdout=slave,
            stderr=slave,
            preexec_fn=controlling_terminal,
            close_fds=True,
            env=environment,
        )
        output = read_until(master, child, PROMPT)
        hidden = termios.tcgetattr(slave)
        assert not hidden[3] & (termios.ECHO | termios.ECHONL | termios.ICANON | termios.ISIG), (
            "Local OAuth input must disable echo and handle cancellation itself"
        )
        assert MARKER not in output, "Synthetic private marker appeared before input"
        os.write(master, entered)
        child.wait(timeout=10)
        while select.select([master], [], [], 0)[0]:
            output.extend(os.read(master, 16384))
        assert child.returncode == 1, f"{label}: expected a locally cancelled/invalid run"
        assert termios.tcgetattr(slave) == original, f"{label}: terminal flags were not restored"
        assert MARKER not in output, f"{label}: private input was echoed"
        assert expected in output, f"{label}: missing sanitized local outcome"
        assert b"One-time authorization code" not in output, f"{label}: attempted browser flow"
        assert not browser_marker.exists(), f"{label}: attempted isolated browser handler"
        print(f"OAuth terminal: {label} passed (no echo; restored; no browser or network)")
    finally:
        if child is not None and child.poll() is None:
            child.kill()
            child.wait(timeout=5)
        os.close(master)
        os.close(slave)


def isolated_browser_environment(directory):
    """A regression may launch only a harmless local stub, never a real browser.

    The stub records an empty marker, discards its arguments and exits. Neither
    authorization URLs nor process arguments are captured in fixtures or logs.
    """
    directory = Path(directory)
    data_home = directory / "data"
    applications = data_home / "applications"
    config_home = directory / "config"
    config_dirs = directory / "config-dirs"
    data_dirs = directory / "data-dirs"
    runtime = directory / "runtime"
    for item in [applications, config_home, config_dirs, data_dirs, runtime]:
        item.mkdir(parents=True, mode=0o700)
    marker = directory / "browser-launched"
    handler = directory / "no-browser.py"
    handler.write_text(
        "from pathlib import Path\n"
        f"Path({str(marker)!r}).touch()\n"
        "raise SystemExit(1)\n",
        encoding="utf-8",
    )
    desktop_name = "karin-test-no-browser.desktop"
    (applications / desktop_name).write_text(
        "[Desktop Entry]\nType=Application\nName=Karin offline test guard\n"
        f"Exec={sys.executable} {handler} %u\n"
        "Terminal=false\nDBusActivatable=false\n"
        "MimeType=x-scheme-handler/https;x-scheme-handler/http;\n",
        encoding="utf-8",
    )
    defaults = (
        "[Default Applications]\n"
        f"x-scheme-handler/https={desktop_name}\n"
        f"x-scheme-handler/http={desktop_name}\n"
    )
    (config_home / "mimeapps.list").write_text(defaults, encoding="utf-8")
    (applications / "mimeapps.list").write_text(defaults, encoding="utf-8")
    environment = os.environ.copy()
    for name in ["DISPLAY", "WAYLAND_DISPLAY", "DESKTOP_STARTUP_ID", "FLATPAK_ID", "SNAP"]:
        environment.pop(name, None)
    environment.update(
        XDG_CONFIG_HOME=str(config_home),
        XDG_CONFIG_DIRS=str(config_dirs),
        XDG_DATA_HOME=str(data_home),
        XDG_DATA_DIRS=str(data_dirs),
        XDG_RUNTIME_DIR=str(runtime),
        XDG_CURRENT_DESKTOP="KarinOfflineTest",
        # Unsupported addresses forbid inherited desktop sessions and D-Bus
        # auto-launch. No real desktop/portal is allowed to handle test URIs.
        DBUS_SESSION_BUS_ADDRESS="disabled:",
        DBUS_SYSTEM_BUS_ADDRESS="disabled:",
        GIO_USE_PORTALS="0",
        GTK_USE_PORTAL="0",
        BROWSER="/usr/bin/false",
    )
    return environment, marker


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", default="target/debug/karin")
    binary = str(Path(parser.parse_args().binary).resolve(strict=True))
    cancelled = b"cancelled locally"
    invalid = b"Invalid local input"
    with tempfile.TemporaryDirectory(prefix="karin-oauth-terminal-") as directory:
        environment, browser_marker = isolated_browser_environment(directory)
        for label, entered, expected in [
            ("Ctrl-C", MARKER + b"\x03", cancelled),
            ("Ctrl-D", MARKER + b"\x04", cancelled),
            ("Escape", MARKER + b"\x1b", cancelled),
            ("empty input", b"\r", cancelled),
            ("invalid control byte", MARKER + b"\x16", invalid),
            ("bounded overflow", MARKER + b"x" * 4097, invalid),
            # Slash is unconditionally rejected by the local Client ID parser;
            # never rely on the marker merely looking unlike a registration.
            ("invalid Client ID", MARKER + b"/\r", b"Check the local Karin Client ID"),
        ]:
            exercise_terminal(binary, label, entered, expected, environment, browser_marker)

        no_tty = subprocess.run(
            [binary, "--oauth-diagnostic"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
            timeout=10,
            check=False,
            env=environment,
        )
        assert no_tty.returncode == 1 and b"requires a local controlling terminal" in no_tty.stderr
        assert PROMPT not in no_tty.stdout + no_tty.stderr
        assert not browser_marker.exists(), "A local test attempted the isolated browser handler"
        print("OAuth terminal: no controlling terminal refused before credential input")


if __name__ == "__main__":
    main()
